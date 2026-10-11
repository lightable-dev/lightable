//! Flattening curves into points (ellipses, NURBS splines).

use std::f64::consts::TAU;

/// A 3D point or vector `[x, y, z]`.
pub type P3 = [f64; 3];

fn add(a: P3, b: P3) -> P3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn scale(a: P3, s: f64) -> P3 {
    [a[0] * s, a[1] * s, a[2] * s]
}

fn cross(a: P3, b: P3) -> P3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// Points along an ellipse or elliptical arc.
///
/// `major` is the major axis end point relative to `center`; the minor axis
/// is `normal × major` scaled by `ratio` (with `normal` a unit vector).
/// Parameters are in radians; an arc runs counter-clockwise from `start` to
/// `end` (wrapping past 2π). Consecutive points are at most `max_step`
/// radians of parameter apart. A full ellipse (`end - start` a multiple of
/// 2π) returns a closed loop whose last point equals the first.
pub fn ellipse_points(
    center: P3,
    major: P3,
    normal: P3,
    ratio: f64,
    start: f64,
    end: f64,
    max_step: f64,
) -> Vec<P3> {
    let minor = scale(cross(normal, major), ratio);
    let mut sweep = (end - start) % TAU;
    if sweep <= 1e-12 {
        sweep += TAU;
    }
    let steps = ((sweep / max_step.max(1e-3)).ceil() as usize).clamp(4, 100_000);
    (0..=steps)
        .map(|i| {
            let t = start + sweep * i as f64 / steps as f64;
            add(center, add(scale(major, t.cos()), scale(minor, t.sin())))
        })
        .collect()
}

/// Points along a NURBS curve of `degree` with the given knot vector,
/// control points and weights (empty = all 1), sampled `per_span` times in
/// every non-empty knot span. Returns `None` if the data is inconsistent
/// (knot count must be control points + degree + 1).
pub fn nurbs_points(
    degree: usize,
    knots: &[f64],
    control: &[P3],
    weights: &[f64],
    per_span: usize,
) -> Option<Vec<P3>> {
    let n = control.len();
    if degree == 0
        || n <= degree
        || knots.len() != n + degree + 1
        || (!weights.is_empty() && weights.len() != n)
        || knots.windows(2).any(|w| w[1] < w[0])
        || knots.iter().chain(weights).any(|v| !v.is_finite())
    {
        return None;
    }
    let weight = |i: usize| weights.get(i).copied().unwrap_or(1.0);
    let (t0, t1) = (knots[degree], knots[n]);
    if t1 <= t0 {
        return None;
    }
    let per_span = per_span.max(1);
    let mut params = Vec::new();
    for span in degree..n {
        let (a, b) = (knots[span], knots[span + 1]);
        if b > a {
            for i in 0..per_span {
                params.push(a + (b - a) * i as f64 / per_span as f64);
            }
        }
    }
    params.push(t1);
    Some(
        params
            .into_iter()
            .map(|t| de_boor(degree, knots, control, &weight, t))
            .collect(),
    )
}

/// Evaluates the curve at `t` with de Boor's algorithm in homogeneous
/// coordinates.
fn de_boor(
    degree: usize,
    knots: &[f64],
    control: &[P3],
    weight: &dyn Fn(usize) -> f64,
    t: f64,
) -> P3 {
    let n = control.len();
    // The span k with knots[k] <= t < knots[k + 1], clamped to the valid range.
    let mut k = degree;
    while k + 1 < n && knots[k + 1] <= t {
        k += 1;
    }
    let mut d: Vec<[f64; 4]> = (0..=degree)
        .map(|j| {
            let i = j + k - degree;
            let w = weight(i);
            let p = control[i];
            [p[0] * w, p[1] * w, p[2] * w, w]
        })
        .collect();
    for r in 1..=degree {
        for j in (r..=degree).rev() {
            let i = j + k - degree;
            let denom = knots[i + degree + 1 - r] - knots[i];
            let alpha = if denom == 0.0 {
                0.0
            } else {
                (t - knots[i]) / denom
            };
            let previous = d[j - 1];
            for (value, earlier) in d[j].iter_mut().zip(previous) {
                *value = (1.0 - alpha) * earlier + alpha * *value;
            }
        }
    }
    let [x, y, z, w] = d[degree];
    if w == 0.0 {
        [x, y, z]
    } else {
        [x / w, y / w, z / w]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::FRAC_PI_2;

    const Z: P3 = [0.0, 0.0, 1.0];

    fn close(a: P3, b: P3) -> bool {
        (0..3).all(|i| (a[i] - b[i]).abs() < 1e-9)
    }

    #[test]
    fn full_ellipse_is_closed_and_on_the_curve() {
        let pts = ellipse_points([5.0, 5.0, 0.0], [4.0, 0.0, 0.0], Z, 0.5, 0.0, TAU, 0.1);
        assert!(pts.len() >= 63);
        assert!(close(pts[0], *pts.last().unwrap()));
        for p in &pts {
            let v = ((p[0] - 5.0) / 4.0).powi(2) + ((p[1] - 5.0) / 2.0).powi(2);
            assert!((v - 1.0).abs() < 1e-12);
        }
    }

    #[test]
    fn elliptical_arc_wraps_past_two_pi() {
        let pts = ellipse_points(
            [0.0; 3],
            [1.0, 0.0, 0.0],
            Z,
            1.0,
            3.0 * FRAC_PI_2,
            FRAC_PI_2,
            0.2,
        );
        assert!(close(pts[0], [0.0, -1.0, 0.0]));
        assert!(close(*pts.last().unwrap(), [0.0, 1.0, 0.0]));
        // It passes through (1, 0), not (-1, 0).
        assert!(pts.iter().any(|p| close(*p, [1.0, 0.0, 0.0])));
        assert!(pts.iter().all(|p| p[0] > -1e-9));
    }

    #[test]
    fn flipped_normal_mirrors_the_minor_axis() {
        let pts = ellipse_points(
            [0.0; 3],
            [2.0, 0.0, 0.0],
            [0.0, 0.0, -1.0],
            0.5,
            FRAC_PI_2,
            FRAC_PI_2 + 0.1,
            1.0,
        );
        assert!(close(pts[0], [0.0, -1.0, 0.0]));
    }

    #[test]
    fn cubic_bezier_midpoint() {
        let ctrl = [
            [0.0, 0.0, 0.0],
            [1.0, 2.0, 0.0],
            [3.0, 2.0, 0.0],
            [4.0, 0.0, 0.0],
        ];
        let knots = [0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0];
        let pts = nurbs_points(3, &knots, &ctrl, &[], 2).unwrap();
        assert_eq!(pts.len(), 3);
        assert!(close(pts[0], ctrl[0]));
        // B(0.5) = (P0 + 3 P1 + 3 P2 + P3) / 8
        assert!(close(pts[1], [2.0, 1.5, 0.0]));
        assert!(close(pts[2], ctrl[3]));
    }

    #[test]
    fn rational_quadratic_is_an_exact_quarter_circle() {
        let w = std::f64::consts::FRAC_1_SQRT_2;
        let ctrl = [[1.0, 0.0, 0.0], [1.0, 1.0, 0.0], [0.0, 1.0, 0.0]];
        let pts = nurbs_points(
            2,
            &[0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
            &ctrl,
            &[1.0, w, 1.0],
            16,
        )
        .unwrap();
        for p in &pts {
            assert!((p[0].hypot(p[1]) - 1.0).abs() < 1e-12, "{p:?}");
        }
        assert!(close(*pts.last().unwrap(), [0.0, 1.0, 0.0]));
    }

    #[test]
    fn multi_span_clamped_spline_hits_its_ends() {
        let ctrl: Vec<P3> = (0..6).map(|i| [i as f64, (i % 2) as f64, 0.0]).collect();
        let knots = [0.0, 0.0, 0.0, 0.0, 1.0, 2.0, 3.0, 3.0, 3.0, 3.0];
        let pts = nurbs_points(3, &knots, &ctrl, &[], 8).unwrap();
        assert_eq!(pts.len(), 3 * 8 + 1);
        assert!(close(pts[0], ctrl[0]));
        assert!(close(*pts.last().unwrap(), ctrl[5]));
    }

    #[test]
    fn inconsistent_splines_are_rejected() {
        let ctrl = [[0.0; 3], [1.0, 0.0, 0.0], [2.0, 0.0, 0.0]];
        assert!(
            nurbs_points(3, &[0.0; 7], &ctrl, &[], 4).is_none(),
            "too few control points"
        );
        assert!(nurbs_points(2, &[0.0, 0.0, 0.0, 1.0, 1.0], &ctrl, &[], 4).is_none());
        assert!(nurbs_points(2, &[0.0, 0.0, 0.0, 1.0, 1.0, 1.0], &ctrl, &[1.0], 4).is_none());
        assert!(nurbs_points(2, &[0.0, 0.0, 1.0, 0.5, 1.0, 1.0], &ctrl, &[], 4).is_none());
        assert!(
            nurbs_points(2, &[0.0; 6], &ctrl, &[], 4).is_none(),
            "zero-length range"
        );
    }
}
