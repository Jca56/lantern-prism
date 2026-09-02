//! Typed generational handle: `{ index, generation }` tagged by a marker type
//! so a vertex handle can never be passed where an edge handle is expected.
//!
//! The generation is a `NonZeroU32`, so `Option<Handle<T>>` is still 8 bytes.

use core::fmt;
use core::hash::{Hash, Hasher};
use core::marker::PhantomData;
use core::num::NonZeroU32;

pub struct Handle<T> {
    index: u32,
    generation: NonZeroU32,
    // `fn() -> T` keeps the handle Send + Sync + Copy regardless of `T`.
    _marker: PhantomData<fn() -> T>,
}

impl<T> Handle<T> {
    /// The generation every slot starts at.
    pub const FIRST_GENERATION: NonZeroU32 = NonZeroU32::MIN;

    #[inline]
    pub const fn new(index: u32, generation: NonZeroU32) -> Self {
        Self { index, generation, _marker: PhantomData }
    }

    /// Handle for `index` at the first generation. Useful in tests and for
    /// containers that never free.
    #[inline]
    pub const fn from_index(index: u32) -> Self {
        Self::new(index, Self::FIRST_GENERATION)
    }

    #[inline]
    pub const fn index(self) -> u32 {
        self.index
    }

    #[inline]
    pub const fn idx(self) -> usize {
        self.index as usize
    }

    #[inline]
    pub const fn generation(self) -> NonZeroU32 {
        self.generation
    }

    /// Pack to a `u64` (`generation << 32 | index`) for storage or hashing.
    #[inline]
    pub const fn to_raw(self) -> u64 {
        ((self.generation.get() as u64) << 32) | self.index as u64
    }

    /// Inverse of [`Self::to_raw`]. `None` if the generation bits are zero.
    #[inline]
    pub const fn from_raw(raw: u64) -> Option<Self> {
        match NonZeroU32::new((raw >> 32) as u32) {
            Some(g) => Some(Self::new(raw as u32, g)),
            None => None,
        }
    }

    /// Reinterpret as a handle of another marker type. For remaps and
    /// serialization only.
    #[inline]
    pub const fn cast<U>(self) -> Handle<U> {
        Handle::new(self.index, self.generation)
    }
}

/// The generation after `g`, skipping zero on wrap.
#[inline]
pub fn next_generation(g: NonZeroU32) -> NonZeroU32 {
    NonZeroU32::new(g.get().wrapping_add(1)).unwrap_or(NonZeroU32::MIN)
}

impl<T> Clone for Handle<T> {
    #[inline]
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> Copy for Handle<T> {}

impl<T> PartialEq for Handle<T> {
    #[inline]
    fn eq(&self, o: &Self) -> bool {
        self.index == o.index && self.generation == o.generation
    }
}
impl<T> Eq for Handle<T> {}

impl<T> PartialOrd for Handle<T> {
    fn partial_cmp(&self, o: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(o))
    }
}
impl<T> Ord for Handle<T> {
    fn cmp(&self, o: &Self) -> core::cmp::Ordering {
        (self.index, self.generation).cmp(&(o.index, o.generation))
    }
}

impl<T> Hash for Handle<T> {
    fn hash<H: Hasher>(&self, h: &mut H) {
        self.to_raw().hash(h);
    }
}

impl<T> fmt::Debug for Handle<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = core::any::type_name::<T>().rsplit("::").next().unwrap_or("?");
        write!(f, "{name}#{}@{}", self.index, self.generation)
    }
}

impl<T> fmt::Display for Handle<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{}@{}", self.index, self.generation)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Vert;
    struct Edge;

    #[test]
    fn size_and_niche() {
        assert_eq!(size_of::<Handle<Vert>>(), 8);
        assert_eq!(size_of::<Option<Handle<Vert>>>(), 8);
    }

    #[test]
    fn raw_roundtrip() {
        let g = NonZeroU32::new(7).unwrap();
        let h = Handle::<Vert>::new(123, g);
        assert_eq!(Handle::<Vert>::from_raw(h.to_raw()), Some(h));
        assert_eq!(Handle::<Vert>::from_raw(123), None);
        assert_eq!(h.cast::<Edge>().index(), 123);
        assert_eq!(format!("{h:?}"), "Vert#123@7");
    }

    #[test]
    fn generation_wraps_past_zero() {
        assert_eq!(next_generation(NonZeroU32::new(1).unwrap()).get(), 2);
        assert_eq!(next_generation(NonZeroU32::new(u32::MAX).unwrap()).get(), 1);
    }

    #[test]
    fn ordering_and_hash() {
        use std::collections::HashSet;
        let a = Handle::<Vert>::from_index(1);
        let b = Handle::<Vert>::from_index(2);
        let a2 = Handle::<Vert>::new(1, NonZeroU32::new(2).unwrap());
        assert!(a < b && a < a2);
        let set: HashSet<_> = [a, b, a2, a].into_iter().collect();
        assert_eq!(set.len(), 3);
    }
}
