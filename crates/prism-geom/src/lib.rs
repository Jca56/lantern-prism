//! Pure geometry with no knowledge of meshes or documents (D011).
//!
//! - [`normal`]: Newell polygon normals, angle weights.
//! - [`triangulate`]: fast paths for triangles and quads, ear clipping for n-gons.
//! - [`predicates`]: orientation tests. Careful `f64` today; the API is shaped
//!   so exact arithmetic can replace the internals later.
//! - [`intersect`]: ray/triangle, closest points.
//! - [`bvh`]: bounding volume hierarchy over boxes with ray, box and
//!   closest-point queries.
//! - [`kdtree`]: points, nearest neighbour and radius queries.

pub mod bvh;
pub mod intersect;
pub mod kdtree;
pub mod normal;
pub mod predicates;
pub mod triangulate;

pub use bvh::Bvh;
pub use intersect::RayHit;
pub use kdtree::KdTree;
