//! Byte views of plain-old-data for GPU uploads and file blobs. This is the
//! one file in the workspace allowed to reinterpret memory.
//!
//! A type may implement [`Pod`] only if **every** bit pattern of its size is
//! a valid value and it has **no padding bytes**: `#[repr(C)]` structs of
//! primitives with explicit padding fields, fixed arrays of those, and the
//! primitives themselves.

/// Marker for types that can be viewed as raw bytes.
///
/// # Safety
/// See the module docs: no padding, no invalid bit patterns, `#[repr(C)]` or
/// `#[repr(transparent)]`.
pub unsafe trait Pod: Copy + 'static {}

macro_rules! impl_pod_prims {
    ($($t:ty),*) => { $(unsafe impl Pod for $t {})* };
}
impl_pod_prims!(u8, u16, u32, u64, u128, i8, i16, i32, i64, i128, f32, f64, usize, isize);

unsafe impl<T: Pod, const N: usize> Pod for [T; N] {}

/// Declare user structs as POD. The caller promises the module-level rules.
#[macro_export]
macro_rules! impl_pod {
    ($($t:ty),* $(,)?) => { $(unsafe impl $crate::bytes::Pod for $t {})* };
}

/// The bytes of one value.
#[inline]
pub fn bytes_of<T: Pod>(v: &T) -> &[u8] {
    // SAFETY: `T: Pod` guarantees every byte is initialized and readable; the
    // slice borrows `v` so it cannot outlive it.
    unsafe { core::slice::from_raw_parts((v as *const T).cast::<u8>(), size_of::<T>()) }
}

/// The bytes of a slice of values.
#[inline]
pub fn slice_as_bytes<T: Pod>(s: &[T]) -> &[u8] {
    // SAFETY: as above; `size_of_val` accounts for the element stride.
    unsafe { core::slice::from_raw_parts(s.as_ptr().cast::<u8>(), size_of_val(s)) }
}

/// Copy bytes into a fresh `Vec<T>`. Panics if `bytes.len()` is not a
/// multiple of `size_of::<T>()`. Alignment of `bytes` does not matter.
pub fn vec_from_bytes<T: Pod>(bytes: &[u8]) -> Vec<T> {
    let sz = size_of::<T>();
    assert!(sz > 0 && bytes.len().is_multiple_of(sz), "byte length {} is not a multiple of {sz}", bytes.len());
    let n = bytes.len() / sz;
    let mut v: Vec<T> = Vec::with_capacity(n);
    // SAFETY: the destination has capacity for `n` elements (`n * sz` bytes),
    // the source has exactly that many bytes, and `T: Pod` makes any bit
    // pattern a valid `T`. `set_len` runs only after the copy.
    unsafe {
        core::ptr::copy_nonoverlapping(bytes.as_ptr(), v.as_mut_ptr().cast::<u8>(), bytes.len());
        v.set_len(n);
    }
    v
}

/// Read one value from the front of `bytes` (unaligned is fine). Panics if
/// there are too few bytes.
pub fn read_from_bytes<T: Pod>(bytes: &[u8]) -> T {
    assert!(bytes.len() >= size_of::<T>(), "not enough bytes for {}", core::any::type_name::<T>());
    // SAFETY: length checked; `read_unaligned` needs no alignment; `T: Pod`.
    unsafe { core::ptr::read_unaligned(bytes.as_ptr().cast::<T>()) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Copy, Debug, PartialEq)]
    #[repr(C)]
    struct Vertex {
        pos: [f32; 3],
        _pad: f32,
        color: [u8; 4],
        id: u32,
    }
    impl_pod!(Vertex);

    #[test]
    fn primitives() {
        assert_eq!(bytes_of(&0x0403_0201u32), &[1, 2, 3, 4]);
        assert_eq!(bytes_of(&1.0f32), &1.0f32.to_le_bytes());
        assert_eq!(slice_as_bytes(&[1u16, 2u16]), &[1, 0, 2, 0]);
    }

    #[test]
    fn structs_roundtrip() {
        let v = [
            Vertex { pos: [1.0, 2.0, 3.0], _pad: 0.0, color: [255, 0, 128, 255], id: 7 },
            Vertex { pos: [-1.0, 0.5, 0.0], _pad: 0.0, color: [0, 0, 0, 0], id: 8 },
        ];
        let bytes = slice_as_bytes(&v);
        assert_eq!(bytes.len(), 2 * 24);
        let back: Vec<Vertex> = vec_from_bytes(bytes);
        assert_eq!(back, v);
        assert_eq!(read_from_bytes::<Vertex>(bytes), v[0]);
        // Unaligned source is fine.
        let mut shifted = vec![0u8];
        shifted.extend_from_slice(bytes);
        assert_eq!(vec_from_bytes::<Vertex>(&shifted[1..]), v);
    }

    #[test]
    #[should_panic(expected = "not a multiple")]
    fn wrong_length_panics() {
        let _ = vec_from_bytes::<u32>(&[1, 2, 3]);
    }
}
