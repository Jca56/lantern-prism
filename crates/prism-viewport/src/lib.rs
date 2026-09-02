//! Prism 3D viewport (D009): the camera, the grid, solid shading, overlays
//! and ID picking. The viewport is a view, never the truth: it draws
//! evaluated meshes from a document snapshot and turns clicks back into
//! element handles.

pub mod camera;
pub mod gpu;
pub mod request;

pub use camera::{Camera, ViewPreset};
pub use gpu::renderer::{PreparedFrame, Renderer};
pub use request::{Drag, GizmoHandle, GizmoMode, PickSet, Overlays, PickMode, PickPurpose, PickRequest, PickResult, Shading, ViewColors, ViewportRequest, ViewportState, scene_bounds};
