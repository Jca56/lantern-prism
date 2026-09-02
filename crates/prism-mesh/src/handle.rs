//! Typed handles for the four element domains.

use prism_core::Handle;

/// Marker types. They never hold data; they make `Handle<Vert>` and
/// `Handle<Edge>` different types.
#[derive(Debug)]
pub struct Vert;
#[derive(Debug)]
pub struct Edge;
#[derive(Debug)]
pub struct Face;
#[derive(Debug)]
pub struct Loop;

pub type VertH = Handle<Vert>;
pub type EdgeH = Handle<Edge>;
pub type FaceH = Handle<Face>;
pub type LoopH = Handle<Loop>;

/// Placeholder stored in required link columns before an euler operator
/// fills them in. Never valid for a live element; `validate` checks.
pub(crate) fn invalid<T>() -> Handle<T> {
    Handle::new(u32::MAX, Handle::<T>::FIRST_GENERATION)
}

pub(crate) fn is_invalid<T>(h: Handle<T>) -> bool {
    h.index() == u32::MAX
}
