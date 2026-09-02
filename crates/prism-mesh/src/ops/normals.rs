//! Winding tools: flip, and make a region's winding consistent and outward.

use std::collections::VecDeque;

use prism_math::Vec3;

use crate::euler::EulerResult;
use crate::handle::FaceH;
use crate::mesh::Mesh;

impl Mesh {
    pub fn flip_faces(&mut self, faces: &[FaceH]) -> EulerResult<()> {
        for &f in faces {
            if self.face_live(f) {
                self.reverse_face(f)?;
            }
        }
        Ok(())
    }

    /// Do `f` and `g` run their shared manifold edge `e` in opposite
    /// directions (the consistent case)?
    fn consistent_across(&self, e: crate::handle::EdgeH, f: FaceH, g: FaceH) -> bool {
        let lf = self.face_loop_on(f, e);
        let lg = self.face_loop_on(g, e);
        match (lf, lg) {
            (Some(lf), Some(lg)) => self.loop_vert(lf) != self.loop_vert(lg),
            _ => true,
        }
    }

    /// Flip faces so each connected component winds consistently, with the
    /// component's seed face pointing away from the component's centroid
    /// ("recalculate normals outside"). Returns how many faces flipped.
    pub fn make_normals_consistent(&mut self, faces: &[FaceH]) -> EulerResult<usize> {
        let mut pending: Vec<FaceH> = faces.iter().copied().filter(|&f| self.face_live(f)).collect();
        let mut flipped = 0;
        while let Some(seed) = pending.pop() {
            // Gather the component reachable through manifold edges.
            let mut component = vec![seed];
            let mut queue = VecDeque::from([seed]);
            while let Some(f) = queue.pop_front() {
                for e in self.edges_of_face(f).collect::<Vec<_>>() {
                    if !self.is_manifold_edge(e) {
                        continue;
                    }
                    for g in self.faces_of_edge(e).collect::<Vec<_>>() {
                        if g != f && pending.contains(&g) && !component.contains(&g) {
                            component.push(g);
                            queue.push_back(g);
                        }
                    }
                }
            }
            pending.retain(|f| !component.contains(f));
            // Orient the seed outward, then propagate.
            let centroid = component.iter().map(|&f| self.face_center(f)).sum::<Vec3>() / component.len() as f64;
            let seed_face = *component
                .iter()
                .max_by(|&&a, &&b| {
                    let da = (self.face_center(a) - centroid).length_squared();
                    let db = (self.face_center(b) - centroid).length_squared();
                    da.total_cmp(&db)
                })
                .expect("non-empty");
            if self.face_normal(seed_face).dot(self.face_center(seed_face) - centroid) < 0.0 {
                self.reverse_face(seed_face)?;
                flipped += 1;
            }
            let mut done = vec![seed_face];
            let mut queue = VecDeque::from([seed_face]);
            while let Some(f) = queue.pop_front() {
                for e in self.edges_of_face(f).collect::<Vec<_>>() {
                    if !self.is_manifold_edge(e) {
                        continue;
                    }
                    for g in self.faces_of_edge(e).collect::<Vec<_>>() {
                        if g == f || done.contains(&g) || !component.contains(&g) {
                            continue;
                        }
                        if !self.consistent_across(e, f, g) {
                            self.reverse_face(g)?;
                            flipped += 1;
                        }
                        done.push(g);
                        queue.push_back(g);
                    }
                }
            }
        }
        Ok(flipped)
    }
}
