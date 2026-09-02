//! Persistent slot allocator shared by every column of a domain: generation
//! counters, a live bitset and an intrusive free list, all `ChunkedVec`s so
//! the allocator itself is copy-on-write like the data it indexes.

use core::marker::PhantomData;
use core::num::NonZeroU32;

use prism_core::handle::next_generation;
use prism_core::{ChunkedVec, Handle};

const NONE: u32 = u32::MAX;

pub struct Slots<T> {
    generation: ChunkedVec<u32>,
    live: ChunkedVec<bool>,
    next_free: ChunkedVec<u32>,
    free_head: u32,
    live_count: usize,
    _marker: PhantomData<fn() -> T>,
}

impl<T> Clone for Slots<T> {
    fn clone(&self) -> Self {
        Self {
            generation: self.generation.clone(),
            live: self.live.clone(),
            next_free: self.next_free.clone(),
            free_head: self.free_head,
            live_count: self.live_count,
            _marker: PhantomData,
        }
    }
}

impl<T> core::fmt::Debug for Slots<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Slots({} live / {} slots)", self.live_count, self.capacity())
    }
}

impl<T> Default for Slots<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Slots<T> {
    pub fn new() -> Self {
        Self {
            generation: ChunkedVec::new(),
            live: ChunkedVec::new(),
            next_free: ChunkedVec::new(),
            free_head: NONE,
            live_count: 0,
            _marker: PhantomData,
        }
    }

    /// Number of live elements.
    #[inline]
    pub fn len(&self) -> usize {
        self.live_count
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.live_count == 0
    }

    /// Number of slots ever allocated; every column has this many rows.
    #[inline]
    pub fn capacity(&self) -> usize {
        self.generation.len()
    }

    #[inline]
    pub fn is_live(&self, h: Handle<T>) -> bool {
        let i = h.idx();
        i < self.capacity() && self.live[i] && self.generation[i] == h.generation().get()
    }

    /// Is slot `index` live (regardless of generation)?
    #[inline]
    pub fn index_live(&self, index: usize) -> bool {
        index < self.capacity() && self.live[index]
    }

    /// The live handle at `index`, if any.
    pub fn handle_at(&self, index: usize) -> Option<Handle<T>> {
        if self.index_live(index) {
            Some(Handle::new(index as u32, NonZeroU32::new(self.generation[index]).expect("nonzero generation")))
        } else {
            None
        }
    }

    /// Take a slot. Returns the handle and whether the slot is brand new (so
    /// the owner must push a row onto every column).
    pub fn alloc(&mut self) -> (Handle<T>, bool) {
        self.live_count += 1;
        if self.free_head != NONE {
            let i = self.free_head as usize;
            self.free_head = self.next_free[i];
            self.live.set(i, true);
            let g = self.generation[i];
            (Handle::new(i as u32, NonZeroU32::new(g).expect("nonzero generation")), false)
        } else {
            let i = self.capacity();
            self.generation.push(1);
            self.live.push(true);
            self.next_free.push(NONE);
            (Handle::new(i as u32, Handle::<T>::FIRST_GENERATION), true)
        }
    }

    /// Release a slot. The generation bumps so the old handle goes stale.
    /// Returns `false` if the handle was not live.
    pub fn free(&mut self, h: Handle<T>) -> bool {
        if !self.is_live(h) {
            return false;
        }
        let i = h.idx();
        self.live.set(i, false);
        let g = next_generation(h.generation()).get();
        self.generation.set(i, g);
        self.next_free.set(i, self.free_head);
        self.free_head = i as u32;
        self.live_count -= 1;
        true
    }

    /// Live handles in index order.
    pub fn iter(&self) -> impl Iterator<Item = Handle<T>> + '_ {
        (0..self.capacity()).filter_map(move |i| self.handle_at(i))
    }

    /// Check the free list is consistent with the live bitset.
    pub fn check(&self) -> Result<(), String> {
        let mut seen = vec![false; self.capacity()];
        let mut cur = self.free_head;
        let mut n = 0;
        while cur != NONE {
            let i = cur as usize;
            if i >= self.capacity() {
                return Err(format!("free list points past capacity ({i})"));
            }
            if self.live[i] {
                return Err(format!("slot {i} is both live and free"));
            }
            if seen[i] {
                return Err(format!("free list cycles through slot {i}"));
            }
            seen[i] = true;
            n += 1;
            cur = self.next_free[i];
        }
        let dead = self.capacity() - self.live_count;
        if n != dead {
            return Err(format!("{n} slots on the free list but {dead} are dead"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct V;

    #[test]
    fn alloc_free_reuse() {
        let mut s: Slots<V> = Slots::new();
        let (a, new_a) = s.alloc();
        let (b, new_b) = s.alloc();
        assert!(new_a && new_b);
        assert_eq!((a.idx(), b.idx()), (0, 1));
        assert_eq!(s.len(), 2);
        assert!(s.free(a));
        assert!(!s.free(a), "double free is rejected");
        assert!(!s.is_live(a));
        assert!(s.is_live(b));
        assert_eq!(s.len(), 1);
        let (c, new_c) = s.alloc();
        assert!(!new_c, "reused slot 0");
        assert_eq!(c.idx(), 0);
        assert_ne!(c, a, "generation moved on");
        assert_eq!(s.capacity(), 2);
        assert_eq!(s.iter().collect::<Vec<_>>(), vec![c, b]);
        assert_eq!(s.handle_at(0), Some(c));
        s.check().unwrap();
    }

    #[test]
    fn persistence() {
        let mut a: Slots<V> = Slots::new();
        for _ in 0..3000 {
            a.alloc();
        }
        let b = a.clone();
        let h = a.handle_at(5).unwrap();
        a.free(h);
        assert!(b.is_live(h), "the clone is untouched");
        assert!(!a.is_live(h));
        a.check().unwrap();
        b.check().unwrap();
    }
}
