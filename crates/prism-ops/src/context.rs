//! What an operator sees: the document and the editing situation. Never the
//! UI directly; requests for it go through [`UiRequest`].

use prism_doc::Doc;
use prism_math::{Mat4, Ray, Rect, Vec2, Vec3};

use crate::input::Modifiers;

/// The 3D view an interactive operator runs in: enough to map pixels to
/// world space and back. The viewport editor fills it; it is `None` when
/// the operator was not started from a 3D view. (The camera type lives
/// above this crate, so this is plain matrices.)
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewInfo {
    /// Viewport body in window pixels.
    pub rect: Rect,
    pub view_proj: Mat4,
    pub inv_view_proj: Mat4,
    /// Camera position (perspective) or a point on the view axis (ortho).
    pub eye: Vec3,
    /// Unit view direction.
    pub forward: Vec3,
    pub ortho: bool,
}

impl ViewInfo {
    pub fn new(rect: Rect, view_proj: Mat4, eye: Vec3, forward: Vec3, ortho: bool) -> Self {
        Self { rect, view_proj, inv_view_proj: view_proj.inverse().unwrap_or(Mat4::IDENTITY), eye, forward, ortho }
    }

    /// Pixel position of a world point, or `None` behind the camera.
    pub fn project(&self, world: Vec3) -> Option<Vec2> {
        let clip = self.view_proj * world.extend(1.0);
        if clip.w <= 1e-9 {
            return None;
        }
        let ndc = Vec2::new(clip.x / clip.w, clip.y / clip.w);
        let r = self.rect;
        Some(Vec2::new(r.min.x + (ndc.x + 1.0) * 0.5 * r.width(), r.min.y + (1.0 - ndc.y) * 0.5 * r.height()))
    }

    /// World-space ray through `pixel`.
    pub fn ray(&self, pixel: Vec2) -> Ray {
        let r = self.rect;
        let ndc = Vec2::new((pixel.x - r.min.x) / r.width() * 2.0 - 1.0, 1.0 - (pixel.y - r.min.y) / r.height() * 2.0);
        let near = self.inv_view_proj.project_point(Vec3::new(ndc.x, ndc.y, 1.0));
        let far = self.inv_view_proj.project_point(Vec3::new(ndc.x, ndc.y, 0.5));
        let dir = (far - near).normalize_or(self.forward);
        let origin = if self.ortho { near } else { self.eye };
        Ray::new(origin, dir)
    }

    /// Where the ray through `pixel` meets the plane through `at` that faces
    /// the camera. Free (unconstrained) drags move on this plane.
    pub fn on_view_plane(&self, at: Vec3, pixel: Vec2) -> Option<Vec3> {
        let ray = self.ray(pixel);
        let denom = self.forward.dot(ray.dir);
        if denom.abs() < 1e-9 {
            return None;
        }
        Some(ray.at(self.forward.dot(at - ray.origin) / denom))
    }

    /// Parameter `t` of the point on the world line `origin + t·dir` nearest
    /// the ray through `pixel`. `None` when the line points (nearly) straight
    /// at the camera, where the answer is unstable.
    pub fn on_axis(&self, origin: Vec3, dir: Vec3, pixel: Vec2) -> Option<f64> {
        let ray = self.ray(pixel);
        let (d1, d2) = (dir, ray.dir);
        let r = origin - ray.origin;
        let (a, b, c, e, f) = (d1.dot(d1), d1.dot(d2), d1.dot(r), d2.dot(d2), d2.dot(r));
        let denom = a * e - b * b;
        if denom <= 1e-4 * a * e {
            return None;
        }
        Some((b * f - c * e) / denom)
    }

    /// World units one pixel spans at the depth of `at`.
    pub fn units_per_pixel(&self, at: Vec3) -> f64 {
        let side = self.forward.any_orthonormal();
        match (self.project(at), self.project(at + side)) {
            (Some(p), Some(q)) if p.distance(q) > 1e-9 => 1.0 / p.distance(q),
            _ => 0.0,
        }
    }
}

/// Something the operator wants the UI layer to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UiRequest {
    Undo,
    Redo,
    /// The document was replaced wholesale (new / open): forget history.
    HistoryClear,
    /// Open a named menu at the pointer.
    Menu(String),
    /// Open the command palette.
    Palette,
    /// Ask for a path, then run `op` with its `path` property set.
    PathDialog { op: String, save: bool },
    /// Frame the scene (or the selection) in the active 3D viewport.
    ViewFrame { selected: bool },
    /// Solid or wireframe shading in the active 3D viewport.
    ViewShading { wire: bool },
    ViewToggleGrid,
    /// Cycle the active viewport's transform gizmo (Move → Rotate → Scale).
    GizmoCycle,
    /// Show this gizmo (index into Move, Rotate, Scale).
    GizmoSet(usize),
    Quit,
}

pub struct Ctx<'a> {
    pub doc: &'a mut Doc,
    /// Pointer in physical pixels (window space).
    pub pointer: Vec2,
    /// The region the event came from.
    pub region: Rect,
    /// The 3D view under the pointer, for interactive tools.
    pub view: Option<ViewInfo>,
    pub mods: Modifiers,
    pub requests: Vec<UiRequest>,
    /// One line for the status area.
    pub report: Option<String>,
}

impl<'a> Ctx<'a> {
    pub fn new(doc: &'a mut Doc) -> Self {
        Self { doc, pointer: Vec2::ZERO, region: Rect::ZERO, view: None, mods: Modifiers::NONE, requests: Vec::new(), report: None }
    }

    pub fn report(&mut self, msg: impl Into<String>) {
        self.report = Some(msg.into());
    }

    pub fn request(&mut self, r: UiRequest) {
        self.requests.push(r);
    }
}

/// Result of `exec`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The document changed (or the op did its job): record undo.
    Finished,
    /// Nothing happened; nothing to record.
    Cancelled,
    /// The op did not want this; let others try.
    PassThrough,
}

/// Result of `invoke` / `modal`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flow {
    Running,
    Finished,
    Cancelled,
    PassThrough,
}

impl From<Outcome> for Flow {
    fn from(o: Outcome) -> Flow {
        match o {
            Outcome::Finished => Flow::Finished,
            Outcome::Cancelled => Flow::Cancelled,
            Outcome::PassThrough => Flow::PassThrough,
        }
    }
}
