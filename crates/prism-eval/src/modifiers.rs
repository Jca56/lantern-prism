//! Modifier evaluation (Phase 8, D029): a pure function of the base mesh and
//! its stack, producing the mesh that is drawn plus a map from every result
//! face back to the base face it came from, so selection follows the surface.

use std::collections::HashMap;

use prism_doc::{MirrorProps, Modifier, SubsurfProps};
use prism_math::Vec3;
use prism_mesh::tables::F_SMOOTH;
use prism_mesh::{EdgeH, FaceH, LoopH, Mesh, VertH};

/// The mesh after its modifiers. `face_origin[f.idx()]` is the base face a
/// result face descends from (`None` only for faces no base face owns).
pub struct EvalMesh {
    pub mesh: Mesh,
    pub face_origin: Vec<Option<FaceH>>,
}

impl EvalMesh {
    pub fn origin(&self, f: FaceH) -> Option<FaceH> {
        self.face_origin.get(f.idx()).copied().flatten()
    }
}

/// Run `mods` over `base`, in order.
pub fn apply_modifiers(base: &Mesh, mods: &[Modifier]) -> EvalMesh {
    let mut origin = Vec::new();
    for f in base.faces() {
        set_origin(&mut origin, f, Some(f));
    }
    let mut stage = EvalMesh { mesh: base.clone(), face_origin: origin };
    for m in mods {
        stage = match m {
            Modifier::Mirror(p) => mirror(&stage, p),
            Modifier::Subsurf(p) => subsurf(&stage, p),
        };
    }
    stage
}

fn set_origin(origin: &mut Vec<Option<FaceH>>, f: FaceH, o: Option<FaceH>) {
    if origin.len() <= f.idx() {
        origin.resize(f.idx() + 1, None);
    }
    origin[f.idx()] = o;
}

/// Make any missing edges, then the face; `None` if the kernel refuses
/// (repeated vertices, fewer than three).
fn try_add_face(mesh: &mut Mesh, verts: &[VertH]) -> Option<FaceH> {
    let n = verts.len();
    if n < 3 || (0..n).any(|i| verts[i + 1..].contains(&verts[i])) {
        return None;
    }
    for i in 0..n {
        let (a, b) = (verts[i], verts[(i + 1) % n]);
        if mesh.edge_between(a, b).is_none() {
            mesh.make_edge(a, b).ok()?;
        }
    }
    mesh.make_face(verts).ok()
}

/// Add a face that descends from `o`, carrying the smooth flag.
fn push_face(out: &mut EvalMesh, verts: &[VertH], o: Option<FaceH>, smooth: bool) {
    if let Some(f) = try_add_face(&mut out.mesh, verts) {
        out.mesh.face_attrs_mut().bools_mut(F_SMOOTH).set(f.idx(), smooth);
        set_origin(&mut out.face_origin, f, o);
    }
}

// ---- mirror ---------------------------------------------------------------

fn mirror(stage: &EvalMesh, p: &MirrorProps) -> EvalMesh {
    let mut cur = EvalMesh { mesh: stage.mesh.clone(), face_origin: stage.face_origin.clone() };
    for (axis, on) in [(0, p.x), (1, p.y), (2, p.z)] {
        if on {
            cur = mirror_axis(&cur, axis, p.merge, p.merge_distance.max(0.0));
        }
    }
    cur
}

/// Reflect across the plane `coord[axis] = 0`. Vertices within `dist` of the
/// plane are shared by both halves when `merge` is on; faces lying in the
/// plane are not duplicated.
fn mirror_axis(stage: &EvalMesh, axis: usize, merge: bool, dist: f64) -> EvalMesh {
    let src = &stage.mesh;
    let mut out = EvalMesh { mesh: Mesh::new(), face_origin: Vec::new() };
    let mut map: HashMap<VertH, (VertH, VertH)> = HashMap::with_capacity(src.vert_count());
    for v in src.verts() {
        let p = src.position(v);
        let a = out.mesh.make_vert(p);
        let b = if merge && p[axis].abs() <= dist {
            a
        } else {
            let mut q = p;
            q[axis] = -q[axis];
            out.mesh.make_vert(q)
        };
        map.insert(v, (a, b));
    }
    let smooth = src.face_attrs().bools(F_SMOOTH);
    for f in src.faces() {
        let verts: Vec<VertH> = src.verts_of_face(f).collect();
        let o = stage.origin(f);
        let same: Vec<VertH> = verts.iter().map(|v| map[v].0).collect();
        push_face(&mut out, &same, o, smooth[f.idx()]);
        // Reversed so the reflection keeps facing outward.
        let reflected: Vec<VertH> = verts.iter().rev().map(|v| map[v].1).collect();
        if reflected.iter().all(|v| same.contains(v)) {
            continue;
        }
        push_face(&mut out, &reflected, o, smooth[f.idx()]);
    }
    out
}

// ---- subdivision surface --------------------------------------------------

fn subsurf(stage: &EvalMesh, p: &SubsurfProps) -> EvalMesh {
    let mut cur = EvalMesh { mesh: stage.mesh.clone(), face_origin: stage.face_origin.clone() };
    for _ in 0..p.levels.clamp(0, 5) {
        cur = catmull_clark(&cur, p.smooth);
    }
    cur
}

fn centroid(m: &Mesh, f: FaceH) -> Vec3 {
    let pts = m.face_positions(f);
    pts.iter().fold(Vec3::ZERO, |s, p| s + *p) / pts.len().max(1) as f64
}

/// One level of Catmull-Clark on any polygon mesh: every face becomes one
/// quad per corner. Boundary edges and vertices use the crease rules so open
/// meshes keep their outline; `smooth` off leaves every point where it was.
fn catmull_clark(stage: &EvalMesh, smooth: bool) -> EvalMesh {
    let m = &stage.mesh;
    let face_pt: HashMap<FaceH, Vec3> = m.faces().map(|f| (f, centroid(m, f))).collect();
    let mut edge_pt: HashMap<EdgeH, Vec3> = HashMap::with_capacity(m.edge_count());
    for e in m.edges() {
        let [a, b] = m.edge_verts(e);
        let (pa, pb) = (m.position(a), m.position(b));
        let faces: Vec<FaceH> = m.faces_of_edge(e).collect();
        let pt = if smooth && faces.len() == 2 { (pa + pb + face_pt[&faces[0]] + face_pt[&faces[1]]) * 0.25 } else { (pa + pb) * 0.5 };
        edge_pt.insert(e, pt);
    }
    let mut vert_pt: HashMap<VertH, Vec3> = HashMap::with_capacity(m.vert_count());
    for v in m.verts() {
        let p = m.position(v);
        let edges: Vec<EdgeH> = m.edges_of(v).collect();
        let n = edges.len();
        let boundary: Vec<EdgeH> = edges.iter().copied().filter(|&e| m.edge_face_count(e) < 2).collect();
        let moved = if !smooth || n < 2 {
            p
        } else if boundary.is_empty() && n >= 3 {
            let faces = m.faces_of_vert(v);
            let q = faces.iter().fold(Vec3::ZERO, |s, f| s + face_pt[f]) / faces.len().max(1) as f64;
            let r = edges.iter().fold(Vec3::ZERO, |s, &e| s + (p + m.position(m.other_vert(e, v))) * 0.5) / n as f64;
            (q + r * 2.0 + p * (n as f64 - 3.0)) / n as f64
        } else if boundary.len() == 2 {
            let a = m.position(m.other_vert(boundary[0], v));
            let b = m.position(m.other_vert(boundary[1], v));
            (p * 6.0 + a + b) / 8.0
        } else {
            p
        };
        vert_pt.insert(v, moved);
    }

    let mut out = EvalMesh { mesh: Mesh::new(), face_origin: Vec::new() };
    let mut nv: HashMap<VertH, VertH> = HashMap::with_capacity(m.vert_count());
    let mut ne: HashMap<EdgeH, VertH> = HashMap::with_capacity(m.edge_count());
    let smooth_attr = m.face_attrs().bools(F_SMOOTH);
    for f in m.faces() {
        let loops: Vec<LoopH> = m.loops_of_face(f).collect();
        let k = loops.len();
        let fv = out.mesh.make_vert(face_pt[&f]);
        let o = stage.origin(f);
        for i in 0..k {
            let l = loops[i];
            let v = m.loop_vert(l);
            let e_next = m.loop_edge(l);
            let e_prev = m.loop_edge(loops[(i + k - 1) % k]);
            let vv = *nv.entry(v).or_insert_with(|| out.mesh.make_vert(vert_pt[&v]));
            let en = *ne.entry(e_next).or_insert_with(|| out.mesh.make_vert(edge_pt[&e_next]));
            let ep = *ne.entry(e_prev).or_insert_with(|| out.mesh.make_vert(edge_pt[&e_prev]));
            push_face(&mut out, &[vv, en, fv, ep], o, smooth_attr[f.idx()]);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use prism_mesh::primitives::cube;

    fn counts(m: &Mesh) -> (usize, usize, usize) {
        (m.vert_count(), m.edge_count(), m.face_count())
    }

    #[test]
    fn subsurf_cube_levels() {
        let base = cube(2.0);
        let one = apply_modifiers(&base, &[Modifier::Subsurf(SubsurfProps { levels: 1, smooth: true })]);
        assert_eq!(counts(&one.mesh), (26, 48, 24), "8 corners + 12 edges + 6 faces; every face becomes 4 quads");
        one.mesh.validate().unwrap();
        // Smoothed: corners pull in (no vertex is still at a cube corner) and
        // nothing leaves the cube; face points stay on the faces, at 1.
        let corner_left = one.mesh.verts().any(|v| one.mesh.position(v).abs().min_element() > 1.0 - 1e-6);
        assert!(!corner_left, "every original corner moved inward");
        let max = one.mesh.verts().map(|v| one.mesh.position(v).abs().max_element()).fold(0.0, f64::max);
        assert!((max - 1.0).abs() < 1e-9, "face points sit on the original faces: {max}");
        // Every result face knows its base face; each base face has 4 children.
        let mut per_base = HashMap::new();
        for f in one.mesh.faces() {
            *per_base.entry(one.origin(f).unwrap()).or_insert(0) += 1;
        }
        assert_eq!(per_base.len(), 6);
        assert!(per_base.values().all(|&n| n == 4));

        let two = apply_modifiers(&base, &[Modifier::Subsurf(SubsurfProps { levels: 2, smooth: true })]);
        assert_eq!(counts(&two.mesh), (98, 192, 96));
        two.mesh.validate().unwrap();

        let simple = apply_modifiers(&base, &[Modifier::Subsurf(SubsurfProps { levels: 1, smooth: false })]);
        let corners = simple.mesh.verts().filter(|&v| simple.mesh.position(v).abs().min_element() > 1.0 - 1e-9).count();
        assert_eq!(corners, 8, "simple subdivision keeps the corners");
    }

    #[test]
    fn mirror_welds_the_seam_and_keeps_normals_outward() {
        // Half a square: x from 0 to 1, in the XZ plane facing +Y.
        let mut half = Mesh::new();
        let v: Vec<VertH> = [(0.0, 0.0, 0.0), (0.0, 0.0, 1.0), (1.0, 0.0, 1.0), (1.0, 0.0, 0.0)].iter().map(|&(x, y, z)| half.make_vert(Vec3::new(x, y, z))).collect();
        half.add_face(&v);
        assert!(half.face_normal(half.faces().next().unwrap()).y > 0.0);
        let full = apply_modifiers(&half, &[Modifier::Mirror(MirrorProps::default())]);
        assert_eq!(counts(&full.mesh), (6, 7, 2), "two verts on the plane are shared");
        full.mesh.validate().unwrap();
        assert!(full.mesh.faces().all(|f| full.mesh.face_normal(f).y > 0.0), "the reflection faces the same way");
        assert_eq!(full.mesh.edges().filter(|&e| full.mesh.edge_face_count(e) == 2).count(), 1, "one interior seam edge");
        let xs: Vec<f64> = full.mesh.verts().map(|v| full.mesh.position(v).x).collect();
        assert!(xs.iter().any(|&x| (x + 1.0).abs() < 1e-9));

        let unmerged = apply_modifiers(&half, &[Modifier::Mirror(MirrorProps { merge: false, ..MirrorProps::default() })]);
        assert_eq!(counts(&unmerged.mesh), (8, 8, 2));

        // Mirror then subdivide: the stack composes and origins survive.
        let both = apply_modifiers(&half, &[Modifier::Mirror(MirrorProps::default()), Modifier::Subsurf(SubsurfProps::default())]);
        assert_eq!(both.mesh.face_count(), 8);
        let base_face = half.faces().next().unwrap();
        assert!(both.mesh.faces().all(|f| both.origin(f) == Some(base_face)));
    }
}
