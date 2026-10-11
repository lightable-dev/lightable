//! Emitting entities. The drawing has been normalised first (handles and
//! owners set, R12 downgrades done), so this is formatting only.

use crate::encoding::{CodePage, decode_mtext, decode_text};
use crate::entity::*;
use crate::model::{Color, LineWeight, Point3, Version, Z_AXIS};
use crate::table::names_equal;
use crate::tokenizer::Pair;

use super::out::Out;

/// Entity-level codes that belong in the `AcDbEntity` part when they turn up
/// in `extra` (linetype scale, transparency, plot style, material, shadow,
/// proxy graphics).
const ENTITY_LEVEL: [i16; 10] = [48, 440, 390, 347, 348, 284, 92, 310, 160, 380];

/// `extra` split by where each part goes when the record is written.
#[derive(Default)]
pub(super) struct ExtraParts<'a> {
    /// `102 {...}` groups: after the handle.
    pub groups: Vec<&'a Pair>,
    /// Entity-level leftovers: after the common properties.
    pub entity_level: Vec<&'a Pair>,
    /// Other leftovers: after the type's own data.
    pub data: Vec<&'a Pair>,
    /// Embedded objects (from `101` on, before XDATA).
    pub embedded: Vec<&'a Pair>,
    /// XDATA (`1001` and later): last.
    pub xdata: Vec<&'a Pair>,
}

pub(super) fn split_extra(extra: &[Pair]) -> ExtraParts<'_> {
    let mut parts = ExtraParts::default();
    let mut depth = 0usize;
    let (mut embedded, mut xdata) = (false, false);
    for p in extra {
        xdata |= p.code >= 1000;
        embedded |= p.code == 101;
        if xdata {
            parts.xdata.push(p);
        } else if embedded {
            parts.embedded.push(p);
        } else if p.code == 102 || depth > 0 {
            let v = p.raw.trim();
            if p.code == 102 && v.starts_with('{') {
                depth += 1;
            } else if p.code == 102 && v == "}" {
                depth = depth.saturating_sub(1);
            }
            parts.groups.push(p);
        } else if ENTITY_LEVEL.contains(&p.code) {
            parts.entity_level.push(p);
        } else {
            parts.data.push(p);
        }
    }
    parts
}

fn pairs(out: &mut Out, pairs: &[&Pair]) {
    for p in pairs {
        out.pair(p.code, &p.raw);
    }
}

/// The text to write: the original form if it still decodes to `value`,
/// otherwise `value` itself.
fn chosen<'a>(value: &'a str, raw: Option<&'a str>, decoded: impl Fn(&'a str) -> bool) -> &'a str {
    match raw {
        Some(raw) if decoded(raw) => raw,
        _ => value,
    }
}

pub(super) struct EntityWriter<'o> {
    pub out: &'o mut Out,
    pub version: Version,
    pub code_page: CodePage,
}

impl EntityWriter<'_> {
    fn modern(&self) -> bool {
        self.version != Version::R12
    }

    fn marker(&mut self, name: &str) {
        if self.modern() {
            self.out.pair(100, name);
        }
    }

    fn opt_float(&mut self, code: i16, value: f64, default: f64) {
        if value != default {
            self.out.float(code, value);
        }
    }

    fn opt_int(&mut self, code: i16, value: i16, default: i16) {
        if value != default {
            self.out.int(code, value);
        }
    }

    fn extrusion(&mut self, e: Point3) {
        if e != Z_AXIS {
            self.out.point(210, e);
        }
    }

    /// Writes an entity and, for polylines and inserts, its children.
    pub fn entity(&mut self, e: &Entity) {
        let parts = split_extra(&e.extra);
        let type_name = e.kind.type_name();
        if let EntityKind::Unknown(u) = &e.kind {
            // Normalisation already fixed the handle and owner pairs.
            self.out.pair(0, type_name);
            self.out.raw(&u.pairs);
            return;
        }
        self.common(e, type_name, &parts);
        self.data(e);
        pairs(self.out, &parts.data);
        if self.modern() {
            pairs(self.out, &parts.embedded);
        }
        pairs(self.out, &parts.xdata);
        match &e.kind {
            EntityKind::Polyline(p) => {
                for v in &p.vertices {
                    self.entity(v);
                }
                self.seqend(e, p.seqend.as_deref());
            }
            EntityKind::Insert(i) if !i.attributes.is_empty() => {
                for a in &i.attributes {
                    self.entity(a);
                }
                self.seqend(e, i.seqend.as_deref());
            }
            _ => {}
        }
    }

    fn seqend(&mut self, parent: &Entity, seqend: Option<&Entity>) {
        let layer = seqend.map_or(parent.layer.as_str(), |s| s.layer.as_str());
        self.out.pair(0, "SEQEND");
        if let Some(h) = seqend.and_then(|s| s.handle) {
            self.out.handle(5, h);
        }
        if self.modern() {
            if let Some(owner) = seqend.and_then(|s| s.owner) {
                self.out.handle(330, owner);
            }
            self.out.pair(100, "AcDbEntity");
        }
        self.out.pair(8, layer);
    }

    fn common(&mut self, e: &Entity, type_name: &str, parts: &ExtraParts<'_>) {
        self.out.pair(0, type_name);
        if let Some(h) = e.handle {
            self.out.handle(5, h);
        }
        if self.modern() {
            pairs(self.out, &parts.groups);
            if let Some(owner) = e.owner {
                self.out.handle(330, owner);
            }
            self.out.pair(100, "AcDbEntity");
        }
        if e.paper_space {
            self.out.int(67, 1);
        }
        self.out.pair(8, &e.layer);
        if !names_equal(&e.linetype, "BYLAYER") {
            self.out.pair(6, &e.linetype);
        }
        match e.color {
            Color::ByLayer => {}
            Color::ByBlock => self.out.int(62, 0),
            Color::Index(i) => self.out.int(62, i),
            Color::Rgb(r, g, b) => {
                self.out.int(62, Color::nearest_aci(r, g, b));
                if self.modern() {
                    self.out.int(
                        420,
                        (i32::from(r) << 16) | (i32::from(g) << 8) | i32::from(b),
                    );
                }
            }
        }
        if self.modern() {
            match e.lineweight {
                LineWeight::ByLayer => {}
                LineWeight::ByBlock => self.out.int(370, -2),
                LineWeight::Default => self.out.int(370, -3),
                LineWeight::Hundredths(n) => self.out.int(370, n),
            }
            if !e.visible {
                self.out.int(60, 1);
            }
            pairs(self.out, &parts.entity_level);
        }
    }

    fn data(&mut self, e: &Entity) {
        match &e.kind {
            EntityKind::Line(l) => {
                self.marker("AcDbLine");
                self.opt_float(39, l.thickness, 0.0);
                self.out.point(10, l.start);
                self.out.point(11, l.end);
                self.extrusion(l.extrusion);
            }
            EntityKind::Circle(c) => {
                self.marker("AcDbCircle");
                self.opt_float(39, c.thickness, 0.0);
                self.out.point(10, c.center);
                self.out.float(40, c.radius);
                self.extrusion(c.extrusion);
            }
            EntityKind::Arc(a) => {
                self.marker("AcDbCircle");
                self.opt_float(39, a.thickness, 0.0);
                self.out.point(10, a.center);
                self.out.float(40, a.radius);
                self.extrusion(a.extrusion);
                self.marker("AcDbArc");
                self.out.float(50, a.start_angle);
                self.out.float(51, a.end_angle);
            }
            EntityKind::LwPolyline(p) => self.lwpolyline(p),
            EntityKind::Ellipse(el) => {
                self.marker("AcDbEllipse");
                self.out.point(10, el.center);
                self.out.point(11, el.major_axis);
                self.extrusion(el.extrusion);
                self.out.float(40, el.ratio);
                self.out.float(41, el.start_param);
                self.out.float(42, el.end_param);
            }
            EntityKind::Point(p) => {
                self.marker("AcDbPoint");
                self.out.point(10, p.location);
                self.opt_float(39, p.thickness, 0.0);
                self.extrusion(p.extrusion);
                self.opt_float(50, p.x_axis_angle, 0.0);
            }
            EntityKind::Text(t) => {
                self.marker("AcDbText");
                self.text(t);
                self.marker("AcDbText");
                self.opt_int(73, t.vertical_align, 0);
            }
            EntityKind::Attrib(a) => {
                self.marker("AcDbText");
                self.text(&a.text);
                self.marker("AcDbAttribute");
                self.out.pair(2, &a.tag);
                self.out.int(70, a.flags);
                self.opt_int(73, a.field_length, 0);
                if self.modern() {
                    self.opt_int(74, a.text.vertical_align, 0);
                }
            }
            EntityKind::AttDef(a) => {
                self.marker("AcDbText");
                self.text(&a.text);
                self.marker("AcDbAttributeDefinition");
                self.out.pair(3, &a.prompt);
                self.out.pair(2, &a.tag);
                self.out.int(70, a.flags);
                self.opt_int(73, a.field_length, 0);
                if self.modern() {
                    self.opt_int(74, a.text.vertical_align, 0);
                }
            }
            EntityKind::MText(m) => self.mtext(m),
            EntityKind::Insert(i) => self.insert(i),
            EntityKind::Dimension(d) => self.dimension(d),
            EntityKind::Spline(s) => self.spline(s),
            EntityKind::Polyline(p) => self.polyline(p),
            EntityKind::Vertex(v) => self.vertex(v, e),
            EntityKind::Unknown(_) => {}
        }
    }

    fn lwpolyline(&mut self, p: &LwPolyline) {
        self.marker("AcDbPolyline");
        self.out.int(90, p.vertices.len());
        self.out.int(70, p.flags);
        if let Some(w) = p.constant_width {
            self.out.float(43, w);
        }
        self.opt_float(38, p.elevation, 0.0);
        self.opt_float(39, p.thickness, 0.0);
        for v in &p.vertices {
            self.out.float(10, v.x);
            self.out.float(20, v.y);
            if let Some(id) = v.id {
                self.out.int(91, id);
            }
            if v.start_width != 0.0 || v.end_width != 0.0 {
                self.out.float(40, v.start_width);
                self.out.float(41, v.end_width);
            }
            self.opt_float(42, v.bulge, 0.0);
        }
        self.extrusion(p.extrusion);
    }

    /// The codes `TEXT`, `ATTRIB` and `ATTDEF` share (in `AcDbText`).
    fn text(&mut self, t: &Text) {
        let cp = self.code_page;
        self.opt_float(39, t.thickness, 0.0);
        self.out.point(10, t.insert);
        self.out.float(40, t.height);
        let value = chosen(&t.value, t.raw_value.as_deref(), |raw| {
            decode_text(raw, cp) == t.value.as_str()
        });
        self.out.pair(1, value);
        self.opt_float(50, t.rotation, 0.0);
        self.opt_float(41, t.width_factor, 1.0);
        self.opt_float(51, t.oblique, 0.0);
        self.out.pair(7, &t.style);
        self.opt_int(71, t.generation_flags, 0);
        self.opt_int(72, t.horizontal_align, 0);
        if let Some(p) = t.align_point {
            self.out.point(11, p);
        }
        self.extrusion(t.extrusion);
    }

    fn mtext(&mut self, m: &MText) {
        let cp = self.code_page;
        self.marker("AcDbMText");
        self.out.point(10, m.insert);
        self.out.float(40, m.height);
        self.out.float(41, m.reference_width);
        self.out.int(71, m.attachment);
        self.out.int(72, m.drawing_direction);
        let text = chosen(&m.text, m.raw_text.as_deref(), |raw| {
            decode_mtext(raw, cp) == m.text.as_str()
        });
        // Values are limited to 250 characters: earlier chunks go in 3s.
        let chars: Vec<char> = text.chars().collect();
        let chunks: Vec<String> = chars.chunks(250).map(|c| c.iter().collect()).collect();
        match chunks.split_last() {
            Some((last, rest)) => {
                for chunk in rest {
                    self.out.pair(3, chunk);
                }
                self.out.pair(1, last);
            }
            None => self.out.pair(1, ""),
        }
        self.out.pair(7, &m.style);
        self.extrusion(m.extrusion);
        if let Some(x) = m.x_direction {
            self.out.point(11, x);
        }
        self.opt_float(50, m.rotation, 0.0);
        self.out.int(73, m.line_spacing_style);
        self.out.float(44, m.line_spacing_factor);
    }

    fn insert(&mut self, i: &Insert) {
        let array = i.columns > 1 || i.rows > 1;
        self.marker(if array {
            "AcDbMInsertBlock"
        } else {
            "AcDbBlockReference"
        });
        if !i.attributes.is_empty() {
            self.out.int(66, 1);
        }
        self.out.pair(2, &i.block);
        self.out.point(10, i.insert);
        self.opt_float(41, i.scale.x, 1.0);
        self.opt_float(42, i.scale.y, 1.0);
        self.opt_float(43, i.scale.z, 1.0);
        self.opt_float(50, i.rotation, 0.0);
        self.opt_int(70, i.columns, 1);
        self.opt_int(71, i.rows, 1);
        self.opt_float(44, i.column_spacing, 0.0);
        self.opt_float(45, i.row_spacing, 0.0);
        self.extrusion(i.extrusion);
    }

    fn dimension(&mut self, d: &Dimension) {
        let cp = self.code_page;
        let modern = self.modern();
        self.marker("AcDbDimension");
        self.out.pair(2, &d.block);
        self.out.point(10, d.definition_point);
        self.out.point(11, d.text_midpoint);
        let kind = match d.kind {
            DimensionKind::Linear { .. } => 0,
            DimensionKind::Aligned { .. } => 1,
            DimensionKind::Angular { .. } => 2,
            DimensionKind::Diameter { .. } => 3,
            DimensionKind::Radius { .. } => 4,
            DimensionKind::Angular3Point { .. } => 5,
            DimensionKind::Ordinate { x_type, .. } => 6 | if x_type { 64 } else { 0 },
        };
        self.out.int(70, d.flags | kind);
        if modern {
            if let Some(a) = d.attachment {
                self.out.int(71, a);
            }
            if let Some(m) = d.measurement {
                self.out.float(42, m);
            }
        }
        let text = chosen(&d.text_override, d.raw_text_override.as_deref(), |raw| {
            decode_text(raw, cp) == d.text_override.as_str()
        });
        if !text.is_empty() {
            self.out.pair(1, text);
        }
        self.opt_float(53, d.text_rotation, 0.0);
        self.opt_float(51, d.horizontal_direction, 0.0);
        self.out.pair(3, &d.style);
        self.extrusion(d.extrusion);
        match &d.kind {
            DimensionKind::Linear {
                first,
                second,
                angle,
                oblique,
            } => {
                self.marker("AcDbAlignedDimension");
                self.out.point(13, *first);
                self.out.point(14, *second);
                self.marker("AcDbRotatedDimension");
                self.opt_float(50, *angle, 0.0);
                self.opt_float(52, *oblique, 0.0);
            }
            DimensionKind::Aligned {
                first,
                second,
                oblique,
            } => {
                self.marker("AcDbAlignedDimension");
                self.out.point(13, *first);
                self.out.point(14, *second);
                self.opt_float(52, *oblique, 0.0);
            }
            DimensionKind::Angular {
                first_start,
                first_end,
                second_start,
                arc_point,
            } => {
                self.marker("AcDb2LineAngularDimension");
                self.out.point(13, *first_start);
                self.out.point(14, *first_end);
                self.out.point(15, *second_start);
                self.out.point(16, *arc_point);
            }
            DimensionKind::Diameter {
                chord_point,
                leader_length,
            } => {
                self.marker("AcDbDiametricDimension");
                self.out.point(15, *chord_point);
                self.out.float(40, *leader_length);
            }
            DimensionKind::Radius {
                chord_point,
                leader_length,
            } => {
                self.marker("AcDbRadialDimension");
                self.out.point(15, *chord_point);
                self.out.float(40, *leader_length);
            }
            DimensionKind::Angular3Point {
                first,
                second,
                vertex,
            } => {
                self.marker("AcDb3PointAngularDimension");
                self.out.point(13, *first);
                self.out.point(14, *second);
                self.out.point(15, *vertex);
            }
            DimensionKind::Ordinate {
                feature,
                leader_end,
                ..
            } => {
                self.marker("AcDbOrdinateDimension");
                self.out.point(13, *feature);
                self.out.point(14, *leader_end);
            }
        }
    }

    fn spline(&mut self, s: &Spline) {
        self.marker("AcDbSpline");
        if let Some(n) = s.normal {
            self.out.point(210, n);
        }
        self.out.int(70, s.flags);
        self.out.int(71, s.degree);
        self.out.int(72, s.knots.len());
        self.out.int(73, s.control_points.len());
        self.out.int(74, s.fit_points.len());
        if let Some(t) = s.knot_tolerance {
            self.out.float(42, t);
        }
        if let Some(t) = s.control_tolerance {
            self.out.float(43, t);
        }
        if let Some(t) = s.fit_tolerance {
            self.out.float(44, t);
        }
        if let Some(t) = s.start_tangent {
            self.out.point(12, t);
        }
        if let Some(t) = s.end_tangent {
            self.out.point(13, t);
        }
        for k in &s.knots {
            self.out.float(40, *k);
        }
        for (i, p) in s.control_points.iter().enumerate() {
            self.out.point(10, *p);
            if let Some(w) = s.weights.get(i) {
                self.out.float(41, *w);
            }
        }
        for p in &s.fit_points {
            self.out.point(11, *p);
        }
    }

    fn polyline(&mut self, p: &Polyline) {
        self.marker(if p.flags & 64 != 0 {
            "AcDbPolyFaceMesh"
        } else if p.flags & 16 != 0 {
            "AcDbPolygonMesh"
        } else if p.flags & 8 != 0 {
            "AcDb3dPolyline"
        } else {
            "AcDb2dPolyline"
        });
        self.out.int(66, 1);
        self.out.point(10, p.location);
        self.opt_float(39, p.thickness, 0.0);
        self.out.int(70, p.flags);
        self.opt_float(40, p.default_start_width, 0.0);
        self.opt_float(41, p.default_end_width, 0.0);
        self.opt_int(71, p.mesh_counts.0, 0);
        self.opt_int(72, p.mesh_counts.1, 0);
        self.opt_int(73, p.smooth_densities.0, 0);
        self.opt_int(74, p.smooth_densities.1, 0);
        self.opt_int(75, p.curve_type, 0);
        self.extrusion(p.extrusion);
    }

    fn vertex(&mut self, v: &Vertex, _e: &Entity) {
        self.marker("AcDbVertex");
        let flags = v.flags;
        self.marker(if flags & 128 != 0 && flags & 64 == 0 {
            "AcDbFaceRecord"
        } else if flags & 128 != 0 {
            "AcDbPolyFaceMeshVertex"
        } else if flags & 64 != 0 {
            "AcDbPolygonMeshVertex"
        } else if flags & 32 != 0 {
            "AcDb3dPolylineVertex"
        } else {
            "AcDb2dVertex"
        });
        self.out.point(10, v.location);
        self.opt_float(40, v.start_width, 0.0);
        self.opt_float(41, v.end_width, 0.0);
        self.opt_float(42, v.bulge, 0.0);
        self.out.int(70, flags);
        if let Some(t) = v.tangent {
            self.out.float(50, t);
        }
        for (i, n) in v.indices.iter().enumerate() {
            self.opt_int(71 + i as i16, *n, 0);
        }
        if self.modern()
            && let Some(id) = v.id
        {
            self.out.int(91, id);
        }
    }
}
