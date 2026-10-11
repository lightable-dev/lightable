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

/// "color changed from 8 to 6"; a position or direction just "changed"
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
    } else {
        subject(e, false)
    };
    format!("{what} {}", if added { "added" } else { "deleted" })
}

/// A sentence from a detail: first letter upper case.
pub fn sentence(s: &str) -> String {
    let mut c = s.chars();
    c.next().map_or(String::new(), |f| {
        f.to_uppercase().collect::<String>() + c.as_str()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
