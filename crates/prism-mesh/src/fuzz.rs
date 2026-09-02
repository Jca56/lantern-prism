//! Random operator sequences with validation after every step. A failure
//! reports the seed and the exact op trace so it replays.

use prism_core::Pcg32;
use prism_math::Vec3;

use crate::handle::{EdgeH, FaceH, VertH};
use crate::mesh::Mesh;
use crate::primitives;

#[derive(Debug)]
pub struct FuzzFailure {
    pub seed: u64,
    pub step: usize,
    pub trace: Vec<String>,
    pub errors: Vec<crate::validate::ValidateError>,
}

impl core::fmt::Display for FuzzFailure {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        writeln!(f, "fuzz seed {} failed at step {}", self.seed, self.step)?;
        for (i, e) in self.errors.iter().enumerate().take(5) {
            writeln!(f, "  {i}: [{}] {}", e.rule, e.detail)?;
        }
        writeln!(f, "  last ops:")?;
        for op in self.trace.iter().rev().take(12).rev() {
            writeln!(f, "    {op}")?;
        }
        Ok(())
    }
}

impl std::error::Error for FuzzFailure {}

fn pick<T: Copy>(rng: &mut Pcg32, items: &[T]) -> Option<T> {
    rng.choose(items).copied()
}

fn start_mesh(rng: &mut Pcg32) -> Mesh {
    match rng.below(4) {
        0 => primitives::cube(2.0),
        1 => primitives::grid(4.0, 4.0, 3, 3),
        2 => primitives::uv_sphere(1.0, 6, 4),
        _ => primitives::cylinder(1.0, 2.0, 6, true),
    }
}

/// Run `steps` random operations from `seed`, validating after every one.
pub fn run(seed: u64, steps: usize) -> Result<Mesh, FuzzFailure> {
    run_with(seed, steps, 1)
}

/// Like [`run`], validating every `validate_every` steps (and at the end).
pub fn run_with(seed: u64, steps: usize, validate_every: usize) -> Result<Mesh, FuzzFailure> {
    let mut rng = Pcg32::new(seed);
    let mut m = start_mesh(&mut rng);
    let mut trace: Vec<String> = Vec::new();
    for step in 0..steps {
        let verts: Vec<VertH> = m.verts().collect();
        let edges: Vec<EdgeH> = m.edges().collect();
        let faces: Vec<FaceH> = m.faces().collect();
        let big = verts.len() > 3000;
        let op = if big { 20 + rng.below(6) } else { rng.below(26) };
        let desc = match op {
            0 | 1 => {
                let p = Vec3::new(rng.range_f64(-3.0, 3.0), rng.range_f64(-3.0, 3.0), rng.range_f64(-3.0, 3.0));
                let v = m.make_vert(p);
                format!("make_vert -> {v}")
            }
            2 | 3 => match (pick(&mut rng, &verts), pick(&mut rng, &verts)) {
                (Some(a), Some(b)) => format!("make_edge({a}, {b}) -> {:?}", m.make_edge(a, b)),
                _ => "skip".into(),
            },
            4..=6 => match pick(&mut rng, &edges) {
                Some(e) => {
                    let side = m.edge_verts(e)[rng.below(2) as usize];
                    format!("semv({e}, {side}) -> {:?}", m.split_edge_make_vert(e, side))
                }
                None => "skip".into(),
            },
            7 | 8 => match pick(&mut rng, &verts) {
                Some(v) => match m.edges_of(v).next() {
                    Some(e) => format!("jekv({e}, {v}) -> {:?}", m.join_edge_kill_vert(e, v)),
                    None => format!("kill_vert({v}) -> {:?}", m.kill_vert(v)),
                },
                None => "skip".into(),
            },
            9..=11 => match pick(&mut rng, &faces) {
                Some(f) if m.face_len(f) >= 4 => {
                    let loops: Vec<_> = m.loops_of_face(f).collect();
                    let i = rng.range(0, loops.len());
                    let j = rng.range(0, loops.len());
                    format!("sfme({f}, {}, {}) -> {:?}", loops[i], loops[j], m.split_face_make_edge(f, loops[i], loops[j]))
                }
                Some(f) => format!("sfme skip (tri {f})"),
                None => "skip".into(),
            },
            12 | 13 => match pick(&mut rng, &edges) {
                Some(e) => format!("dissolve_edge({e}) -> {:?}", m.dissolve_edge(e)),
                None => "skip".into(),
            },
            14 => match pick(&mut rng, &faces) {
                Some(f) => format!("reverse_face({f}) -> {:?}", m.reverse_face(f)),
                None => "skip".into(),
            },
            15 | 16 => match pick(&mut rng, &edges) {
                Some(e) => format!("collapse_edge({e}) -> {:?}", m.collapse_edge(e)),
                None => "skip".into(),
            },
            17 => match (pick(&mut rng, &verts), pick(&mut rng, &verts)) {
                (Some(a), Some(b)) => format!("weld_verts({a}, {b}) -> {:?}", m.weld_verts(a, b)),
                _ => "skip".into(),
            },
            18 => {
                let n = rng.range(1, 4).min(faces.len().max(1));
                let sel: Vec<FaceH> = (0..n).filter_map(|_| pick(&mut rng, &faces)).collect();
                let r = m.extrude_faces(&sel);
                let d = Vec3::new(0.0, rng.range_f64(0.1, 1.0), 0.0);
                if let Ok(res) = &r {
                    m.translate_verts(&res.verts, d);
                }
                format!("extrude({sel:?}) -> {}", r.as_ref().map(|r| format!("{} faces", r.faces.len())).unwrap_or_else(|e| format!("{e:?}")))
            }
            19 => match pick(&mut rng, &edges) {
                Some(e) => format!("subdivide({e}, 2) -> {:?}", m.subdivide_edges(&[e], 2).map(|v| v.len())),
                None => "skip".into(),
            },
            20 | 21 => match pick(&mut rng, &faces) {
                Some(f) => format!("kill_face({f}) -> {:?}", m.kill_face(f)),
                None => "skip".into(),
            },
            22 => match pick(&mut rng, &edges) {
                Some(e) => format!("kill_edge({e}) -> {:?}", m.kill_edge(e)),
                None => "skip".into(),
            },
            23 => match pick(&mut rng, &verts) {
                Some(v) => format!("dissolve_vert({v}) -> {:?}", m.dissolve_vert(v)),
                None => "skip".into(),
            },
            24 => {
                let n = rng.range(1, 3).min(faces.len().max(1));
                let sel: Vec<FaceH> = (0..n).filter_map(|_| pick(&mut rng, &faces)).collect();
                format!("delete_faces({sel:?}) -> {:?}", m.delete_faces(&sel, rng.chance(0.5)))
            }
            _ => match pick(&mut rng, &faces) {
                Some(f) => {
                    let vs: Vec<VertH> = m.verts_of_face(f).collect();
                    if vs.len() >= 4 {
                        let a = vs[rng.range(0, vs.len())];
                        let b = vs[rng.range(0, vs.len())];
                        format!("connect({f}, {a}, {b}) -> {:?}", m.connect_verts(f, a, b))
                    } else {
                        "connect skip".into()
                    }
                }
                None => "skip".into(),
            },
        };
        trace.push(format!("{step}: {desc}"));
        if trace.len() > 64 {
            trace.remove(0);
        }
        if (step % validate_every.max(1) == 0 || step + 1 == steps)
            && let Err(errors) = m.validate()
        {
            return Err(FuzzFailure { seed, step, trace, errors });
        }
        if m.is_empty() {
            m = start_mesh(&mut rng);
            trace.push(format!("{step}: (mesh emptied, restarted)"));
        }
    }
    Ok(m)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_seeds() {
        for seed in 0..12 {
            if let Err(f) = run(seed, 400) {
                panic!("{f}");
            }
        }
    }

    /// The Phase 3 gate: a million operations. `cargo test --release -p prism-mesh -- --ignored`.
    #[test]
    #[ignore]
    fn one_million_ops() {
        for seed in 100..120 {
            if let Err(f) = run_with(seed, 50_000, 25) {
                panic!("{f}");
            }
        }
    }
}
