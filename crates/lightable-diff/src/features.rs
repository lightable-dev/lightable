//! An entity as labelled values, for comparing and for describing changes
//! in plain words. Handles, owners and file formatting are left out, so
//! only what the entity *is* gets compared.

use lightable_dxf::{Color, DimensionKind, Entity, EntityKind, LineWeight, Point3, Unknown};

/// How a number is compared.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NumKind {
    /// A length or distance: compared with the drawing tolerance.
    Length,
    /// Degrees, compared modulo 360.
    Angle,
    /// A count, factor or other plain number: compared almost exactly.
    Plain,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// A name (layer, style, block): shown as is.
    Name(String),
    /// Text content: shown in quotes.
    Text(String),
    Num(f64, NumKind),
    /// A position: moves with the entity.
    Pos([f64; 3]),
    /// A direction: does not move with the entity.
    Dir([f64; 3]),
}

/// A field name: static for most, built for numbered ones ("vertex 3").
pub type Label = std::borrow::Cow<'static, str>;

pub type Fields = Vec<(Label, Value)>;

/// The values that make up an entity.
#[derive(Debug, Clone)]
pub struct Features {
    /// Properties shared by all entities (layer, colour, ...). Changing only
    /// these does not change the drawn geometry.
    pub common: Fields,
    /// The entity's own content (geometry, text, block, ...).
    pub shape: Fields,
    /// False for types this library cannot interpret, whose content is only
    /// compared as raw data.
    pub interpreted: bool,
}

fn p3(p: Point3) -> [f64; 3] {
    [p.x, p.y, p.z]
}

fn name(label: impl Into<Label>, v: impl Into<String>) -> (Label, Value) {
    (label.into(), Value::Name(v.into()))
}

fn text(label: impl Into<Label>, v: impl Into<String>) -> (Label, Value) {
    (label.into(), Value::Text(v.into()))
}

fn len(label: impl Into<Label>, v: f64) -> (Label, Value) {
    (label.into(), Value::Num(v, NumKind::Length))
}

fn angle(label: impl Into<Label>, v: f64) -> (Label, Value) {
    (label.into(), Value::Num(v, NumKind::Angle))
}

fn plain(label: impl Into<Label>, v: f64) -> (Label, Value) {
    (label.into(), Value::Num(v, NumKind::Plain))
}

fn pos(label: impl Into<Label>, p: Point3) -> (Label, Value) {
    (label.into(), Value::Pos(p3(p)))
}

fn dir(label: impl Into<Label>, p: Point3) -> (Label, Value) {
    (label.into(), Value::Dir(p3(p)))
}

/// A colour in words: "blue (5)", "layer colour", "#ff8000".
pub(crate) fn color_name(c: Color) -> String {
    match c {
        Color::ByLayer => "layer colour".into(),
        Color::ByBlock => "block colour".into(),
        // The standard colours by name, with their number.
        Color::Index(i @ 1..=9) => {
            let names = [
                "red",
                "yellow",
                "green",
                "cyan",
                "blue",
                "magenta",
                "white",
                "dark grey",
                "light grey",
            ];
            format!("{} ({i})", names[usize::from(i) - 1])
        }
        Color::Index(i) => format!("colour {i}"),
        Color::Rgb(r, g, b) => format!("#{r:02x}{g:02x}{b:02x}"),
    }
}

fn lineweight_name(w: LineWeight) -> String {
    match w {
        LineWeight::ByLayer => "ByLayer".into(),
        LineWeight::ByBlock => "ByBlock".into(),
        LineWeight::Default => "Default".into(),
        LineWeight::Hundredths(n) => format!("{:.2} mm", f64::from(n) / 100.0),
    }
}

/// The text value with its original form ignored (that is formatting).
fn text_fields(out: &mut Fields, t: &lightable_dxf::Text) {
    out.push(text("text", t.value.clone()));
    // Left/baseline text is placed by its first point, other alignments
    // by the second; writers fill the unused one differently.
    let at = match (t.horizontal_align, t.vertical_align) {
        (0, 0) | (3, _) | (5, _) => t.insert,
        _ => t.align_point.unwrap_or(t.insert),
    };
    out.push(pos("position", at));
    out.push(len("height", t.height));
    out.push(angle("rotation", t.rotation));
    out.push(name("style", t.style.to_uppercase()));
    out.push(plain("width factor", t.width_factor));
    out.push(angle("oblique angle", t.oblique));
    out.push(plain("horizontal alignment", f64::from(t.horizontal_align)));
    out.push(plain("vertical alignment", f64::from(t.vertical_align)));
}

fn extrusion(out: &mut Fields, e: Point3) {
    if e != lightable_dxf::Z_AXIS {
        out.push(dir("extrusion", e));
    }
}

/// Raw pairs of an entity this library does not interpret, without what
/// changes on every save (handle, owner, reactors, subclass markers), with
/// numbers normalised so formatting differences do not count.
pub fn canonical_pairs(pairs: &[lightable_dxf::Pair]) -> Vec<(i16, String)> {
    let mut depth = 0usize;
    let mut out = Vec::new();
    for p in pairs {
        if p.code == 102 {
            let v = p.raw.trim();
            if v.starts_with('{') {
                depth += 1;
            } else if v == "}" {
                depth = depth.saturating_sub(1);
            }
            continue;
        }
        if depth > 0 || matches!(p.code, 5 | 100 | 330 | 360) {
            continue;
        }
        let value = match p.value() {
            lightable_dxf::Value::Double(v) => format!("{}", (v * 1e6).round() / 1e6),
            lightable_dxf::Value::Int(v) => v.to_string(),
            lightable_dxf::Value::Bool(v) => v.to_string(),
            lightable_dxf::Value::Str(s) => s.trim().to_owned(),
        };
        out.push((p.code, value));
    }
    out
}

/// Hatch codes worth naming in a description.
fn hatch_fields(out: &mut Fields, u: &Unknown) -> bool {
    let pairs = canonical_pairs(&u.pairs);
    let first = |code: i16| {
        pairs
            .iter()
            .find(|(c, _)| *c == code)
            .map(|(_, v)| v.clone())
    };
    if let Some(v) = first(2) {
        out.push(name("pattern", v));
    }
    if let Some(v) = first(70) {
        out.push(name("solid fill", if v == "1" { "yes" } else { "no" }));
    }
    if let Some(v) = first(41).and_then(|v| v.parse().ok()) {
        out.push(plain("pattern scale", v));
    }
    if let Some(v) = first(52).and_then(|v| v.parse().ok()) {
        out.push(angle("pattern angle", v));
    }
    // The rest, in the parts of a hatch record: boundary paths (from
    // group 91), pattern line definitions (from group 78), and anything
    // else (elevation, seed points, gradient).
    let mut parts: [Vec<String>; 3] = Default::default();
    let mut part = 2;
    for (c, v) in &pairs {
        part = match c {
            91 => 0,
            75 | 76 | 77 | 47 | 98 | 450..=470 => 2,
            78 => 1,
            _ => part,
        };
        if !matches!(c, 2 | 70 | 41 | 52 | 8 | 6 | 62 | 420 | 370 | 60 | 67) {
            parts[part].push(format!("{c}={v}"));
        }
    }
    for (label, values) in ["boundary", "pattern lines", "other hatch data"]
        .into_iter()
        .zip(parts)
    {
        out.push(name(label, values.join(" ")));
    }
    true
}

/// SOLID, TRACE and 3DFACE: four corners (groups 10–13, 20–23, 30–33;
/// without z values, the elevation in group 38), thickness and extrusion.
fn corner_fields(out: &mut Fields, u: &Unknown) {
    let num = |code: i16| {
        u.pairs
            .iter()
            .find(|p| p.code == code)
            .and_then(|p| lightable_dxf::value::parse_f64(&p.raw))
    };
    let elevation = num(38).unwrap_or(0.0);
    for k in 0..4 {
        let p = Point3::new(
            num(10 + k).unwrap_or(0.0),
            num(20 + k).unwrap_or(0.0),
            num(30 + k).unwrap_or(elevation),
        );
        out.push(pos(format!("corner {}", k + 1), p));
    }
    out.push(len("thickness", num(39).unwrap_or(0.0)));
    let e = Point3::new(
        num(210).unwrap_or(0.0),
        num(220).unwrap_or(0.0),
        num(230).unwrap_or(1.0),
    );
    extrusion(out, e);
    // Other data (3DFACE invisible-edge flags, ...).
    let rest: Vec<String> = canonical_pairs(&u.pairs)
        .into_iter()
        .filter(|(c, _)| !matches!(c, 10..=13 | 20..=23 | 30..=33 | 38 | 39 | 210 | 220 | 230 | 8 | 6 | 62 | 420 | 370 | 60 | 67))
        .map(|(c, v)| format!("{c}={v}"))
        .collect();
    out.push(name("data", rest.join(" ")));
}

/// What a dimension measures, from its points (for files that do not
/// store group 42): along the dimension's direction for linear ones,
/// between the points for aligned ones, centre or opposite point to the
/// chord point for radius and diameter. `None` for angular and ordinate
/// dimensions.
fn measured(d: &lightable_dxf::Dimension) -> Option<f64> {
    let dist = |a: Point3, b: Point3| (b.x - a.x).hypot(b.y - a.y);
    match &d.kind {
        DimensionKind::Linear {
            first,
            second,
            angle,
            ..
        } => {
            let (s, c) = angle.to_radians().sin_cos();
            Some(((second.x - first.x) * c + (second.y - first.y) * s).abs())
        }
        DimensionKind::Aligned { first, second, .. } => Some(dist(*first, *second)),
        DimensionKind::Diameter { chord_point, .. } | DimensionKind::Radius { chord_point, .. } => {
            Some(dist(d.definition_point, *chord_point))
        }
        _ => None,
    }
}

/// The features of an entity.
pub fn features(e: &Entity) -> Features {
    let mut common = vec![
        name("layer", e.layer.clone()),
        name("colour", color_name(e.color)),
        name("linetype", e.linetype.to_uppercase()),
        name("lineweight", lineweight_name(e.lineweight)),
        name(
            "visibility",
            if e.visible { "visible" } else { "invisible" },
        ),
    ];
    // Extended data (other programs' data attached to the entity).
    let xdata: Vec<(i16, String)> = canonical_pairs(
        e.extra
            .iter()
            .skip_while(|p| p.code < 1000)
            .cloned()
            .collect::<Vec<_>>()
            .as_slice(),
    );
    if !xdata.is_empty() {
        let s: Vec<String> = xdata.iter().map(|(c, v)| format!("{c}={v}")).collect();
        common.push(name("extended data", s.join(" ")));
    }
    let mut s: Fields = Vec::new();
    let mut interpreted = true;
    match &e.kind {
        EntityKind::Line(l) => {
            s.push(pos("start", l.start));
            s.push(pos("end", l.end));
            s.push(len("thickness", l.thickness));
            extrusion(&mut s, l.extrusion);
        }
        EntityKind::Circle(c) => {
            s.push(pos("centre", c.center));
            s.push(len("radius", c.radius));
            s.push(len("thickness", c.thickness));
            extrusion(&mut s, c.extrusion);
        }
        EntityKind::Arc(a) => {
            s.push(pos("centre", a.center));
            s.push(len("radius", a.radius));
            s.push(angle("start angle", a.start_angle));
            s.push(angle("end angle", a.end_angle));
            s.push(len("thickness", a.thickness));
            extrusion(&mut s, a.extrusion);
        }
        EntityKind::LwPolyline(p) => {
            s.push(name("closed", if p.is_closed() { "yes" } else { "no" }));
            s.push(plain("vertex count", p.vertices.len() as f64));
            for (i, v) in p.vertices.iter().enumerate() {
                s.push(pos(
                    format!("vertex {}", i + 1),
                    Point3::new(v.x, v.y, p.elevation),
                ));
                s.push(plain(format!("bulge {}", i + 1), v.bulge));
                s.push(len(
                    format!("width {}", i + 1),
                    v.start_width.max(v.end_width),
                ));
            }
            s.push(len("width", p.constant_width.unwrap_or(0.0)));
            extrusion(&mut s, p.extrusion);
        }
        EntityKind::Polyline(p) => {
            s.push(plain("flags", f64::from(p.flags & !128)));
            s.push(plain("vertex count", p.vertices.len() as f64));
            for (i, v) in p.vertices.iter().enumerate() {
                if let EntityKind::Vertex(v) = &v.kind {
                    s.push(pos(
                        format!("vertex {}", i + 1),
                        Point3::new(
                            v.location.x,
                            v.location.y,
                            v.location.z + if p.flags & 8 == 0 { p.location.z } else { 0.0 },
                        ),
                    ));
                    s.push(plain(format!("bulge {}", i + 1), v.bulge));
                }
            }
            extrusion(&mut s, p.extrusion);
        }
        EntityKind::Ellipse(el) => {
            s.push(pos("centre", el.center));
            s.push(dir("major axis", el.major_axis));
            s.push(plain("axis ratio", el.ratio));
            s.push(plain("start parameter", el.start_param));
            s.push(plain("end parameter", el.end_param));
            extrusion(&mut s, el.extrusion);
        }
        EntityKind::Point(p) => s.push(pos("location", p.location)),
        EntityKind::Text(t) => text_fields(&mut s, t),
        EntityKind::Attrib(a) => {
            s.push(name("tag", a.tag.clone()));
            text_fields(&mut s, &a.text);
        }
        EntityKind::AttDef(a) => {
            s.push(name("tag", a.tag.clone()));
            s.push(text("prompt", a.prompt.clone()));
            text_fields(&mut s, &a.text);
        }
        EntityKind::MText(m) => {
            // The words as read, and separately the inline formatting codes
            // (fonts, colours, line breaks written as `\P`).
            s.push(text("text", m.plain_lines().join(" / ")));
            s.push(name("formatting", m.text.clone()));
            s.push(pos("position", m.insert));
            s.push(len("height", m.height));
            s.push(len("width", m.reference_width));
            s.push(plain("attachment", f64::from(m.attachment)));
            let rotation = match m.x_direction {
                Some(x) if x.x != 0.0 || x.y != 0.0 => x.y.atan2(x.x).to_degrees(),
                _ => m.rotation,
            };
            s.push(angle("rotation", rotation));
            s.push(name("style", m.style.to_uppercase()));
        }
        EntityKind::Insert(i) => {
            s.push(name("block", i.block.clone()));
            s.push(pos("position", i.insert));
            s.push(plain("scale x", i.scale.x));
            s.push(plain("scale y", i.scale.y));
            s.push(plain("scale z", i.scale.z));
            s.push(angle("rotation", i.rotation));
            s.push(plain("columns", f64::from(i.columns.max(1))));
            s.push(plain("rows", f64::from(i.rows.max(1))));
            if i.columns > 1 || i.rows > 1 {
                s.push(len("column spacing", i.column_spacing));
                s.push(len("row spacing", i.row_spacing));
            }
            extrusion(&mut s, i.extrusion);
            // Attributes, by tag (their order in the file does not matter).
            let mut attrs: Vec<(String, String, Point3)> = i
                .attributes
                .iter()
                .filter_map(|a| match &a.kind {
                    EntityKind::Attrib(a) => {
                        Some((a.tag.clone(), a.text.value.clone(), a.text.insert))
                    }
                    _ => None,
                })
                .collect();
            attrs.sort_by(|a, b| a.0.cmp(&b.0));
            for (tag, value, at) in attrs {
                s.push(text(format!("attribute {tag}"), value));
                s.push(pos(format!("attribute {tag} position"), at));
            }
        }
        EntityKind::Dimension(d) => {
            let (kind, points): (&str, Vec<(&str, Point3)>) = match &d.kind {
                DimensionKind::Linear { first, second, .. } => (
                    "linear",
                    vec![("first point", *first), ("second point", *second)],
                ),
                DimensionKind::Aligned { first, second, .. } => (
                    "aligned",
                    vec![("first point", *first), ("second point", *second)],
                ),
                DimensionKind::Angular {
                    first_start,
                    first_end,
                    second_start,
                    arc_point,
                } => (
                    "angular",
                    vec![
                        ("first line start", *first_start),
                        ("first line end", *first_end),
                        ("second line start", *second_start),
                        ("arc point", *arc_point),
                    ],
                ),
                DimensionKind::Diameter { chord_point, .. } => {
                    ("diameter", vec![("chord point", *chord_point)])
                }
                DimensionKind::Radius { chord_point, .. } => {
                    ("radius", vec![("chord point", *chord_point)])
                }
                DimensionKind::Angular3Point {
                    first,
                    second,
                    vertex,
                } => (
                    "angular",
                    vec![
                        ("first point", *first),
                        ("second point", *second),
                        ("vertex", *vertex),
                    ],
                ),
                DimensionKind::Ordinate {
                    feature,
                    leader_end,
                    ..
                } => (
                    "ordinate",
                    vec![("feature point", *feature), ("leader end", *leader_end)],
                ),
            };
            s.push(name("dimension type", kind));
            // The value the dimension shows: group 42 when stored, else
            // measured from its points as a CAD program does.
            if let Some(m) = d.measurement.or_else(|| measured(d)) {
                if kind == "angular" {
                    s.push(angle("measurement", m.to_degrees()));
                } else {
                    s.push(len("measurement", m));
                }
            }
            s.push(name("dimension style", d.style.to_uppercase()));
            s.push(text("text override", d.text_override.clone()));
            s.push(pos("definition point", d.definition_point));
            s.push(pos("text position", d.text_midpoint));
            for (label, p) in points {
                s.push(pos(label, p));
            }
            if let DimensionKind::Linear { angle: a, .. } = d.kind {
                s.push(angle("dimension angle", a));
            }
        }
        EntityKind::Spline(sp) => {
            s.push(plain("degree", f64::from(sp.degree)));
            // A spline given by fit points is defined by them; writers add
            // the control points they compute from them (or drop them), so
            // only the fit points are compared then.
            s.push(plain("fit point count", sp.fit_points.len() as f64));
            if sp.fit_points.is_empty() {
                s.push(plain("control point count", sp.control_points.len() as f64));
            }
            for (i, p) in sp
                .control_points
                .iter()
                .enumerate()
                .filter(|_| sp.fit_points.is_empty())
            {
                s.push(pos(format!("control point {}", i + 1), *p));
            }
            for (i, p) in sp.fit_points.iter().enumerate() {
                s.push(pos(format!("fit point {}", i + 1), *p));
            }
            for (i, k) in sp
                .knots
                .iter()
                .enumerate()
                .filter(|_| sp.fit_points.is_empty())
            {
                s.push(plain(format!("knot {}", i + 1), *k));
            }
            for (i, w) in sp
                .weights
                .iter()
                .enumerate()
                .filter(|_| sp.fit_points.is_empty())
            {
                s.push(plain(format!("weight {}", i + 1), *w));
            }
        }
        EntityKind::Vertex(v) => s.push(pos("location", v.location)),
        EntityKind::Unknown(u) => {
            if u.type_name.eq_ignore_ascii_case("HATCH") {
                hatch_fields(&mut s, u);
                // Hatches are partly interpreted: the named codes above.
            } else if ["SOLID", "TRACE", "3DFACE"]
                .iter()
                .any(|t| u.type_name.eq_ignore_ascii_case(t))
            {
                corner_fields(&mut s, u);
            } else {
                interpreted = false;
                let raw: Vec<String> = canonical_pairs(&u.pairs)
                    .into_iter()
                    .filter(|(c, _)| !matches!(c, 8 | 6 | 62 | 420 | 370 | 60 | 67))
                    .map(|(c, v)| format!("{c}={v}"))
                    .collect();
                s.push(name("data", raw.join(" ")));
            }
        }
    }
    Features {
        common,
        shape: s,
        interpreted,
    }
}

/// Tolerances for comparing values.
#[derive(Debug, Clone, Copy)]
pub struct Tolerance {
    pub length: f64,
}

impl Tolerance {
    fn num_equal(&self, a: f64, b: f64, kind: NumKind) -> bool {
        match kind {
            NumKind::Length => (a - b).abs() <= self.length,
            NumKind::Angle => {
                let d = (a - b).rem_euclid(360.0);
                d.min(360.0 - d) <= 1e-6
            }
            NumKind::Plain => (a - b).abs() <= 1e-9 * (1.0 + a.abs().max(b.abs())),
        }
    }

    fn point_equal(&self, a: [f64; 3], b: [f64; 3]) -> bool {
        (0..3).all(|i| (a[i] - b[i]).abs() <= self.length)
    }

    pub fn value_equal(&self, a: &Value, b: &Value) -> bool {
        match (a, b) {
            (Value::Name(x), Value::Name(y)) | (Value::Text(x), Value::Text(y)) => x == y,
            (Value::Num(x, k), Value::Num(y, _)) => self.num_equal(*x, *y, *k),
            (Value::Pos(x), Value::Pos(y)) | (Value::Dir(x), Value::Dir(y)) => {
                self.point_equal(*x, *y)
            }
            _ => false,
        }
    }

    /// Same labels in the same order, and equal values.
    pub fn fields_equal(&self, a: &Fields, b: &Fields) -> bool {
        a.len() == b.len()
            && a.iter()
                .zip(b)
                .all(|((la, va), (lb, vb))| la == lb && self.value_equal(va, vb))
    }

    /// If `b` is `a` moved by one vector: that vector (not zero).
    pub fn moved_by(&self, a: &Fields, b: &Fields) -> Option<[f64; 3]> {
        if a.len() != b.len() {
            return None;
        }
        let mut delta: Option<[f64; 3]> = None;
        for ((la, va), (lb, vb)) in a.iter().zip(b) {
            if la != lb {
                return None;
            }
            match (va, vb) {
                (Value::Pos(x), Value::Pos(y)) => {
                    let d = [y[0] - x[0], y[1] - x[1], y[2] - x[2]];
                    match delta {
                        None => delta = Some(d),
                        Some(prev) if self.point_equal(prev, d) => {}
                        Some(_) => return None,
                    }
                }
                _ if self.value_equal(va, vb) => {}
                _ => return None,
            }
        }
        delta.filter(|d| !self.point_equal(*d, [0.0; 3]))
    }
}

/// A number in plain words: at most 4 decimals, no trailing zeros.
pub fn fmt_num(v: f64) -> String {
    lightable_render::svg::num(v)
}

pub fn fmt_point(p: [f64; 3]) -> String {
    if p[2] == 0.0 {
        format!("({}, {})", fmt_num(p[0]), fmt_num(p[1]))
    } else {
        format!("({}, {}, {})", fmt_num(p[0]), fmt_num(p[1]), fmt_num(p[2]))
    }
}

fn fmt_value(v: &Value) -> String {
    match v {
        Value::Name(s) if s.is_empty() => "(none)".into(),
        Value::Name(s) => s.clone(),
        Value::Text(s) => format!("\"{s}\""),
        Value::Num(n, _) => fmt_num(*n),
        Value::Pos(p) | Value::Dir(p) => fmt_point(*p),
    }
}

/// Values only described as "changed" (raw data, too long to show).
const OPAQUE: [&str; 6] = [
    "boundary",
    "pattern lines",
    "other hatch data",
    "data",
    "extended data",
    "formatting",
];

/// For raw `code=value` lists: " (groups 10, 40)", the group codes whose
/// values differ.
fn changed_codes(a: &str, b: &str) -> String {
    let split = |s: &str| -> Vec<(String, String)> {
        s.split(' ')
            .filter_map(|t| t.split_once('='))
            .map(|(c, v)| (c.to_owned(), v.to_owned()))
            .collect()
    };
    let (a, b) = (split(a), split(b));
    let mut codes: Vec<i64> = Vec::new();
    for (x, y) in [(&a, &b), (&b, &a)] {
        for (k, p) in x.iter().enumerate() {
            if y.get(k) != Some(p)
                && let Ok(c) = p.0.parse()
            {
                codes.push(c);
            }
        }
    }
    codes.sort_unstable();
    codes.dedup();
    if codes.is_empty() {
        return String::new();
    }
    let shown: Vec<String> = codes.iter().take(8).map(i64::to_string).collect();
    let more = if codes.len() > 8 { ", ..." } else { "" };
    format!(" (group {}{more})", shown.join(", "))
}

/// Plain-words differences between two field lists.
pub fn describe(tol: &Tolerance, a: &Fields, b: &Fields, out: &mut Vec<String>) {
    if let Some(d) = tol.moved_by(a, b) {
        out.push(format!("moved by {}", fmt_point(d)));
        return;
    }
    // Different vertex counts: say so instead of listing every vertex.
    let count = |f: &Fields| {
        f.iter()
            .find(|(l, _)| l.ends_with("count"))
            .map(|(l, v)| (l.clone(), v.clone()))
    };
    if let (Some((label, va)), Some((_, vb))) = (count(a), count(b))
        && !tol.value_equal(&va, &vb)
    {
        out.push(format!(
            "{label} changed from {} to {}",
            fmt_value(&va),
            fmt_value(&vb)
        ));
        let shared: Vec<&Label> = a
            .iter()
            .filter(|(l, _)| {
                !l.contains(' ') || !l.chars().last().is_some_and(|c| c.is_ascii_digit())
            })
            .map(|(l, _)| l)
            .collect();
        for label in shared {
            if let (Some((_, x)), Some((_, y))) = (
                a.iter().find(|(l, _)| l == label),
                b.iter().find(|(l, _)| l == label),
            ) && !tol.value_equal(x, y)
                && !label.ends_with("count")
            {
                out.push(format!(
                    "{label} changed from {} to {}",
                    fmt_value(x),
                    fmt_value(y)
                ));
            }
        }
        return;
    }
    let mut listed = 0;
    for (label, va) in a {
        let Some((_, vb)) = b.iter().find(|(l, _)| l == label) else {
            out.push(format!("{label} removed"));
            continue;
        };
        if tol.value_equal(va, vb) {
            continue;
        }
        listed += 1;
        if listed > 8 {
            continue;
        }
        match (va, vb) {
            (Value::Pos(x), Value::Pos(y)) => {
                out.push(format!(
                    "{label} moved by {}",
                    fmt_point([y[0] - x[0], y[1] - x[1], y[2] - x[2]])
                ));
            }
            (Value::Name(_), Value::Name(_))
                if label == "formatting" && out.iter().any(|d| d.starts_with("text changed")) => {}
            (Value::Name(x), Value::Name(y)) if OPAQUE.contains(&label.as_ref()) => {
                out.push(format!("{label} changed{}", changed_codes(x, y)));
            }
            _ => out.push(format!(
                "{label} changed from {} to {}",
                fmt_value(va),
                fmt_value(vb)
            )),
        }
    }
    if listed > 8 {
        out.push(format!("and {} more values changed", listed - 8));
    }
    for (label, _) in b {
        if !a.iter().any(|(l, _)| l == label) {
            out.push(format!("{label} added"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOL: Tolerance = Tolerance { length: 1e-6 };

    #[test]
    fn moves_are_one_vector() {
        let a = vec![
            pos("start", Point3::new(0.0, 0.0, 0.0)),
            pos("end", Point3::new(1.0, 0.0, 0.0)),
            len("thickness", 0.0),
        ];
        let b = vec![
            pos("start", Point3::new(10.0, 5.0, 0.0)),
            pos("end", Point3::new(11.0, 5.0, 0.0)),
            len("thickness", 0.0),
        ];
        assert_eq!(TOL.moved_by(&a, &b), Some([10.0, 5.0, 0.0]));
        let mut out = Vec::new();
        describe(&TOL, &a, &b, &mut out);
        assert_eq!(out, ["moved by (10, 5)"]);
        // Different vectors: not a move; each point is described.
        let c = vec![
            pos("start", Point3::new(0.0, 0.0, 0.0)),
            pos("end", Point3::new(1.0, 10.0, 0.0)),
            len("thickness", 0.0),
        ];
        assert_eq!(TOL.moved_by(&a, &c), None);
        out.clear();
        describe(&TOL, &a, &c, &mut out);
        assert_eq!(out, ["end moved by (0, 10)"]);
    }

    #[test]
    fn values_compare_within_tolerance() {
        assert!(TOL.value_equal(&len("r", 1.0).1, &len("r", 1.0 + 1e-7).1));
        assert!(!TOL.value_equal(&len("r", 1.0).1, &len("r", 1.001).1));
        assert!(TOL.value_equal(&angle("a", 0.0).1, &angle("a", 360.0).1));
        assert!(TOL.value_equal(&angle("a", -90.0).1, &angle("a", 270.0).1));
        assert!(!TOL.value_equal(&text("t", "a").1, &text("t", "b").1));
    }

    #[test]
    fn descriptions_quote_text_and_name_names() {
        let mut out = Vec::new();
        describe(
            &TOL,
            &vec![text("text", "LIVING"), len("height", 3.0)],
            &vec![text("text", "LOUNGE"), len("height", 2.5)],
            &mut out,
        );
        assert_eq!(
            out,
            [
                "text changed from \"LIVING\" to \"LOUNGE\"",
                "height changed from 3 to 2.5"
            ]
        );
        out.clear();
        describe(
            &TOL,
            &vec![name("layer", "WALLS")],
            &vec![name("layer", "OTHER")],
            &mut out,
        );
        assert_eq!(out, ["layer changed from WALLS to OTHER"]);
    }

    #[test]
    fn vertex_count_changes_are_summarised() {
        let a = vec![
            plain("vertex count", 2.0),
            pos("vertex 1", Point3::default()),
            pos("vertex 2", Point3::new(1.0, 0.0, 0.0)),
            name("closed", "no"),
        ];
        let b = vec![
            plain("vertex count", 3.0),
            pos("vertex 1", Point3::default()),
            pos("vertex 2", Point3::new(1.0, 0.0, 0.0)),
            pos("vertex 3", Point3::new(1.0, 1.0, 0.0)),
            name("closed", "yes"),
        ];
        let mut out = Vec::new();
        describe(&TOL, &a, &b, &mut out);
        assert_eq!(
            out,
            [
                "vertex count changed from 2 to 3",
                "closed changed from no to yes"
            ]
        );
    }

    #[test]
    fn canonical_pairs_drop_handles_and_normalise_numbers() {
        let pairs = vec![
            lightable_dxf::Pair::new(5, "2A"),
            lightable_dxf::Pair::new(330, "1F"),
            lightable_dxf::Pair::new(102, "{ACAD_REACTORS"),
            lightable_dxf::Pair::new(330, "99"),
            lightable_dxf::Pair::new(102, "}"),
            lightable_dxf::Pair::new(100, "AcDbHatch"),
            lightable_dxf::Pair::new(10, "1.000000"),
            lightable_dxf::Pair::new(2, " ANSI31 "),
        ];
        assert_eq!(
            canonical_pairs(&pairs),
            vec![(10, "1".to_owned()), (2, "ANSI31".to_owned())]
        );
    }
}
