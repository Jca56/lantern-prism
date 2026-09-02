//! Bounding volume hierarchy over axis-aligned boxes. Built with a binned
//! surface-area heuristic; queried with an explicit stack. Primitives are
//! whatever the caller's boxes stand for; the closures get their indices.

use prism_math::{Aabb, Ray, Vec3};

const LEAF_SIZE: usize = 4;
const BINS: usize = 12;

#[derive(Clone, Debug)]
struct Node {
    bounds: Aabb,
    /// Leaf: `first..first+count` into `indices`. Inner: `first` is the left
    /// child, `first + 1` the right, `count == 0`.
    first: u32,
    count: u32,
}

#[derive(Clone, Debug, Default)]
pub struct Bvh {
    nodes: Vec<Node>,
    indices: Vec<u32>,
    /// A copy of the primitive boxes so box queries are exact.
    boxes: Vec<Aabb>,
}

impl Bvh {
    /// Build over `boxes` (one per primitive).
    pub fn build(boxes: &[Aabb]) -> Bvh {
        let mut bvh = Bvh { nodes: Vec::new(), indices: (0..boxes.len() as u32).collect(), boxes: boxes.to_vec() };
        if boxes.is_empty() {
            return bvh;
        }
        let centroids: Vec<Vec3> = boxes.iter().map(Aabb::center).collect();
        bvh.nodes.push(Node { bounds: Aabb::EMPTY, first: 0, count: boxes.len() as u32 });
        bvh.subdivide(0, boxes, &centroids);
        bvh
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    pub fn bounds(&self) -> Aabb {
        self.nodes.first().map_or(Aabb::EMPTY, |n| n.bounds)
    }

    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// The box of primitive `i`.
    pub fn primitive_bounds(&self, i: u32) -> Aabb {
        self.boxes[i as usize]
    }

    fn subdivide(&mut self, node: usize, boxes: &[Aabb], centroids: &[Vec3]) {
        let (first, count) = (self.nodes[node].first as usize, self.nodes[node].count as usize);
        let range = &self.indices[first..first + count];
        let bounds = range.iter().fold(Aabb::EMPTY, |b, &i| b.union(&boxes[i as usize]));
        self.nodes[node].bounds = bounds;
        if count <= LEAF_SIZE {
            return;
        }
        let cb = range.iter().fold(Aabb::EMPTY, |b, &i| b.including(centroids[i as usize]));
        let Some((axis, split)) = best_split(range, boxes, centroids, &cb) else {
            return;
        };
        // Partition in place.
        let slice = &mut self.indices[first..first + count];
        let mut mid = 0;
        for i in 0..count {
            if centroids[slice[i] as usize][axis] < split {
                slice.swap(i, mid);
                mid += 1;
            }
        }
        if mid == 0 || mid == count {
            mid = count / 2; // all centroids in one bin: fall back to a median split
        }
        let left = self.nodes.len();
        self.nodes.push(Node { bounds: Aabb::EMPTY, first: first as u32, count: mid as u32 });
        self.nodes.push(Node { bounds: Aabb::EMPTY, first: (first + mid) as u32, count: (count - mid) as u32 });
        self.nodes[node].first = left as u32;
        self.nodes[node].count = 0;
        self.subdivide(left, boxes, centroids);
        self.subdivide(left + 1, boxes, centroids);
    }

    /// Closest hit along `ray`. `hit(prim, t_max)` tests one primitive and
    /// returns its `t` when closer than `t_max`.
    pub fn intersect_ray(&self, ray: &Ray, mut hit: impl FnMut(u32, f64) -> Option<f64>) -> Option<(u32, f64)> {
        if self.nodes.is_empty() {
            return None;
        }
        let mut best: Option<(u32, f64)> = None;
        let mut stack = vec![0usize];
        while let Some(n) = stack.pop() {
            let node = &self.nodes[n];
            let t_max = best.map_or(f64::INFINITY, |(_, t)| t);
            let Some((t0, _)) = ray.intersect_aabb(&node.bounds) else {
                continue;
            };
            if t0 > t_max {
                continue;
            }
            if node.count > 0 {
                for &i in &self.indices[node.first as usize..(node.first + node.count) as usize] {
                    let t_max = best.map_or(f64::INFINITY, |(_, t)| t);
                    if let Some(t) = hit(i, t_max)
                        && t < t_max
                    {
                        best = Some((i, t));
                    }
                }
            } else {
                // Visit the nearer child first.
                let (l, r) = (node.first as usize, node.first as usize + 1);
                let tl = ray.intersect_aabb(&self.nodes[l].bounds).map_or(f64::INFINITY, |(t, _)| t);
                let tr = ray.intersect_aabb(&self.nodes[r].bounds).map_or(f64::INFINITY, |(t, _)| t);
                if tl <= tr {
                    stack.push(r);
                    stack.push(l);
                } else {
                    stack.push(l);
                    stack.push(r);
                }
            }
        }
        best
    }

    /// Every primitive whose box overlaps `query`.
    pub fn query_aabb(&self, query: &Aabb, mut visit: impl FnMut(u32)) {
        if self.nodes.is_empty() {
            return;
        }
        let mut stack = vec![0usize];
        while let Some(n) = stack.pop() {
            let node = &self.nodes[n];
            if !node.bounds.intersects(query) {
                continue;
            }
            if node.count > 0 {
                for &i in &self.indices[node.first as usize..(node.first + node.count) as usize] {
                    if self.boxes[i as usize].intersects(query) {
                        visit(i);
                    }
                }
            } else {
                stack.push(node.first as usize);
                stack.push(node.first as usize + 1);
            }
        }
    }

    /// Closest primitive to `p`. `dist2(prim)` returns the squared distance
    /// from `p` to that primitive.
    pub fn closest(&self, p: Vec3, mut dist2: impl FnMut(u32) -> f64) -> Option<(u32, f64)> {
        if self.nodes.is_empty() {
            return None;
        }
        let mut best: Option<(u32, f64)> = None;
        let mut stack = vec![0usize];
        while let Some(n) = stack.pop() {
            let node = &self.nodes[n];
            let bound = node.bounds.distance_squared(p);
            if best.is_some_and(|(_, d)| bound >= d) {
                continue;
            }
            if node.count > 0 {
                for &i in &self.indices[node.first as usize..(node.first + node.count) as usize] {
                    let d = dist2(i);
                    if best.is_none_or(|(_, bd)| d < bd) {
                        best = Some((i, d));
                    }
                }
            } else {
                let (l, r) = (node.first as usize, node.first as usize + 1);
                let dl = self.nodes[l].bounds.distance_squared(p);
                let dr = self.nodes[r].bounds.distance_squared(p);
                if dl <= dr {
                    stack.push(r);
                    stack.push(l);
                } else {
                    stack.push(l);
                    stack.push(r);
                }
            }
        }
        best
    }
}

/// Binned SAH over the widest centroid axis. `None` when splitting cannot
/// beat a leaf.
fn best_split(range: &[u32], boxes: &[Aabb], centroids: &[Vec3], cb: &Aabb) -> Option<(usize, f64)> {
    let axis = cb.longest_axis();
    let (lo, hi) = (cb.min[axis], cb.max[axis]);
    if hi - lo <= 0.0 {
        return None;
    }
    let scale = BINS as f64 / (hi - lo);
    let mut counts = [0usize; BINS];
    let mut bounds = [Aabb::EMPTY; BINS];
    for &i in range {
        let b = (((centroids[i as usize][axis] - lo) * scale) as usize).min(BINS - 1);
        counts[b] += 1;
        bounds[b] = bounds[b].union(&boxes[i as usize]);
    }
    // Sweep to get costs for every split plane.
    let mut best = None;
    let mut best_cost = f64::INFINITY;
    for s in 1..BINS {
        let (mut lb, mut ln) = (Aabb::EMPTY, 0usize);
        for b in 0..s {
            lb = lb.union(&bounds[b]);
            ln += counts[b];
        }
        let (mut rb, mut rn) = (Aabb::EMPTY, 0usize);
        for b in s..BINS {
            rb = rb.union(&bounds[b]);
            rn += counts[b];
        }
        if ln == 0 || rn == 0 {
            continue;
        }
        let cost = lb.surface_area() * ln as f64 + rb.surface_area() * rn as f64;
        if cost < best_cost {
            best_cost = cost;
            best = Some((axis, lo + s as f64 / scale));
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intersect::{closest_on_triangle, ray_triangle};
    use prism_core::Pcg32;

    fn random_tris(n: usize, seed: u64) -> Vec<[Vec3; 3]> {
        let mut rng = Pcg32::new(seed);
        (0..n)
            .map(|_| {
                let c = Vec3::new(rng.range_f64(-10.0, 10.0), rng.range_f64(-10.0, 10.0), rng.range_f64(-10.0, 10.0));
                let j = |r: &mut Pcg32| Vec3::new(r.range_f64(-1.0, 1.0), r.range_f64(-1.0, 1.0), r.range_f64(-1.0, 1.0));
                [c + j(&mut rng), c + j(&mut rng), c + j(&mut rng)]
            })
            .collect()
    }

    #[test]
    fn matches_brute_force() {
        let tris = random_tris(500, 3);
        let boxes: Vec<Aabb> = tris.iter().map(|t| Aabb::from_points(t.iter().copied())).collect();
        let bvh = Bvh::build(&boxes);
        assert!(bvh.node_count() > 1);
        let mut rng = Pcg32::new(9);
        for _ in 0..200 {
            let o = Vec3::new(rng.range_f64(-15.0, 15.0), rng.range_f64(-15.0, 15.0), 20.0);
            let d = Vec3::new(rng.range_f64(-0.3, 0.3), rng.range_f64(-0.3, 0.3), -1.0);
            let ray = Ray::new(o, d);
            let fast = bvh.intersect_ray(&ray, |i, tmax| {
                let t = tris[i as usize];
                ray_triangle(&ray, t[0], t[1], t[2]).map(|h| h.t).filter(|&t| t < tmax)
            });
            let slow = tris
                .iter()
                .enumerate()
                .filter_map(|(i, t)| ray_triangle(&ray, t[0], t[1], t[2]).map(|h| (i as u32, h.t)))
                .min_by(|a, b| a.1.total_cmp(&b.1));
            assert_eq!(fast.map(|(_, t)| t), slow.map(|(_, t)| t));
        }
        for _ in 0..100 {
            let p = Vec3::new(rng.range_f64(-12.0, 12.0), rng.range_f64(-12.0, 12.0), rng.range_f64(-12.0, 12.0));
            let fast = bvh.closest(p, |i| {
                let t = tris[i as usize];
                closest_on_triangle(p, t[0], t[1], t[2]).distance_squared(p)
            });
            let slow = tris
                .iter()
                .map(|t| closest_on_triangle(p, t[0], t[1], t[2]).distance_squared(p))
                .min_by(|a, b| a.total_cmp(b));
            assert!((fast.unwrap().1 - slow.unwrap()).abs() < 1e-9);
        }
        let q = Aabb::new(Vec3::splat(-2.0), Vec3::splat(2.0));
        let mut found = Vec::new();
        bvh.query_aabb(&q, |i| found.push(i));
        found.sort_unstable();
        let expect: Vec<u32> = boxes.iter().enumerate().filter(|(_, b)| b.intersects(&q)).map(|(i, _)| i as u32).collect();
        assert_eq!(found, expect);
    }

    #[test]
    fn degenerate_inputs() {
        let empty = Bvh::build(&[]);
        assert!(empty.is_empty());
        assert!(empty.intersect_ray(&Ray::new(Vec3::ZERO, Vec3::X), |_, _| None).is_none());
        // Many identical boxes: median fallback keeps recursion finite.
        let same = vec![Aabb::new(Vec3::ZERO, Vec3::ONE); 100];
        let bvh = Bvh::build(&same);
        let mut n = 0;
        bvh.query_aabb(&Aabb::new(Vec3::ZERO, Vec3::ONE), |_| n += 1);
        assert_eq!(n, 100);
    }
}
