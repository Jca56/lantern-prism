//! Orbit camera. Y up, right-handed, looks down -Z in view space, reverse-Z
//! projection. Everything the GPU receives is camera-relative (D004).

use prism_math::{Aabb, Mat3, Mat4, Quat, Ray, Rect, Vec2, Vec3, deg_to_rad};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewPreset {
    Front,
    Back,
    Right,
    Left,
    Top,
    Bottom,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    pub target: Vec3,
    pub distance: f64,
    /// Rotation about +Y, radians.
    pub yaw: f64,
    /// Rotation about the camera's X, radians; negative looks down.
    pub pitch: f64,
    /// Vertical field of view, radians.
    pub fov_y: f64,
    pub ortho: bool,
}

impl Default for Camera {
    fn default() -> Self {
        Self { target: Vec3::ZERO, distance: 12.0, yaw: deg_to_rad(40.0), pitch: deg_to_rad(-30.0), fov_y: deg_to_rad(50.0), ortho: false }
    }
}

const PITCH_LIMIT: f64 = core::f64::consts::FRAC_PI_2 - 1e-4;

impl Camera {
    pub fn rotation(&self) -> Quat {
        Quat::from_rotation_y(self.yaw) * Quat::from_rotation_x(self.pitch)
    }

    pub fn forward(&self) -> Vec3 {
        self.rotation() * Vec3::NEG_Z
    }

    pub fn right(&self) -> Vec3 {
        self.rotation() * Vec3::X
    }

    pub fn up(&self) -> Vec3 {
        self.rotation() * Vec3::Y
    }

    pub fn position(&self) -> Vec3 {
        self.target - self.forward() * self.distance
    }

    /// World → view rotation, no translation.
    pub fn view_rotation(&self) -> Mat3 {
        self.rotation().conjugate().to_mat3()
    }

    /// Full view matrix (for CPU-side math).
    pub fn view_matrix(&self) -> Mat4 {
        Mat4::from_mat3(self.view_rotation()) * Mat4::from_translation(-self.position())
    }

    pub fn near(&self) -> f64 {
        (self.distance * 0.002).max(1e-4)
    }

    pub fn projection(&self, aspect: f64) -> Mat4 {
        let aspect = aspect.max(1e-3);
        if self.ortho {
            let h = self.distance * (self.fov_y * 0.5).tan();
            Mat4::orthographic_reverse_z(-h * aspect, h * aspect, -h, h, self.near(), self.distance * 200.0)
        } else {
            Mat4::perspective_infinite_reverse_z(self.fov_y, aspect, self.near())
        }
    }

    /// Projection × rotation-only view: what the GPU uses with
    /// camera-relative object matrices.
    pub fn view_proj_relative(&self, aspect: f64) -> Mat4 {
        self.projection(aspect) * Mat4::from_mat3(self.view_rotation())
    }

    pub fn view_proj(&self, aspect: f64) -> Mat4 {
        self.projection(aspect) * self.view_matrix()
    }

    /// World units covered by one pixel at the target distance.
    pub fn units_per_pixel(&self, viewport_h: f64) -> f64 {
        2.0 * self.distance * (self.fov_y * 0.5).tan() / viewport_h.max(1.0)
    }

    /// Rotate around the target by a pointer delta in pixels.
    pub fn orbit(&mut self, dx: f64, dy: f64) {
        self.yaw -= dx * 0.008;
        self.pitch = (self.pitch - dy * 0.008).clamp(-PITCH_LIMIT, PITCH_LIMIT);
    }

    /// Slide the target so the scene follows the pointer.
    pub fn pan(&mut self, dx: f64, dy: f64, viewport_h: f64) {
        let k = self.units_per_pixel(viewport_h);
        self.target = self.target - self.right() * (dx * k) + self.up() * (dy * k);
    }

    /// Positive steps zoom in.
    pub fn zoom(&mut self, steps: f64) {
        self.distance = (self.distance * 0.85f64.powf(steps)).clamp(1e-3, 1e7);
    }

    /// Fit `bounds` in view.
    pub fn frame(&mut self, bounds: &Aabb) {
        if bounds.is_empty() {
            return;
        }
        self.target = bounds.center();
        let radius = bounds.half_extents().length().max(1e-3);
        self.distance = radius / (self.fov_y * 0.5).sin() * 1.15;
    }

    pub fn set_view(&mut self, preset: ViewPreset) {
        let (yaw, pitch) = match preset {
            ViewPreset::Front => (0.0, 0.0),
            ViewPreset::Back => (core::f64::consts::PI, 0.0),
            ViewPreset::Right => (core::f64::consts::FRAC_PI_2, 0.0),
            ViewPreset::Left => (-core::f64::consts::FRAC_PI_2, 0.0),
            ViewPreset::Top => (0.0, -PITCH_LIMIT),
            ViewPreset::Bottom => (0.0, PITCH_LIMIT),
        };
        self.yaw = yaw;
        self.pitch = pitch;
    }

    /// World-space ray through pixel `p` of viewport `rect`.
    pub fn ray_from_screen(&self, p: Vec2, rect: Rect) -> Ray {
        let ndc = Vec2::new((p.x - rect.min.x) / rect.width() * 2.0 - 1.0, 1.0 - (p.y - rect.min.y) / rect.height() * 2.0);
        let inv = self.view_proj(rect.width() / rect.height()).inverse().unwrap_or(Mat4::IDENTITY);
        let near = inv.project_point(Vec3::new(ndc.x, ndc.y, 1.0));
        let far = inv.project_point(Vec3::new(ndc.x, ndc.y, 0.5));
        let dir = (far - near).normalize_or(self.forward());
        let origin = if self.ortho { near } else { self.position() };
        Ray::new(origin, dir)
    }

    /// Pixel position of a world point, or `None` behind the camera.
    pub fn project(&self, world: Vec3, rect: Rect) -> Option<Vec2> {
        let clip = self.view_proj(rect.width() / rect.height()) * world.extend(1.0);
        if clip.w <= 1e-9 {
            return None;
        }
        let ndc = Vec2::new(clip.x / clip.w, clip.y / clip.w);
        Some(Vec2::new(rect.min.x + (ndc.x + 1.0) * 0.5 * rect.width(), rect.min.y + (1.0 - ndc.y) * 0.5 * rect.height()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use prism_math::EPS;

    fn rect() -> Rect {
        Rect::from_xywh(100.0, 50.0, 800.0, 600.0)
    }

    #[test]
    fn default_looks_at_the_target_from_above_the_xz_octant() {
        let c = Camera::default();
        let p = c.position();
        assert!(p.x > 0.0 && p.y > 0.0 && p.z > 0.0, "{p:?}");
        let v = c.view_matrix().transform_point(c.target);
        assert!(v.approx_eq(Vec3::new(0.0, 0.0, -c.distance), 1e-9), "{v:?}");
        assert!(c.view_matrix().transform_point(p).approx_eq(Vec3::ZERO, 1e-9));
        assert!(c.forward().dot(c.target - p) > 0.0);
        assert!(c.up().y > 0.0);
    }

    #[test]
    fn presets() {
        let mut c = Camera::default();
        c.set_view(ViewPreset::Front);
        assert!(c.forward().approx_eq(Vec3::NEG_Z, EPS));
        c.set_view(ViewPreset::Right);
        assert!(c.forward().approx_eq(Vec3::NEG_X, EPS));
        c.set_view(ViewPreset::Top);
        assert!(c.forward().approx_eq(Vec3::NEG_Y, 1e-3));
        assert!(c.up().length() > 0.99);
        c.set_view(ViewPreset::Back);
        assert!(c.forward().approx_eq(Vec3::Z, EPS));
    }

    #[test]
    fn rays_and_projection_agree() {
        let c = Camera::default();
        let r = rect();
        let centre = c.project(c.target, r).unwrap();
        assert!(centre.approx_eq(r.center(), 1e-6), "{centre:?}");
        let ray = c.ray_from_screen(r.center(), r);
        assert!(ray.origin.approx_eq(c.position(), 1e-9));
        assert!(ray.dir.approx_eq(c.forward(), 1e-9));
        // A point off-centre projects, and its ray passes back through it.
        let world = c.target + c.right() * 2.0 + c.up() * 1.0;
        let px = c.project(world, r).unwrap();
        assert!(px.x > r.center().x && px.y < r.center().y);
        let ray = c.ray_from_screen(px, r);
        let t = ray.closest_t(world);
        assert!(ray.at(t).approx_eq(world, 1e-6));
        // Behind the camera: no projection.
        assert!(c.project(c.position() - c.forward() * 5.0, r).is_none());
    }

    #[test]
    fn ortho_rays_are_parallel() {
        let c = Camera { ortho: true, ..Camera::default() };
        let r = rect();
        let a = c.ray_from_screen(r.min, r);
        let b = c.ray_from_screen(r.max, r);
        assert!(a.dir.approx_eq(b.dir, 1e-9));
        assert!(!a.origin.approx_eq(b.origin, 1e-6));
    }

    #[test]
    fn navigation() {
        let mut c = Camera::default();
        let d0 = c.distance;
        c.zoom(1.0);
        assert!(c.distance < d0);
        c.zoom(-1.0);
        assert!((c.distance - d0).abs() < 1e-9);
        let t0 = c.target;
        c.pan(100.0, 0.0, 600.0);
        assert!(c.target.distance(t0) > 0.0);
        c.orbit(0.0, -10000.0);
        assert!(c.pitch <= PITCH_LIMIT + 1e-12, "pitch clamps");
        c.frame(&Aabb::new(Vec3::splat(-1.0), Vec3::splat(1.0)));
        assert!(c.target.approx_eq(Vec3::ZERO, EPS));
        let r = rect();
        for corner in Aabb::new(Vec3::splat(-1.0), Vec3::splat(1.0)).corners() {
            let p = c.project(corner, r).unwrap();
            assert!(r.contains(p), "corner {corner:?} at {p:?} outside {r:?}");
        }
    }
}
