//! A read-only typed view of `HATCH` entities.
//!
//! Hatches stay [`Unknown`] in the model, so their raw pairs (and anything
//! this view does not read) round-trip unchanged; [`Unknown::hatch`] reads
//! the fill style and boundary paths from those pairs.

use crate::Pair;
use crate::entity::Unknown;
use crate::model::Point3;
use crate::value::parse_f64;

/// The parts of a hatch needed to draw its boundary and fill.
#[derive(Debug, Clone, PartialEq)]
pub struct Hatch {
    /// Group 2.
    pub pattern: String,
    /// Group 70: solid fill rather than a pattern.
    pub solid: bool,
    /// Z of the hatch plane in its OCS (group 30 of the elevation point).
    pub elevation: f64,
    /// Groups 210/220/230.
    pub extrusion: Point3,
    /// Boundary paths in OCS coordinates; the filled region is the
    /// even-odd combination of all of them.
    pub paths: Vec<BoundaryPath>,
}

/// One boundary loop of a hatch.
#[derive(Debug, Clone, PartialEq)]
pub enum BoundaryPath {
    /// Polyline path (path flag 2): points with optional bulges.
    Polyline {
        /// Vertices (groups 10/20).
        points: Vec<[f64; 2]>,
        /// Bulge of the segment after each vertex (group 42); 0 for straight.
        bulges: Vec<f64>,
        /// Group 73. Hatch boundaries are treated as closed either way.
        closed: bool,
    },
    /// A loop of edges.
    Edges(Vec<Edge>),
}

/// One edge of an edge-defined boundary path.
#[derive(Debug, Clone, PartialEq)]
pub enum Edge {
    /// A straight edge.
    Line {
        /// Start point.
        start: [f64; 2],
        /// End point.
        end: [f64; 2],
    },
    /// Angles in degrees as written in the file. For clockwise arcs
    /// (`ccw == false`) the file stores the angles mirrored: the arc runs
    /// clockwise from `-start_angle` to `-end_angle`.
    Arc {
        /// Center.
        center: [f64; 2],
        /// Radius.
        radius: f64,
        /// Start angle in degrees (mirrored when clockwise).
        start_angle: f64,
        /// End angle in degrees (mirrored when clockwise).
        end_angle: f64,
        /// True for counter-clockwise (group 73).
        ccw: bool,
    },
    /// `major` is the end of the major axis relative to `center`; angles
    /// are ellipse parameters in degrees, mirrored like arcs when
    /// clockwise.
    Ellipse {
        /// Center.
        center: [f64; 2],
        /// End of the major axis, relative to the center.
        major: [f64; 2],
        /// Minor axis length as a fraction of the major axis.
        ratio: f64,
        /// Start parameter in degrees (mirrored when clockwise).
        start_angle: f64,
        /// End parameter in degrees (mirrored when clockwise).
        end_angle: f64,
        /// True for counter-clockwise (group 73).
        ccw: bool,
    },
    /// A NURBS edge.
    Spline {
        /// Degree (group 94).
        degree: usize,
        /// Knot values (group 40).
        knots: Vec<f64>,
        /// Control points (groups 10/20).
        control: Vec<[f64; 2]>,
        /// Weights (group 42); empty when all are 1.
        weights: Vec<f64>,
        /// Fit points (groups 11/21), if any.
        fit: Vec<[f64; 2]>,
    },
}

/// Reads pairs in order.
struct Cursor<'a> {
    pairs: &'a [Pair],
    at: usize,
}

impl Cursor<'_> {
    fn peek(&self) -> Option<i16> {
        self.pairs.get(self.at).map(|p| p.code)
    }

    /// The next pair if it has `code`.
    fn take(&mut self, code: i16) -> Option<&str> {
        let p = self.pairs.get(self.at)?;
        if p.code != code {
            return None;
        }
        self.at += 1;
        Some(p.raw.trim())
    }

    fn num(&mut self, code: i16) -> Option<f64> {
        self.take(code).and_then(parse_f64)
    }

    fn int(&mut self, code: i16) -> Option<i64> {
        self.num(code).map(|v| v as i64)
    }

    fn point(&mut self, x: i16) -> Option<[f64; 2]> {
        Some([self.num(x)?, self.num(x + 10)?])
    }

    /// Skips to the next pair with `code`, if any is before a pair in
    /// `stop`.
    fn seek(&mut self, code: i16, stop: &[i16]) -> bool {
        while let Some(c) = self.peek() {
            if c == code {
                return true;
            }
            if stop.contains(&c) {
                return false;
            }
            self.at += 1;
        }
        false
    }
}

/// At most this many items in any count, so a damaged file cannot ask for
/// huge allocations.
const MAX_COUNT: i64 = 1_000_000;

fn count(v: Option<i64>) -> Option<usize> {
    v.filter(|n| (0..=MAX_COUNT).contains(n))
        .map(|n| n as usize)
}

fn edge(c: &mut Cursor) -> Option<Edge> {
    Some(match c.int(72)? {
        1 => Edge::Line {
            start: c.point(10)?,
            end: c.point(11)?,
        },
        2 => Edge::Arc {
            center: c.point(10)?,
            radius: c.num(40)?,
            start_angle: c.num(50)?,
            end_angle: c.num(51)?,
            ccw: c.int(73).unwrap_or(1) != 0,
        },
        3 => Edge::Ellipse {
            center: c.point(10)?,
            major: c.point(11)?,
            ratio: c.num(40)?,
            start_angle: c.num(50)?,
            end_angle: c.num(51)?,
            ccw: c.int(73).unwrap_or(1) != 0,
        },
        4 => {
            let degree = count(c.int(94))?;
            let rational = c.int(73).unwrap_or(0) != 0;
            let _periodic = c.int(74);
            let knot_count = count(c.int(95))?;
            let control_count = count(c.int(96))?;
            let knots = (0..knot_count)
                .map(|_| c.num(40))
                .collect::<Option<Vec<_>>>()?;
            let mut control = Vec::with_capacity(control_count);
            let mut weights = Vec::new();
            for _ in 0..control_count {
                control.push(c.point(10)?);
                if rational || c.peek() == Some(42) {
                    weights.push(c.num(42).unwrap_or(1.0));
                }
            }
            let mut fit = Vec::new();
            if let Some(n) = count(c.int(97)) {
                for _ in 0..n {
                    fit.push(c.point(11)?);
                }
                // Optional start and end tangents.
                let _ = c.point(12);
                let _ = c.point(13);
            }
            Edge::Spline {
                degree,
                knots,
                control,
                weights,
                fit,
            }
        }
        _ => return None,
    })
}

fn path(c: &mut Cursor) -> Option<BoundaryPath> {
    let flags = c.int(92)?;
    let path = if flags & 2 != 0 {
        let has_bulge = c.int(72).unwrap_or(0) != 0;
        let closed = c.int(73).unwrap_or(1) != 0;
        let n = count(c.int(93))?;
        let mut points = Vec::with_capacity(n);
        let mut bulges = Vec::with_capacity(n);
        for _ in 0..n {
            points.push(c.point(10)?);
            bulges.push(if has_bulge || c.peek() == Some(42) {
                c.num(42).unwrap_or(0.0)
            } else {
                0.0
            });
        }
        BoundaryPath::Polyline {
            points,
            bulges,
            closed,
        }
    } else {
        let n = count(c.int(93))?;
        let edges = (0..n).map(|_| edge(c)).collect::<Option<Vec<_>>>()?;
        BoundaryPath::Edges(edges)
    };
    // Source boundary objects.
    if let Some(n) = count(c.int(97)) {
        for _ in 0..n {
            c.take(330);
        }
    }
    Some(path)
}

impl Unknown {
    /// The boundary and fill of a `HATCH`, or `None` for other types or
    /// when the boundary data cannot be read. Paths that cannot be read
    /// end the list (the ones before are kept).
    pub fn hatch(&self) -> Option<Hatch> {
        if !self.type_name.eq_ignore_ascii_case("HATCH") {
            return None;
        }
        let first = |code: i16| {
            self.pairs
                .iter()
                .find(|p| p.code == code)
                .map(|p| p.raw.trim())
        };
        let num = |code: i16| first(code).and_then(parse_f64);
        let mut c = Cursor {
            pairs: &self.pairs,
            at: 0,
        };
        let mut paths = Vec::new();
        if c.seek(91, &[]) {
            let n = count(c.int(91)).unwrap_or(0);
            for _ in 0..n {
                match path(&mut c) {
                    Some(p) => paths.push(p),
                    None => break,
                }
            }
        }
        Some(Hatch {
            pattern: first(2).unwrap_or("").to_owned(),
            solid: first(70).and_then(parse_f64) == Some(1.0),
            elevation: num(30).unwrap_or(0.0),
            extrusion: Point3::new(
                num(210).unwrap_or(0.0),
                num(220).unwrap_or(0.0),
                num(230).unwrap_or(1.0),
            ),
            paths,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// "code value code value ..." (values without spaces).
    fn unknown(text: &str) -> Unknown {
        let words: Vec<&str> = text.split_whitespace().collect();
        Unknown {
            type_name: "HATCH".into(),
            pairs: words
                .chunks(2)
                .map(|c| Pair::new(c[0].parse().unwrap(), c[1]))
                .collect(),
        }
    }

    #[test]
    fn polyline_and_edge_paths_are_read() {
        let u = unknown(
            "8 0
             100 AcDbHatch
             10 0 20 0 30 2.5
             210 0 220 0 230 1
             2 SOLID 70 1 71 0 91 2
             92 7 72 1 73 1 93 2 10 0 20 0 42 0.5 10 4 20 0 42 0 97 1 330 1F
             92 1 93 2
               72 1 10 0 20 0 11 1 21 0
               72 2 10 0 20 0 40 1 50 0 51 180 73 0
             97 0
             75 0 76 1 98 1 10 0 20 0",
        );
        let h = u.hatch().unwrap();
        assert!(h.solid);
        assert_eq!(h.pattern, "SOLID");
        assert_eq!(h.elevation, 2.5);
        assert_eq!(h.paths.len(), 2);
        assert_eq!(
            h.paths[0],
            BoundaryPath::Polyline {
                points: vec![[0.0, 0.0], [4.0, 0.0]],
                bulges: vec![0.5, 0.0],
                closed: true,
            }
        );
        let BoundaryPath::Edges(edges) = &h.paths[1] else {
            panic!()
        };
        assert_eq!(edges.len(), 2);
        assert!(matches!(edges[1], Edge::Arc { ccw: false, end_angle, .. } if end_angle == 180.0));
    }

    #[test]
    fn damaged_boundaries_are_dropped_without_panicking() {
        let pairs = vec![
            Pair::new(91, "3"),
            Pair::new(92, "2"),
            Pair::new(72, "0"),
            Pair::new(73, "1"),
            Pair::new(93, "999999999"),
        ];
        let h = Unknown {
            type_name: "hatch".into(),
            pairs,
        }
        .hatch()
        .unwrap();
        assert!(h.paths.is_empty());
        let other = Unknown {
            type_name: "LINE".into(),
            pairs: Vec::new(),
        };
        assert!(other.hatch().is_none());
    }
}
