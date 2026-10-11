//! The scene: what to draw, already resolved (layers, colours, linetypes,
//! blocks), in world XY coordinates.

use std::collections::BTreeMap;

pub type P2 = [f64; 2];

/// An sRGB colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    pub fn hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.0, self.1, self.2)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LayerInfo {
    pub name: String,
    pub color: Rgb,
    /// On and not frozen in the file. The app keeps its own on/off state,
    /// starting from this.
    pub visible: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HAlign {
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VAlign {
    Baseline,
    Bottom,
    Middle,
    Top,
}

/// One line of text.
#[derive(Debug, Clone, PartialEq)]
pub struct TextShape {
    /// The alignment point.
    pub at: P2,
    pub height: f64,
    /// Radians, counter-clockwise from world X.
    pub rotation: f64,
    pub width_factor: f64,
    /// Radians.
    pub oblique: f64,
    pub text: String,
    pub halign: HAlign,
    pub valign: VAlign,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Shape {
    /// A polyline. `bulges[i]` bends the segment from point `i` to the next
    /// (see `lightable_geom::bulge`); empty means all straight.
    Path {
        points: Vec<P2>,
        bulges: Vec<f64>,
        closed: bool,
    },
    /// A filled polygon (SOLID, TRACE).
    Fill {
        points: Vec<P2>,
    },
    Point(P2),
    Text(TextShape),
    /// A filled region: the even-odd combination of closed loops (hatch
    /// fills). Drawn under all lines.
    Region {
        loops: Vec<Vec<P2>>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    /// Index into [`Scene::layers`]: hiding that layer hides the item.
    pub layer: u32,
    pub color: Rgb,
    /// Index into [`Scene::dashes`], or `None` for a continuous line.
    pub dash: Option<u32>,
    pub shape: Shape,
    /// The entity it was drawn from: an index into the entity list the
    /// scene was built from (block contents count as their reference's).
    pub entity: u32,
}

/// An axis-aligned box.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bounds {
    pub min: P2,
    pub max: P2,
}

impl Bounds {
    pub fn point(p: P2) -> Self {
        Self { min: p, max: p }
    }

    pub fn add(&mut self, p: P2) {
        self.min = [self.min[0].min(p[0]), self.min[1].min(p[1])];
        self.max = [self.max[0].max(p[0]), self.max[1].max(p[1])];
    }

    pub fn union(&mut self, other: &Bounds) {
        self.add(other.min);
        self.add(other.max);
    }

    pub fn center(&self) -> P2 {
        [
            (self.min[0] + self.max[0]) / 2.0,
            (self.min[1] + self.max[1]) / 2.0,
        ]
    }

    pub fn size(&self) -> P2 {
        [self.max[0] - self.min[0], self.max[1] - self.min[1]]
    }

    pub fn intersects(&self, other: &Bounds) -> bool {
        self.min[0] <= other.max[0]
            && other.min[0] <= self.max[0]
            && self.min[1] <= other.max[1]
            && other.min[1] <= self.max[1]
    }
}

impl Shape {
    /// The box around the shape. Arcs include their mid-points, text an
    /// estimate of its extent (0.6 × height per character).
    pub fn bounds(&self) -> Option<Bounds> {
        match self {
            Shape::Path {
                points,
                bulges,
                closed,
            } => {
                let mut b = Bounds::point(*points.first()?);
                for (i, p) in points.iter().enumerate() {
                    b.add(*p);
                    let next = match points.get(i + 1) {
                        Some(n) => *n,
                        None if *closed => points[0],
                        None => break,
                    };
                    let bulge = bulges.get(i).copied().unwrap_or(0.0);
                    if bulge != 0.0
                        && let Some(arc) = lightable_geom::bulge::arc_from_bulge(*p, next, bulge)
                    {
                        // The arc's extreme points at 0°, 90°, 180°, 270° that it passes.
                        for k in 0..4 {
                            let a = f64::from(k) * std::f64::consts::FRAC_PI_2;
                            if angle_in_sweep(a, arc.start_angle, arc.sweep) {
                                b.add([
                                    arc.center[0] + arc.radius * a.cos(),
                                    arc.center[1] + arc.radius * a.sin(),
                                ]);
                            }
                        }
                    }
                }
                Some(b)
            }
            Shape::Fill { points } => {
                let mut b = Bounds::point(*points.first()?);
                points.iter().for_each(|p| b.add(*p));
                Some(b)
            }
            Shape::Point(p) => Some(Bounds::point(*p)),
            Shape::Region { loops } => {
                let mut points = loops.iter().flatten();
                let mut b = Bounds::point(*points.next()?);
                points.for_each(|p| b.add(*p));
                Some(b)
            }
            Shape::Text(t) => {
                let width = t.height * 0.6 * t.width_factor * t.text.chars().count() as f64;
                let (s, c) = t.rotation.sin_cos();
                let mut b = Bounds::point(t.at);
                // Symmetric around the anchor: alignment can put the text on
                // any side of it.
                for (dx, dy) in [
                    (width, t.height),
                    (width, -t.height),
                    (-width, t.height),
                    (-width, -t.height),
                ] {
                    b.add([t.at[0] + dx * c - dy * s, t.at[1] + dx * s + dy * c]);
                }
                Some(b)
            }
        }
    }
}

/// True if angle `a` lies on the sweep from `start` (signed, radians).
pub(crate) fn angle_in_sweep(a: f64, start: f64, sweep: f64) -> bool {
    let tau = std::f64::consts::TAU;
    let rel = if sweep >= 0.0 {
        (a - start).rem_euclid(tau)
    } else {
        (start - a).rem_euclid(tau)
    };
    rel <= sweep.abs()
}

/// Everything to draw.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Scene {
    pub layers: Vec<LayerInfo>,
    /// Dash patterns in world units: positive = dash, negative = gap, 0 = dot.
    pub dashes: Vec<Vec<f64>>,
    pub items: Vec<Item>,
    /// Entity types that were not drawn, with counts.
    pub skipped: BTreeMap<String, usize>,
}

impl Scene {
    /// The box around the items on visible layers (`visible[layer]`), or
    /// around everything when `visible` is `None`.
    pub fn bounds(&self, visible: Option<&[bool]>) -> Option<Bounds> {
        let mut out: Option<Bounds> = None;
        for item in &self.items {
            if let Some(v) = visible
                && !v.get(item.layer as usize).copied().unwrap_or(true)
            {
                continue;
            }
            if let Some(b) = item.shape.bounds() {
                match &mut out {
                    Some(o) => o.union(&b),
                    None => out = Some(b),
                }
            }
        }
        out
    }

    /// The initial layer visibility (on and not frozen in the file).
    pub fn default_visibility(&self) -> Vec<bool> {
        self.layers.iter().map(|l| l.visible).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arc_bounds_include_extremes() {
        // A semicircle from (0,0) to (2,0) bulging down reaches y = -1.
        let path = Shape::Path {
            points: vec![[0.0, 0.0], [2.0, 0.0]],
            bulges: vec![1.0],
            closed: false,
        };
        let b = path.bounds().unwrap();
        assert!((b.min[1] + 1.0).abs() < 1e-12, "{b:?}");
        assert!(b.max[1].abs() < 1e-12);
        // A full circle as two bulges covers the whole disc's box.
        let circle = Shape::Path {
            points: vec![[1.0, 0.0], [-1.0, 0.0]],
            bulges: vec![1.0, 1.0],
            closed: true,
        };
        let b = circle.bounds().unwrap();
        assert!(
            (b.min[0] + 1.0).abs() < 1e-12
                && (b.max[1] - 1.0).abs() < 1e-12
                && (b.min[1] + 1.0).abs() < 1e-12
        );
    }

    #[test]
    fn sweeps() {
        assert!(angle_in_sweep(0.0, -0.1, 0.2));
        assert!(!angle_in_sweep(1.0, -0.1, 0.2));
        assert!(angle_in_sweep(3.0, 3.1, -0.2));
    }
}
