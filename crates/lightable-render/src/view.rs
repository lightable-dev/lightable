//! Pan and zoom: the map between world coordinates (Y up) and screen
//! pixels (Y down).

use crate::scene::{Bounds, P2};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct View {
    /// World point at the centre of the screen.
    pub center: P2,
    /// Pixels per world unit.
    pub scale: f64,
    /// Screen size in pixels.
    pub size: [f64; 2],
}

const MIN_SCALE: f64 = 1e-12;
const MAX_SCALE: f64 = 1e12;

impl View {
    pub fn new(size: [f64; 2]) -> Self {
        Self {
            center: [0.0, 0.0],
            scale: 1.0,
            size,
        }
    }

    pub fn to_screen(&self, p: P2) -> [f64; 2] {
        [
            (p[0] - self.center[0]) * self.scale + self.size[0] / 2.0,
            self.size[1] / 2.0 - (p[1] - self.center[1]) * self.scale,
        ]
    }

    pub fn to_world(&self, q: [f64; 2]) -> P2 {
        [
            self.center[0] + (q[0] - self.size[0] / 2.0) / self.scale,
            self.center[1] - (q[1] - self.size[1] / 2.0) / self.scale,
        ]
    }

    /// Moves the drawing by a screen offset in pixels (drag direction).
    pub fn pan(&mut self, dx: f64, dy: f64) {
        self.center[0] -= dx / self.scale;
        self.center[1] += dy / self.scale;
    }

    /// Zooms by `factor` keeping the world point under `cursor` in place.
    pub fn zoom_at(&mut self, cursor: [f64; 2], factor: f64) {
        if !factor.is_finite() || factor <= 0.0 {
            return;
        }
        let before = self.to_world(cursor);
        self.scale = (self.scale * factor).clamp(MIN_SCALE, MAX_SCALE);
        let after = self.to_world(cursor);
        self.center[0] += before[0] - after[0];
        self.center[1] += before[1] - after[1];
    }

    /// Shows all of `bounds`, with a margin (fraction of the screen).
    pub fn fit(&mut self, bounds: &Bounds, margin: f64) {
        self.center = bounds.center();
        let size = bounds.size();
        let usable = 1.0 - 2.0 * margin.clamp(0.0, 0.45);
        let sx = if size[0] > 0.0 {
            self.size[0] * usable / size[0]
        } else {
            f64::INFINITY
        };
        let sy = if size[1] > 0.0 {
            self.size[1] * usable / size[1]
        } else {
            f64::INFINITY
        };
        let s = sx.min(sy);
        self.scale = if s.is_finite() {
            s.clamp(MIN_SCALE, MAX_SCALE)
        } else {
            1.0
        };
    }

    /// The world box currently on screen.
    pub fn world_bounds(&self) -> Bounds {
        let a = self.to_world([0.0, self.size[1]]);
        let b = self.to_world([self.size[0], 0.0]);
        Bounds { min: a, max: b }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: P2, b: P2) -> bool {
        (a[0] - b[0]).abs() < 1e-9 * (1.0 + a[0].abs())
            && (a[1] - b[1]).abs() < 1e-9 * (1.0 + a[1].abs())
    }

    #[test]
    fn screen_and_world_round_trip_with_y_flipped() {
        let mut v = View::new([800.0, 600.0]);
        v.center = [100.0, 50.0];
        v.scale = 2.0;
        assert!(close(v.to_screen([100.0, 50.0]), [400.0, 300.0]));
        assert!(
            close(v.to_screen([101.0, 51.0]), [402.0, 298.0]),
            "world Y up is screen Y down"
        );
        assert!(close(v.to_world(v.to_screen([7.0, -3.0])), [7.0, -3.0]));
    }

    #[test]
    fn zoom_keeps_the_point_under_the_cursor() {
        let mut v = View::new([800.0, 600.0]);
        v.center = [1e7, -2e7];
        v.scale = 0.37;
        let cursor = [123.0, 456.0];
        let under = v.to_world(cursor);
        for factor in [1.25, 0.8, 10.0, 0.01] {
            v.zoom_at(cursor, factor);
            assert!(close(v.to_world(cursor), under), "{factor}");
        }
        v.zoom_at(cursor, f64::NAN);
        assert!(v.scale.is_finite());
    }

    #[test]
    fn pan_moves_with_the_drag() {
        let mut v = View::new([800.0, 600.0]);
        let p = v.to_screen([5.0, 5.0]);
        v.pan(10.0, -20.0);
        let q = v.to_screen([5.0, 5.0]);
        assert!(close(q, [p[0] + 10.0, p[1] - 20.0]));
    }

    #[test]
    fn fit_shows_everything() {
        let mut v = View::new([800.0, 600.0]);
        let b = Bounds {
            min: [0.0, 0.0],
            max: [200.0, 50.0],
        };
        v.fit(&b, 0.05);
        let shown = v.world_bounds();
        assert!(
            shown.min[0] <= 0.0
                && shown.max[0] >= 200.0
                && shown.min[1] <= 0.0
                && shown.max[1] >= 50.0
        );
        assert!(close(v.center, [100.0, 25.0]));
        // A single point (zero size) does not divide by zero.
        v.fit(&Bounds::point([3.0, 4.0]), 0.05);
        assert!(v.scale.is_finite() && close(v.center, [3.0, 4.0]));
    }
}
