//! Prism math: `f64` everywhere on the CPU, `f32` only as a transmission format.
//!
//! Conventions (see `docs/ARCHITECTURE.md` §4):
//! - Column-major storage, column vectors, `M * v`.
//! - Right-handed, **+Y up**. View space looks down **-Z**.
//! - Clip depth is **reverse-Z** in `[0, 1]` with an infinite far plane.
//! - Radians everywhere. The props layer displays degrees.
//!
//! No `f32` math types exist here. If you want one, you are about to lose
//! precision by accident — call `to_gpu()` at the upload boundary instead.

mod macros;

pub mod aabb;
pub mod color;
pub mod frustum;
pub mod mat3;
pub mod mat4;
pub mod plane;
pub mod quat;
pub mod ray;
pub mod rect;
pub mod scalar;
pub mod transform;
pub mod vec2;
pub mod vec3;
pub mod vec4;

pub use aabb::Aabb;
pub use color::Color;
pub use frustum::Frustum;
pub use mat3::Mat3;
pub use mat4::Mat4;
pub use plane::Plane;
pub use quat::Quat;
pub use ray::Ray;
pub use rect::Rect;
pub use scalar::*;
pub use transform::Transform;
pub use vec2::Vec2;
pub use vec3::Vec3;
pub use vec4::Vec4;
