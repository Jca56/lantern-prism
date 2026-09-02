//! Prism core: the containers and utilities everything else is made of.
//!
//! - [`Handle`] / [`Arena`]: typed generational handles and slot storage.
//! - [`ChunkedVec`]: *the* persistent array. Cloning is O(chunks); editing
//!   copies one 1024-element chunk. Every mutation stamps a globally unique
//!   version, so `(id, version)` is a sound cache key across undo branches.
//! - [`Id`] / [`IdAllocator`]: document-global, never-reused 64-bit ids.
//! - [`jobs`]: thread pool with structured `scope` and `parallel_for`.
//! - [`block_on`], [`bytes`], [`Pcg32`], [`log`]: the small things we refuse
//!   to import.
//!
//! Nothing here knows about meshes, documents, or the GPU.

pub mod arena;
pub mod block_on;
pub mod bytes;
pub mod chunked;
pub mod handle;
pub mod id;
pub mod jobs;
pub mod log;
pub mod prng;

pub use arena::Arena;
pub use block_on::block_on;
pub use bytes::Pod;
pub use chunked::{CHUNK, ChunkedVec};
pub use handle::Handle;
pub use id::{Id, IdAllocator};
pub use jobs::{Pool, Scope};
pub use prng::Pcg32;
