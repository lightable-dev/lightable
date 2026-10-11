//! Drawing → Scene: resolves layers, colours and linetypes, expands block
//! references (nested, arrays) and converts OCS coordinates to world XY.

use std::collections::HashMap;

use lightable_dxf::{
    Color, Drawing, Entity, EntityKind, Insert, Linetype, Point3, Text, Unknown, names_equal,
};
use lightable_geom::bulge::arc_from_bulge;
use lightable_geom::curve::{ellipse_points, nurbs_points};
use lightable_geom::transform::Affine3;

use crate::scene::*;

/// Block nesting deeper than this is not drawn (it is almost certainly a
/// cycle the name check missed, or a pathological file).
const MAX_DEPTH: usize = 32;

const WHITE: Rgb = Rgb(255, 255, 255);

/// Builds the scene for a drawing's model space.
pub fn build(d: &Drawing) -> Scene {
    build_with(d, false)
}

/// Builds the scene for model space and, with `paper_space`, also for the
/// paper-space entities of the ENTITIES section. [`Item::entity`] is the
/// index in `d.entities`.
pub fn build_with(d: &Drawing, paper_space: bool) -> Scene {
    let mut b = Builder::new(d);
    let top = Ctx::top();
    for (i, e) in d.entities.iter().enumerate() {
        if paper_space || !e.paper_space {
            b.current = i as u32;
            b.entity(e, &top);
        }
    }
    b.scene
}

/// Builds the content of block `name` on its own (in block coordinates),
/// or `None` if there is no such block. [`Item::entity`] is the index in
/// the block's entity list.
pub fn build_block(d: &Drawing, name: &str) -> Option<Scene> {
    let block = d.blocks.get(name)?;
    let mut b = Builder::new(d);
    b.stack.push(block.name.clone());
    let top = Ctx::top();
    for (i, e) in block.entities.iter().enumerate() {
        b.current = i as u32;
        b.entity(e, &top);
    }
    Some(b.scene)
}

impl Ctx {
    fn top() -> Self {
        Ctx {
            transform: Affine3::IDENTITY,
            layer: None,
            color: WHITE,
            linetype: None,
            scale: 1.0,
        }
    }
}

impl<'d> Builder<'d> {
    fn new(d: &'d Drawing) -> Self {
        let mut b = Builder {
            d,
            scene: Scene::default(),
            layer_index: HashMap::new(),
            layer_linetype: Vec::new(),
            dash_index: HashMap::new(),
            ltscale: header_float(d, "$LTSCALE", 40).unwrap_or(1.0),
            stack: Vec::new(),
            current: 0,
        };
        for layer in d.tables.layers.iter() {
            b.layer(&layer.name);
        }
        b
    }
}

fn header_float(d: &Drawing, name: &str, code: i16) -> Option<f64> {
    d.header
        .get(name)?
        .pairs
        .iter()
        .find(|p| p.code == code)?
        .raw
        .trim()
        .parse()
        .ok()
}

/// What an entity inherits from the block reference it is drawn through.
#[derive(Clone)]
struct Ctx {
    transform: Affine3,
    /// The reference's layer, for entities on layer "0" inside a block.
    layer: Option<u32>,
    /// The reference's colour, for BYBLOCK.
    color: Rgb,
    /// The reference's linetype, for BYBLOCK.
    linetype: Option<u32>,
    /// Linear scale of `transform`, for linetype patterns.
    scale: f64,
}

struct Builder<'d> {
    d: &'d Drawing,
    scene: Scene,
    layer_index: HashMap<String, u32>,
    /// Each scene layer's linetype dash index.
    layer_linetype: Vec<Option<u32>>,
    dash_index: HashMap<Vec<u64>, u32>,
    ltscale: f64,
    /// Names of the blocks being expanded, to stop cycles.
    stack: Vec<String>,
    /// Index of the top-level entity being drawn.
    current: u32,
}

fn aci(index: u8) -> Rgb {
    let (r, g, b) = Color::aci_rgb(index);
    Rgb(r, g, b)
}

fn arr(p: Point3) -> [f64; 3] {
    [p.x, p.y, p.z]
}

fn xy(p: [f64; 3]) -> P2 {
    [p[0], p[1]]
}

/// The pattern of a linetype in drawing units, or `None` if continuous.
fn pattern(lt: &Linetype) -> Option<Vec<f64>> {
    let lengths: Vec<f64> = lt.elements.iter().map(|e| e.length).collect();
    (lengths.iter().any(|l| *l < 0.0)).then_some(lengths)
}

impl Builder<'_> {
    fn skip(&mut self, what: &str) {
        *self.scene.skipped.entry(what.to_owned()).or_default() += 1;
    }

    /// The scene index of a layer, adding layers the table lacks (white,
    /// visible, continuous).
    fn layer(&mut self, name: &str) -> u32 {
        let key = name.to_uppercase();
        if let Some(&i) = self.layer_index.get(&key) {
            return i;
        }
        let i = self.scene.layers.len() as u32;
        let (color, visible, linetype) = match self.d.tables.layers.get(name) {
            Some(l) => {
                let color = match l.color {
                    Color::Index(i) => aci(i),
                    Color::Rgb(r, g, b) => Rgb(r, g, b),
                    Color::ByLayer | Color::ByBlock => WHITE,
                };
                (color, l.on && !l.frozen(), l.linetype.clone())
            }
            None => (WHITE, true, String::new()),
        };
        self.scene.layers.push(LayerInfo {
            name: name.to_owned(),
            color,
            visible,
        });
        self.layer_index.insert(key, i);
        let dash = self.dash_for(&linetype, 1.0);
        self.layer_linetype.push(dash);
        i
    }

    /// The dash index for a named linetype at `scale`, or `None` for
    /// continuous lines, BYLAYER/BYBLOCK and unknown names.
    fn dash_for(&mut self, name: &str, scale: f64) -> Option<u32> {
        let lt = self.d.tables.linetypes.get(name)?;
        let lengths = pattern(lt)?;
        let factor = self.ltscale * scale;
        let scaled: Vec<f64> = lengths.iter().map(|l| l * factor).collect();
        if scaled.iter().map(|l| l.abs()).sum::<f64>() <= 0.0
            || !scaled.iter().all(|l| l.is_finite())
        {
            return None;
        }
        let key: Vec<u64> = scaled.iter().map(|l| l.to_bits()).collect();
        if let Some(&i) = self.dash_index.get(&key) {
            return Some(i);
        }
        let i = self.scene.dashes.len() as u32;
        self.scene.dashes.push(scaled);
        self.dash_index.insert(key, i);
        Some(i)
    }

    /// The entity's style: layer index, colour and dash pattern.
    fn style(&mut self, e: &Entity, ctx: &Ctx) -> (u32, Rgb, Option<u32>) {
        let layer = match ctx.layer {
            Some(parent) if e.layer == "0" => parent,
            _ => self.layer(&e.layer),
        };
        let color = match e.color {
            Color::ByLayer => self.scene.layers[layer as usize].color,
            Color::ByBlock => ctx.color,
            Color::Index(i) => aci(i),
            Color::Rgb(r, g, b) => Rgb(r, g, b),
        };
        let ltscale = e
            .extra
            .iter()
            .find(|p| p.code == 48)
            .and_then(|p| p.raw.trim().parse::<f64>().ok())
            .unwrap_or(1.0);
        let dash = if names_equal(&e.linetype, "BYLAYER") || e.linetype.is_empty() {
            let base = self.layer_linetype[layer as usize];
            // Re-scale the layer's pattern for this entity.
            match base {
                Some(_) if ltscale * ctx.scale != 1.0 => {
                    let name = self.layer_linetype_name(layer);
                    self.dash_for(&name, ltscale * ctx.scale)
                }
                other => other,
            }
        } else if names_equal(&e.linetype, "BYBLOCK") {
            ctx.linetype
        } else {
            self.dash_for(&e.linetype, ltscale * ctx.scale)
        };
        (layer, color, dash)
    }

    fn layer_linetype_name(&self, layer: u32) -> String {
        let name = &self.scene.layers[layer as usize].name;
        self.d
            .tables
            .layers
            .get(name)
            .map(|l| l.linetype.clone())
            .unwrap_or_default()
    }

    fn push(&mut self, style: (u32, Rgb, Option<u32>), shape: Shape) {
        let (layer, color, dash) = style;
        let dash = match shape {
            Shape::Path { .. } => dash,
            _ => None,
        };
        self.scene.items.push(Item {
            layer,
            color,
            dash,
            shape,
            entity: self.current,
        });
    }

    /// A path given in a plane mapped to world by `t`. Arcs stay arcs when
    /// `t` keeps circles circular (seen from above); otherwise they are
    /// flattened first.
    fn path(
        &mut self,
        style: (u32, Rgb, Option<u32>),
        t: &Affine3,
        points: &[[f64; 3]],
        bulges: &[f64],
        closed: bool,
    ) {
        if points.is_empty() {
            return;
        }
        let shape = match t.plan_similarity() {
            Some((_, mirrored)) => Shape::Path {
                points: points.iter().map(|p| xy(t.apply(*p))).collect(),
                bulges: if bulges.iter().all(|b| *b == 0.0) {
                    Vec::new()
                } else {
                    bulges
                        .iter()
                        .map(|b| if mirrored { -b } else { *b })
                        .collect()
                },
                closed,
            },
            None => {
                let mut out = Vec::new();
                let n = points.len();
                let segments = if closed { n } else { n - 1 };
                out.push(xy(t.apply(points[0])));
                for i in 0..segments {
                    let (a, b) = (points[i], points[(i + 1) % n]);
                    let bulge = bulges.get(i).copied().unwrap_or(0.0);
                    if let Some(arc) = arc_from_bulge([a[0], a[1]], [b[0], b[1]], bulge) {
                        let steps =
                            ((arc.sweep.abs() / (5f64).to_radians()).ceil() as usize).max(2);
                        for k in 1..steps {
                            let ang = arc.start_angle + arc.sweep * k as f64 / steps as f64;
                            let p = [
                                arc.center[0] + arc.radius * ang.cos(),
                                arc.center[1] + arc.radius * ang.sin(),
                                a[2],
                            ];
                            out.push(xy(t.apply(p)));
                        }
                    }
                    if !(closed && i == segments - 1) {
                        out.push(xy(t.apply(b)));
                    }
                }
                Shape::Path {
                    points: out,
                    bulges: Vec::new(),
                    closed,
                }
            }
        };
        self.push(style, shape);
    }

    fn entity(&mut self, e: &Entity, ctx: &Ctx) {
        if !e.visible {
            return;
        }
        let style = self.style(e, ctx);
        let t = &ctx.transform;
        match &e.kind {
            EntityKind::Line(l) => self.path(style, t, &[arr(l.start), arr(l.end)], &[], false),
            EntityKind::Circle(c) => {
                let ocs = t.then_after(&Affine3::from_ocs(arr(c.extrusion)));
                let (x, y, z, r) = (c.center.x, c.center.y, c.center.z, c.radius);
                self.path(
                    style,
                    &ocs,
                    &[[x + r, y, z], [x - r, y, z]],
                    &[1.0, 1.0],
                    true,
                );
            }
            EntityKind::Arc(a) => {
                let ocs = t.then_after(&Affine3::from_ocs(arr(a.extrusion)));
                let start = a.start_angle.to_radians();
                let mut sweep = (a.end_angle - a.start_angle).rem_euclid(360.0).to_radians();
                if sweep == 0.0 {
                    sweep = std::f64::consts::TAU;
                }
                // Two halves keep each bulge finite (a full turn would be ∞).
                let at = |ang: f64| {
                    [
                        a.center.x + a.radius * ang.cos(),
                        a.center.y + a.radius * ang.sin(),
                        a.center.z,
                    ]
                };
                let bulge = (sweep / 8.0).tan();
                self.path(
                    style,
                    &ocs,
                    &[at(start), at(start + sweep / 2.0), at(start + sweep)],
                    &[bulge, bulge],
                    false,
                );
            }
            EntityKind::LwPolyline(p) => {
                let ocs = t.then_after(&Affine3::from_ocs(arr(p.extrusion)));
                let points: Vec<[f64; 3]> =
                    p.vertices.iter().map(|v| [v.x, v.y, p.elevation]).collect();
                let bulges: Vec<f64> = p.vertices.iter().map(|v| v.bulge).collect();
                self.path(style, &ocs, &points, &bulges, p.is_closed());
            }
            EntityKind::Polyline(p) => self.polyline(e, p, style, ctx),
            EntityKind::Ellipse(el) => {
                let normal = Affine3::from_ocs(arr(el.extrusion)).m;
                let n = [normal[0][2], normal[1][2], normal[2][2]];
                let pts = ellipse_points(
                    arr(el.center),
                    arr(el.major_axis),
                    n,
                    el.ratio,
                    el.start_param,
                    el.end_param,
                    5f64.to_radians(),
                );
                self.path(style, t, &pts, &[], false);
            }
            EntityKind::Spline(s) => {
                let degree = usize::try_from(s.degree).unwrap_or(0);
                let ctrl: Vec<[f64; 3]> = s.control_points.iter().map(|p| arr(*p)).collect();
                let pts = nurbs_points(degree, &s.knots, &ctrl, &s.weights, 16).or_else(|| {
                    (s.fit_points.len() >= 2)
                        .then(|| s.fit_points.iter().map(|p| arr(*p)).collect())
                });
                match pts {
                    Some(pts) => self.path(style, t, &pts, &[], false),
                    None => self.skip("SPLINE (unusable data)"),
                }
            }
            EntityKind::Point(p) => {
                let at = xy(t.apply(arr(p.location)));
                self.push(style, Shape::Point(at));
            }
            EntityKind::Text(text) => self.text(style, t, text),
            EntityKind::Attrib(a) => {
                if a.flags & 1 == 0 {
                    self.text(style, t, &a.text);
                }
            }
            EntityKind::AttDef(a) => {
                // Inside a block reference only constant attributes show;
                // on their own, attribute definitions show their tag.
                if ctx.layer.is_some() {
                    if a.flags & 2 != 0 {
                        self.text(style, t, &a.text);
                    }
                } else {
                    let tag = Text {
                        value: a.tag.clone(),
                        ..a.text.clone()
                    };
                    self.text(style, t, &tag);
                }
            }
            EntityKind::MText(m) => self.mtext(style, t, m),
            EntityKind::Insert(i) => self.insert(e, i, style, ctx),
            EntityKind::Dimension(dim) => {
                // The dimension's drawing is in its block, in world coordinates.
                if let Some(block) = self.d.blocks.get(&dim.block) {
                    let inner = Ctx {
                        transform: ctx.transform,
                        layer: Some(style.0),
                        color: style.1,
                        linetype: style.2,
                        scale: ctx.scale,
                    };
                    self.block(block, &inner);
                } else {
                    self.skip("DIMENSION (missing block)");
                }
            }
            EntityKind::Vertex(_) => {}
            EntityKind::Unknown(u) => self.unknown(u, style, t),
        }
    }

    fn polyline(
        &mut self,
        e: &Entity,
        p: &lightable_dxf::Polyline,
        style: (u32, Rgb, Option<u32>),
        ctx: &Ctx,
    ) {
        let vertices: Vec<&lightable_dxf::Vertex> = p
            .vertices
            .iter()
            .filter_map(|v| match &v.kind {
                EntityKind::Vertex(v) => Some(v),
                _ => None,
            })
            .collect();
        let t = &ctx.transform;
        let _ = e;
        if p.flags & 64 != 0 {
            // Polyface mesh: vertices first, then faces (indices, 1-based;
            // negative = invisible edge).
            let points: Vec<[f64; 3]> = vertices
                .iter()
                .filter(|v| v.flags & 64 != 0)
                .map(|v| arr(v.location))
                .collect();
            for face in vertices
                .iter()
                .filter(|v| v.flags & 128 != 0 && v.flags & 64 == 0)
            {
                let idx: Vec<i16> = face.indices.iter().copied().filter(|i| *i != 0).collect();
                for k in 0..idx.len() {
                    let (a, b) = (idx[k], idx[(k + 1) % idx.len()]);
                    if a < 0 {
                        continue;
                    }
                    let get = |i: i16| {
                        points
                            .get(usize::from(i.unsigned_abs()).checked_sub(1)?)
                            .copied()
                    };
                    if let (Some(pa), Some(pb)) = (get(a), get(b)) {
                        self.path(style, t, &[pa, pb], &[], false);
                    }
                }
            }
            return;
        }
        if p.flags & 16 != 0 {
            // Polygon mesh: an M × N grid of vertices.
            let (m, n) = (
                p.mesh_counts.0.max(0) as usize,
                p.mesh_counts.1.max(0) as usize,
            );
            let pts: Vec<[f64; 3]> = vertices.iter().map(|v| arr(v.location)).collect();
            if m * n == pts.len() && m > 0 && n > 0 {
                for row in pts.chunks(n) {
                    self.path(style, t, row, &[], p.flags & 32 != 0);
                }
                for c in 0..n {
                    let column: Vec<[f64; 3]> = (0..m).map(|r| pts[r * n + c]).collect();
                    self.path(style, t, &column, &[], p.flags & 1 != 0);
                }
            }
            return;
        }
        // Spline-fit polylines: draw the fitted vertices, not the frame.
        let drawn: Vec<&&lightable_dxf::Vertex> = if p.flags & 4 != 0 {
            vertices.iter().filter(|v| v.flags & 16 == 0).collect()
        } else {
            vertices.iter().collect()
        };
        let closed = p.flags & 1 != 0;
        if p.flags & 8 != 0 {
            // 3D polyline: world coordinates, no bulges.
            let pts: Vec<[f64; 3]> = drawn.iter().map(|v| arr(v.location)).collect();
            self.path(style, t, &pts, &[], closed);
        } else {
            let ocs = t.then_after(&Affine3::from_ocs(arr(p.extrusion)));
            let pts: Vec<[f64; 3]> = drawn
                .iter()
                .map(|v| [v.location.x, v.location.y, p.location.z])
                .collect();
            let bulges: Vec<f64> = drawn.iter().map(|v| v.bulge).collect();
            self.path(style, &ocs, &pts, &bulges, closed);
        }
    }

    /// A text placed in its OCS, through `t`.
    fn text(&mut self, style: (u32, Rgb, Option<u32>), t: &Affine3, text: &Text) {
        let ocs = t.then_after(&Affine3::from_ocs(arr(text.extrusion)));
        let (halign, valign, at) = match (text.horizontal_align, text.vertical_align) {
            (0, 0) | (3, _) | (5, _) => (HAlign::Left, VAlign::Baseline, text.insert),
            (h, v) => {
                let halign = match h {
                    1 | 4 => HAlign::Center,
                    2 => HAlign::Right,
                    _ => HAlign::Left,
                };
                let valign = match (h, v) {
                    (4, _) | (_, 2) => VAlign::Middle,
                    (_, 1) => VAlign::Bottom,
                    (_, 3) => VAlign::Top,
                    _ => VAlign::Baseline,
                };
                (halign, valign, text.align_point.unwrap_or(text.insert))
            }
        };
        let content = display_text(&text.value);
        let rot = text.rotation.to_radians();
        self.place_text(
            style,
            &ocs,
            arr(at),
            rot,
            text.height,
            text.width_factor,
            text.oblique.to_radians(),
            content,
            halign,
            valign,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn place_text(
        &mut self,
        style: (u32, Rgb, Option<u32>),
        t: &Affine3,
        at: [f64; 3],
        rotation: f64,
        height: f64,
        width_factor: f64,
        oblique: f64,
        text: String,
        halign: HAlign,
        valign: VAlign,
    ) {
        if text.trim().is_empty() || height.is_nan() || height <= 0.0 {
            return;
        }
        let (s, c) = rotation.sin_cos();
        let dir = t.apply_vector([c, s, 0.0]);
        let up = t.apply_vector([-s * height, c * height, 0.0]);
        let height = up[0].hypot(up[1]);
        if height == 0.0 {
            return;
        }
        self.push(
            style,
            Shape::Text(TextShape {
                at: xy(t.apply(at)),
                height,
                rotation: dir[1].atan2(dir[0]),
                width_factor: if width_factor > 0.0 {
                    width_factor
                } else {
                    1.0
                },
                oblique,
                text,
                halign,
                valign,
            }),
        );
    }

    fn mtext(&mut self, style: (u32, Rgb, Option<u32>), t: &Affine3, m: &lightable_dxf::MText) {
        let lines = m.plain_lines();
        let rotation = match m.x_direction {
            Some(x) if x.x != 0.0 || x.y != 0.0 => x.y.atan2(x.x),
            _ => m.rotation.to_radians(),
        };
        let step = m.height * 5.0 / 3.0 * m.line_spacing_factor;
        let total = step * lines.len().saturating_sub(1) as f64 + m.height;
        let attachment = m.attachment.clamp(1, 9) - 1;
        let first = match attachment / 3 {
            0 => -m.height,
            1 => total / 2.0 - m.height,
            _ => total - m.height,
        };
        let halign = match attachment % 3 {
            0 => HAlign::Left,
            1 => HAlign::Center,
            _ => HAlign::Right,
        };
        let (s, c) = rotation.sin_cos();
        for (i, line) in lines.into_iter().enumerate() {
            let dy = first - step * i as f64;
            let at = [m.insert.x - dy * s, m.insert.y + dy * c, m.insert.z];
            self.place_text(
                style,
                t,
                at,
                rotation,
                m.height,
                1.0,
                0.0,
                display_text(&line),
                halign,
                VAlign::Baseline,
            );
        }
    }

    fn insert(&mut self, e: &Entity, i: &Insert, style: (u32, Rgb, Option<u32>), ctx: &Ctx) {
        // Attributes are placed already; they follow the outer transform.
        for a in &i.attributes {
            self.entity(a, ctx);
        }
        let Some(block) = self.d.blocks.get(&i.block) else {
            self.skip("INSERT (missing block)");
            return;
        };
        if self.stack.len() >= MAX_DEPTH || self.stack.iter().any(|n| names_equal(n, &block.name)) {
            self.skip("INSERT (block refers to itself)");
            return;
        }
        let _ = e;
        let base = arr(block.base_point);
        let ocs = Affine3::from_ocs(arr(i.extrusion));
        let scale = (i.scale.x * i.scale.y).abs().sqrt();
        for row in 0..i.rows.max(1) {
            for col in 0..i.columns.max(1) {
                let local = Affine3::translate(arr(i.insert))
                    .then_after(&Affine3::rotate_z(i.rotation.to_radians()))
                    .then_after(&Affine3::translate([
                        f64::from(col) * i.column_spacing,
                        f64::from(row) * i.row_spacing,
                        0.0,
                    ]))
                    .then_after(&Affine3::scale(arr(i.scale)))
                    .then_after(&Affine3::translate([-base[0], -base[1], -base[2]]));
                let inner = Ctx {
                    transform: ctx.transform.then_after(&ocs).then_after(&local),
                    layer: Some(style.0),
                    color: style.1,
                    linetype: style.2,
                    scale: ctx.scale * scale,
                };
                self.stack.push(block.name.clone());
                self.block(block, &inner);
                self.stack.pop();
            }
        }
    }

    fn block(&mut self, block: &lightable_dxf::Block, ctx: &Ctx) {
        for e in &block.entities {
            self.entity(e, ctx);
        }
    }

    fn unknown(&mut self, u: &Unknown, style: (u32, Rgb, Option<u32>), t: &Affine3) {
        let num = |code: i16| -> Option<f64> {
            u.pairs
                .iter()
                .find(|p| p.code == code)?
                .raw
                .trim()
                .parse()
                .ok()
        };
        let point = |x: i16| -> Option<[f64; 3]> {
            Some([num(x)?, num(x + 10)?, num(x + 20).unwrap_or(0.0)])
        };
        match u.type_name.to_ascii_uppercase().as_str() {
            "SOLID" | "TRACE" => {
                let extrusion = [
                    num(210).unwrap_or(0.0),
                    num(220).unwrap_or(0.0),
                    num(230).unwrap_or(1.0),
                ];
                let ocs = t.then_after(&Affine3::from_ocs(extrusion));
                // Vertex order 1, 2, 4, 3 (the fourth defaults to the third).
                let (Some(a), Some(b), Some(c)) = (point(10), point(11), point(12)) else {
                    return self.skip(&u.type_name);
                };
                let d = point(13).unwrap_or(c);
                let points = [a, b, d, c].iter().map(|p| xy(ocs.apply(*p))).collect();
                self.push(style, Shape::Fill { points });
            }
            "3DFACE" => {
                let pts: Vec<[f64; 3]> =
                    [10, 11, 12, 13].iter().filter_map(|c| point(*c)).collect();
                if pts.len() >= 3 {
                    self.path(style, t, &pts, &[], true);
                }
            }
            "LEADER" => {
                let xs: Vec<f64> = u
                    .pairs
                    .iter()
                    .filter(|p| p.code == 10)
                    .filter_map(|p| p.raw.trim().parse().ok())
                    .collect();
                let ys: Vec<f64> = u
                    .pairs
                    .iter()
                    .filter(|p| p.code == 20)
                    .filter_map(|p| p.raw.trim().parse().ok())
                    .collect();
                let pts: Vec<[f64; 3]> = xs.iter().zip(&ys).map(|(x, y)| [*x, *y, 0.0]).collect();
                if pts.len() >= 2 {
                    self.path(style, t, &pts, &[], false);
                }
            }
            "HATCH" => self.hatch(u, style, t),
            other => self.skip(other),
        }
    }

    /// A hatch: its boundary loops as lines, and the region they enclose
    /// filled (in full colour when solid; dimmed when it has a pattern,
    /// since patterns are not drawn yet).
    fn hatch(&mut self, u: &Unknown, style: (u32, Rgb, Option<u32>), t: &Affine3) {
        let Some(h) = u.hatch() else {
            return self.skip("HATCH");
        };
        let loops: Vec<Vec<[f64; 2]>> = h
            .paths
            .iter()
            .map(boundary_points)
            .filter(|l| l.len() >= 3)
            .collect();
        if loops.is_empty() {
            return self.skip("HATCH (boundary not read)");
        }
        let e = h.extrusion;
        let ocs = t.then_after(&Affine3::from_ocs([e.x, e.y, e.z]));
        let world: Vec<Vec<P2>> = loops
            .iter()
            .map(|l| {
                l.iter()
                    .map(|p| xy(ocs.apply([p[0], p[1], h.elevation])))
                    .collect()
            })
            .collect();
        let (layer, color, dash) = style;
        let fill = if h.solid {
            color
        } else {
            let mix = |c: u8, bg: u8| ((f64::from(c) * 0.3 + f64::from(bg) * 0.7).round()) as u8;
            let bg = crate::svg::BACKGROUND;
            Rgb(mix(color.0, bg.0), mix(color.1, bg.1), mix(color.2, bg.2))
        };
        self.push(
            (layer, fill, None),
            Shape::Region {
                loops: world.clone(),
            },
        );
        for points in world {
            self.push(
                (layer, color, dash),
                Shape::Path {
                    points,
                    bulges: Vec::new(),
                    closed: true,
                },
            );
        }
    }
}

/// Joins `next` to the end of `out`, reversing it when it was written in
/// the other direction, and dropping the shared point.
fn join(out: &mut Vec<[f64; 2]>, mut next: Vec<[f64; 2]>) {
    if next.is_empty() {
        return;
    }
    if let Some(&last) = out.last() {
        let d = |a: [f64; 2], b: [f64; 2]| (a[0] - b[0]).hypot(a[1] - b[1]);
        let (first, end) = (next[0], next[next.len() - 1]);
        if d(last, end) < d(last, first) {
            next.reverse();
        }
        if d(last, next[0]) <= 1e-9 * (1.0 + last[0].abs().max(last[1].abs())) {
            next.remove(0);
        }
    }
    out.extend(next);
}

/// A hatch boundary path as one flattened loop (OCS coordinates).
fn boundary_points(path: &lightable_dxf::hatch::BoundaryPath) -> Vec<[f64; 2]> {
    use lightable_dxf::hatch::{BoundaryPath, Edge};
    const STEP: f64 = 5.0 * std::f64::consts::PI / 180.0;
    let mut out: Vec<[f64; 2]> = Vec::new();
    match path {
        BoundaryPath::Polyline {
            points,
            bulges,
            closed: _,
        } => {
            // Hatch boundaries are always closed loops.
            out = crate::tess::flatten(points, bulges, true);
            out.pop();
        }
        BoundaryPath::Edges(edges) => {
            for edge in edges {
                let piece: Vec<[f64; 2]> = match edge {
                    Edge::Line { start, end } => vec![*start, *end],
                    Edge::Arc {
                        center,
                        radius,
                        start_angle,
                        end_angle,
                        ccw,
                    } => {
                        // Clockwise arcs are stored with mirrored angles.
                        let (s, e) = if *ccw {
                            (start_angle.to_radians(), end_angle.to_radians())
                        } else {
                            (-start_angle.to_radians(), -end_angle.to_radians())
                        };
                        let tau = std::f64::consts::TAU;
                        let mut sweep = if *ccw {
                            (e - s).rem_euclid(tau)
                        } else {
                            -(s - e).rem_euclid(tau)
                        };
                        if sweep == 0.0 {
                            sweep = if *ccw { tau } else { -tau };
                        }
                        let steps = ((sweep.abs() / STEP).ceil() as usize).clamp(2, 10_000);
                        (0..=steps)
                            .map(|k| {
                                let a = s + sweep * k as f64 / steps as f64;
                                [center[0] + radius * a.cos(), center[1] + radius * a.sin()]
                            })
                            .collect()
                    }
                    Edge::Ellipse {
                        center,
                        major,
                        ratio,
                        start_angle,
                        end_angle,
                        ccw,
                    } => {
                        let c = [center[0], center[1], 0.0];
                        let m = [major[0], major[1], 0.0];
                        let (s, e) = (start_angle.to_radians(), end_angle.to_radians());
                        let pts = if *ccw {
                            ellipse_points(c, m, [0.0, 0.0, 1.0], *ratio, s, e, STEP)
                        } else {
                            // Clockwise from -s to -e = counter-clockwise
                            // from -e to -s, reversed.
                            let mut p = ellipse_points(c, m, [0.0, 0.0, 1.0], *ratio, -e, -s, STEP);
                            p.reverse();
                            p
                        };
                        pts.into_iter().map(|p| [p[0], p[1]]).collect()
                    }
                    Edge::Spline {
                        degree,
                        knots,
                        control,
                        weights,
                        fit,
                    } => {
                        let control3: Vec<[f64; 3]> =
                            control.iter().map(|p| [p[0], p[1], 0.0]).collect();
                        match nurbs_points(*degree, knots, &control3, weights, 32) {
                            Some(p) => p.into_iter().map(|p| [p[0], p[1]]).collect(),
                            None => fit.clone(),
                        }
                    }
                };
                join(&mut out, piece);
            }
            // The loop closes on its first point.
            if out.len() > 1 {
                let (a, b) = (out[0], out[out.len() - 1]);
                if (a[0] - b[0]).hypot(a[1] - b[1]) <= 1e-9 * (1.0 + a[0].abs().max(a[1].abs())) {
                    out.pop();
                }
            }
        }
    }
    out
}

/// Text as displayed: overline/underline toggles removed.
fn display_text(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(i) = rest.find("%%") {
        out.push_str(&rest[..i]);
        let code = rest[i + 2..].chars().next();
        if matches!(code, Some('u' | 'U' | 'o' | 'O')) {
            rest = &rest[i + 3..];
        } else {
            out.push_str("%%");
            rest = &rest[i + 2..];
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dxf(pairs: &[(i16, &str)]) -> Drawing {
        let text: String = pairs.iter().map(|(c, v)| format!("{c}\n{v}\n")).collect();
        lightable_dxf::read_str(&text).expect("test drawing").0
    }

    /// Tables, blocks and entities wrapped in sections.
    fn drawing(
        tables: &[(i16, &'static str)],
        blocks: &[(i16, &'static str)],
        entities: &[(i16, &'static str)],
    ) -> Drawing {
        let mut pairs = vec![(0, "SECTION"), (2, "TABLES")];
        pairs.extend_from_slice(tables);
        pairs.extend_from_slice(&[(0, "ENDSEC"), (0, "SECTION"), (2, "BLOCKS")]);
        pairs.extend_from_slice(blocks);
        pairs.extend_from_slice(&[(0, "ENDSEC"), (0, "SECTION"), (2, "ENTITIES")]);
        pairs.extend_from_slice(entities);
        pairs.extend_from_slice(&[(0, "ENDSEC"), (0, "EOF")]);
        dxf(&pairs)
    }

    const LAYERS: [(i16, &str); 18] = [
        (0, "TABLE"),
        (2, "LAYER"),
        (0, "LAYER"),
        (2, "0"),
        (62, "7"),
        (6, "CONTINUOUS"),
        (0, "LAYER"),
        (2, "RED"),
        (62, "1"),
        (6, "CONTINUOUS"),
        (0, "LAYER"),
        (2, "GREEN"),
        (62, "3"),
        (6, "CONTINUOUS"),
        (0, "LAYER"),
        (2, "OFF"),
        (62, "-5"),
        (0, "ENDTAB"),
    ];

    fn paths(scene: &Scene) -> Vec<(&Vec<P2>, &Vec<f64>, Rgb, String)> {
        scene
            .items
            .iter()
            .filter_map(|i| match &i.shape {
                Shape::Path { points, bulges, .. } => Some((
                    points,
                    bulges,
                    i.color,
                    scene.layers[i.layer as usize].name.clone(),
                )),
                _ => None,
            })
            .collect()
    }

    fn close(a: P2, b: P2) -> bool {
        (a[0] - b[0]).abs() < 1e-9 && (a[1] - b[1]).abs() < 1e-9
    }

    #[test]
    fn colours_resolve_bylayer_byblock_and_index() {
        let d = drawing(
            &LAYERS,
            &[],
            &[
                (0, "LINE"),
                (8, "RED"),
                (10, "0"),
                (20, "0"),
                (11, "1"),
                (21, "0"),
                (0, "LINE"),
                (8, "RED"),
                (62, "3"),
                (10, "0"),
                (20, "0"),
                (11, "1"),
                (21, "0"),
                (0, "LINE"),
                (8, "RED"),
                (62, "0"),
                (10, "0"),
                (20, "0"),
                (11, "1"),
                (21, "0"),
                (0, "LINE"),
                (8, "RED"),
                (420, "1193046"),
                (10, "0"),
                (20, "0"),
                (11, "1"),
                (21, "0"),
            ],
        );
        let s = build(&d);
        let colors: Vec<Rgb> = s.items.iter().map(|i| i.color).collect();
        assert_eq!(
            colors,
            [Rgb(255, 0, 0), Rgb(0, 255, 0), WHITE, Rgb(0x12, 0x34, 0x56)]
        );
    }

    #[test]
    fn nested_blocks_inherit_layer_zero_and_byblock() {
        // INNER: a line on layer 0 (BYLAYER), a line BYBLOCK, a line on GREEN.
        // OUTER: INNER inserted on layer 0 with BYBLOCK colour, moved by (100, 0).
        // Top: OUTER inserted on RED, rotated 90°, at (0, 50).
        let blocks = [
            (0, "BLOCK"),
            (2, "INNER"),
            (10, "0"),
            (20, "0"),
            (0, "LINE"),
            (8, "0"),
            (10, "0"),
            (20, "0"),
            (11, "1"),
            (21, "0"),
            (0, "LINE"),
            (8, "0"),
            (62, "0"),
            (10, "0"),
            (20, "0"),
            (11, "1"),
            (21, "0"),
            (0, "LINE"),
            (8, "GREEN"),
            (10, "0"),
            (20, "0"),
            (11, "1"),
            (21, "0"),
            (0, "ENDBLK"),
            (0, "BLOCK"),
            (2, "OUTER"),
            (10, "0"),
            (20, "0"),
            (0, "INSERT"),
            (8, "0"),
            (62, "0"),
            (2, "INNER"),
            (10, "100"),
            (20, "0"),
            (0, "ENDBLK"),
        ];
        let d = drawing(
            &LAYERS,
            &blocks,
            &[
                (0, "INSERT"),
                (8, "RED"),
                (2, "OUTER"),
                (10, "0"),
                (20, "50"),
                (50, "90"),
            ],
        );
        let s = build(&d);
        let p = paths(&s);
        assert_eq!(p.len(), 3);
        // Layer-0 entities take the outermost reference's layer and colour.
        assert_eq!((p[0].2, p[0].3.as_str()), (Rgb(255, 0, 0), "RED"));
        assert_eq!((p[1].2, p[1].3.as_str()), (Rgb(255, 0, 0), "RED"));
        // An entity on its own layer keeps it.
        assert_eq!((p[2].2, p[2].3.as_str()), (Rgb(0, 255, 0), "GREEN"));
        // (0,0) in INNER is (100,0) in OUTER, rotated 90° and moved to (0,50).
        assert!(close(p[0].0[0], [0.0, 150.0]), "{:?}", p[0].0);
        assert!(close(p[0].0[1], [0.0, 151.0]), "{:?}", p[0].0);
    }

    #[test]
    fn self_referencing_blocks_stop() {
        let blocks = [
            (0, "BLOCK"),
            (2, "A"),
            (10, "0"),
            (20, "0"),
            (0, "LINE"),
            (10, "0"),
            (20, "0"),
            (11, "1"),
            (21, "0"),
            (0, "INSERT"),
            (2, "B"),
            (10, "1"),
            (20, "0"),
            (0, "ENDBLK"),
            (0, "BLOCK"),
            (2, "B"),
            (10, "0"),
            (20, "0"),
            (0, "INSERT"),
            (2, "a"),
            (10, "1"),
            (20, "0"),
            (0, "ENDBLK"),
        ];
        let d = drawing(
            &LAYERS,
            &blocks,
            &[(0, "INSERT"), (2, "A"), (10, "0"), (20, "0")],
        );
        let s = build(&d);
        assert_eq!(paths(&s).len(), 1);
        assert_eq!(s.skipped.get("INSERT (block refers to itself)"), Some(&1));
    }

    #[test]
    fn insert_arrays_offset_rows_and_columns_in_the_rotated_frame() {
        let blocks = [
            (0, "BLOCK"),
            (2, "DOT"),
            (10, "0"),
            (20, "0"),
            (0, "POINT"),
            (10, "0"),
            (20, "0"),
            (0, "ENDBLK"),
        ];
        let d = drawing(
            &LAYERS,
            &blocks,
            &[
                (0, "INSERT"),
                (2, "DOT"),
                (10, "0"),
                (20, "0"),
                (50, "90"),
                (41, "5"),
                (42, "5"),
                (70, "2"),
                (71, "2"),
                (44, "10"),
                (45, "20"),
            ],
        );
        let s = build(&d);
        let mut pts: Vec<P2> = s
            .items
            .iter()
            .filter_map(|i| match i.shape {
                Shape::Point(p) => Some(p),
                _ => None,
            })
            .collect();
        pts.sort_by(|a, b| a.partial_cmp(b).unwrap());
        // Spacing is not scaled; rotation turns columns (+x) into +y.
        let expected = [[-20.0, 0.0], [-20.0, 10.0], [0.0, 0.0], [0.0, 10.0]];
        for (p, e) in pts.iter().zip(expected) {
            assert!(close(*p, e), "{pts:?}");
        }
    }

    #[test]
    fn mirrored_extrusion_and_inserts_flip_arcs() {
        // An arc 0°→90° of radius 1 with extrusion (0,0,-1) runs, seen from
        // above, from (-1,0) to (0,1) clockwise.
        let d = drawing(
            &LAYERS,
            &[],
            &[
                (0, "ARC"),
                (10, "0"),
                (20, "0"),
                (40, "1"),
                (50, "0"),
                (51, "90"),
                (230, "-1"),
            ],
        );
        let s = build(&d);
        let (pts, bulges, _, _) = &paths(&s)[0];
        assert!(
            close(pts[0], [-1.0, 0.0]) && close(pts[2], [0.0, 1.0]),
            "{pts:?}"
        );
        assert!(bulges.iter().all(|b| *b < 0.0), "clockwise: {bulges:?}");
    }

    #[test]
    fn non_uniform_scale_flattens_arcs_into_an_ellipse() {
        let blocks = [
            (0, "BLOCK"),
            (2, "C"),
            (10, "0"),
            (20, "0"),
            (0, "CIRCLE"),
            (10, "0"),
            (20, "0"),
            (40, "1"),
            (0, "ENDBLK"),
        ];
        let d = drawing(
            &LAYERS,
            &blocks,
            &[
                (0, "INSERT"),
                (2, "C"),
                (10, "0"),
                (20, "0"),
                (41, "2"),
                (42, "1"),
            ],
        );
        let s = build(&d);
        let (pts, bulges, _, _) = &paths(&s)[0];
        assert!(bulges.is_empty());
        assert!(pts.len() > 36);
        for p in pts.iter() {
            assert!(
                ((p[0] / 2.0).powi(2) + p[1].powi(2) - 1.0).abs() < 1e-9,
                "{p:?}"
            );
        }
    }

    #[test]
    fn hidden_things_are_marked_or_left_out() {
        let d = drawing(
            &LAYERS,
            &[],
            &[
                (0, "LINE"),
                (8, "OFF"),
                (10, "0"),
                (20, "0"),
                (11, "1"),
                (21, "0"),
                (0, "LINE"),
                (60, "1"),
                (10, "0"),
                (20, "0"),
                (11, "1"),
                (21, "0"),
                (0, "LINE"),
                (67, "1"),
                (10, "0"),
                (20, "0"),
                (11, "1"),
                (21, "0"),
                (0, "LINE"),
                (8, "NEW"),
                (10, "0"),
                (20, "0"),
                (11, "1"),
                (21, "0"),
            ],
        );
        let s = build(&d);
        // The invisible entity and the paper-space one are not drawn; the
        // one on an "off" layer is drawn but its layer starts hidden.
        assert_eq!(s.items.len(), 2);
        let vis = s.default_visibility();
        assert!(!vis[s.items[0].layer as usize]);
        assert!(
            vis[s.items[1].layer as usize],
            "unknown layers are added, visible"
        );
        assert_eq!(s.layers[s.items[1].layer as usize].name, "NEW");
    }

    #[test]
    fn linetype_patterns_scale_with_ltscale_entity_scale_and_inserts() {
        let tables = [
            (0, "TABLE"),
            (2, "LTYPE"),
            (0, "LTYPE"),
            (2, "DASHED"),
            (72, "65"),
            (73, "2"),
            (40, "1.5"),
            (49, "1.0"),
            (49, "-0.5"),
            (0, "ENDTAB"),
        ];
        let blocks = [
            (0, "BLOCK"),
            (2, "B"),
            (10, "0"),
            (20, "0"),
            (0, "LINE"),
            (6, "DASHED"),
            (10, "0"),
            (20, "0"),
            (11, "1"),
            (21, "0"),
            (0, "ENDBLK"),
        ];
        let mut d = drawing(
            &tables,
            &blocks,
            &[
                (0, "LINE"),
                (6, "DASHED"),
                (10, "0"),
                (20, "0"),
                (11, "1"),
                (21, "0"),
                (0, "LINE"),
                (6, "DASHED"),
                (48, "3"),
                (10, "0"),
                (20, "0"),
                (11, "1"),
                (21, "0"),
                (0, "INSERT"),
                (2, "B"),
                (10, "0"),
                (20, "0"),
                (41, "4"),
                (42, "4"),
            ],
        );
        d.header.variables.push(lightable_dxf::HeaderVariable {
            name: "$LTSCALE".into(),
            pairs: vec![lightable_dxf::Pair::new(40, "2")],
        });
        let s = build(&d);
        let dashes: Vec<Vec<f64>> = s
            .items
            .iter()
            .map(|i| s.dashes[i.dash.unwrap() as usize].clone())
            .collect();
        assert_eq!(dashes, [vec![2.0, -1.0], vec![6.0, -3.0], vec![8.0, -4.0]]);
    }

    #[test]
    fn text_alignment_and_mtext_lines() {
        let d = drawing(
            &LAYERS,
            &[],
            &[
                (0, "TEXT"),
                (10, "0"),
                (20, "0"),
                (40, "2"),
                (1, "A%%uB"),
                (72, "1"),
                (11, "5"),
                (21, "5"),
                (73, "2"),
                (0, "MTEXT"),
                (10, "0"),
                (20, "100"),
                (40, "3"),
                (71, "1"),
                (1, "one\\Ptwo"),
            ],
        );
        let s = build(&d);
        let texts: Vec<&TextShape> = s
            .items
            .iter()
            .filter_map(|i| match &i.shape {
                Shape::Text(t) => Some(t),
                _ => None,
            })
            .collect();
        assert_eq!(texts.len(), 3);
        assert_eq!(
            (texts[0].text.as_str(), texts[0].halign, texts[0].valign),
            ("AB", HAlign::Center, VAlign::Middle)
        );
        assert!(close(texts[0].at, [5.0, 5.0]));
        // MTEXT top-left: first baseline one height below the insertion point.
        assert!(close(texts[1].at, [0.0, 97.0]), "{:?}", texts[1].at);
        assert!(close(texts[2].at, [0.0, 92.0]), "{:?}", texts[2].at);
    }

    #[test]
    fn solids_and_faces() {
        let d = drawing(
            &LAYERS,
            &[],
            &[
                (0, "SOLID"),
                (10, "0"),
                (20, "0"),
                (11, "1"),
                (21, "0"),
                (12, "0"),
                (22, "1"),
                (13, "1"),
                (23, "1"),
                (0, "HATCH"),
                (8, "0"),
            ],
        );
        let s = build(&d);
        match &s.items[0].shape {
            Shape::Fill { points } => assert_eq!(
                points,
                &vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]
            ),
            other => panic!("{other:?}"),
        }
        // A hatch without a readable boundary is counted as not drawn.
        assert_eq!(s.skipped.get("HATCH (boundary not read)"), Some(&1));
    }

    #[test]
    fn dimensions_draw_their_block() {
        let blocks = [
            (0, "BLOCK"),
            (2, "*D1"),
            (70, "1"),
            (10, "0"),
            (20, "0"),
            (0, "LINE"),
            (8, "0"),
            (62, "0"),
            (10, "0"),
            (20, "0"),
            (11, "10"),
            (21, "0"),
            (0, "ENDBLK"),
        ];
        let d = drawing(
            &LAYERS,
            &blocks,
            &[
                (0, "DIMENSION"),
                (8, "GREEN"),
                (2, "*D1"),
                (10, "0"),
                (20, "0"),
                (11, "5"),
                (21, "1"),
                (70, "32"),
                (13, "0"),
                (23, "0"),
                (14, "10"),
                (24, "0"),
            ],
        );
        let s = build(&d);
        let p = paths(&s);
        assert_eq!(p.len(), 1);
        assert_eq!((p[0].2, p[0].3.as_str()), (Rgb(0, 255, 0), "GREEN"));
    }
}
