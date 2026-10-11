//! Scene → vertex data for a GPU, in chunks.
//!
//! Items are grouped by layer and by cell of a grid over the scene, so the
//! app can hide a layer or skip off-screen cells without touching vertex
//! data. Each chunk's vertices are `f32` offsets from the chunk's centre;
//! the app subtracts the view centre from that centre in `f64` before
//! drawing, so precision does not depend on how far the drawing is from the
//! origin.

use std::collections::BTreeMap;

use lightable_geom::bulge::arc_from_bulge;

use crate::scene::*;

/// Largest angle per segment when arcs become lines (fine detail).
const ARC_STEP: f64 = std::f64::consts::PI / 36.0;
/// The same for the coarse level of detail, used while arcs are small on
/// screen: 30° segments on a 30 px radius are within about 1 px.
const COARSE_ARC_STEP: f64 = std::f64::consts::PI / 6.0;
/// Use the coarse detail while the chunk's largest arc radius is under
/// this many pixels.
pub const COARSE_BELOW_PX: f64 = 30.0;
/// Patterns finer than this many repeats per path are drawn solid.
const MAX_DASHES: f64 = 20_000.0;

/// Vertex data for one (layer, cell).
#[derive(Debug, Clone, Default)]
pub struct Chunk {
    pub layer: u32,
    /// World position the vertex offsets are relative to.
    pub origin: P2,
    pub bounds: Option<Bounds>,
    /// Line segments: pairs of vertices.
    pub lines: Vec<[f32; 2]>,
    pub line_colors: Vec<[u8; 4]>,
    /// The same lines with arcs in coarse segments (empty when the chunk
    /// has no arcs: then `lines` serve both).
    pub coarse_lines: Vec<[f32; 2]>,
    pub coarse_line_colors: Vec<[u8; 4]>,
    /// The largest arc radius in the chunk (world units), 0 if none.
    pub max_radius: f64,
    /// Triangles: triples of vertices.
    pub triangles: Vec<[f32; 2]>,
    pub triangle_colors: Vec<[u8; 4]>,
    pub points: Vec<[f32; 2]>,
    pub point_colors: Vec<[u8; 4]>,
    /// Indexes of text items (drawn by the UI's text renderer).
    pub texts: Vec<u32>,
}

#[derive(Debug, Clone, Default)]
pub struct Geometry {
    pub chunks: Vec<Chunk>,
}

impl Geometry {
    pub fn vertex_count(&self) -> usize {
        self.chunks
            .iter()
            .map(|c| c.lines.len() + c.triangles.len() + c.points.len())
            .sum()
    }
}

/// Splits a path into straight pieces: arcs become segments of at most 5°.
pub fn flatten(points: &[P2], bulges: &[f64], closed: bool) -> Vec<P2> {
    flatten_with(points, bulges, closed, ARC_STEP)
}

/// Like [`flatten`] with a given largest angle per segment.
pub fn flatten_with(points: &[P2], bulges: &[f64], closed: bool, step: f64) -> Vec<P2> {
    let n = points.len();
    if n == 0 {
        return Vec::new();
    }
    let segments = if closed { n } else { n - 1 };
    let mut out = vec![points[0]];
    for i in 0..segments {
        let (a, b) = (points[i], points[(i + 1) % n]);
        let bulge = bulges.get(i).copied().unwrap_or(0.0);
        if let Some(arc) = arc_from_bulge(a, b, bulge) {
            let steps = ((arc.sweep.abs() / step).ceil() as usize).max(2);
            for k in 1..steps {
                let ang = arc.start_angle + arc.sweep * k as f64 / steps as f64;
                out.push([
                    arc.center[0] + arc.radius * ang.cos(),
                    arc.center[1] + arc.radius * ang.sin(),
                ]);
            }
        }
        out.push(b);
    }
    out
}

/// Line segments of a polyline with a dash pattern applied (world units:
/// dash > 0, gap < 0, dot = 0). Patterns too fine to see are drawn solid.
pub fn dashed(line: &[P2], pattern: &[f64]) -> Vec<(P2, P2)> {
    let solid = || line.windows(2).map(|w| (w[0], w[1])).collect();
    let period: f64 = pattern.iter().map(|l| l.abs()).sum();
    let length: f64 = line
        .windows(2)
        .map(|w| (w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1]))
        .sum();
    if pattern.is_empty() || period <= 0.0 || length / period > MAX_DASHES {
        return solid();
    }
    let dot = period * 0.01;
    let element = |i: usize| {
        let l = pattern[i % pattern.len()];
        if l == 0.0 {
            (true, dot)
        } else {
            (l > 0.0, l.abs())
        }
    };
    let mut out = Vec::new();
    let mut index = 0;
    let (mut on, mut left) = element(0);
    for w in line.windows(2) {
        let (a, b) = (w[0], w[1]);
        let seg = (b[0] - a[0]).hypot(b[1] - a[1]);
        let mut pos = 0.0;
        while pos < seg {
            let step = left.min(seg - pos);
            if on && step > 0.0 {
                let t0 = pos / seg;
                let t1 = (pos + step) / seg;
                let at = |t: f64| [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
                out.push((at(t0), at(t1)));
            }
            pos += step;
            left -= step;
            if left <= 1e-12 {
                index += 1;
                (on, left) = element(index);
            }
        }
    }
    out
}

fn rgba(c: Rgb) -> [u8; 4] {
    [c.0, c.1, c.2, 255]
}

/// Builds chunked vertex data on a `grid` × `grid` raster over the scene.
pub fn tessellate(scene: &Scene, grid: usize) -> Geometry {
    let grid = grid.max(1);
    let Some(all) = scene.bounds(None) else {
        return Geometry::default();
    };
    let size = all.size();
    let cell = [
        size[0].max(1e-9) / grid as f64,
        size[1].max(1e-9) / grid as f64,
    ];
    let mut chunks: BTreeMap<(u32, usize, usize), Chunk> = BTreeMap::new();
    for (index, item) in scene.items.iter().enumerate() {
        let Some(b) = item.shape.bounds() else {
            continue;
        };
        let c = b.center();
        let gx = (((c[0] - all.min[0]) / cell[0]) as usize).min(grid - 1);
        let gy = (((c[1] - all.min[1]) / cell[1]) as usize).min(grid - 1);
        let chunk = chunks.entry((item.layer, gx, gy)).or_insert_with(|| Chunk {
            layer: item.layer,
            origin: [
                all.min[0] + (gx as f64 + 0.5) * cell[0],
                all.min[1] + (gy as f64 + 0.5) * cell[1],
            ],
            ..Chunk::default()
        });
        match &mut chunk.bounds {
            Some(cb) => cb.union(&b),
            None => chunk.bounds = Some(b),
        }
        let o = chunk.origin;
        let local = |p: P2| [(p[0] - o[0]) as f32, (p[1] - o[1]) as f32];
        let color = rgba(item.color);
        match &item.shape {
            Shape::Path {
                points,
                bulges,
                closed,
            } => {
                let segments = |step: f64| -> Vec<(P2, P2)> {
                    let line = flatten_with(points, bulges, *closed, step);
                    match item.dash {
                        Some(d) => dashed(&line, &scene.dashes[d as usize]),
                        None => line.windows(2).map(|w| (w[0], w[1])).collect(),
                    }
                };
                let fine = segments(ARC_STEP);
                let has_arcs = bulges.iter().any(|b| *b != 0.0);
                let coarse = if has_arcs {
                    segments(COARSE_ARC_STEP)
                } else {
                    fine.clone()
                };
                if has_arcs {
                    let n = points.len();
                    for (i, b) in bulges.iter().enumerate() {
                        if let Some(arc) = lightable_geom::bulge::arc_from_bulge(
                            points[i],
                            points[(i + 1) % n],
                            *b,
                        ) {
                            chunk.max_radius = chunk.max_radius.max(arc.radius);
                        }
                    }
                }
                for (a, b) in fine {
                    chunk.lines.push(local(a));
                    chunk.lines.push(local(b));
                    chunk.line_colors.extend([color, color]);
                }
                for (a, b) in coarse {
                    chunk.coarse_lines.push(local(a));
                    chunk.coarse_lines.push(local(b));
                    chunk.coarse_line_colors.extend([color, color]);
                }
            }
            Shape::Fill { points } => {
                for i in 1..points.len().saturating_sub(1) {
                    for p in [points[0], points[i], points[i + 1]] {
                        chunk.triangles.push(local(p));
                        chunk.triangle_colors.push(color);
                    }
                }
            }
            Shape::Region { loops } => {
                for t in lightable_geom::fill::even_odd_triangles(loops) {
                    for p in t {
                        chunk.triangles.push(local(p));
                        chunk.triangle_colors.push(color);
                    }
                }
            }
            Shape::Point(p) => {
                chunk.points.push(local(*p));
                chunk.point_colors.push(color);
            }
            Shape::Text(_) => chunk.texts.push(index as u32),
        }
    }
    let mut chunks: Vec<Chunk> = chunks.into_values().collect();
    for c in &mut chunks {
        if c.max_radius == 0.0 {
            c.coarse_lines = Vec::new();
            c.coarse_line_colors = Vec::new();
        }
    }
    Geometry { chunks }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(layer: u32, shape: Shape) -> Item {
        Item {
            layer,
            color: Rgb(255, 0, 0),
            dash: None,
            shape,
            entity: 0,
        }
    }

    #[test]
    fn arcs_are_flattened_to_short_segments() {
        let pts = flatten(&[[1.0, 0.0], [-1.0, 0.0]], &[1.0, 1.0], true);
        assert_eq!(pts.len(), 73, "36 segments per half circle, closed");
        assert!(pts.iter().all(|p| (p[0].hypot(p[1]) - 1.0).abs() < 1e-12));
    }

    #[test]
    fn dashes_follow_the_pattern_across_corners() {
        // 1 on, 1 off along an L of length 2 + 2.
        let segs = dashed(&[[0.0, 0.0], [2.0, 0.0], [2.0, 2.0]], &[1.0, -1.0]);
        assert_eq!(
            segs,
            vec![([0.0, 0.0], [1.0, 0.0]), ([2.0, 0.0], [2.0, 1.0])]
        );
        // A dash running over the corner is split at it.
        let segs = dashed(&[[0.0, 0.0], [1.5, 0.0], [1.5, 1.5]], &[2.0, -1.0]);
        assert_eq!(
            segs[..2],
            [([0.0, 0.0], [1.5, 0.0]), ([1.5, 0.0], [1.5, 0.5])]
        );
    }

    #[test]
    fn chunks_with_arcs_have_a_coarse_version() {
        let scene = Scene {
            items: vec![
                item(
                    0,
                    Shape::Path {
                        points: vec![[1.0, 0.0], [-1.0, 0.0]],
                        bulges: vec![1.0, 1.0],
                        closed: true,
                    },
                ),
                item(
                    1,
                    Shape::Path {
                        points: vec![[0.0, 0.0], [1.0, 0.0]],
                        bulges: vec![],
                        closed: false,
                    },
                ),
            ],
            ..Scene::default()
        };
        let g = tessellate(&scene, 1);
        let circle = &g.chunks[0];
        assert_eq!(circle.lines.len(), 72 * 2);
        assert_eq!(circle.coarse_lines.len(), 12 * 2);
        assert_eq!(circle.max_radius, 1.0);
        let line = &g.chunks[1];
        assert!(line.coarse_lines.is_empty() && line.max_radius == 0.0);
    }

    #[test]
    fn invisible_fine_patterns_are_solid() {
        let segs = dashed(&[[0.0, 0.0], [1e6, 0.0]], &[1.0, -1.0]);
        assert_eq!(segs.len(), 1);
    }

    #[test]
    fn chunks_split_by_layer_and_cell_with_local_coordinates() {
        // Two lines far from the origin in different corners, on two layers.
        let far = 1e9;
        let scene = Scene {
            items: vec![
                item(
                    0,
                    Shape::Path {
                        points: vec![[far, far], [far + 1.0, far]],
                        bulges: vec![],
                        closed: false,
                    },
                ),
                item(
                    0,
                    Shape::Path {
                        points: vec![[far + 100.0, far + 100.0], [far + 101.0, far + 100.0]],
                        bulges: vec![],
                        closed: false,
                    },
                ),
                item(1, Shape::Point([far, far])),
            ],
            ..Scene::default()
        };
        let g = tessellate(&scene, 4);
        assert_eq!(g.chunks.len(), 3);
        // Offsets from each chunk's origin are small, so f32 keeps the
        // 1-unit line exact even at 1e9.
        let c = &g.chunks[0];
        let len = c.lines[1][0] - c.lines[0][0];
        assert_eq!(len, 1.0);
        assert!(
            c.lines
                .iter()
                .all(|v| v[0].abs() < 100.0 && v[1].abs() < 100.0)
        );
    }

    #[test]
    fn fills_become_triangles_and_texts_are_indexed() {
        let scene = Scene {
            items: vec![
                item(
                    0,
                    Shape::Fill {
                        points: vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
                    },
                ),
                item(
                    0,
                    Shape::Text(TextShape {
                        at: [0.5, 0.5],
                        height: 0.1,
                        rotation: 0.0,
                        width_factor: 1.0,
                        oblique: 0.0,
                        text: "x".into(),
                        halign: HAlign::Left,
                        valign: VAlign::Baseline,
                    }),
                ),
            ],
            ..Scene::default()
        };
        let g = tessellate(&scene, 1);
        assert_eq!(g.chunks[0].triangles.len(), 6);
        assert_eq!(g.chunks[0].texts, vec![1]);
    }
}
