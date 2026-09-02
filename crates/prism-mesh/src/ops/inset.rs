//! Inset: the selected region shrinks inward behind a rim of quads along its
//! boundary (Phase 7). A region insets as one; `individual` insets each face
//! by itself — and a closed shell, having no boundary to inset from, always
//! does. Built on the region extrude: extrude in place, then slide the rim
//! vertices inward along the bisector of their two rim edges, scaled so the
//! rim stays an even width round corners.

use std::collections::{HashMap, HashSet};

use prism_math::Vec3;

use crate::euler::EulerResult;
use crate::handle::{FaceH, VertH};
use crate::mesh::Mesh;

/// A vertex of the inset region: where it started, the direction it moves per
/// unit of thickness (already scaled for its corner; zero for interior
/// vertices), and its normal for depth.
#[derive(Clone, Copy, Debug)]
pub struct InsetVert {
    pub vert: VertH,
    pub base: Vec3,
    pub inward: Vec3,
    pub normal: Vec3,
}

#[derive(Clone, Debug, Default)]
pub struct InsetResult {
    /// The inner faces (replacements for the input faces).
    pub faces: Vec<FaceH>,
    /// The rim quads.
    pub side_faces: Vec<FaceH>,
    /// Every vertex of the inner region; place them with [`Mesh::place_inset`].
    pub verts: Vec<InsetVert>,
}

/// Smallest cosine of a corner half-angle honoured; sharper corners would
/// shoot their vertex off toward infinity.
const MIN_CORNER_COS: f64 = 0.2;

impl Mesh {
    /// Inset `faces` by `thickness`, pushed along their normals by `depth`.
    pub fn inset_faces(&mut self, faces: &[FaceH], thickness: f64, depth: f64, individual: bool) -> EulerResult<InsetResult> {
        let live: Vec<FaceH> = faces.iter().copied().filter(|&f| self.face_live(f)).collect();
        let mut out = InsetResult::default();
        if live.is_empty() {
            return Ok(out);
        }
        if individual || !self.region_has_boundary(&live) {
            for f in live {
                self.inset_region(&[f], &mut out)?;
            }
        } else {
            self.inset_region(&live, &mut out)?;
        }
        self.place_inset(&out.verts, thickness, depth);
        Ok(out)
    }

    /// Does some edge of the region border a face outside it (or nothing)?
    fn region_has_boundary(&self, faces: &[FaceH]) -> bool {
        let region: HashSet<FaceH> = faces.iter().copied().collect();
        faces.iter().any(|&f| self.edges_of_face(f).any(|e| self.edge_face_count(e) < 2 || self.faces_of_edge(e).any(|g| !region.contains(&g))))
    }

    /// Extrude the region in place and work out where its vertices go.
    fn inset_region(&mut self, faces: &[FaceH], out: &mut InsetResult) -> EulerResult<()> {
        let r = self.extrude_faces(faces)?;
        let rim: HashSet<FaceH> = r.side_faces.iter().copied().collect();
        let mut inward: HashMap<VertH, Vec<Vec3>> = HashMap::new();
        let mut normals: HashMap<VertH, Vec3> = HashMap::new();
        for &nf in &r.faces {
            let n = self.face_normal(nf);
            for l in self.loops_of_face(nf).collect::<Vec<_>>() {
                let v = self.loop_vert(l);
                *normals.entry(v).or_insert(Vec3::ZERO) += n;
                let e = self.loop_edge(l);
                if !self.faces_of_edge(e).any(|g| rim.contains(&g)) {
                    continue;
                }
                // A rim edge: the face interior lies to the left of the loop
                // direction, which is normal × direction for a counter-clockwise face.
                let w = self.loop_vert(self.loop_next(l));
                let d = (self.position(w) - self.position(v)).normalize_or_zero();
                let toward_inside = n.cross(d).normalize_or_zero();
                inward.entry(v).or_default().push(toward_inside);
                inward.entry(w).or_default().push(toward_inside);
            }
        }
        for &v in &r.verts {
            let normal = normals.get(&v).copied().unwrap_or(Vec3::ZERO).normalize_or_zero();
            let inward = match inward.get(&v) {
                Some(dirs) => {
                    let sum = dirs.iter().fold(Vec3::ZERO, |s, d| s + *d);
                    match sum.try_normalize() {
                        // Along the corner's bisector, far enough that the rim
                        // is `thickness` wide measured from either edge.
                        Some(dir) => dir / dirs.iter().map(|d| d.dot(dir)).fold(f64::MAX, f64::min).max(MIN_CORNER_COS),
                        None => Vec3::ZERO,
                    }
                }
                None => Vec3::ZERO,
            };
            out.verts.push(InsetVert { vert: v, base: self.position(v), inward, normal });
        }
        out.faces.extend(r.faces);
        out.side_faces.extend(r.side_faces);
        Ok(())
    }

    /// Put the inset region's vertices where `thickness` and `depth` say.
    pub fn place_inset(&mut self, verts: &[InsetVert], thickness: f64, depth: f64) {
        for iv in verts {
            if self.vert_live(iv.vert) {
                self.set_position(iv.vert, iv.base + iv.inward * thickness + iv.normal * depth);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::primitives::cube;

    fn top(m: &Mesh) -> FaceH {
        m.faces().find(|&f| m.face_normal(f).approx_eq(Vec3::Y, 1e-9)).unwrap()
    }

    #[test]
    fn inset_one_face_shrinks_it_evenly_and_grows_a_rim() {
        let mut m = cube(2.0);
        let f = top(&m);
        let r = m.inset_faces(&[f], 0.5, 0.25, false).unwrap();
        assert_eq!((r.faces.len(), r.side_faces.len()), (1, 4));
        assert_eq!(m.face_count(), 10);
        m.validate().unwrap();
        for v in m.verts_of_face(r.faces[0]) {
            let p = m.position(v);
            assert!((p.x.abs() - 0.5).abs() < 1e-9 && (p.z.abs() - 0.5).abs() < 1e-9, "rim is 0.5 wide on both sides of every corner: {p:?}");
            assert!((p.y - 1.25).abs() < 1e-9, "depth pushes along the normal");
        }
        // Re-placing is what the interactive drag does.
        m.place_inset(&r.verts, 0.1, 0.0);
        let p = m.position(r.verts[0].vert);
        assert!((p.x.abs() - 0.9).abs() < 1e-9 && (p.y - 1.0).abs() < 1e-9);
    }

    #[test]
    fn closed_shell_and_individual_inset_every_face_on_its_own() {
        let mut m = cube(2.0);
        let all: Vec<FaceH> = m.faces().collect();
        let r = m.inset_faces(&all, 0.2, 0.0, false).unwrap();
        assert_eq!((r.faces.len(), r.side_faces.len()), (6, 24), "no boundary to inset from, so each face by itself");
        assert_eq!(m.face_count(), 30);
        m.validate().unwrap();
        let mut m2 = cube(2.0);
        let two: Vec<FaceH> = m2.faces().take(2).collect();
        let r2 = m2.inset_faces(&two, 0.2, 0.0, true).unwrap();
        assert_eq!(r2.side_faces.len(), 8);
        m2.validate().unwrap();
    }

    #[test]
    fn region_of_two_faces_insets_as_one() {
        let mut m = cube(2.0);
        let f = top(&m);
        // The top face and one neighbour: their shared edge is not a rim edge.
        let neighbour = m.edges_of_face(f).flat_map(|e| m.faces_of_edge(e).collect::<Vec<_>>()).find(|&g| g != f).unwrap();
        let r = m.inset_faces(&[f, neighbour], 0.25, 0.0, false).unwrap();
        assert_eq!((r.faces.len(), r.side_faces.len()), (2, 6), "six rim edges round an L of two faces");
        m.validate().unwrap();
    }
}
