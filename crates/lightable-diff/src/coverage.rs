//! Geometric equivalence: whether what an entity draws is already drawn by
//! other entities. A polyline saved as separate lines and arcs by another
//! program draws the same thing; that is a change of representation, not
//! of the drawing.

use std::collections::HashMap;

use lightable_geom::bulge::arc_from_bulge;
use lightable_render::{Item, P2, Shape};

/// A drawn piece: a segment, an arc or a point.
#[derive(Debug, Clone, Copy)]
pub enum Piece {
    Line(P2, P2),
    Arc {
        center: P2,
        radius: f64,
        start: f64,
        sweep: f64,
    },
    Point(P2),
}

/// The pieces drawn by `items` (text is not geometry and is left out).
pub fn pieces(items: &[&Item]) -> Vec<Piece> {
    let mut out = Vec::new();
    for item in items {
        match &item.shape {
            Shape::Path {
                points,
                bulges,
                closed,
            } => {
                let n = points.len();
                if n == 1 {
                    out.push(Piece::Point(points[0]));
                    continue;
                }
                let segments = if *closed { n } else { n.saturating_sub(1) };
                for i in 0..segments {
                    let (a, b) = (points[i], points[(i + 1) % n]);
                    match arc_from_bulge(a, b, bulges.get(i).copied().unwrap_or(0.0)) {
                        Some(arc) => out.push(Piece::Arc {
                            center: arc.center,
                            radius: arc.radius,
                            start: arc.start_angle,
                            sweep: arc.sweep,
                        }),
                        None => out.push(Piece::Line(a, b)),
                    }
                }
            }
            Shape::Fill { points } => {
                for i in 0..points.len() {
                    out.push(Piece::Line(points[i], points[(i + 1) % points.len()]));
                }
            }
            Shape::Point(p) => out.push(Piece::Point(*p)),
            // Text and filled areas are not lines and arcs.
            Shape::Text(_) | Shape::Region { .. } => {}
        }
    }
    out
}

impl Piece {
    fn bounds(&self) -> (P2, P2) {
        match *self {
            Piece::Line(a, b) => (
                [a[0].min(b[0]), a[1].min(b[1])],
                [a[0].max(b[0]), a[1].max(b[1])],
            ),
            Piece::Arc { center, radius, .. } => (
                [center[0] - radius, center[1] - radius],
                [center[0] + radius, center[1] + radius],
            ),
            Piece::Point(p) => (p, p),
        }
    }

    /// Distance from `p` to the piece.
    fn distance(&self, p: P2) -> f64 {
        match *self {
            Piece::Line(a, b) => {
                let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
                let len2 = dx * dx + dy * dy;
                let t = if len2 == 0.0 {
                    0.0
                } else {
                    (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / len2).clamp(0.0, 1.0)
                };
                (p[0] - a[0] - t * dx).hypot(p[1] - a[1] - t * dy)
            }
            Piece::Arc {
                center,
                radius,
                start,
                sweep,
            } => {
                let ang = (p[1] - center[1]).atan2(p[0] - center[0]);
                if sweep.abs() >= TAU || normalised(ang - start, sweep) <= sweep.abs() {
                    ((p[0] - center[0]).hypot(p[1] - center[1]) - radius).abs()
                } else {
                    let end = start + sweep;
                    let e0 = [
                        center[0] + radius * start.cos(),
                        center[1] + radius * start.sin(),
                    ];
                    let e1 = [
                        center[0] + radius * end.cos(),
                        center[1] + radius * end.sin(),
                    ];
                    (p[0] - e0[0])
                        .hypot(p[1] - e0[1])
                        .min((p[0] - e1[0]).hypot(p[1] - e1[1]))
                }
            }
            Piece::Point(q) => (p[0] - q[0]).hypot(p[1] - q[1]),
        }
    }

    /// The part of `self` that `other` covers within `tol`, as an interval
    /// of `self`'s parameter (0..1 along a line, 0..|sweep| radians along
    /// an arc), or `None`.
    fn covered_by(&self, other: &Piece, tol: f64) -> Option<(f64, f64)> {
        match (*self, *other) {
            (Piece::Line(a, b), Piece::Line(c, d)) => {
                let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
                let len = dx.hypot(dy);
                if len == 0.0 {
                    return None;
                }
                // `other` must lie on this line.
                let off = |p: P2| ((p[0] - a[0]) * dy - (p[1] - a[1]) * dx).abs() / len;
                if off(c) > tol || off(d) > tol {
                    return None;
                }
                let t = |p: P2| ((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / (len * len);
                let (t0, t1) = (t(c).min(t(d)), t(c).max(t(d)));
                Some((t0.max(0.0), t1.min(1.0))).filter(|(x, y)| x <= y)
            }
            (
                Piece::Arc {
                    center,
                    radius,
                    start,
                    sweep,
                },
                Piece::Arc {
                    center: c2,
                    radius: r2,
                    start: s2,
                    sweep: w2,
                },
            ) => {
                if (center[0] - c2[0]).hypot(center[1] - c2[1]) > tol || (radius - r2).abs() > tol {
                    return None;
                }
                if w2.abs() >= TAU - 1e-12 {
                    return Some((0.0, sweep.abs()));
                }
                // The other arc as a counter-clockwise span.
                let (o_start, o_len) = if w2 >= 0.0 { (s2, w2) } else { (s2 + w2, -w2) };
                // Its start measured along this arc's direction.
                let from = if sweep >= 0.0 {
                    normalised(o_start - start, 1.0)
                } else {
                    normalised(start - (o_start + o_len), 1.0)
                };
                Some((from, from + o_len))
            }
            _ => None,
        }
    }
}

const TAU: f64 = std::f64::consts::TAU;

/// An angle difference in 0..2π, measured counter-clockwise when
/// `direction` is positive and clockwise otherwise.
fn normalised(d: f64, direction: f64) -> f64 {
    if direction >= 0.0 {
        d.rem_euclid(TAU)
    } else {
        (-d).rem_euclid(TAU)
    }
}

/// True if the intervals cover 0..`length` with no gap wider than `gap`.
/// Intervals may wrap around `period` (for arcs) when `period` is given.
fn intervals_cover(mut spans: Vec<(f64, f64)>, length: f64, gap: f64, period: Option<f64>) -> bool {
    if let Some(p) = period {
        // Unwrap spans that start before 0 or run past one turn.
        let mut extra = Vec::new();
        for s in &spans {
            if s.1 > p {
                extra.push((s.0 - p, s.1 - p));
            }
        }
        spans.extend(extra);
    }
    spans.sort_by(|x, y| x.0.total_cmp(&y.0));
    let mut reached = 0.0;
    for (a, b) in spans {
        if a > reached + gap {
            break;
        }
        reached = f64::max(reached, b);
    }
    reached + gap >= length
}

/// Pieces indexed on a grid for nearest-piece queries.
pub struct Index {
    pieces: Vec<Piece>,
    cell: f64,
    grid: HashMap<(i64, i64), Vec<usize>>,
}

impl Index {
    pub fn new(pieces: Vec<Piece>, cell: f64) -> Self {
        let cell = cell.max(1e-12);
        let mut grid: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
        for (i, p) in pieces.iter().enumerate() {
            let (lo, hi) = p.bounds();
            let (x0, y0) = ((lo[0] / cell).floor() as i64, (lo[1] / cell).floor() as i64);
            let (x1, y1) = ((hi[0] / cell).floor() as i64, (hi[1] / cell).floor() as i64);
            // Very large pieces would fill too many cells; put them in one
            // shared list checked for every query.
            if (x1 - x0 + 1) * (y1 - y0 + 1) > 4096 {
                grid.entry((i64::MAX, i64::MAX)).or_default().push(i);
                continue;
            }
            for x in x0..=x1 {
                for y in y0..=y1 {
                    grid.entry((x, y)).or_default().push(i);
                }
            }
        }
        Self { pieces, cell, grid }
    }

    /// Pieces whose grid cells meet the box `lo..hi` grown by `tol`.
    fn nearby(&self, lo: P2, hi: P2, tol: f64) -> Vec<usize> {
        let cell = |v: f64| (v / self.cell).floor() as i64;
        let (x0, y0, x1, y1) = (
            cell(lo[0] - tol),
            cell(lo[1] - tol),
            cell(hi[0] + tol),
            cell(hi[1] + tol),
        );
        let mut out: Vec<usize> = self
            .grid
            .get(&(i64::MAX, i64::MAX))
            .cloned()
            .unwrap_or_default();
        if (x1 - x0 + 1) * (y1 - y0 + 1) > 65536 {
            return (0..self.pieces.len()).collect();
        }
        for x in x0..=x1 {
            for y in y0..=y1 {
                if let Some(list) = self.grid.get(&(x, y)) {
                    out.extend(list);
                }
            }
        }
        out.sort_unstable();
        out.dedup();
        out
    }

    /// True if `piece` is drawn, within `tol`, by this index's pieces.
    fn covers_piece(&self, piece: &Piece, tol: f64) -> bool {
        let (lo, hi) = piece.bounds();
        let near = self.nearby(lo, hi, tol);
        match *piece {
            Piece::Point(p) => near.iter().any(|&i| self.pieces[i].distance(p) <= tol),
            Piece::Line(a, b) => {
                let len = (b[0] - a[0]).hypot(b[1] - a[1]);
                if len <= tol {
                    return near.iter().any(|&i| self.pieces[i].distance(a) <= tol);
                }
                let spans = near
                    .iter()
                    .filter_map(|&i| piece.covered_by(&self.pieces[i], tol))
                    .collect();
                intervals_cover(spans, 1.0, tol / len, None)
            }
            Piece::Arc { radius, sweep, .. } => {
                let length = sweep.abs().min(TAU);
                if radius * length <= tol {
                    return true;
                }
                let spans = near
                    .iter()
                    .filter_map(|&i| piece.covered_by(&self.pieces[i], tol))
                    .collect();
                intervals_cover(spans, length, tol / radius.max(tol), Some(TAU))
            }
        }
    }

    /// True if all of `pieces` are drawn by this index's pieces (and there
    /// is something to cover).
    pub fn covers(&self, pieces: &[Piece], tol: f64) -> bool {
        !pieces.is_empty() && pieces.iter().all(|p| self.covers_piece(p, tol))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arc_piece(center: P2, radius: f64, start_deg: f64, sweep_deg: f64) -> Piece {
        Piece::Arc {
            center,
            radius,
            start: start_deg.to_radians(),
            sweep: sweep_deg.to_radians(),
        }
    }

    #[test]
    fn lines_cover_a_polyline_made_of_the_same_segments() {
        let polyline = vec![
            Piece::Line([0.0, 0.0], [10.0, 0.0]),
            Piece::Line([10.0, 0.0], [10.0, 5.0]),
        ];
        let lines = Index::new(
            vec![
                Piece::Line([10.0, 5.0], [10.0, 0.0]),
                Piece::Line([0.0, 0.0], [10.0, 0.0]),
            ],
            1.0,
        );
        assert!(lines.covers(&polyline, 1e-9));
        // A gap is not covered.
        let short = Index::new(
            vec![
                Piece::Line([0.0, 0.0], [9.0, 0.0]),
                Piece::Line([10.0, 0.0], [10.0, 5.0]),
            ],
            1.0,
        );
        assert!(!short.covers(&polyline, 1e-9));
    }

    #[test]
    fn arcs_split_differently_cover_each_other() {
        // A full circle as two halves vs as one 360° arc starting elsewhere.
        let halves = vec![
            arc_piece([0.0, 0.0], 2.0, 0.0, 180.0),
            arc_piece([0.0, 0.0], 2.0, 180.0, 180.0),
        ];
        let whole = Index::new(vec![arc_piece([0.0, 0.0], 2.0, 37.0, 360.0)], 0.5);
        assert!(whole.covers(&halves, 1e-9));
        // Clockwise and counter-clockwise arcs over the same span.
        let cw = Index::new(vec![arc_piece([0.0, 0.0], 1.0, 90.0, -90.0)], 0.5);
        assert!(cw.covers(&[arc_piece([0.0, 0.0], 1.0, 0.0, 90.0)], 1e-9));
        assert!(!cw.covers(&[arc_piece([0.0, 0.0], 1.0, 0.0, 180.0)], 1e-9));
        // A slightly larger radius is not the same arc.
        assert!(!cw.covers(&[arc_piece([0.0, 0.0], 1.01, 0.0, 90.0)], 1e-6));
    }

    #[test]
    fn nothing_covers_nothing() {
        assert!(!Index::new(Vec::new(), 1.0).covers(&[], 1.0));
    }
}
