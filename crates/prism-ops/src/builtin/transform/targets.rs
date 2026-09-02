//! What a transform acts on and how it is written back: the selected
//! objects, or the selected vertices of the edit mesh, always in world space
//! from the values they had when the transform started.

use prism_core::Id;
use prism_doc::{Doc, ObjectMode};
use prism_math::{Mat4, Quat, Vec3};
use prism_mesh::VertH;

use crate::builtin::select;

/// The things a transform acts on, with every value as it was at the start,
/// so each update is computed fresh rather than accumulated.
pub(super) enum Targets {
    /// `(id, location, rotation, scale)` per selected object.
    Objects(Vec<(Id, Vec3, Vec3, Vec3)>),
    /// Selected vertices of the edit mesh, positions in mesh space.
    Verts { mesh: Id, space: Box<Space>, verts: Vec<(VertH, Vec3)> },
}

/// Mesh space ↔ world space for the object being edited.
pub(super) struct Space {
    to_world: Mat4,
    to_local: Mat4,
}

fn editing(doc: &Doc) -> Option<Id> {
    let id = doc.active_object_id();
    let o = doc.objects.get(id)?;
    (o.mode == ObjectMode::Edit && doc.object_mesh(id).is_some()).then_some(id)
}

/// Cheap enough for `poll`.
pub(super) fn has_targets(doc: &Doc) -> bool {
    match editing(doc) {
        Some(id) => doc.object_mesh(id).is_some_and(|b| select::any_selected(&b.mesh)),
        None => !doc.selected_objects().is_empty(),
    }
}

impl Targets {
    pub(super) fn gather(doc: &Doc) -> Option<Targets> {
        if let Some(id) = editing(doc) {
            let block = doc.object_mesh(id)?;
            let verts: Vec<(VertH, Vec3)> = select::selected_verts(&block.mesh).into_iter().map(|v| (v, block.mesh.position(v))).collect();
            if verts.is_empty() {
                return None;
            }
            let to_world = doc.object_matrix(id);
            let to_local = to_world.inverse().unwrap_or(Mat4::IDENTITY);
            let mesh = doc.objects.get(id)?.data;
            return Some(Targets::Verts { mesh, space: Box::new(Space { to_world, to_local }), verts });
        }
        let items: Vec<_> =
            doc.selected_objects().into_iter().filter_map(|id| doc.objects.get(id).map(|o| (id, o.location, o.rotation, o.scale))).collect();
        (!items.is_empty()).then_some(Targets::Objects(items))
    }

    /// Mean of the targets, in world space.
    pub(super) fn pivot(&self) -> Vec3 {
        let (sum, n) = match self {
            Targets::Objects(items) => (items.iter().fold(Vec3::ZERO, |s, i| s + i.1), items.len()),
            Targets::Verts { space, verts, .. } => (verts.iter().fold(Vec3::ZERO, |s, (_, p)| s + space.to_world.transform_point(*p)), verts.len()),
        };
        if n == 0 { Vec3::ZERO } else { sum / n as f64 }
    }
}

/// A transform in world space about a pivot.
#[derive(Clone, Copy, Debug)]
pub(super) enum Op {
    Translate(Vec3),
    Rotate { axis: Vec3, angle: f64 },
    Scale(Vec3),
}

impl Op {
    fn point(self, pivot: Vec3, p: Vec3) -> Vec3 {
        match self {
            Op::Translate(d) => p + d,
            Op::Rotate { axis, angle } => pivot + Quat::from_axis_angle(axis, angle) * (p - pivot),
            Op::Scale(f) => pivot + (p - pivot) * f,
        }
    }
}

/// Write `op` onto the targets from their starting values.
pub(super) fn apply(doc: &mut Doc, targets: &Targets, pivot: Vec3, op: Op) {
    match targets {
        Targets::Objects(items) => {
            for &(id, loc0, rot0, scale0) in items {
                let Some(o) = doc.objects.get_mut(id) else {
                    continue;
                };
                o.location = op.point(pivot, loc0);
                match op {
                    Op::Rotate { axis, angle } => {
                        let q = Quat::from_axis_angle(axis, angle) * Quat::from_euler_xyz(rot0.x, rot0.y, rot0.z);
                        let (x, y, z) = q.to_euler_xyz();
                        o.rotation = Vec3::new(x, y, z);
                    }
                    Op::Scale(f) => o.scale = scale0 * f,
                    Op::Translate(_) => {}
                }
            }
        }
        Targets::Verts { mesh, space, verts } => {
            if let Some(block) = doc.meshes.get_mut(*mesh) {
                for &(v, p0) in verts {
                    let w = op.point(pivot, space.to_world.transform_point(p0));
                    block.mesh.set_position(v, space.to_local.transform_point(w));
                }
            }
        }
    }
}
