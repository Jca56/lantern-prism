//! Golden counts and behaviours for the compound operations, on paranoid
//! meshes (every euler op validates).

use prism_math::Vec3;
use prism_mesh::tables::F_SMOOTH;
use prism_mesh::{FaceH, Mesh, primitives};

fn counts(m: &Mesh) -> (usize, usize, usize, usize) {
    (m.vert_count(), m.edge_count(), m.face_count(), m.loop_count())
}

fn paranoid(mut m: Mesh) -> Mesh {
    m.paranoid = true;
    m
}

#[test]
fn extrude_one_cube_face() {
    let mut m = paranoid(primitives::cube(2.0));
    let top = m.faces().find(|&f| m.face_normal(f).approx_eq(Vec3::Y, 1e-12)).unwrap();
    let r = m.extrude_faces(&[top]).unwrap();
    assert_eq!(r.faces.len(), 1);
    assert_eq!(r.side_faces.len(), 4);
    assert_eq!(r.verts.len(), 4);
    m.translate_verts(&r.verts, Vec3::Y);
    m.validate().unwrap();
    // 8 + 4 verts, 12 + 4 rim + 4 vertical edges, 6 - 1 + 1 + 4 faces.
    assert_eq!(counts(&m), (12, 20, 10, 40));
    let nf = r.faces[0];
    assert!(m.face_normal(nf).approx_eq(Vec3::Y, 1e-12));
    assert!(m.face_center(nf).approx_eq(Vec3::new(0.0, 2.0, 0.0), 1e-12));
    assert!(m.edges().all(|e| m.is_manifold_edge(e)), "still a closed solid");
    for &s in &r.side_faces {
        let n = m.face_normal(s);
        let c = m.face_center(s);
        assert!(n.dot(Vec3::new(c.x, 0.0, c.z)) > 0.0, "side quad faces outward");
    }
}

#[test]
fn extrude_region_removes_interior() {
    // A 2×2 grid: extrude all four faces. Interior edges/vert of the old
    // grid disappear; the rim grows 8 side quads.
    let mut m = paranoid(primitives::grid(2.0, 2.0, 2, 2));
    assert_eq!(counts(&m), (9, 12, 4, 16));
    let faces: Vec<FaceH> = m.faces().collect();
    let r = m.extrude_faces(&faces).unwrap();
    m.translate_verts(&r.verts, Vec3::Y);
    m.validate().unwrap();
    assert_eq!(r.faces.len(), 4);
    assert_eq!(r.side_faces.len(), 8);
    // Old ring stays (8 verts, 8 edges), old centre vert and 4 spokes gone.
    // New grid: 9 verts, 12 edges. Plus 8 vertical edges.
    assert_eq!(counts(&m), (17, 28, 12, 48));
    assert_eq!(m.edges().filter(|&e| m.is_boundary_edge(e)).count(), 8, "open bottom");
}

#[test]
fn extrude_single_plane_is_an_open_box() {
    let mut m = paranoid(primitives::plane(2.0));
    let f = m.faces().next().unwrap();
    let r = m.extrude_faces(&[f]).unwrap();
    m.translate_verts(&r.verts, Vec3::Y * 2.0);
    assert_eq!(counts(&m), (8, 12, 5, 20));
    assert_eq!(m.edges().filter(|&e| m.is_boundary_edge(e)).count(), 4);
}

#[test]
fn delete_flavours() {
    let mut m = paranoid(primitives::cube(1.0));
    let f = m.faces().next().unwrap();
    m.delete_faces(&[f], true).unwrap();
    assert_eq!(counts(&m), (8, 12, 5, 20), "only faces: edges stay");
    let mut m = paranoid(primitives::cube(1.0));
    let f = m.faces().next().unwrap();
    m.delete_faces(&[f], false).unwrap();
    assert_eq!(counts(&m), (8, 12, 5, 20), "on a cube every edge is still used");
    let mut m = paranoid(primitives::plane(1.0));
    let f = m.faces().next().unwrap();
    m.delete_faces(&[f], false).unwrap();
    assert!(m.is_empty(), "a lone face takes its edges and verts along");
    let mut m = paranoid(primitives::cube(1.0));
    let v = m.verts().next().unwrap();
    m.delete_verts(&[v]).unwrap();
    assert_eq!(counts(&m), (7, 9, 3, 12));
    let mut m = paranoid(primitives::cube(1.0));
    let e = m.edges().next().unwrap();
    m.delete_edges(&[e], true).unwrap();
    assert_eq!(counts(&m), (8, 11, 4, 16));
}

#[test]
fn dissolve_grid_to_one_face() {
    let mut m = paranoid(primitives::grid(3.0, 3.0, 3, 3));
    let faces: Vec<FaceH> = m.faces().collect();
    let left = m.dissolve_faces(&faces).unwrap();
    assert_eq!(left.len(), 1);
    // Boundary verts with two edges dissolve too: the corners remain.
    assert_eq!(counts(&m), (4, 4, 1, 4));
    assert!(m.face_normal(left[0]).approx_eq(Vec3::Y, 1e-12));
    assert!(m.face_center(left[0]).approx_eq(Vec3::ZERO, 1e-12));
}

#[test]
fn dissolve_vertex_and_edge() {
    let mut m = paranoid(primitives::grid(2.0, 2.0, 2, 2));
    let centre = m.verts().find(|&v| m.position(v).approx_eq(Vec3::ZERO, 1e-12)).unwrap();
    m.dissolve_vert(centre).unwrap();
    assert_eq!(counts(&m), (8, 8, 1, 8), "centre gone, one face of 8 corners");
    let mut m = paranoid(primitives::grid(2.0, 2.0, 2, 1));
    let shared = m.edges().find(|&e| m.is_manifold_edge(e)).unwrap();
    let f = m.dissolve_edge(shared).unwrap();
    assert_eq!(m.face_len(f), 6);
    assert_eq!(counts(&m), (6, 6, 1, 6));
}

#[test]
fn subdivide_and_connect() {
    let mut m = paranoid(primitives::plane(2.0));
    let f = m.faces().next().unwrap();
    let e = m.edges().next().unwrap();
    let [a, b] = m.edge_verts(e);
    let new = m.subdivide_edges(&[e], 3).unwrap();
    assert_eq!(new.len(), 3);
    assert_eq!(m.face_len(f), 7);
    let (pa, pb) = (m.position(a), m.position(b));
    for (k, &v) in new.iter().enumerate() {
        assert!(m.position(v).approx_eq(pa.lerp(pb, (k + 1) as f64 / 4.0), 1e-12));
    }
    // Connect the middle new vertex to the opposite corner.
    let opposite = m.verts_of_face(f).find(|&v| v != a && v != b && !new.contains(&v)).unwrap();
    let (nf, ne) = m.connect_verts(f, new[1], opposite).unwrap();
    assert_eq!(m.edge_face_count(ne), 2);
    assert_eq!(m.face_len(f) + m.face_len(nf), 7 + 2);
}

#[test]
fn collapse_and_merge_by_distance() {
    let mut m = paranoid(primitives::cube(2.0));
    let e = m.edges().next().unwrap();
    let kept = m.collapse_edge(e).unwrap();
    assert!(m.vert_live(kept));
    // Collapsing a cube edge: two quads become triangles.
    assert_eq!(counts(&m), (7, 11, 6, 22));
    assert_eq!(m.faces().filter(|&f| m.face_len(f) == 3).count(), 2);

    // Two grids sharing a seam, welded.
    let mut m = paranoid(primitives::grid(2.0, 1.0, 2, 1));
    let other = primitives::grid(2.0, 1.0, 2, 1);
    let mut map = Vec::new();
    for v in other.verts() {
        map.push((v, m.make_vert(other.position(v) + Vec3::new(0.0, 0.0, 1.0))));
    }
    for f in other.faces() {
        let ring: Vec<_> = other.verts_of_face(f).map(|v| map.iter().find(|(o, _)| *o == v).unwrap().1).collect();
        m.add_face(&ring);
    }
    assert_eq!(counts(&m), (12, 14, 4, 16));
    let removed = m.merge_by_distance(1e-6);
    assert_eq!(removed, 3, "three seam vertices coincide");
    assert_eq!(counts(&m), (9, 12, 4, 16), "a 2×2 grid");
    assert_eq!(m.edges().filter(|&e| m.is_manifold_edge(e)).count(), 4);
}

#[test]
fn recalculate_normals_outside() {
    let mut m = paranoid(primitives::cube(2.0));
    let faces: Vec<FaceH> = m.faces().collect();
    m.flip_faces(&faces[..3]).unwrap();
    assert_eq!(m.faces().filter(|&f| m.face_normal(f).dot(m.face_center(f)) < 0.0).count(), 3);
    let flipped = m.make_normals_consistent(&faces).unwrap();
    assert_eq!(flipped, 3);
    assert!(m.faces().all(|f| m.face_normal(f).dot(m.face_center(f)) > 0.0));
    // A sphere flipped entirely comes back out.
    let mut s = paranoid(primitives::uv_sphere(1.0, 8, 4));
    let all: Vec<FaceH> = s.faces().collect();
    s.flip_faces(&all).unwrap();
    s.make_normals_consistent(&all).unwrap();
    assert!(s.faces().all(|f| s.face_normal(f).dot(s.face_center(f)) > 0.0));
}

#[test]
fn attributes_follow_topology() {
    let mut m = paranoid(primitives::cube(2.0));
    let f = m.faces().next().unwrap();
    m.face_attrs_mut().bools_mut(F_SMOOTH).set(f.idx(), true);
    let r = m.extrude_faces(&[f]).unwrap();
    assert!(m.face_attrs().bools(F_SMOOTH)[r.faces[0].idx()], "smooth flag rides along");
    let e = m.edges().next().unwrap();
    let crease = m.edge_attrs().index(prism_mesh::names::CREASE).unwrap();
    m.edge_attrs_mut().f64s_mut(crease).set(e.idx(), 1.0);
    let [a, _] = m.edge_verts(e);
    let (_, ne) = m.split_edge_make_vert(e, a).unwrap();
    assert_eq!(m.edge_attrs().f64s(crease)[ne.idx()], 1.0, "split edge inherits crease");
}
