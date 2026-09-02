//! `ChunkedVec<T>`: the persistent array under every large dataset in Prism.
//!
//! Storage is `Vec<Arc<Vec<T>>>` with fixed 1024-element chunks. Cloning is
//! O(chunks) pointer copies; writing element `i` calls `Arc::make_mut` on one
//! chunk, so an edit touching 50 vertices of a 5M-vertex mesh copies ~50
//! chunks, not 5M elements. Old clones keep their chunks untouched, which is
//! what makes undo "just an old root".
//!
//! Every mutation stamps the vector with a **globally unique** version drawn
//! from one atomic counter. Two `ChunkedVec`s with equal versions therefore
//! have identical contents even across undo branches, so `(id, version)` is a
//! sound cache key. A clone keeps its source's version (same contents).

use core::fmt;
use core::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Elements per chunk.
pub const CHUNK: usize = 1024;

static NEXT_VERSION: AtomicU64 = AtomicU64::new(1);

/// A version number no `ChunkedVec` has ever carried before.
#[inline]
pub fn fresh_version() -> u64 {
    NEXT_VERSION.fetch_add(1, Ordering::Relaxed)
}

pub struct ChunkedVec<T> {
    chunks: Vec<Arc<Vec<T>>>,
    len: usize,
    version: u64,
}

impl<T> Default for ChunkedVec<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Clone for ChunkedVec<T> {
    /// O(chunks). Shares every chunk with the source and keeps its version.
    fn clone(&self) -> Self {
        Self { chunks: self.chunks.clone(), len: self.len, version: self.version }
    }
}

impl<T> ChunkedVec<T> {
    pub fn new() -> Self {
        Self { chunks: Vec::new(), len: 0, version: fresh_version() }
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Changes on every mutation; equal versions imply equal contents.
    #[inline]
    pub fn version(&self) -> u64 {
        self.version
    }

    #[inline]
    pub fn chunk_count(&self) -> usize {
        self.chunks.len()
    }

    #[inline]
    fn split(i: usize) -> (usize, usize) {
        (i / CHUNK, i % CHUNK)
    }

    #[inline]
    pub fn get(&self, i: usize) -> Option<&T> {
        if i >= self.len {
            return None;
        }
        let (c, o) = Self::split(i);
        // SAFETY-free: bounds are guaranteed by the length invariant.
        Some(&self.chunks[c][o])
    }

    #[inline]
    pub fn first(&self) -> Option<&T> {
        self.get(0)
    }

    #[inline]
    pub fn last(&self) -> Option<&T> {
        self.len.checked_sub(1).and_then(|i| self.get(i))
    }

    pub fn iter(&self) -> impl DoubleEndedIterator<Item = &T> + '_ {
        self.chunks.iter().flat_map(|c| c.iter())
    }

    /// The chunks themselves, for parallel iteration and sharing checks.
    #[inline]
    pub fn chunks(&self) -> &[Arc<Vec<T>>] {
        &self.chunks
    }

    /// Elements of chunk `c` as a slice.
    #[inline]
    pub fn chunk(&self, c: usize) -> &[T] {
        &self.chunks[c]
    }

    /// Index of the first element in chunk `c`.
    #[inline]
    pub fn chunk_start(c: usize) -> usize {
        c * CHUNK
    }

    /// How many chunks this vector shares (by pointer) with `other`.
    pub fn shared_chunks_with(&self, other: &Self) -> usize {
        self.chunks
            .iter()
            .zip(other.chunks.iter())
            .filter(|(a, b)| Arc::ptr_eq(a, b))
            .count()
    }

    /// Bytes owned by chunks not shared with any other clone.
    pub fn unique_heap_bytes(&self) -> usize {
        self.chunks
            .iter()
            .filter(|c| Arc::strong_count(c) == 1)
            .map(|c| c.capacity() * size_of::<T>())
            .sum()
    }
}

impl<T: Clone> ChunkedVec<T> {
    pub fn from_elem(value: T, n: usize) -> Self {
        let mut v = Self::new();
        v.resize(n, value);
        v
    }

    #[inline]
    fn touch(&mut self) {
        self.version = fresh_version();
    }

    /// Mutable access to one element. Copies only its chunk if shared.
    #[inline]
    pub fn get_mut(&mut self, i: usize) -> Option<&mut T> {
        if i >= self.len {
            return None;
        }
        self.touch();
        let (c, o) = Self::split(i);
        Arc::make_mut(&mut self.chunks[c]).get_mut(o)
    }

    #[inline]
    pub fn set(&mut self, i: usize, value: T) {
        *self.get_mut(i).expect("ChunkedVec::set out of range") = value;
    }

    /// Mutable access to a whole chunk with a single version bump. Use for
    /// bulk edits instead of many `get_mut` calls.
    pub fn chunk_mut(&mut self, c: usize) -> &mut [T] {
        self.touch();
        Arc::make_mut(&mut self.chunks[c]).as_mut_slice()
    }

    /// Mutable iteration. Un-shares **every** chunk; prefer `chunk_mut` when
    /// the edit is local.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut T> + '_ {
        self.touch();
        self.chunks.iter_mut().flat_map(|c| Arc::make_mut(c).iter_mut())
    }

    pub fn push(&mut self, value: T) {
        self.touch();
        if self.len.is_multiple_of(CHUNK) {
            self.chunks.push(Arc::new(Vec::with_capacity(CHUNK)));
        }
        let last = Arc::make_mut(self.chunks.last_mut().expect("chunk exists"));
        if last.capacity() < CHUNK {
            last.reserve_exact(CHUNK - last.len());
        }
        last.push(value);
        self.len += 1;
    }

    pub fn pop(&mut self) -> Option<T> {
        if self.len == 0 {
            return None;
        }
        self.touch();
        let last = self.chunks.last_mut().expect("chunk exists");
        let v = Arc::make_mut(last).pop();
        if last.is_empty() {
            self.chunks.pop();
        }
        self.len -= 1;
        v
    }

    pub fn truncate(&mut self, n: usize) {
        if n >= self.len {
            return;
        }
        self.touch();
        let keep_chunks = n.div_ceil(CHUNK);
        self.chunks.truncate(keep_chunks);
        if !n.is_multiple_of(CHUNK) {
            let last = self.chunks.last_mut().expect("chunk exists");
            Arc::make_mut(last).truncate(n % CHUNK);
        }
        self.len = n;
    }

    pub fn clear(&mut self) {
        self.truncate(0);
    }

    pub fn resize(&mut self, n: usize, value: T) {
        if n < self.len {
            self.truncate(n);
        } else {
            for _ in self.len..n {
                self.push(value.clone());
            }
        }
    }

    pub fn swap(&mut self, a: usize, b: usize) {
        if a == b {
            return;
        }
        let va = self[a].clone();
        let vb = self[b].clone();
        self.set(a, vb);
        self.set(b, va);
    }
}

impl<T> core::ops::Index<usize> for ChunkedVec<T> {
    type Output = T;
    #[inline]
    fn index(&self, i: usize) -> &T {
        self.get(i).unwrap_or_else(|| panic!("ChunkedVec index {i} out of range (len {})", self.len))
    }
}

impl<T: Clone> core::ops::IndexMut<usize> for ChunkedVec<T> {
    #[inline]
    fn index_mut(&mut self, i: usize) -> &mut T {
        let len = self.len;
        self.get_mut(i).unwrap_or_else(|| panic!("ChunkedVec index {i} out of range (len {len})"))
    }
}

impl<T: Clone> FromIterator<T> for ChunkedVec<T> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        let mut v = Self::new();
        v.extend(iter);
        v
    }
}

impl<T: Clone> Extend<T> for ChunkedVec<T> {
    fn extend<I: IntoIterator<Item = T>>(&mut self, iter: I) {
        for x in iter {
            self.push(x);
        }
    }
}

impl<T: PartialEq> PartialEq for ChunkedVec<T> {
    /// Content equality; versions are not compared.
    fn eq(&self, o: &Self) -> bool {
        self.len == o.len && self.iter().zip(o.iter()).all(|(a, b)| a == b)
    }
}

impl<T: fmt::Debug> fmt::Debug for ChunkedVec<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seq(n: usize) -> ChunkedVec<usize> {
        (0..n).collect()
    }

    #[test]
    fn push_pop_across_chunk_boundaries() {
        let mut v = ChunkedVec::new();
        for i in 0..(CHUNK * 2 + 5) {
            v.push(i);
        }
        assert_eq!(v.len(), CHUNK * 2 + 5);
        assert_eq!(v.chunk_count(), 3);
        assert_eq!(v.chunk(0).len(), CHUNK);
        assert_eq!(v.chunk(2).len(), 5);
        assert_eq!(v[CHUNK], CHUNK);
        assert_eq!(*v.last().unwrap(), CHUNK * 2 + 4);
        for _ in 0..6 {
            v.pop();
        }
        assert_eq!(v.len(), CHUNK * 2 - 1);
        assert_eq!(v.chunk_count(), 2, "emptied chunk is dropped");
        assert_eq!(v.pop(), Some(CHUNK * 2 - 2));
        v.truncate(CHUNK + 1);
        assert_eq!(v.len(), CHUNK + 1);
        assert_eq!(v.chunk_count(), 2);
        v.truncate(CHUNK);
        assert_eq!(v.chunk_count(), 1);
        v.clear();
        assert!(v.is_empty() && v.chunk_count() == 0);
        assert_eq!(v.pop(), None);
    }

    #[test]
    fn clone_is_persistent() {
        let a = seq(CHUNK * 10);
        let mut b = a.clone();
        assert_eq!(a.shared_chunks_with(&b), 10);
        b.set(CHUNK * 3 + 7, 999);
        // The original is untouched and nine of ten chunks are still shared.
        assert_eq!(a[CHUNK * 3 + 7], CHUNK * 3 + 7);
        assert_eq!(b[CHUNK * 3 + 7], 999);
        assert_eq!(a.shared_chunks_with(&b), 9);
        assert_eq!(a.len(), b.len());
        // Editing the same chunk again copies nothing further.
        b.set(CHUNK * 3 + 8, 998);
        assert_eq!(a.shared_chunks_with(&b), 9);
        // Push onto the clone does not disturb the original's last chunk
        // (which is full, so a new chunk is created).
        b.push(1);
        assert_eq!(a.shared_chunks_with(&b), 9);
        assert_eq!(a.len(), CHUNK * 10);
    }

    #[test]
    fn push_onto_shared_partial_chunk_copies_it() {
        let a = seq(10);
        let mut b = a.clone();
        b.push(10);
        assert_eq!(a.len(), 10);
        assert_eq!(b.len(), 11);
        assert_eq!(a.shared_chunks_with(&b), 0);
        // Chunks reserve their full capacity up front; both now own one chunk.
        assert_eq!(a.unique_heap_bytes(), CHUNK * size_of::<usize>());
        assert_eq!(b.unique_heap_bytes(), CHUNK * size_of::<usize>());
        assert_eq!(a.clone().unique_heap_bytes(), 0, "a shared chunk is not unique");
    }

    #[test]
    fn versions_are_globally_unique() {
        let mut a = seq(5);
        let v0 = a.version();
        let b = a.clone();
        assert_eq!(b.version(), v0, "a clone has the same contents, so the same version");
        a.set(0, 42);
        let v1 = a.version();
        assert_ne!(v1, v0);
        assert_eq!(b.version(), v0);
        // Two divergent edits from the same ancestor never collide.
        let mut c = b.clone();
        c.set(0, 43);
        assert_ne!(c.version(), a.version());
        assert_ne!(c.version(), v0);
        // Reading does not bump.
        let _ = a[0];
        let _ = a.iter().count();
        assert_eq!(a.version(), v1);
        // chunk_mut bumps once.
        a.chunk_mut(0)[1] = 7;
        assert_ne!(a.version(), v1);
    }

    #[test]
    fn bulk_and_iteration() {
        let mut v = ChunkedVec::from_elem(0u32, 2500);
        assert_eq!(v.chunk_count(), 3);
        for (i, x) in v.iter_mut().enumerate() {
            *x = i as u32;
        }
        assert_eq!(v.iter().map(|&x| x as usize).sum::<usize>(), (0..2500).sum());
        assert_eq!(v.iter().next_back(), Some(&2499));
        v.resize(3000, 5);
        assert_eq!(v[2999], 5);
        v.resize(100, 0);
        assert_eq!(v.len(), 100);
        v.swap(0, 99);
        assert_eq!((v[0], v[99]), (99, 0));
        let w: ChunkedVec<u32> = v.iter().copied().collect();
        assert_eq!(v, w);
        assert_ne!(v.version(), w.version());
        assert_eq!(format!("{:?}", seq(3)), "[0, 1, 2]");
        assert_eq!(seq(0).first(), None);
    }

    #[test]
    #[should_panic(expected = "out of range")]
    fn index_out_of_range_panics() {
        let v = seq(3);
        let _ = v[3];
    }

    #[test]
    fn get_mut_on_shared_leaves_clone_intact() {
        let a = seq(CHUNK + 1);
        let mut b = a.clone();
        *b.get_mut(CHUNK).unwrap() = 0;
        assert_eq!(a[CHUNK], CHUNK);
        assert_eq!(b[CHUNK], 0);
        assert_eq!(a.shared_chunks_with(&b), 1);
    }
}
