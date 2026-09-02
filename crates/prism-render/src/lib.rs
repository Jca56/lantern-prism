//! Prism render: the wgpu wrapper. Device and surface setup, a small render
//! graph, the 2D draw-list pass that draws panels and glyphs from one vertex
//! stream against one atlas, and the shader preprocessor.
//!
//! This is the first crate allowed to touch wgpu (D011). Everything below it
//! tests headless; everything above it describes *what* to draw, never how.

pub mod atlas_gpu;
pub mod clear;
pub mod draw2d;
pub mod gpu;
pub mod graph;
pub mod pass2d;
pub mod shader;
pub mod surface;

pub use atlas_gpu::AtlasTexture;
pub use clear::clear_pass;
pub use draw2d::{DrawList, Vertex2d};
pub use gpu::{Gpu, GpuError};
pub use graph::{RenderGraph, TexDesc, TexId, TexturePool, Views};
pub use pass2d::Pass2d;
pub use surface::SurfaceTarget;
/// Re-exported so the app can create a surface without naming wgpu itself.
pub use wgpu;
