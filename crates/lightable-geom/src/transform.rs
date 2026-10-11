//! Coordinate systems and affine transforms.
//!
//! DXF stores many 2D entities (circles, arcs, polylines, text, inserts) in
//! an *object coordinate system* (OCS) defined by the entity's extrusion
//! direction, using the "arbitrary axis algorithm" below. Block references
//! add translation, rotation and (possibly non-uniform) scale on top.

/// A 3D point or vector `[x, y, z]`.
pub type V3 = [f64; 3];

fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn normalize(v: V3) -> Option<V3> {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    (len > 0.0 && len.is_finite()).then(|| [v[0] / len, v[1] / len, v[2] / len])
}

/// The OCS axes (x, y, z) for an extrusion direction, by the arbitrary
/// axis algorithm. A zero or invalid direction counts as the world Z axis.
pub fn ocs_axes(extrusion: V3) -> (V3, V3, V3) {
    let az = normalize(extrusion).unwrap_or([0.0, 0.0, 1.0]);
    let limit = 1.0 / 64.0;
    let ax = if az[0].abs() < limit && az[1].abs() < limit {
        cross([0.0, 1.0, 0.0], az)
    } else {
        cross([0.0, 0.0, 1.0], az)
    };
    let ax = normalize(ax).unwrap_or([1.0, 0.0, 0.0]);
    let ay = normalize(cross(az, ax)).unwrap_or([0.0, 1.0, 0.0]);
    (ax, ay, az)
}

/// An affine map of 3D space: `p -> m * p + t` (row-major 3×3 plus offset).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Affine3 {
    /// The linear part, row-major.
    pub m: [[f64; 3]; 3],
    /// The offset added after the linear part.
    pub t: V3,
}

impl Affine3 {
    /// The map that changes nothing.
    pub const IDENTITY: Affine3 = Affine3 {
        m: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        t: [0.0; 3],
    };

    /// Moves every point by `t`.
    pub fn translate(t: V3) -> Self {
        Self {
            t,
            ..Self::IDENTITY
        }
    }

    /// Scales by `s[0]`, `s[1]`, `s[2]` along the axes (about the origin).
    pub fn scale(s: V3) -> Self {
        Self {
            m: [[s[0], 0.0, 0.0], [0.0, s[1], 0.0], [0.0, 0.0, s[2]]],
            t: [0.0; 3],
        }
    }

    /// Rotation about the Z axis, counter-clockwise, in radians.
    pub fn rotate_z(angle: f64) -> Self {
        let (s, c) = angle.sin_cos();
        Self {
            m: [[c, -s, 0.0], [s, c, 0.0], [0.0, 0.0, 1.0]],
            t: [0.0; 3],
        }
    }

    /// The map from an OCS (given by its extrusion) to world coordinates.
    pub fn from_ocs(extrusion: V3) -> Self {
        let (ax, ay, az) = ocs_axes(extrusion);
        Self {
            m: [
                [ax[0], ay[0], az[0]],
                [ax[1], ay[1], az[1]],
                [ax[2], ay[2], az[2]],
            ],
            t: [0.0; 3],
        }
    }

    /// `self ∘ other`: apply `other` first, then `self`.
    pub fn then_after(&self, other: &Affine3) -> Affine3 {
        let mut m = [[0.0; 3]; 3];
        for (i, row) in m.iter_mut().enumerate() {
            for (j, cell) in row.iter_mut().enumerate() {
                *cell = (0..3).map(|k| self.m[i][k] * other.m[k][j]).sum();
            }
        }
        let t = self.apply(other.t);
        Affine3 { m, t }
    }

    /// Maps a point.
    pub fn apply(&self, p: V3) -> V3 {
        let m = &self.m;
        [
            m[0][0] * p[0] + m[0][1] * p[1] + m[0][2] * p[2] + self.t[0],
            m[1][0] * p[0] + m[1][1] * p[1] + m[1][2] * p[2] + self.t[1],
            m[2][0] * p[0] + m[2][1] * p[1] + m[2][2] * p[2] + self.t[2],
        ]
    }

    /// Applies the linear part only (for directions).
    pub fn apply_vector(&self, v: V3) -> V3 {
        let m = &self.m;
        [
            m[0][0] * v[0] + m[0][1] * v[1] + m[0][2] * v[2],
            m[1][0] * v[0] + m[1][1] * v[1] + m[1][2] * v[2],
            m[2][0] * v[0] + m[2][1] * v[1] + m[2][2] * v[2],
        ]
    }

    /// The 2×2 map from a plane's XY to the drawing plane (world XY seen
    /// from above): how the plane's X and Y axes appear on screen.
    pub fn plan_2x2(&self) -> [[f64; 2]; 2] {
        [[self.m[0][0], self.m[0][1]], [self.m[1][0], self.m[1][1]]]
    }

    /// If this map, viewed from above (world XY), turns circles in its XY
    /// plane into circles: `Some((scale, mirrored))`. `None` when circles
    /// would become ellipses (non-uniform scale or a tilted plane), in which
    /// case arcs must be flattened before transforming.
    pub fn plan_similarity(&self) -> Option<(f64, bool)> {
        let [[a, b], [c, d]] = self.plan_2x2();
        let (len_x, len_y) = ((a * a + c * c).sqrt(), (b * b + d * d).sqrt());
        let dot = a * b + c * d;
        let tol = 1e-9 * (len_x.max(len_y)).max(1e-300);
        if len_x == 0.0 || (len_x - len_y).abs() > tol || dot.abs() > tol * len_x.max(1.0) {
            return None;
        }
        Some((len_x, a * d - b * c < 0.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: V3, b: V3) -> bool {
        (0..3).all(|i| (a[i] - b[i]).abs() < 1e-12)
    }

    #[test]
    fn default_extrusion_is_the_identity() {
        let ocs = Affine3::from_ocs([0.0, 0.0, 1.0]);
        assert!(close(ocs.apply([1.0, 2.0, 3.0]), [1.0, 2.0, 3.0]));
    }

    #[test]
    fn flipped_extrusion_mirrors_x() {
        // The classic case: arcs drawn with extrusion (0, 0, -1).
        let ocs = Affine3::from_ocs([0.0, 0.0, -1.0]);
        assert!(close(ocs.apply([1.0, 2.0, 0.0]), [-1.0, 2.0, 0.0]));
        assert!(close(ocs.apply([0.0, 0.0, 5.0]), [0.0, 0.0, -5.0]));
        let (_, mirrored) = ocs.plan_similarity().unwrap();
        assert!(mirrored);
    }

    #[test]
    fn arbitrary_axis_near_z_uses_world_y() {
        let (ax, ay, az) = ocs_axes([0.01, 0.0, 1.0]);
        // Axes are orthonormal and right-handed.
        let dot = |a: V3, b: V3| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
        assert!(dot(ax, ay).abs() < 1e-12 && dot(ax, az).abs() < 1e-12);
        assert!(close(cross(ax, ay), az));
        // Ax = Wy × Az has no y component.
        assert!(ax[1].abs() < 1e-12);
    }

    #[test]
    fn arbitrary_axis_far_from_z_uses_world_z() {
        let (ax, _, _) = ocs_axes([1.0, 0.0, 0.0]);
        // Ax = Wz × (1,0,0) = (0,1,0).
        assert!(close(ax, [0.0, 1.0, 0.0]));
    }

    #[test]
    fn zero_extrusion_counts_as_z() {
        assert_eq!(Affine3::from_ocs([0.0; 3]), Affine3::IDENTITY);
    }

    #[test]
    fn insert_transform_composes_in_order() {
        // translate(10, 0) ∘ rotate(90°) ∘ scale(2) ∘ translate(-base)
        let base = [1.0, 0.0, 0.0];
        let t = Affine3::translate([10.0, 0.0, 0.0])
            .then_after(&Affine3::rotate_z(std::f64::consts::FRAC_PI_2))
            .then_after(&Affine3::scale([2.0, 2.0, 2.0]))
            .then_after(&Affine3::translate([-base[0], -base[1], -base[2]]));
        // The base point lands on the insertion point.
        assert!(close(t.apply(base), [10.0, 0.0, 0.0]));
        // (2,0) is 1 unit along x from the base: scaled to 2, rotated to +y.
        assert!(close(t.apply([2.0, 0.0, 0.0]), [10.0, 2.0, 0.0]));
        assert_eq!(
            t.plan_similarity().map(|(s, m)| ((s * 1e9).round(), m)),
            Some((2e9, false))
        );
    }

    #[test]
    fn non_uniform_scale_or_tilt_is_not_a_similarity() {
        assert!(Affine3::scale([2.0, 1.0, 1.0]).plan_similarity().is_none());
        assert!(
            Affine3::from_ocs([1.0, 0.0, 1.0])
                .plan_similarity()
                .is_none()
        );
        let mirror = Affine3::scale([-1.0, 1.0, 1.0]).plan_similarity().unwrap();
        assert_eq!(mirror, (1.0, true));
    }
}
