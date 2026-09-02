//! Slot storage with a free list and generation counters: the backing store
//! for [`Handle`]s in non-persistent, runtime-only data (UI state, GPU caches).
//! Mesh domains use `ChunkedVec` columns with their own slot logic instead.

use core::num::NonZeroU32;

use crate::handle::{Handle, next_generation};

struct Slot<T> {
    generation: NonZeroU32,
    value: Option<T>,
}

pub struct Arena<T> {
    slots: Vec<Slot<T>>,
    free: Vec<u32>,
    len: usize,
}

impl<T> Default for Arena<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Arena<T> {
    pub const fn new() -> Self {
        Self { slots: Vec::new(), free: Vec::new(), len: 0 }
    }

    pub fn with_capacity(n: usize) -> Self {
        Self { slots: Vec::with_capacity(n), free: Vec::new(), len: 0 }
    }

    /// Number of live values.
    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Number of slots ever allocated (live + free).
    #[inline]
    pub fn slot_count(&self) -> usize {
        self.slots.len()
    }

    pub fn insert(&mut self, value: T) -> Handle<T> {
        self.len += 1;
        if let Some(index) = self.free.pop() {
            let slot = &mut self.slots[index as usize];
            slot.value = Some(value);
            return Handle::new(index, slot.generation);
        }
        let index = u32::try_from(self.slots.len()).expect("Arena overflow");
        self.slots.push(Slot { generation: Handle::<T>::FIRST_GENERATION, value: Some(value) });
        Handle::new(index, Handle::<T>::FIRST_GENERATION)
    }

    /// Remove and return the value, bumping the slot's generation so the old
    /// handle (and any copies) become stale.
    pub fn remove(&mut self, h: Handle<T>) -> Option<T> {
        let slot = self.slots.get_mut(h.idx())?;
        if slot.generation != h.generation() || slot.value.is_none() {
            return None;
        }
        let v = slot.value.take();
        slot.generation = next_generation(slot.generation);
        self.free.push(h.index());
        self.len -= 1;
        v
    }

    #[inline]
    pub fn contains(&self, h: Handle<T>) -> bool {
        self.get(h).is_some()
    }

    #[inline]
    pub fn get(&self, h: Handle<T>) -> Option<&T> {
        let slot = self.slots.get(h.idx())?;
        if slot.generation == h.generation() { slot.value.as_ref() } else { None }
    }

    #[inline]
    pub fn get_mut(&mut self, h: Handle<T>) -> Option<&mut T> {
        let slot = self.slots.get_mut(h.idx())?;
        if slot.generation == h.generation() { slot.value.as_mut() } else { None }
    }

    /// The live handle for a slot index, if that slot is occupied.
    pub fn handle_at(&self, index: u32) -> Option<Handle<T>> {
        let slot = self.slots.get(index as usize)?;
        slot.value.as_ref().map(|_| Handle::new(index, slot.generation))
    }

    pub fn iter(&self) -> impl Iterator<Item = (Handle<T>, &T)> {
        self.slots.iter().enumerate().filter_map(|(i, s)| {
            s.value.as_ref().map(|v| (Handle::new(i as u32, s.generation), v))
        })
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = (Handle<T>, &mut T)> {
        self.slots.iter_mut().enumerate().filter_map(|(i, s)| {
            let g = s.generation;
            s.value.as_mut().map(|v| (Handle::new(i as u32, g), v))
        })
    }

    pub fn handles(&self) -> impl Iterator<Item = Handle<T>> + '_ {
        self.iter().map(|(h, _)| h)
    }

    pub fn values(&self) -> impl Iterator<Item = &T> {
        self.iter().map(|(_, v)| v)
    }

    /// Remove every value whose predicate returns `false`.
    pub fn retain(&mut self, mut keep: impl FnMut(Handle<T>, &mut T) -> bool) {
        for i in 0..self.slots.len() {
            let slot = &mut self.slots[i];
            let g = slot.generation;
            if let Some(v) = slot.value.as_mut()
                && !keep(Handle::new(i as u32, g), v)
            {
                slot.value = None;
                slot.generation = next_generation(g);
                self.free.push(i as u32);
                self.len -= 1;
            }
        }
    }

    /// Drop everything; existing handles all go stale.
    pub fn clear(&mut self) {
        for (i, slot) in self.slots.iter_mut().enumerate() {
            if slot.value.take().is_some() {
                slot.generation = next_generation(slot.generation);
                self.free.push(i as u32);
            }
        }
        self.len = 0;
    }
}

impl<T> core::ops::Index<Handle<T>> for Arena<T> {
    type Output = T;
    fn index(&self, h: Handle<T>) -> &T {
        self.get(h).unwrap_or_else(|| panic!("stale or invalid handle {h}"))
    }
}

impl<T> core::ops::IndexMut<Handle<T>> for Arena<T> {
    fn index_mut(&mut self, h: Handle<T>) -> &mut T {
        self.get_mut(h).unwrap_or_else(|| panic!("stale or invalid handle {h}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_get_remove() {
        let mut a = Arena::new();
        let h1 = a.insert("one");
        let h2 = a.insert("two");
        assert_eq!(a.len(), 2);
        assert_eq!(a[h1], "one");
        assert_eq!(a.get(h2), Some(&"two"));
        assert_eq!(a.remove(h1), Some("one"));
        assert_eq!(a.remove(h1), None, "double remove is a no-op");
        assert!(!a.contains(h1));
        assert_eq!(a.len(), 1);
    }

    #[test]
    fn stale_handles_are_detected() {
        let mut a = Arena::new();
        let h1 = a.insert(1);
        a.remove(h1);
        let h2 = a.insert(2);
        assert_eq!(h1.index(), h2.index(), "slot is reused");
        assert_ne!(h1, h2, "but the generation differs");
        assert_eq!(a.get(h1), None);
        assert_eq!(a.get(h2), Some(&2));
        assert_eq!(a.slot_count(), 1);
    }

    #[test]
    fn iteration_and_retain() {
        let mut a = Arena::new();
        let hs: Vec<_> = (0..10).map(|i| a.insert(i)).collect();
        a.remove(hs[3]);
        let live: Vec<i32> = a.values().copied().collect();
        assert_eq!(live, vec![0, 1, 2, 4, 5, 6, 7, 8, 9]);
        a.retain(|_, v| *v % 2 == 0);
        assert_eq!(a.len(), 5);
        assert!(a.handles().all(|h| a[h] % 2 == 0));
        for (_, v) in a.iter_mut() {
            *v *= 10;
        }
        assert_eq!(a[hs[4]], 40);
        assert_eq!(a.handle_at(4), Some(hs[4]));
        assert_eq!(a.handle_at(3), None);
        a.clear();
        assert!(a.is_empty());
        assert_eq!(a.get(hs[4]), None);
    }

    #[test]
    #[should_panic(expected = "stale or invalid handle")]
    fn index_stale_panics() {
        let mut a = Arena::new();
        let h = a.insert(5);
        a.remove(h);
        let _ = a[h];
    }
}
