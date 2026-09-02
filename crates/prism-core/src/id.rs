//! Document-global identifiers. Never reused, allocated monotonically, saved
//! in the file. Datablocks reference each other by `Id`, never by pointer.

use core::fmt;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[repr(transparent)]
pub struct Id(pub u64);

impl Id {
    /// The null id. Never allocated.
    pub const NONE: Id = Id(0);

    #[inline]
    pub const fn is_none(self) -> bool {
        self.0 == 0
    }

    #[inline]
    pub const fn is_some(self) -> bool {
        self.0 != 0
    }

    #[inline]
    pub const fn raw(self) -> u64 {
        self.0
    }
}

impl fmt::Debug for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_none() { write!(f, "Id(NONE)") } else { write!(f, "Id({})", self.0) }
    }
}

impl fmt::Display for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{}", self.0)
    }
}

/// Hands out ids `1, 2, 3, …`. Saved with the document so ids stay unique
/// across sessions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IdAllocator {
    next: u64,
}

impl Default for IdAllocator {
    fn default() -> Self {
        Self::new()
    }
}

impl IdAllocator {
    pub const fn new() -> Self {
        Self { next: 1 }
    }

    /// Resume after loading: the next id handed out will be `next`.
    pub const fn resume(next: u64) -> Self {
        Self { next: if next == 0 { 1 } else { next } }
    }

    #[inline]
    pub fn alloc(&mut self) -> Id {
        let id = Id(self.next);
        self.next += 1;
        id
    }

    /// The id the next call to [`Self::alloc`] will return.
    #[inline]
    pub const fn peek(&self) -> Id {
        Id(self.next)
    }

    /// Make sure `id` (loaded from a file) is never handed out again.
    pub fn reserve(&mut self, id: Id) {
        if id.0 >= self.next {
            self.next = id.0 + 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allocation() {
        let mut a = IdAllocator::new();
        assert_eq!(a.alloc(), Id(1));
        assert_eq!(a.alloc(), Id(2));
        assert_eq!(a.peek(), Id(3));
        a.reserve(Id(10));
        assert_eq!(a.alloc(), Id(11));
        a.reserve(Id(4));
        assert_eq!(a.alloc(), Id(12));
        assert_eq!(IdAllocator::resume(0).alloc(), Id(1));
        assert_eq!(IdAllocator::resume(99).alloc(), Id(99));
    }

    #[test]
    fn none() {
        assert!(Id::NONE.is_none());
        assert!(Id(1).is_some());
        assert_eq!(format!("{:?} {}", Id::NONE, Id(7)), "Id(NONE) #7");
        assert_eq!(size_of::<Id>(), 8);
    }
}
