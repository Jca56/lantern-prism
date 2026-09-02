//! Compound operations, composed from euler operators (plus the vertex weld,
//! the one kernel-level primitive the euler set lacks). These are the tools'
//! building blocks; `prism-ops` wraps them as user-facing operators later.

mod delete;
mod dissolve;
mod extrude;
mod inset;
mod normals;
mod weld;

pub use extrude::ExtrudeResult;
pub use inset::{InsetResult, InsetVert};
