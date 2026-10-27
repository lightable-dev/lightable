//! One-line headlines for changes, in plain words: "Circle Ø12 moved 5 mm
//! right", "Circle diameter changed from 12 mm to 14 mm", "Text changed
//! from M12 to M16". Every headline is computed from the values compared,
//! so it is mechanically true; lengths carry a unit only when the drawing's
//! `$INSUNITS` gives one.
//!
//! Also decides what counts as revision markup (revision clouds and title
//! block text), which the app counts apart from design changes.

use lightable_dxf::{Drawing, Entity, EntityKind};

use crate::features::{Fields, NumKind, Tolerance, Value, fmt_num, fmt_point};

/// The length unit named by `$INSUNITS`, if it names one.
pub fn units_of(d: &Drawing) -> Option<&'static str> {
    Some(match d.header.insunits()? {
        1 => "in",
        2 => "ft",
        4 => "mm",
        5 => "cm",
        6 => "m",
        7 => "km",
        8 => "µin",
        9 => "mil",
        10 => "yd",
        13 => "µm",
        14 => "dm",
        _ => return None,
    })
}

/// A length with its unit, if known: "5 mm" or "5".
pub fn length(v: f64, units: Option<&str>) -> String {
    match units {
        Some(u) => format!("{} {u}", fmt_num(v)),
        None => fmt_num(v),
    }
}

/// "5 mm right", "3 mm left and 2 mm up", for a displacement.
pub fn displacement(d: [f64; 3], tol: f64, units: Option<&str>) -> String {
    let mut parts = Vec::new();
    if d[0].abs() > tol {
        parts.push(format!(
            "{} {}",
            length(d[0].abs(), units),
            if d[0] > 0.0 { "right" } else { "left" }
        ));
    }
    if d[1].abs() > tol {
        parts.push(format!(
            "{} {}",
            length(d[1].abs(), units),
            if d[1] > 0.0 { "up" } else { "down" }
        ));
    }
    if d[2].abs() > tol {
        parts.push(format!("{} along Z", length(d[2], units)));
    }
    parts.join(" and ")
}

fn text_of(e: &Entity) -> Option<String> {
    match &e.kind {
        EntityKind::Text(t) => Some(t.value.clone()),
        EntityKind::Attrib(a) => Some(a.text.value.clone()),
        EntityKind::MText(m) => Some(m.plain_lines().join(" / ")),
        _ => None,
    }
}

/// What the entity is, in a few words: "Circle Ø12", "Arc R5", "Text
/// “M12”", "Block BOLT". `plain` leaves out the value that changed (so the
/// headline says "Circle diameter changed", not "Circle Ø12 diameter").
fn subject(e: &Entity, plain: bool) -> String {
    match &e.kind {
        EntityKind::Line(_) => "Line".into(),
        EntityKind::Circle(_) if plain => "Circle".into(),
        EntityKind::Circle(c) => format!("Circle Ø{}", fmt_num(2.0 * c.radius)),
        EntityKind::Arc(a) if !plain => format!("Arc R{}", fmt_num(a.radius)),
        EntityKind::Arc(_) => "Arc".into(),
        EntityKind::LwPolyline(_) | EntityKind::Polyline(_) => "Polyline".into(),
        EntityKind::Ellipse(_) => "Ellipse".into(),
        EntityKind::Point(_) => "Point".into(),
        EntityKind::Text(_)
        | EntityKind::MText(_)
        | EntityKind::Attrib(_)
        | EntityKind::AttDef(_)
            if plain =>
        {
            "Text".into()
        }
        EntityKind::Text(_) | EntityKind::MText(_) | EntityKind::Attrib(_) => {
            format!("Text “{}”", text_of(e).unwrap_or_default())
        }
        EntityKind::AttDef(a) => format!("Attribute definition {}", a.tag),
        EntityKind::Insert(i) => format!("Block {}", i.block),
        EntityKind::Dimension(_) => "Dimension".into(),
        EntityKind::Spline(_) => "Spline".into(),
        EntityKind::Vertex(_) => "Vertex".into(),
        EntityKind::Unknown(u) => {
            let t = u.type_name.to_lowercase();
            let mut c = t.chars();
            c.next().map_or(String::new(), |f| {
                f.to_uppercase().collect::<String>() + c.as_str()
            })
        }
    }
}

/// Vertices of a polyline as (x, y, bulge), and whether it is closed.
fn vertices(e: &Entity) -> Option<(Vec<[f64; 3]>, bool)> {
    match &e.kind {
        EntityKind::LwPolyline(p) => Some((
            p.vertices.iter().map(|v| [v.x, v.y, v.bulge]).collect(),
            p.is_closed(),
        )),
        EntityKind::Polyline(p) => Some((
            p.vertices
                .iter()
                .filter_map(|v| match &v.kind {
                    EntityKind::Vertex(v) => Some([v.location.x, v.location.y, v.bulge]),
                    _ => None,
                })
                .collect(),
            p.is_closed(),
        )),
        _ => None,
    }
}

/// A revision cloud: a closed polyline of at least three segments, all of
/// them arcs.
pub fn is_revision_cloud(e: &Entity) -> bool {
    vertices(e).is_some_and(|(v, closed)| closed && v.len() >= 3 && v.iter().all(|p| p[2] != 0.0))
}

/// Revision markup rather than a design change: revision clouds, and
/// anything on a layer named for revisions, the title block or the
/// border: `REV` exactly, or a name starting with `REV-`, `REV_`,
/// `REVISION`, `TITLE` or `BORDER` (ignoring case). Layers such as
/// `REVIEW` or `REVOLVE` are not markup.
pub fn is_markup(e: &Entity) -> bool {
    is_revision_cloud(e) || is_markup_layer(&e.layer)
}

/// The layer-name part of [`is_markup`].
pub fn is_markup_layer(layer: &str) -> bool {
    let l = layer.trim().to_uppercase();
    l == "REV"
        || ["REV-", "REV_", "REVISION", "TITLE", "BORDER"]
            .iter()
            .any(|p| l.starts_with(p))
}

/// Distance from `p` to the segment `a`–`b`, and whether its foot lies on
/// the segment.
fn on_segment(p: [f64; 2], a: [f64; 2], b: [f64; 2], tol: f64) -> bool {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let len2 = dx * dx + dy * dy;
    if len2 == 0.0 {
        return false;
    }
    let t = ((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / len2;
    let off = ((p[0] - a[0]) * dy - (p[1] - a[1]) * dx).abs() / len2.sqrt();
    (-1e-9..=1.0 + 1e-9).contains(&t) && off <= tol
}

/// How a polyline's vertex list changed, when it can be said simply:
/// "corner at (160, 100) chamfered 10 × 10 mm", "2 vertices added".
fn vertex_phrase(
    a: &Entity,
    b: &Entity,
    tol: f64,
    units: Option<&str>,
) -> Option<(String, Option<String>)> {
    let ((va, closed), (vb, _)) = (vertices(a)?, vertices(b)?);
    let near = |p: [f64; 3], q: [f64; 3]| (p[0] - q[0]).abs() <= tol && (p[1] - q[1]).abs() <= tol;
    let removed: Vec<usize> = (0..va.len())
        .filter(|&i| !vb.iter().any(|q| near(va[i], *q)))
        .collect();
    let added: Vec<usize> = (0..vb.len())
        .filter(|&j| !va.iter().any(|p| near(vb[j], *p)))
        .collect();
    if removed.len() == 1 && added.len() == 2 && va.len() >= 3 {
        let i = removed[0];
        let n = va.len();
        let at = |k: usize| [va[k][0], va[k][1]];
        let v = at(i);
        if !closed && (i == 0 || i == n - 1) {
            return None;
        }
        let (prev, next) = (at((i + n - 1) % n), at((i + 1) % n));
        let (p, q) = (
            [vb[added[0]][0], vb[added[0]][1]],
            [vb[added[1]][0], vb[added[1]][1]],
        );
        let straight = vb[added[0]][2] == 0.0 && (added[1] == added[0] + 1);
        let on = |x: [f64; 2]| on_segment(x, v, prev, tol) || on_segment(x, v, next, tol);
        let one_each = (on_segment(p, v, prev, tol) && on_segment(q, v, next, tol))
            || (on_segment(p, v, next, tol) && on_segment(q, v, prev, tol));
        if on(p) && on(q) && one_each {
            let d = |x: [f64; 2]| (x[0] - v[0]).hypot(x[1] - v[1]);
            let corner = fmt_point([v[0], v[1], 0.0]);
            // The headline names the change; where it is goes in the details.
            return Some(if straight {
                let size = format!("{} × {}", fmt_num(d(p)), length(d(q), units));
                (
                    format!("corner chamfered {size}"),
                    Some(format!("corner at {corner} chamfered {size}")),
                )
            } else {
                (
                    "corner rounded".to_owned(),
                    Some(format!("corner at {corner} rounded")),
                )
            });
        }
    }
    let plural = |k: usize| if k == 1 { "vertex" } else { "vertices" };
    let phrase = match (removed.len(), added.len()) {
        (0, 0) => return None,
        (0, k) => format!("{k} {} added", plural(k)),
        (k, 0) => format!("{k} {} removed", plural(k)),
        (r, k) => format!("{k} {} added and {r} removed", plural(k)),
    };
    Some((phrase, None))
}

fn value_words(v: &Value, units: Option<&str>) -> String {
    match v {
        Value::Name(s) if s.is_empty() => "(none)".into(),
        Value::Name(s) | Value::Text(s) => s.clone(),
        Value::Num(n, NumKind::Length) => length(*n, units),
        Value::Num(n, NumKind::Angle) => format!("{}°", fmt_num(*n)),
        Value::Num(n, _) => fmt_num(*n),
        // Coordinates belong in the details, not in a headline.
        Value::Pos(_) | Value::Dir(_) => "elsewhere".into(),
    }
}

/// "colour changed from 8 to 6"; a position or direction just "changed"
/// (its coordinates are in the details).
fn from_to(label: &str, a: &Value, b: &Value, units: Option<&str>) -> String {
    match (a, b) {
        (Value::Pos(_) | Value::Dir(_), _) | (_, Value::Pos(_) | Value::Dir(_)) => {
            format!("{label} changed")
        }
        _ => format!(
            "{label} changed from {} to {}",
            value_words(a, units),
            value_words(b, units)
        ),
    }
}

/// A changed entity in words: the headline, and detail lines the headline
/// leaves out (where a corner is, for example).
pub struct Changed {
    pub headline: String,
    pub details: Vec<String>,
}

/// The headline of a changed entity.
#[allow(clippy::too_many_arguments)]
pub fn changed(
    tol: &Tolerance,
    a: &Entity,
    a_common: &Fields,
    a_shape: &Fields,
    b: &Entity,
    b_common: &Fields,
    b_shape: &Fields,
    units: Option<&str>,
) -> Changed {
    let find = |f: &Fields, label: &str| f.iter().find(|(l, _)| l == label).map(|(_, v)| v.clone());
    let differs = |label: &str, x: &Fields, y: &Fields| match (find(x, label), find(y, label)) {
        (Some(p), Some(q)) => !tol.value_equal(&p, &q),
        (p, q) => p.is_some() != q.is_some(),
    };
    let mut details = Vec::new();
    let circle = matches!(a.kind, EntityKind::Circle(_));
    let dimension = matches!(a.kind, EntityKind::Dimension(_));
    let text_changed = text_of(a).is_some() && differs("text", a_shape, b_shape);
    let radius_changed = differs("radius", a_shape, b_shape);
    // A dimension is described by the value it shows; its points are
    // details.
    if dimension && differs("measurement", a_shape, b_shape) {
        let words =
            |f: &Fields| find(f, "measurement").map_or("(none)".into(), |v| value_words(&v, units));
        let mut phrases = vec![format!(
            "changed from {} to {}",
            words(a_shape),
            words(b_shape)
        )];
        for (label, va) in a_common {
            if let Some(vb) = find(b_common, label)
                && !tol.value_equal(va, &vb)
            {
                phrases.push(from_to(label, va, &vb, units));
            }
        }
        return Changed {
            headline: format!("Dimension {}", phrases.join("; ")),
            details,
        };
    }
    let plain = text_changed || radius_changed;
    let mut phrases: Vec<String> = Vec::new();
    let mut vertex_headline = None;
    if let Some(d) = tol.moved_by(a_shape, b_shape) {
        phrases.push(format!("moved {}", displacement(d, tol.length, units)));
    } else {
        let mut vertex_words = None;
        if vertices(a).is_some() {
            vertex_words = vertex_phrase(a, b, tol.length, units);
        }
        let mut seen_vertices = false;
        for (label, va) in a_shape {
            let Some(vb) = find(b_shape, label) else {
                continue;
            };
            if tol.value_equal(va, &vb) || (label == "formatting" && text_changed) {
                continue;
            }
            let is_vertex_field = label.starts_with("vertex")
                || label.starts_with("bulge")
                || label.starts_with("width ");
            if is_vertex_field && let Some((words, detail)) = &vertex_words {
                if !seen_vertices {
                    phrases.push(words.clone());
                    details.extend(detail.clone());
                    vertex_headline = Some(words.clone());
                    seen_vertices = true;
                }
                continue;
            }
            let label_s = label.as_ref();
            let phrase = match (label_s, va, &vb) {
                ("radius", Value::Num(x, _), Value::Num(y, _)) if circle => {
                    format!(
                        "diameter changed from {} to {}",
                        length(2.0 * x, units),
                        length(2.0 * y, units)
                    )
                }
                ("text", _, _) if text_changed => {
                    format!(
                        "changed from {} to {}",
                        value_words(va, units),
                        value_words(&vb, units)
                    )
                }
                (_, Value::Pos(x), Value::Pos(y)) => format!(
                    "{label_s} moved {}",
                    displacement([y[0] - x[0], y[1] - x[1], y[2] - x[2]], tol.length, units)
                ),
                (
                    "boundary" | "pattern lines" | "other hatch data" | "data" | "extended data"
                    | "formatting",
                    _,
                    _,
                ) => {
                    format!("{label_s} changed")
                }
                _ => from_to(label_s, va, &vb, units),
            };
            // What the entity is first (its size, its text), then where.
            if label_s == "radius" || label_s == "text" {
                phrases.insert(0, phrase);
            } else {
                phrases.push(phrase);
            }
        }
        if !seen_vertices && let Some((w, detail)) = vertex_words {
            details.extend(detail);
            if phrases.is_empty() {
                vertex_headline = Some(w.clone());
            }
            phrases.insert(0, w);
        }
    }
    for (label, va) in a_common {
        let Some(vb) = find(b_common, label) else {
            continue;
        };
        if !tol.value_equal(va, &vb) {
            phrases.push(from_to(label, va, &vb, units));
        }
    }
    // A hatch whose pattern changed: the pattern says it; its fill flag,
    // line definitions and other data follow from it (they stay in the
    // details).
    let hatch =
        matches!(&a.kind, EntityKind::Unknown(u) if u.type_name.eq_ignore_ascii_case("HATCH"));
    if hatch && phrases.iter().any(|p| p.starts_with("pattern changed")) {
        phrases.retain(|p| {
            !["solid fill", "pattern lines", "other hatch data"]
                .iter()
                .any(|derived| p.starts_with(derived))
        });
    }
    if phrases.is_empty() {
        phrases.push("changed".into());
    }
    // A polyline whose only change is a chamfered or rounded corner:
    // "Corner chamfered 10 × 10 mm".
    if phrases.len() == 1
        && let Some(w) = vertex_headline
        && w.starts_with("corner")
    {
        return Changed {
            headline: sentence(&w),
            details,
        };
    }
    // "Text changed from M12 to M16": the subject already says what changed.
    Changed {
        headline: format!("{} {}", subject(a, plain), phrases.join("; ")),
        details,
    }
}

/// The headline of an added or deleted entity.
pub fn added_or_deleted(e: &Entity, added: bool) -> String {
    let what = if is_revision_cloud(e) {
        "Revision cloud".to_owned()
    } else if let Some(m) = markup_shape(e) {
        m.to_owned()
    } else {
        subject(e, false)
    };
    format!("{what} {}", if added { "added" } else { "deleted" })
}

/// Lines and polylines of revision markup in plain words, from their
/// shape only: "Markup line", "Markup triangle" (a closed polyline of three
/// straight sides), "Markup outline" (another closed one), "Markup lines"
/// (an open polyline). Not for design content, nor for revision clouds.
fn markup_shape(e: &Entity) -> Option<&'static str> {
    if !is_markup_layer(&e.layer) || is_revision_cloud(e) {
        return None;
    }
    if matches!(e.kind, EntityKind::Line(_)) {
        return Some("Markup line");
    }
    let (v, closed) = vertices(e)?;
    let straight = v.iter().all(|p| p[2] == 0.0);
    Some(match (closed, v.len()) {
        (true, 3) if straight => "Markup triangle",
        (true, _) => "Markup outline",
        (false, _) => "Markup lines",
    })
}

/// A sentence from a detail: first letter upper case.
pub fn sentence(s: &str) -> String {
    let mut c = s.chars();
    c.next().map_or(String::new(), |f| {
        f.to_uppercase().collect::<String>() + c.as_str()
    })
}

/// The Details view of a change in plain labels: what object it is
/// ("Object: circle (handle A5), layer HOLES") and its exact values with
/// arrows and units ("Diameter 13.5 → 17.5 mm; centre moved 5 mm left,
/// 5 mm up"). A circle whose headline speaks of its diameter is given by
/// diameter here too. The change's own `details` (and the JSON export)
/// are unchanged.
pub fn exact(c: &crate::Change, unit: Option<&str>) -> (String, String) {
    let noun = match c.entity_type.as_str() {
        "LWPOLYLINE" | "POLYLINE" => "polyline".to_owned(),
        "MTEXT" => "text".to_owned(),
        "INSERT" => "block reference".to_owned(),
        "LAYER" => "layer".to_owned(),
        t => t.to_lowercase(),
    };
    let mut object = format!("Object: {noun}");
    if let Some(h) = c.handle() {
        object.push_str(&format!(" (handle {h})"));
    }
    let layer = c.layer();
    if !layer.is_empty() && c.place != crate::Place::Layers {
        object.push_str(&format!(", layer {layer}"));
    }
    if let crate::Place::Block(b) = &c.place {
        object.push_str(&format!(", in block {b}"));
    }
    let diameter = c.headline.contains("diameter");
    let with_unit = |v: &str| match unit {
        Some(u) if v.parse::<f64>().is_ok() => format!("{v} {u}"),
        _ => v.to_owned(),
    };
    // Lengths get the unit; other values (counts, names, colours) do not.
    let lengths = [
        "radius",
        "diameter",
        "measurement",
        "length",
        "width",
        "height",
        "thickness",
        "text height",
        "elevation",
        "offset",
    ];
    // Values that changed first, then what moved.
    let (mut values, mut moves) = (Vec::new(), Vec::new());
    for d in c.details.iter().filter(|d| *d != "not shown") {
        // "radius changed from 6.75 to 8.75" (and "text changed from "a"
        // to "b"").
        if let Some((label, rest)) = d.split_once(" changed from ")
            && let Some((from, to)) = rest.split_once(" to ")
        {
            let (label, from, to) = match (label, from.parse::<f64>(), to.parse::<f64>()) {
                ("radius", Ok(a), Ok(b)) if diameter => {
                    ("diameter", fmt_num(2.0 * a), fmt_num(2.0 * b))
                }
                _ => (label, from.to_owned(), to.to_owned()),
            };
            let to = if lengths.contains(&label) {
                with_unit(&to)
            } else {
                to
            };
            values.push(format!("{label} {from} → {to}"));
            continue;
        }
        // "centre moved by (-5, 5)", "moved by (0, 600)".
        if let Some((what, v)) = d.split_once("moved by ")
            && let Some(v) = v.strip_prefix('(').and_then(|v| v.strip_suffix(')'))
        {
            let parts: Vec<f64> = v.split(", ").filter_map(|x| x.parse().ok()).collect();
            if parts.len() >= 2 {
                let mut steps = Vec::new();
                for (k, (pos, neg)) in [("right", "left"), ("up", "down"), ("up (Z)", "down (Z)")]
                    .iter()
                    .enumerate()
                {
                    if let Some(&x) = parts.get(k).filter(|x| **x != 0.0) {
                        steps.push(format!(
                            "{} {}",
                            with_unit(&fmt_num(x.abs())),
                            if x > 0.0 { pos } else { neg }
                        ));
                    }
                }
                let what = what.trim();
                let label = if what.is_empty() {
                    "moved".to_owned()
                } else {
                    format!("{what} moved")
                };
                moves.push(format!("{} {}", label, steps.join(", ")));
                continue;
            }
        }
        values.push(d.clone());
    }
    values.extend(moves);
    let values = values.join("; ");
    let mut chars = values.chars();
    let values = chars.next().map_or(String::new(), |f| {
        f.to_uppercase().collect::<String>() + chars.as_str()
    });
    (object, values)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markup_lines_and_polylines_in_plain_words() {
        use lightable_dxf::{Line, LwPolyline, LwVertex, Point3};
        let poly = |layer: &str, pts: &[(f64, f64)], closed: bool| {
            let mut e = Entity::new(EntityKind::LwPolyline(LwPolyline {
                flags: i16::from(closed),
                constant_width: None,
                elevation: 0.0,
                thickness: 0.0,
                extrusion: lightable_dxf::Z_AXIS,
                vertices: pts
                    .iter()
                    .map(|&(x, y)| LwVertex {
                        x,
                        y,
                        ..Default::default()
                    })
                    .collect(),
            }));
            e.layer = layer.into();
            e
        };
        let tri = [(0.0, 0.0), (10.0, 0.0), (5.0, 9.0)];
        assert_eq!(
            added_or_deleted(&poly("REVISION", &tri, true), true),
            "Markup triangle added"
        );
        assert_eq!(
            added_or_deleted(&poly("TITLE", &tri[..2], false), true),
            "Markup lines added"
        );
        let square = [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)];
        assert_eq!(
            added_or_deleted(&poly("BORDER", &square, true), false),
            "Markup outline deleted"
        );
        let mut line = Entity::new(EntityKind::Line(Line {
            start: Point3::new(0.0, 0.0, 0.0),
            end: Point3::new(1.0, 0.0, 0.0),
            thickness: 0.0,
            extrusion: lightable_dxf::Z_AXIS,
        }));
        line.layer = "TITLE".into();
        assert_eq!(added_or_deleted(&line, true), "Markup line added");
        // Design content keeps its CAD name.
        assert_eq!(
            added_or_deleted(&poly("OUTLINE", &tri, true), true),
            "Polyline added"
        );
    }

    #[test]
    fn displacements_name_their_directions() {
        assert_eq!(
            displacement([5.0, 0.0, 0.0], 1e-9, Some("mm")),
            "5 mm right"
        );
        assert_eq!(
            displacement([-3.0, 2.5, 0.0], 1e-9, None),
            "3 left and 2.5 up"
        );
        assert_eq!(
            displacement([0.0, -1.0, 0.0], 1e-9, Some("in")),
            "1 in down"
        );
    }

    #[test]
    fn markup_layers_are_named_for_revisions_titles_or_borders() {
        for yes in [
            "REV",
            "rev",
            "REV-1",
            "REV_TABLE",
            "REVISION",
            "Revisions",
            "TITLE",
            "TITLEBLOCK",
            "BORDER",
        ] {
            assert!(is_markup_layer(yes), "{yes}");
        }
        for no in [
            "REVIEW", "REVOLVE", "REVERSE", "PREV", "SUBTITLE", "0", "HOLES",
        ] {
            assert!(!is_markup_layer(no), "{no}");
        }
    }

    #[test]
    fn segments() {
        assert!(on_segment([5.0, 0.0], [0.0, 0.0], [10.0, 0.0], 1e-9));
        assert!(!on_segment([11.0, 0.0], [0.0, 0.0], [10.0, 0.0], 1e-9));
        assert!(!on_segment([5.0, 1.0], [0.0, 0.0], [10.0, 0.0], 1e-9));
    }

    #[test]
    fn exact_details_in_plain_labels_with_units() {
        let c = crate::Change {
            kind: crate::ChangeKind::Changed,
            place: crate::Place::Model,
            entity_type: "CIRCLE".into(),
            details: vec![
                "centre moved by (-5, 5)".into(),
                "radius changed from 6.75 to 8.75".into(),
            ],
            headline: "Circle diameter changed from 13.5 mm to 17.5 mm".into(),
            geometry: true,
            shown: true,
            certain: true,
            old: Some(crate::EntityRef {
                handle: Some("A5".into()),
                index: 0,
                layer: "HOLES".into(),
            }),
            new: None,
            bounds: None,
            used_by_old: Vec::new(),
            used_by_new: Vec::new(),
            markup: false,
            revision_cloud: false,
        };
        let (object, values) = exact(&c, Some("mm"));
        assert_eq!(object, "Object: circle (handle A5), layer HOLES");
        assert_eq!(
            values,
            "Diameter 13.5 → 17.5 mm; centre moved 5 mm left, 5 mm up"
        );
        let d = crate::Change {
            entity_type: "DIMENSION".into(),
            details: vec![
                "measurement changed from 20 to 25".into(),
                "text changed from \"A\" to \"B\"".into(),
            ],
            ..c
        };
        let (_, values) = exact(&d, Some("mm"));
        assert_eq!(values, "Measurement 20 → 25 mm; text \"A\" → \"B\"");
    }
}
