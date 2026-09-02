//! Polygon triangulation. Triangles pass through, quads split along the
//! diagonal that keeps both halves facing the polygon normal, and larger
//! n-gons are ear-clipped in the plane of their Newell normal.

use prism_math::{Vec2, Vec3};

use crate::normal::newell;
use crate::predicates::{is_ccw, point_in_triangle};

/// Append triangles (as index triples into `points`) covering the polygon.
/// Winding of every triangle follows the polygon's. Degenerate input
/// (fewer than three points) produces nothing; a polygon ear clipping cannot
/// resolve falls back to a fan so nothing is ever silently dropped.
pub fn triangulate(points: &[Vec3], out: &mut Vec<[u32; 3]>) {
    match points.len() {
        0..=2 => {}
        3 => out.push([0, 1, 2]),
        4 => triangulate_quad(points, out),
        _ => ear_clip(points, out),
    }
}

fn triangulate_quad(p: &[Vec3], out: &mut Vec<[u32; 3]>) {
    let n = newell(p);
    // Diagonal 0-2 splits into (0,1,2) and (0,2,3); diagonal 1-3 into
    // (1,2,3) and (1,3,0). Reject a diagonal whose triangle folds against the
    // normal (concave quad); otherwise prefer the shorter one.
    let faces_up = |a: usize, b: usize, c: usize| (p[b] - p[a]).cross(p[c] - p[a]).dot(n) > 0.0;
    let d02_ok = faces_up(0, 1, 2) && faces_up(0, 2, 3);
    let d13_ok = faces_up(1, 2, 3) && faces_up(1, 3, 0);
    let d02 = p[0].distance_squared(p[2]);
    let d13 = p[1].distance_squared(p[3]);
    let use_02 = match (d02_ok, d13_ok) {
        (true, false) => true,
        (false, true) => false,
        _ => d02 <= d13,
    };
    if use_02 {
        out.push([0, 1, 2]);
        out.push([0, 2, 3]);
    } else {
        out.push([1, 2, 3]);
        out.push([1, 3, 0]);
    }
}

/// Project onto the plane perpendicular to the polygon normal so that the
/// polygon reads counter-clockwise.
fn project(points: &[Vec3]) -> Vec<Vec2> {
    let n = newell(points).normalize_or(Vec3::Y);
    let (u, v) = n.orthonormal_basis();
    points.iter().map(|&p| Vec2::new(p.dot(u), p.dot(v))).collect()
}

fn ear_clip(points: &[Vec3], out: &mut Vec<[u32; 3]>) {
    let p2 = project(points);
    let mut idx: Vec<u32> = (0..points.len() as u32).collect();
    let start_len = out.len();
    let mut guard = 0usize;
    while idx.len() > 3 {
        let n = idx.len();
        let mut found = None;
        for i in 0..n {
            let (ia, ib, ic) = (idx[(i + n - 1) % n], idx[i], idx[(i + 1) % n]);
            let (a, b, c) = (p2[ia as usize], p2[ib as usize], p2[ic as usize]);
            if !is_ccw(a, b, c) {
                continue; // reflex or degenerate corner
            }
            let blocked = idx
                .iter()
                .filter(|&&j| j != ia && j != ib && j != ic)
                .any(|&j| point_in_triangle(p2[j as usize], a, b, c));
            if !blocked {
                found = Some(i);
                break;
            }
        }
        match found {
            Some(i) => {
                let n = idx.len();
                out.push([idx[(i + n - 1) % n], idx[i], idx[(i + 1) % n]]);
                idx.remove(i);
            }
            None => {
                // Self-intersecting or fully degenerate: fan from the first
                // remaining vertex so every corner is still covered.
                out.truncate(start_len);
                for i in 1..points.len() - 1 {
                    out.push([0, i as u32, i as u32 + 1]);
                }
                return;
            }
        }
        guard += 1;
        if guard > points.len() * 2 {
            break;
        }
    }
    if idx.len() == 3 {
        out.push([idx[0], idx[1], idx[2]]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::normal::polygon_normal;

    fn tri_normal(p: &[Vec3], t: [u32; 3]) -> Vec3 {
        let (a, b, c) = (p[t[0] as usize], p[t[1] as usize], p[t[2] as usize]);
        (b - a).cross(c - a).normalize_or_zero()
    }

    fn check(points: &[Vec3], expect_tris: usize) {
        let mut out = Vec::new();
        triangulate(points, &mut out);
        assert_eq!(out.len(), expect_tris, "{points:?}");
        let n = polygon_normal(points);
        let mut used = vec![false; points.len()];
        for t in &out {
            assert!(tri_normal(points, *t).dot(n) > 0.0, "triangle {t:?} faces the wrong way");
            for &i in t {
                used[i as usize] = true;
            }
        }
        assert!(used.iter().all(|&u| u), "every corner is covered");
    }

    #[test]
    fn small_cases() {
        check(&[Vec3::ZERO, Vec3::X, Vec3::Y], 1);
        let mut out = Vec::new();
        triangulate(&[Vec3::ZERO, Vec3::X], &mut out);
        assert!(out.is_empty());
        // Square in the XZ plane, CCW from +Y.
        check(&[Vec3::ZERO, Vec3::new(1.0, 0.0, 0.0), Vec3::new(1.0, 0.0, -1.0), Vec3::new(0.0, 0.0, -1.0)], 2);
    }

    #[test]
    fn concave_quad_uses_the_safe_diagonal() {
        // Arrowhead: vertex 3 pokes inward, so diagonal 0-2 would fold.
        let p = [Vec3::new(0.0, 0.0, 0.0), Vec3::new(2.0, 0.0, 0.0), Vec3::new(2.0, 2.0, 0.0), Vec3::new(1.5, 0.5, 0.0)];
        let mut out = Vec::new();
        triangulate(&p, &mut out);
        assert_eq!(out, vec![[1, 2, 3], [1, 3, 0]]);
        check(&p, 2);
    }

    #[test]
    fn ngons() {
        // Regular hexagon.
        let hex: Vec<Vec3> = (0..6).map(|i| {
            let a = i as f64 * std::f64::consts::TAU / 6.0;
            Vec3::new(a.cos(), 0.0, -a.sin())
        }).collect();
        check(&hex, 4);
        // Concave "C" shape (8 points) in XY.
        let c = [
            Vec3::new(0.0, 0.0, 0.0), Vec3::new(3.0, 0.0, 0.0), Vec3::new(3.0, 1.0, 0.0), Vec3::new(1.0, 1.0, 0.0),
            Vec3::new(1.0, 2.0, 0.0), Vec3::new(3.0, 2.0, 0.0), Vec3::new(3.0, 3.0, 0.0), Vec3::new(0.0, 3.0, 0.0),
        ];
        check(&c, 6);
        // Same shape reversed (clockwise) still covers everything.
        let mut rev = c;
        rev.reverse();
        check(&rev, 6);
    }

    #[test]
    fn self_intersecting_falls_back_to_a_fan() {
        // Bow-tie: no valid ear exists, fan covers all corners anyway.
        let bow = [Vec3::new(0.0, 0.0, 0.0), Vec3::new(1.0, 1.0, 0.0), Vec3::new(1.0, 0.0, 0.0), Vec3::new(0.0, 1.0, 0.0), Vec3::new(0.5, 2.0, 0.0)];
        let mut out = Vec::new();
        triangulate(&bow, &mut out);
        assert_eq!(out.len(), 3);
    }
}
