//! Primitive meshes, built with euler operators only. All are centred on the
//! origin with **outward** normals (counter-clockwise seen from outside).

use prism_math::{TAU, Vec3};

use crate::handle::{FaceH, VertH};
use crate::mesh::Mesh;

impl Mesh {
    /// Make any missing edges, then the face. The primitives' convenience.
    pub fn add_face(&mut self, verts: &[VertH]) -> FaceH {
        let n = verts.len();
        for i in 0..n {
            let (a, b) = (verts[i], verts[(i + 1) % n]);
            if self.edge_between(a, b).is_none() {
                self.make_edge(a, b).expect("distinct live vertices");
            }
        }
        self.make_face(verts).expect("closed ring of edges")
    }
}

/// A square in the XZ plane facing +Y.
pub fn plane(size: f64) -> Mesh {
    grid(size, size, 1, 1)
}

/// `nx × nz` quads in the XZ plane facing +Y.
pub fn grid(size_x: f64, size_z: f64, nx: usize, nz: usize) -> Mesh {
    let (nx, nz) = (nx.max(1), nz.max(1));
    let mut m = Mesh::new();
    let mut verts = Vec::with_capacity((nx + 1) * (nz + 1));
    for j in 0..=nz {
        for i in 0..=nx {
            let x = (i as f64 / nx as f64 - 0.5) * size_x;
            let z = (j as f64 / nz as f64 - 0.5) * size_z;
            verts.push(m.make_vert(Vec3::new(x, 0.0, z)));
        }
    }
    let at = |i: usize, j: usize| verts[j * (nx + 1) + i];
    for j in 0..nz {
        for i in 0..nx {
            // +Z first, then +X: counter-clockwise seen from +Y.
            m.add_face(&[at(i, j), at(i, j + 1), at(i + 1, j + 1), at(i + 1, j)]);
        }
    }
    m
}

/// A cube of edge length `size`.
pub fn cube(size: f64) -> Mesh {
    let h = size * 0.5;
    let mut m = Mesh::new();
    let p = |x: f64, y: f64, z: f64| Vec3::new(x * h, y * h, z * h);
    // Bit 0 = +x, bit 1 = +y, bit 2 = +z.
    let v: Vec<VertH> = (0..8)
        .map(|i| {
            let sx = if i & 1 == 0 { -1.0 } else { 1.0 };
            let sy = if i & 2 == 0 { -1.0 } else { 1.0 };
            let sz = if i & 4 == 0 { -1.0 } else { 1.0 };
            m.make_vert(p(sx, sy, sz))
        })
        .collect();
    let faces: [[usize; 4]; 6] = [
        [0, 2, 3, 1], // -z (back)
        [4, 5, 7, 6], // +z (front)
        [0, 4, 6, 2], // -x
        [1, 3, 7, 5], // +x
        [0, 1, 5, 4], // -y (bottom)
        [2, 6, 7, 3], // +y (top)
    ];
    for f in faces {
        m.add_face(&[v[f[0]], v[f[1]], v[f[2]], v[f[3]]]);
    }
    m
}

/// UV sphere: `segments` around, `rings` from pole to pole.
pub fn uv_sphere(radius: f64, segments: usize, rings: usize) -> Mesh {
    let (segments, rings) = (segments.max(3), rings.max(2));
    let mut m = Mesh::new();
    let top = m.make_vert(Vec3::new(0.0, radius, 0.0));
    let mut ring_verts: Vec<Vec<VertH>> = Vec::new();
    for r in 1..rings {
        let phi = r as f64 / rings as f64 * core::f64::consts::PI;
        let y = phi.cos() * radius;
        let rr = phi.sin() * radius;
        ring_verts.push(
            (0..segments)
                .map(|s| {
                    let theta = s as f64 / segments as f64 * TAU;
                    m.make_vert(Vec3::new(rr * theta.cos(), y, -rr * theta.sin()))
                })
                .collect(),
        );
    }
    let bottom = m.make_vert(Vec3::new(0.0, -radius, 0.0));
    let first = &ring_verts[0];
    for s in 0..segments {
        m.add_face(&[top, first[s], first[(s + 1) % segments]]);
    }
    for r in 0..ring_verts.len() - 1 {
        let (a, b) = (&ring_verts[r], &ring_verts[r + 1]);
        for s in 0..segments {
            let s1 = (s + 1) % segments;
            m.add_face(&[a[s], b[s], b[s1], a[s1]]);
        }
    }
    let last = ring_verts.last().expect("at least one ring");
    for s in 0..segments {
        m.add_face(&[bottom, last[(s + 1) % segments], last[s]]);
    }
    m
}

/// Cylinder along Y with optional n-gon caps.
pub fn cylinder(radius: f64, height: f64, segments: usize, caps: bool) -> Mesh {
    let segments = segments.max(3);
    let mut m = Mesh::new();
    let h = height * 0.5;
    let ring = |m: &mut Mesh, y: f64| -> Vec<VertH> {
        (0..segments)
            .map(|s| {
                let theta = s as f64 / segments as f64 * TAU;
                m.make_vert(Vec3::new(radius * theta.cos(), y, -radius * theta.sin()))
            })
            .collect()
    };
    let bottom = ring(&mut m, -h);
    let top = ring(&mut m, h);
    for s in 0..segments {
        let s1 = (s + 1) % segments;
        m.add_face(&[bottom[s], bottom[s1], top[s1], top[s]]);
    }
    if caps {
        m.add_face(&top);
        let mut rev = bottom.clone();
        rev.reverse();
        m.add_face(&rev);
    }
    m
}

/// A circle in the XZ plane: a wire loop, or one n-gon facing +Y.
pub fn circle(radius: f64, segments: usize, fill: bool) -> Mesh {
    let segments = segments.max(3);
    let mut m = Mesh::new();
    let verts: Vec<VertH> = (0..segments)
        .map(|s| {
            let theta = s as f64 / segments as f64 * TAU;
            m.make_vert(Vec3::new(radius * theta.cos(), 0.0, -radius * theta.sin()))
        })
        .collect();
    if fill {
        m.add_face(&verts);
    } else {
        for s in 0..segments {
            m.make_edge(verts[s], verts[(s + 1) % segments]).expect("ring");
        }
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    fn counts(m: &Mesh) -> (usize, usize, usize, usize) {
        (m.vert_count(), m.edge_count(), m.face_count(), m.loop_count())
    }

    /// Every face of a closed, origin-centred convex solid faces away from the origin.
    fn assert_outward(m: &Mesh) {
        for f in m.faces() {
            let n = m.face_normal(f);
            let c = m.face_center(f);
            assert!(n.dot(c) > 0.0, "face {f} points inward: n={n:?} c={c:?}");
        }
    }

    #[test]
    fn plane_and_grid() {
        let p = plane(2.0);
        p.validate().unwrap();
        assert_eq!(counts(&p), (4, 4, 1, 4));
        let f = p.faces().next().unwrap();
        assert!(p.face_normal(f).approx_eq(Vec3::Y, 1e-12));
        let g = grid(4.0, 2.0, 4, 2);
        g.validate().unwrap();
        assert_eq!(counts(&g), (15, 22, 8, 32));
        assert!(g.faces().all(|f| g.face_normal(f).approx_eq(Vec3::Y, 1e-12)));
        assert_eq!(g.edges().filter(|&e| g.is_boundary_edge(e)).count(), 12);
    }

    #[test]
    fn cube_is_a_cube() {
        let c = cube(2.0);
        c.validate().unwrap();
        assert_eq!(counts(&c), (8, 12, 6, 24));
        assert_outward(&c);
        assert!(c.edges().all(|e| c.is_manifold_edge(e)));
        assert!(c.verts().all(|v| c.vert_edge_count(v) == 3));
        let mut normals: Vec<Vec3> = c.faces().map(|f| c.face_normal(f)).collect();
        for axis in [Vec3::X, Vec3::Y, Vec3::Z, Vec3::NEG_X, Vec3::NEG_Y, Vec3::NEG_Z] {
            let i = normals.iter().position(|n| n.approx_eq(axis, 1e-12)).unwrap_or_else(|| panic!("no face facing {axis:?}"));
            normals.remove(i);
        }
    }

    #[test]
    fn sphere_and_cylinder() {
        let s = uv_sphere(1.0, 8, 4);
        s.validate().unwrap();
        // 2 poles + 3 rings of 8. Edges: 3 rings × 8 + 2 gaps × 8 + 16 pole spokes.
        assert_eq!(counts(&s), (26, 24 + 16 + 16, 32, 8 * 3 + 16 * 4 + 8 * 3));
        assert_outward(&s);
        assert!(s.edges().all(|e| s.is_manifold_edge(e)));
        let c = cylinder(1.0, 2.0, 6, true);
        c.validate().unwrap();
        assert_eq!(counts(&c), (12, 18, 8, 24 + 12));
        assert_outward(&c);
        assert!(c.edges().all(|e| c.is_manifold_edge(e)));
        let open = cylinder(1.0, 2.0, 6, false);
        open.validate().unwrap();
        assert_eq!(open.face_count(), 6);
        assert_eq!(open.edges().filter(|&e| open.is_boundary_edge(e)).count(), 12);
    }

    #[test]
    fn circles() {
        let wire = circle(1.0, 12, false);
        wire.validate().unwrap();
        assert_eq!(counts(&wire), (12, 12, 0, 0));
        assert!(wire.edges().all(|e| wire.is_wire_edge(e)));
        let disc = circle(1.0, 12, true);
        disc.validate().unwrap();
        assert_eq!(counts(&disc), (12, 12, 1, 12));
        assert!(disc.face_normal(disc.faces().next().unwrap()).approx_eq(Vec3::Y, 1e-12));
    }

    #[test]
    fn primitives_survive_paranoid_teardown() {
        let mut c = cube(1.0);
        c.paranoid = true;
        let faces: Vec<FaceH> = c.faces().collect();
        for f in faces {
            c.kill_face(f).unwrap();
        }
        let edges: Vec<_> = c.edges().collect();
        for e in edges {
            c.kill_edge(e).unwrap();
        }
        let verts: Vec<_> = c.verts().collect();
        for v in verts {
            c.kill_vert(v).unwrap();
        }
        assert!(c.is_empty());
        c.validate().unwrap();
    }
}
