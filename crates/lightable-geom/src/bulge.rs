//! Polyline bulges.
//!
//! A polyline segment from `start` to `end` with bulge `b` is a circular arc
//! whose included angle is `4 * atan(b)`. Positive bulges turn
//! counter-clockwise, negative ones clockwise, and `b = 0` is a straight
//! segment. `|b| = 1` is a semicircle.

/// The arc a bulged segment describes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BulgeArc {
    /// Center of the arc.
    pub center: [f64; 2],
    /// Radius (positive).
    pub radius: f64,
    /// Angle of `start` seen from the center, in radians (-π, π].
    pub start_angle: f64,
    /// Signed sweep from `start` to `end`, in radians: positive is
    /// counter-clockwise. Equal to `4 * atan(bulge)`.
    pub sweep: f64,
}

impl BulgeArc {
    /// Angle of the end point seen from the center (not normalised).
    pub fn end_angle(&self) -> f64 {
        self.start_angle + self.sweep
    }

    /// The point on the arc halfway along the sweep.
    pub fn midpoint(&self) -> [f64; 2] {
        let a = self.start_angle + self.sweep / 2.0;
        [
            self.center[0] + self.radius * a.cos(),
            self.center[1] + self.radius * a.sin(),
        ]
    }
}

/// Converts a bulged segment to an arc.
///
/// Returns `None` for a straight segment (`bulge == 0`), for coincident end
/// points (no arc is defined), and for non-finite input.
pub fn arc_from_bulge(start: [f64; 2], end: [f64; 2], bulge: f64) -> Option<BulgeArc> {
    if bulge == 0.0 || !bulge.is_finite() || start.iter().chain(&end).any(|v| !v.is_finite()) {
        return None;
    }
    let (dx, dy) = (end[0] - start[0], end[1] - start[1]);
    let chord = dx.hypot(dy);
    if chord == 0.0 {
        return None;
    }
    let sweep = 4.0 * bulge.atan();
    // Sagitta geometry: radius = chord * (1 + b²) / (4 |b|).
    let radius = chord * (1.0 + bulge * bulge) / (4.0 * bulge.abs());
    // Signed distance from the chord midpoint to the center, measured along
    // the chord's left normal: (1 - b²) / (4 b) * chord.
    let offset = (1.0 - bulge * bulge) / (4.0 * bulge);
    let mid = [(start[0] + end[0]) / 2.0, (start[1] + end[1]) / 2.0];
    let center = [mid[0] - dy * offset, mid[1] + dx * offset];
    let start_angle = (start[1] - center[1]).atan2(start[0] - center[0]);
    Some(BulgeArc {
        center,
        radius,
        start_angle,
        sweep,
    })
}

/// The bulge for an arc of signed `sweep` radians (inverse of the sweep
/// relation above).
pub fn bulge_from_sweep(sweep: f64) -> f64 {
    (sweep / 4.0).tan()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::{FRAC_PI_2, PI};

    const EPS: f64 = 1e-12;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < EPS
    }

    fn close_pt(a: [f64; 2], b: [f64; 2]) -> bool {
        close(a[0], b[0]) && close(a[1], b[1])
    }

    fn end_point(arc: &BulgeArc) -> [f64; 2] {
        let a = arc.end_angle();
        [
            arc.center[0] + arc.radius * a.cos(),
            arc.center[1] + arc.radius * a.sin(),
        ]
    }

    #[test]
    fn semicircle_counter_clockwise() {
        let arc = arc_from_bulge([0.0, 0.0], [2.0, 0.0], 1.0).unwrap();
        assert!(close_pt(arc.center, [1.0, 0.0]));
        assert!(close(arc.radius, 1.0));
        assert!(close(arc.sweep, PI));
        // Counter-clockwise from (0,0) to (2,0) passes below the chord.
        assert!(close_pt(arc.midpoint(), [1.0, -1.0]));
    }

    #[test]
    fn negative_bulge_turns_clockwise() {
        let arc = arc_from_bulge([0.0, 0.0], [2.0, 0.0], -1.0).unwrap();
        assert!(close(arc.sweep, -PI));
        assert!(close_pt(arc.midpoint(), [1.0, 1.0]));
    }

    #[test]
    fn quarter_circle() {
        // tan(90° / 4) gives a quarter arc from (1,0) to (0,1) around the origin.
        let b = bulge_from_sweep(FRAC_PI_2);
        let arc = arc_from_bulge([1.0, 0.0], [0.0, 1.0], b).unwrap();
        assert!(close_pt(arc.center, [0.0, 0.0]));
        assert!(close(arc.radius, 1.0));
        assert!(close(arc.start_angle, 0.0));
        assert!(close(arc.end_angle(), FRAC_PI_2));
    }

    #[test]
    fn arc_ends_at_end_point_for_many_bulges() {
        let (s, e) = ([3.0, -1.0], [-2.0, 4.0]);
        for b in [-5.0, -1.0, -0.3, 0.001, 0.4142, 1.0, 2.0, 50.0] {
            let arc = arc_from_bulge(s, e, b).unwrap();
            let start = [
                arc.center[0] + arc.radius * arc.start_angle.cos(),
                arc.center[1] + arc.radius * arc.start_angle.sin(),
            ];
            assert!(close_pt(start, s), "bulge {b}");
            let end = end_point(&arc);
            assert!(
                (end[0] - e[0]).abs() < 1e-9 && (end[1] - e[1]).abs() < 1e-9,
                "bulge {b}"
            );
        }
    }

    #[test]
    fn large_bulge_is_more_than_a_semicircle() {
        let arc = arc_from_bulge([0.0, 0.0], [2.0, 0.0], 2.0).unwrap();
        assert!(arc.sweep > PI);
        assert!(arc.radius > 1.0);
    }

    #[test]
    fn straight_and_degenerate_segments() {
        assert_eq!(arc_from_bulge([0.0, 0.0], [1.0, 0.0], 0.0), None);
        assert_eq!(arc_from_bulge([1.0, 1.0], [1.0, 1.0], 1.0), None);
        assert_eq!(arc_from_bulge([0.0, 0.0], [1.0, 0.0], f64::NAN), None);
        assert_eq!(arc_from_bulge([0.0, f64::INFINITY], [1.0, 0.0], 1.0), None);
    }

    #[test]
    fn bulge_and_sweep_round_trip() {
        for sweep in [-3.0, -1.0, 0.5, 2.0, 3.1] {
            assert!(close(4.0 * bulge_from_sweep(sweep).atan(), sweep));
        }
    }
}
