//! What the UI asks the viewport renderer to draw and to pick.

use prism_core::Id;
use prism_math::{Color, Rect, Vec2};
use prism_mesh::{EdgeH, FaceH, VertH};

use crate::camera::Camera;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Shading {
    #[default]
    Solid,
    Wire,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Overlays {
    pub grid: bool,
    /// Wireframe on top of solid shading (always on in edit mode).
    pub wire: bool,
    /// Vertex dots in edit mode.
    pub verts: bool,
}

impl Default for Overlays {
    fn default() -> Self {
        Self { grid: true, wire: false, verts: true }
    }
}

/// A navigation drag in progress.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Nav {
    #[default]
    None,
    Orbit,
    Pan,
}

/// Per-area viewport state. Lives on the area, so every viewport has its own.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct ViewportState {
    pub camera: Camera,
    pub shading: Shading,
    pub overlays: Overlays,
    pub nav: Nav,
}

/// World bounds of the scene, or of the selected objects only.
pub fn scene_bounds(doc: &prism_doc::Doc, selected_only: bool) -> prism_math::Aabb {
    let mut bounds = prism_math::Aabb::EMPTY;
    for id in doc.scene_objects() {
        let Some(obj) = doc.objects.get(id) else {
            continue;
        };
        if selected_only && !obj.selected && doc.active_object_id() != id {
            continue;
        }
        let world = doc.object_matrix(id);
        match doc.object_mesh(id) {
            Some(block) => {
                let m = &block.mesh;
                let mut local = prism_math::Aabb::EMPTY;
                for v in m.verts() {
                    local.include(m.position(v));
                }
                if !local.is_empty() {
                    bounds = bounds.union(&local.transformed(&world));
                }
            }
            None => bounds.include(world.transform_point(prism_math::Vec3::ZERO)),
        }
    }
    bounds
}

/// Colors the theme decides.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewColors {
    pub bg: Color,
    pub grid_minor: Color,
    pub grid_major: Color,
    pub axis_x: Color,
    pub axis_z: Color,
    pub wire: Color,
    pub vertex: Color,
    pub selected: Color,
    pub active: Color,
    pub default_object: Color,
    /// Vertex dot diameter in physical pixels.
    pub point_size: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewportRequest {
    pub area: usize,
    /// Body rect in physical pixels.
    pub rect: Rect,
    pub state: ViewportState,
    pub colors: ViewColors,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PickMode {
    Object,
    Vertex,
    Edge,
    Face,
}

/// Why a pick was requested.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum PickPurpose {
    #[default]
    Select,
    /// Right click: open the context menu for what was hit.
    ContextMenu,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PickRequest {
    pub purpose: PickPurpose,
    pub area: usize,
    pub rect: Rect,
    pub camera: Camera,
    /// Window-space pixel.
    pub pos: Vec2,
    pub mode: PickMode,
    /// Search radius in pixels for vertices and edges.
    pub radius: f64,
    pub extend: bool,
    pub toggle: bool,
    pub colors: ViewColors,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PickResult {
    Nothing,
    Object(Id),
    Vert(Id, VertH),
    Edge(Id, EdgeH),
    Face(Id, FaceH),
}
