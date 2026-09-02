//! kd-tree over points for nearest-neighbour and radius queries (snapping,
//! merge by distance). Balanced by median splits; stored implicitly as a
//! reordered index array.

use prism_math::Vec3;

#[derive(Clone, Debug, Default)]
pub struct KdTree {
    points: Vec<Vec3>,
    /// Indices into `points`, arranged so each subrange's median is its root.
    order: Vec<u32>,
}

impl KdTree {
    pub fn build(points: &[Vec3]) -> KdTree {
        let mut order: Vec<u32> = (0..points.len() as u32).collect();
        build_range(points, &mut order, 0);
        KdTree { points: points.to_vec(), order }
    }

    pub fn len(&self) -> usize {
        self.points.len()
    }

    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }

    /// Nearest point to `p`: `(index, squared distance)`.
    pub fn nearest(&self, p: Vec3) -> Option<(u32, f64)> {
        if self.order.is_empty() {
            return None;
        }
        let mut best = (u32::MAX, f64::INFINITY);
        self.nearest_in(p, 0, self.order.len(), 0, &mut best);
        (best.0 != u32::MAX).then_some(best)
    }

    fn nearest_in(&self, p: Vec3, lo: usize, hi: usize, depth: usize, best: &mut (u32, f64)) {
        if lo >= hi {
            return;
        }
        let mid = (lo + hi) / 2;
        let i = self.order[mid];
        let q = self.points[i as usize];
        let d = p.distance_squared(q);
        if d < best.1 {
            *best = (i, d);
        }
        let axis = depth % 3;
        let diff = p[axis] - q[axis];
        let (near, far) = if diff < 0.0 { ((lo, mid), (mid + 1, hi)) } else { ((mid + 1, hi), (lo, mid)) };
        self.nearest_in(p, near.0, near.1, depth + 1, best);
        if diff * diff < best.1 {
            self.nearest_in(p, far.0, far.1, depth + 1, best);
        }
    }

    /// Every point within `radius` of `p`.
    pub fn within(&self, p: Vec3, radius: f64, out: &mut Vec<u32>) {
        self.within_in(p, radius * radius, 0, self.order.len(), 0, out);
    }

    fn within_in(&self, p: Vec3, r2: f64, lo: usize, hi: usize, depth: usize, out: &mut Vec<u32>) {
        if lo >= hi {
            return;
        }
        let mid = (lo + hi) / 2;
        let i = self.order[mid];
        let q = self.points[i as usize];
        if p.distance_squared(q) <= r2 {
            out.push(i);
        }
        let axis = depth % 3;
        let diff = p[axis] - q[axis];
        if diff <= 0.0 || diff * diff <= r2 {
            self.within_in(p, r2, lo, mid, depth + 1, out);
        }
        if diff >= 0.0 || diff * diff <= r2 {
            self.within_in(p, r2, mid + 1, hi, depth + 1, out);
        }
    }

    /// All pairs `(i, j)` with `i < j` closer than `radius`.
    pub fn pairs_within(&self, radius: f64) -> Vec<(u32, u32)> {
        let mut pairs = Vec::new();
        let mut scratch = Vec::new();
        for (i, &p) in self.points.iter().enumerate() {
            scratch.clear();
            self.within(p, radius, &mut scratch);
            for &j in &scratch {
                if (j as usize) > i {
                    pairs.push((i as u32, j));
                }
            }
        }
        pairs.sort_unstable();
        pairs
    }
}

fn build_range(points: &[Vec3], order: &mut [u32], depth: usize) {
    if order.len() <= 1 {
        return;
    }
    let axis = depth % 3;
    let mid = order.len() / 2;
    order.select_nth_unstable_by(mid, |&a, &b| points[a as usize][axis].total_cmp(&points[b as usize][axis]));
    let (left, right) = order.split_at_mut(mid);
    build_range(points, left, depth + 1);
    build_range(points, &mut right[1..], depth + 1);
}

#[cfg(test)]
mod tests {
    use super::*;
    use prism_core::Pcg32;

    fn cloud(n: usize, seed: u64) -> Vec<Vec3> {
        let mut rng = Pcg32::new(seed);
        (0..n).map(|_| Vec3::new(rng.range_f64(-5.0, 5.0), rng.range_f64(-5.0, 5.0), rng.range_f64(-5.0, 5.0))).collect()
    }

    #[test]
    fn nearest_matches_brute_force() {
        let pts = cloud(1000, 1);
        let tree = KdTree::build(&pts);
        assert_eq!(tree.len(), 1000);
        let mut rng = Pcg32::new(2);
        for _ in 0..300 {
            let p = Vec3::new(rng.range_f64(-6.0, 6.0), rng.range_f64(-6.0, 6.0), rng.range_f64(-6.0, 6.0));
            let (i, d) = tree.nearest(p).unwrap();
            let slow = pts.iter().map(|q| q.distance_squared(p)).fold(f64::INFINITY, f64::min);
            assert!((d - slow).abs() < 1e-12);
            assert!((pts[i as usize].distance_squared(p) - slow).abs() < 1e-12);
        }
        assert!(KdTree::build(&[]).nearest(Vec3::ZERO).is_none());
    }

    #[test]
    fn radius_and_pairs() {
        let pts = cloud(400, 5);
        let tree = KdTree::build(&pts);
        let p = Vec3::ZERO;
        let mut got = Vec::new();
        tree.within(p, 2.0, &mut got);
        got.sort_unstable();
        let expect: Vec<u32> = (0..pts.len() as u32).filter(|&i| pts[i as usize].distance(p) <= 2.0).collect();
        assert_eq!(got, expect);

        let pairs = tree.pairs_within(0.7);
        let mut expect = Vec::new();
        for i in 0..pts.len() {
            for j in i + 1..pts.len() {
                if pts[i].distance(pts[j]) <= 0.7 {
                    expect.push((i as u32, j as u32));
                }
            }
        }
        assert_eq!(pairs, expect);
        assert!(!pairs.is_empty(), "400 points in a 10-unit cube overlap at 0.7");
    }
}
