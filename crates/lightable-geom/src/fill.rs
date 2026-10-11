//! Filling regions bounded by closed loops (hatch fills).
//!
//! The region is the even-odd combination of the loops (a point is inside
//! when a ray from it crosses the loops an odd number of times), which is
//! how hatch boundaries with holes and islands are filled.

/// A 2D point `[x, y]`.
pub type P2 = [f64; 2];

/// Triangles covering the even-odd region of `loops` (each loop is closed
/// implicitly). Works by cutting the plane into horizontal slabs at every
/// vertex height and filling between pairs of edges in each slab, so it
/// handles holes, islands and concave loops. Self-intersecting edges inside
/// one slab are not split, so such loops are filled approximately.
pub fn even_odd_triangles(loops: &[Vec<P2>]) -> Vec<[P2; 3]> {
    // Non-horizontal edges, lower end first.
    let mut edges: Vec<(P2, P2)> = Vec::new();
    for l in loops.iter().filter(|l| l.len() >= 3) {
        for i in 0..l.len() {
            let (a, b) = (l[i], l[(i + 1) % l.len()]);
            if !(a[0].is_finite() && a[1].is_finite() && b[0].is_finite() && b[1].is_finite()) {
                continue;
            }
            if a[1] < b[1] {
                edges.push((a, b));
            } else if b[1] < a[1] {
                edges.push((b, a));
            }
        }
    }
    edges.sort_by(|x, y| x.0[1].total_cmp(&y.0[1]));
    let mut ys: Vec<f64> = edges.iter().flat_map(|e| [e.0[1], e.1[1]]).collect();
    ys.sort_by(f64::total_cmp);
    ys.dedup();

    let x_at = |e: &(P2, P2), y: f64| {
        let t = (y - e.0[1]) / (e.1[1] - e.0[1]);
        e.0[0] + (e.1[0] - e.0[0]) * t
    };
    let mut out = Vec::new();
    let mut active: Vec<usize> = Vec::new();
    let mut next = 0;
    let mut crossing: Vec<(f64, f64, f64)> = Vec::new();
    for w in ys.windows(2) {
        let (y0, y1) = (w[0], w[1]);
        while next < edges.len() && edges[next].0[1] <= y0 {
            active.push(next);
            next += 1;
        }
        active.retain(|&i| edges[i].1[1] > y0);
        crossing.clear();
        let ym = (y0 + y1) / 2.0;
        for &i in &active {
            let e = &edges[i];
            if e.0[1] <= y0 && e.1[1] >= y1 {
                crossing.push((x_at(e, ym), x_at(e, y0), x_at(e, y1)));
            }
        }
        crossing.sort_by(|a, b| a.0.total_cmp(&b.0));
        for pair in crossing.as_chunks::<2>().0 {
            let (l, r) = (pair[0], pair[1]);
            let (a, b, c, d) = ([l.1, y0], [r.1, y0], [r.2, y1], [l.2, y1]);
            if r.1 > l.1 {
                out.push([a, b, c]);
            }
            if r.2 > l.2 {
                out.push([a, c, d]);
            }
        }
    }
    out
}

/// The area of a triangle (positive).
pub fn triangle_area(t: &[P2; 3]) -> f64 {
    ((t[1][0] - t[0][0]) * (t[2][1] - t[0][1]) - (t[2][0] - t[0][0]) * (t[1][1] - t[0][1])).abs()
        / 2.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area(loops: &[Vec<P2>]) -> f64 {
        even_odd_triangles(loops).iter().map(triangle_area).sum()
    }

    fn square(x: f64, y: f64, s: f64) -> Vec<P2> {
        vec![[x, y], [x + s, y], [x + s, y + s], [x, y + s]]
    }

    #[test]
    fn a_square_is_two_triangles() {
        let t = even_odd_triangles(&[square(0.0, 0.0, 2.0)]);
        assert_eq!(t.len(), 2);
        assert!((area(&[square(0.0, 0.0, 2.0)]) - 4.0).abs() < 1e-12);
    }

    #[test]
    fn holes_and_islands_alternate() {
        let outer = square(0.0, 0.0, 10.0);
        let hole = square(2.0, 2.0, 6.0);
        let island = square(4.0, 4.0, 2.0);
        assert!((area(&[outer.clone(), hole.clone()]) - 64.0).abs() < 1e-9);
        assert!((area(&[outer, hole, island]) - 68.0).abs() < 1e-9);
    }

    #[test]
    fn overlapping_loops_cancel_where_they_overlap() {
        // Two 4×4 squares overlapping in a 2×2 square: 16 + 16 - 2 × 4.
        let a = area(&[square(0.0, 0.0, 4.0), square(2.0, 2.0, 4.0)]);
        assert!((a - 24.0).abs() < 1e-9);
    }

    #[test]
    fn concave_loops_and_either_winding() {
        // An L shape, clockwise.
        let l = vec![
            [0.0, 0.0],
            [0.0, 3.0],
            [1.0, 3.0],
            [1.0, 1.0],
            [3.0, 1.0],
            [3.0, 0.0],
        ];
        assert!((area(&[l]) - 5.0).abs() < 1e-12);
        let tri = vec![[0.0, 0.0], [4.0, 0.0], [2.0, 3.0]];
        assert!((area(&[tri]) - 6.0).abs() < 1e-12);
    }

    #[test]
    fn degenerate_input_gives_nothing() {
        assert!(even_odd_triangles(&[]).is_empty());
        assert!(even_odd_triangles(&[vec![[0.0, 0.0], [1.0, 1.0]]]).is_empty());
        assert!(even_odd_triangles(&[vec![[0.0, 0.0], [1.0, 0.0], [2.0, 0.0]]]).is_empty());
        assert!(even_odd_triangles(&[vec![[0.0, 0.0], [f64::NAN, 0.0], [2.0, 1.0]]]).len() <= 1);
    }
}
