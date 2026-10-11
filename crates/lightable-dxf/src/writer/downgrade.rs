//! R12 downgrades: newer entities become R12 ones (or are dropped, with a
//! warning), and records lose what R12 cannot store.

use std::f64::consts::TAU;

use lightable_geom::curve::{ellipse_points, nurbs_points};

use std::collections::HashMap;

use crate::encoding::CodePage;
use crate::entity::*;
use crate::model::{Color, Drawing, LineWeight, Point3, Z_AXIS};
use crate::table::{Linetype, LinetypeElement, names_equal};
use crate::tokenizer::Pair;

use super::normalize::{Notes, split_raw_table};

/// Unknown entity types R12 can store as they are (minus newer codes).
const R12_RAW_TYPES: [&str; 4] = ["SOLID", "TRACE", "3DFACE", "SHAPE"];

/// Codes newer versions added to records; R12 output drops them.
const NEWER_CODES: [i16; 16] = [
    100, 330, 360, 370, 420, 430, 440, 390, 347, 348, 284, 48, 92, 310, 160, 60,
];

/// Groups each R12 table record may contain.
fn r12_table_codes(table: &str) -> &'static [i16] {
    match table.to_ascii_uppercase().as_str() {
        "VPORT" => &[
            2, 70, 10, 20, 11, 21, 12, 22, 13, 23, 14, 24, 15, 25, 16, 26, 36, 17, 27, 37, 40, 41,
            42, 43, 44, 50, 51, 71, 72, 73, 74, 75, 76, 77, 78,
        ],
        "VIEW" => &[
            2, 70, 40, 10, 20, 41, 11, 21, 31, 12, 22, 32, 42, 43, 44, 50, 71,
        ],
        "UCS" => &[2, 70, 10, 20, 30, 11, 21, 31, 12, 22, 32],
        _ => &[2, 70],
    }
}

/// DIMSTYLE variables R12 knows.
fn r12_dim_variable(code: i16) -> bool {
    matches!(code, 3..=7 | 40..=48 | 140..=147 | 71..=78 | 170..=175)
}

/// Removes `102 {...}` groups from `pairs`.
fn without_groups(pairs: &[Pair]) -> Vec<Pair> {
    let mut depth = 0usize;
    let mut out = Vec::with_capacity(pairs.len());
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
        if depth == 0 {
            out.push(p.clone());
        }
    }
    out
}

/// Downgrades every entity in `list` to R12, in place.
pub(super) fn entities(list: &mut Vec<Entity>, notes: &mut Notes) {
    let mut out = Vec::with_capacity(list.len());
    for e in std::mem::take(list) {
        for mut e in downgrade(e, notes) {
            strip_newer_properties(&mut e, notes);
            out.push(e);
        }
    }
    *list = out;
}

fn strip_newer_properties(e: &mut Entity, notes: &mut Notes) {
    if let Color::Rgb(r, g, b) = e.color {
        e.color = Color::Index(Color::nearest_aci(r, g, b));
        notes.add(
            "{n} true colour(s) replaced by the nearest index colour (R12 has no true colour)",
        );
    }
    if e.lineweight != LineWeight::ByLayer {
        e.lineweight = LineWeight::ByLayer;
        notes.add("{n} entity lineweight(s) dropped (R12 has none)");
    }
    if !e.visible {
        e.visible = true;
        notes.add("{n} invisible entity flag(s) dropped (R12 has none)");
    }
    // Keep extended data (R12 has it); other leftovers are newer codes.
    let xdata_from = e
        .extra
        .iter()
        .position(|p| p.code >= 1000)
        .unwrap_or(e.extra.len());
    notes.add_n(
        "{n} pair(s) of newer entity data dropped (not valid in R12)",
        xdata_from,
    );
    e.extra.drain(..xdata_from);
    match &mut e.kind {
        EntityKind::Polyline(p) => {
            p.vertices
                .iter_mut()
                .for_each(|v| strip_newer_properties(v, notes));
        }
        EntityKind::Insert(i) => {
            i.attributes
                .iter_mut()
                .for_each(|a| strip_newer_properties(a, notes));
        }
        _ => {}
    }
}

/// One entity as zero or more R12 entities.
fn downgrade(e: Entity, notes: &mut Notes) -> Vec<Entity> {
    let mut e = e;
    let kind = std::mem::replace(
        &mut e.kind,
        EntityKind::Unknown(Unknown {
            type_name: String::new(),
            pairs: Vec::new(),
        }),
    );
    match kind {
        EntityKind::LwPolyline(p) => vec![from_lwpolyline(&e, &p)],
        EntityKind::Ellipse(el) => {
            notes.add("{n} ELLIPSE(s) written as polylines (R12 has no ELLIPSE)");
            let points = ellipse_points(
                arr(el.center),
                arr(el.major_axis),
                arr(unit(el.extrusion)),
                el.ratio,
                el.start_param,
                el.end_param,
                TAU / 72.0,
            );
            let full = ((el.end_param - el.start_param).abs() - TAU).abs() < 1e-9
                || (el.end_param - el.start_param).abs() < 1e-12;
            vec![polyline_through(&e, points, full)]
        }
        EntityKind::Spline(s) => {
            let degree = usize::try_from(s.degree).unwrap_or(0);
            let ctrl: Vec<[f64; 3]> = s.control_points.iter().map(|p| arr(*p)).collect();
            let points = nurbs_points(degree, &s.knots, &ctrl, &s.weights, 16)
                .filter(|p| p.len() >= 2)
                .or_else(|| {
                    (s.fit_points.len() >= 2)
                        .then(|| s.fit_points.iter().map(|p| arr(*p)).collect())
                });
            match points {
                Some(points) => {
                    notes.add("{n} SPLINE(s) written as polylines (R12 has no SPLINE)");
                    vec![polyline_through(&e, points, false)]
                }
                None => {
                    notes.add("{n} SPLINE(s) with unusable data not written to R12");
                    Vec::new()
                }
            }
        }
        EntityKind::MText(m) => {
            notes.add("{n} MTEXT(s) written as TEXT lines, without formatting (R12 has no MTEXT)");
            mtext_lines(&e, &m)
        }
        EntityKind::Unknown(u) => {
            if R12_RAW_TYPES
                .iter()
                .any(|t| u.type_name.eq_ignore_ascii_case(t))
            {
                let pairs = without_groups(&u.pairs)
                    .into_iter()
                    .filter(|p| p.code >= 1000 || !NEWER_CODES.contains(&p.code))
                    .collect();
                e.kind = EntityKind::Unknown(Unknown {
                    type_name: u.type_name,
                    pairs,
                });
                vec![e]
            } else {
                notes.add(format!(
                    "{{n}} {} entit(ies) not written (R12 has no such type)",
                    u.type_name
                ));
                Vec::new()
            }
        }
        EntityKind::Insert(mut i) => {
            let mut attrs = Vec::new();
            for a in std::mem::take(&mut i.attributes) {
                attrs.extend(downgrade(a, notes));
            }
            i.attributes = attrs;
            e.kind = EntityKind::Insert(i);
            vec![e]
        }
        other => {
            e.kind = other;
            vec![e]
        }
    }
}

fn arr(p: Point3) -> [f64; 3] {
    [p.x, p.y, p.z]
}

fn unit(p: Point3) -> Point3 {
    let len = (p.x * p.x + p.y * p.y + p.z * p.z).sqrt();
    if len == 0.0 {
        Z_AXIS
    } else {
        Point3::new(p.x / len, p.y / len, p.z / len)
    }
}

/// A child entity with the parent's properties.
fn child(parent: &Entity, kind: EntityKind) -> Entity {
    let mut c = Entity::new(kind);
    c.layer = parent.layer.clone();
    c.color = parent.color;
    c.linetype = parent.linetype.clone();
    c.paper_space = parent.paper_space;
    c
}

fn vertex(location: Point3, flags: i16) -> Vertex {
    Vertex {
        location,
        start_width: 0.0,
        end_width: 0.0,
        bulge: 0.0,
        flags,
        tangent: None,
        indices: [0; 4],
        id: None,
    }
}

fn from_lwpolyline(e: &Entity, p: &LwPolyline) -> Entity {
    let width = p.constant_width.unwrap_or(0.0);
    let vertices = p
        .vertices
        .iter()
        .map(|v| {
            let mut vx = vertex(Point3::new(v.x, v.y, p.elevation), 0);
            vx.start_width = v.start_width;
            vx.end_width = v.end_width;
            vx.bulge = v.bulge;
            child(e, EntityKind::Vertex(vx))
        })
        .collect();
    let mut out = e.clone();
    out.kind = EntityKind::Polyline(Polyline {
        flags: p.flags & (1 | 128),
        location: Point3::new(0.0, 0.0, p.elevation),
        default_start_width: width,
        default_end_width: width,
        thickness: p.thickness,
        mesh_counts: (0, 0),
        smooth_densities: (0, 0),
        curve_type: 0,
        extrusion: p.extrusion,
        vertices,
        seqend: None,
    });
    out
}

/// A polyline through `points` (in WCS): 2D at a constant elevation when
/// possible, otherwise 3D. A closed loop drops its repeated last point.
fn polyline_through(e: &Entity, mut points: Vec<[f64; 3]>, closed: bool) -> Entity {
    if closed && points.len() > 1 {
        points.pop();
    }
    let z = points.first().map_or(0.0, |p| p[2]);
    let flat = points.iter().all(|p| (p[2] - z).abs() < 1e-12);
    let (flags, vertex_flags) = if flat { (0, 0) } else { (8, 32) };
    let vertices = points
        .iter()
        .map(|p| {
            child(
                e,
                EntityKind::Vertex(vertex(Point3::new(p[0], p[1], p[2]), vertex_flags)),
            )
        })
        .collect();
    let mut out = e.clone();
    out.kind = EntityKind::Polyline(Polyline {
        flags: flags | i16::from(closed),
        location: Point3::new(0.0, 0.0, if flat { z } else { 0.0 }),
        default_start_width: 0.0,
        default_end_width: 0.0,
        thickness: 0.0,
        mesh_counts: (0, 0),
        smooth_densities: (0, 0),
        curve_type: 0,
        extrusion: Z_AXIS,
        vertices,
        seqend: None,
    });
    out
}

fn mtext_lines(e: &Entity, m: &MText) -> Vec<Entity> {
    let lines = m.plain_lines();
    let angle = match m.x_direction {
        Some(x) if x.x != 0.0 || x.y != 0.0 => x.y.atan2(x.x),
        _ => m.rotation.to_radians(),
    };
    let (sin, cos) = angle.sin_cos();
    let step = m.height * 5.0 / 3.0 * m.line_spacing_factor;
    let total = step * (lines.len().saturating_sub(1)) as f64 + m.height;
    // Attachment 1–9: rows top/middle/bottom, columns left/center/right.
    let row = (m.attachment.clamp(1, 9) - 1) / 3;
    let column = (m.attachment.clamp(1, 9) - 1) % 3;
    let first_baseline = match row {
        0 => -m.height,
        1 => total / 2.0 - m.height,
        _ => total - m.height,
    };
    lines
        .into_iter()
        .enumerate()
        .filter(|(_, l)| !l.is_empty())
        .map(|(i, line)| {
            let dy = first_baseline - step * i as f64;
            let at = Point3::new(m.insert.x - dy * sin, m.insert.y + dy * cos, m.insert.z);
            let mut t = e.clone();
            // Only the first line keeps the MTEXT's handle.
            if i > 0 {
                t.handle = None;
            }
            t.kind = EntityKind::Text(Text {
                insert: at,
                align_point: (column != 0).then_some(at),
                height: m.height,
                value: line,
                raw_value: None,
                rotation: angle.to_degrees(),
                width_factor: 1.0,
                oblique: 0.0,
                style: m.style.clone(),
                generation_flags: 0,
                horizontal_align: column,
                vertical_align: 0,
                thickness: 0.0,
                extrusion: m.extrusion,
            });
            t
        })
        .collect()
}

/// Tables in R12 form: linetypes without complex elements, layers and
/// styles without newer properties, dim styles with R12 variables only, and
/// raw tables filtered to R12 groups.
pub(super) fn tables(d: &mut Drawing, notes: &mut Notes) {
    let t = &mut d.tables;
    for lt in &mut t.linetypes.entries {
        downgrade_linetype(lt, notes);
    }
    for layer in &mut t.layers.entries {
        if let Color::Rgb(r, g, b) = layer.color {
            layer.color = Color::Index(Color::nearest_aci(r, g, b));
            notes.add(
                "{n} true colour(s) replaced by the nearest index colour (R12 has no true colour)",
            );
        }
        layer.extra.retain(|p| p.code >= 1000);
    }
    for style in &mut t.text_styles.entries {
        style.extra.retain(|p| p.code >= 1000);
    }
    for dim in &mut t.dim_styles.entries {
        dim.variables.retain(|p| r12_dim_variable(p.code));
        dim.extra.retain(|p| p.code >= 1000);
    }
    drop_long_appids(d, notes);
    let t = &mut d.tables;
    for raw in &mut t.other {
        let allowed = r12_table_codes(&raw.name);
        let (_, entries) = split_raw_table(&raw.pairs);
        let mut pairs = Vec::new();
        for entry in entries {
            let mut entry = without_groups(&entry);
            let head = entry.remove(0);
            pairs.push(head);
            pairs.extend(entry.into_iter().filter(|p| allowed.contains(&p.code)));
        }
        raw.pairs = pairs;
    }
}

/// R12 names are at most 31 characters. Application names longer than that
/// (e.g. `ACAD_DSTYLE_DIMARC_LENGTH_SYMBOL`) are dropped, with the extended
/// data that uses them, since other programs drop them anyway.
fn drop_long_appids(d: &mut Drawing, notes: &mut Notes) {
    let too_long = |name: &str| name.trim().chars().count() > 31;
    let mut dropped: Vec<String> = Vec::new();
    for raw in d
        .tables
        .other
        .iter_mut()
        .filter(|r| r.name.eq_ignore_ascii_case("APPID"))
    {
        let (_, entries) = split_raw_table(&raw.pairs);
        let mut kept = Vec::new();
        for entry in entries {
            match entry.iter().find(|p| p.code == 2) {
                Some(name) if too_long(&name.raw) => dropped.push(name.raw.trim().to_owned()),
                _ => kept.extend(entry),
            }
        }
        raw.pairs = kept;
    }
    let mut groups = 0;
    let mut strip = |extra: &mut Vec<Pair>| {
        // XDATA is a sequence of 1001 groups; drop the groups of long names.
        let Some(start) = extra.iter().position(|p| p.code == 1001) else {
            return;
        };
        let mut out: Vec<Pair> = extra[..start].to_vec();
        let mut keep = true;
        for p in &extra[start..] {
            if p.code == 1001 {
                keep = !too_long(&p.raw);
                if !keep {
                    groups += 1;
                }
            }
            if keep {
                out.push(p.clone());
            }
        }
        *extra = out;
    };
    // Everything that can carry extended data.
    fn entity(e: &mut Entity, strip: &mut dyn FnMut(&mut Vec<Pair>)) {
        strip(&mut e.extra);
        match &mut e.kind {
            EntityKind::Polyline(p) => p.vertices.iter_mut().for_each(|v| entity(v, strip)),
            EntityKind::Insert(i) => i.attributes.iter_mut().for_each(|a| entity(a, strip)),
            _ => {}
        }
    }
    for e in d.entities.iter_mut() {
        entity(e, &mut strip);
    }
    for b in &mut d.blocks.entries {
        strip(&mut b.extra);
        for e in &mut b.entities {
            entity(e, &mut strip);
        }
    }
    let t = &mut d.tables;
    t.layers
        .entries
        .iter_mut()
        .for_each(|e| strip(&mut e.extra));
    t.linetypes
        .entries
        .iter_mut()
        .for_each(|e| strip(&mut e.extra));
    t.text_styles
        .entries
        .iter_mut()
        .for_each(|e| strip(&mut e.extra));
    t.dim_styles
        .entries
        .iter_mut()
        .for_each(|e| strip(&mut e.extra));
    notes.add_n(
        "{n} application name(s) longer than 31 characters not written to R12",
        dropped.len(),
    );
    notes.add_n(
        "{n} extended data group(s) of those applications dropped",
        groups,
    );
}

fn downgrade_linetype(lt: &mut Linetype, notes: &mut Notes) {
    if lt.elements.iter().any(|e| e.flags != 0) {
        notes.add("{n} complex linetype(s) written as simple dashes (R12 has no shapes or text in linetypes)");
        for e in &mut lt.elements {
            *e = LinetypeElement {
                length: e.length,
                scale: 1.0,
                ..LinetypeElement::default()
            };
        }
    }
    lt.extra.retain(|p| p.code >= 1000);
}

/// Adds subclass markers to an unknown entity that has none (it came from
/// an R12 file) so it is valid in R2018: `AcDbEntity` after the owner, and
/// the type's own marker after the common properties when it is known.
pub(super) fn upgrade_unknown_markers(type_name: &str, pairs: &mut Vec<Pair>) {
    if pairs.iter().any(|p| p.code == 100) {
        return;
    }
    let own = match type_name.to_ascii_uppercase().as_str() {
        "SOLID" | "TRACE" => Some("AcDbTrace"),
        "3DFACE" => Some("AcDbFace"),
        "SHAPE" => Some("AcDbShape"),
        _ => None,
    };
    let Some(owner_at) = pairs.iter().position(|p| p.code == 330) else {
        return;
    };
    pairs.insert(owner_at + 1, Pair::new(100, "AcDbEntity"));
    if let Some(own) = own {
        let mut at = owner_at + 2;
        while pairs
            .get(at)
            .is_some_and(|p| matches!(p.code, 8 | 6 | 62 | 67 | 370 | 420 | 48 | 60))
        {
            at += 1;
        }
        pairs.insert(at, Pair::new(100, own));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markers_are_added_to_r12_records() {
        let mut pairs = vec![
            Pair::new(5, "1F"),
            Pair::new(330, "17"),
            Pair::new(8, "0"),
            Pair::new(62, "1"),
            Pair::new(10, "0"),
        ];
        upgrade_unknown_markers("SOLID", &mut pairs);
        let codes: Vec<(i16, &str)> = pairs.iter().map(|p| (p.code, p.raw.as_str())).collect();
        assert_eq!(
            codes,
            [
                (5, "1F"),
                (330, "17"),
                (100, "AcDbEntity"),
                (8, "0"),
                (62, "1"),
                (100, "AcDbTrace"),
                (10, "0")
            ]
        );
    }
}

/// A valid R12 name for `name`: at most 31 characters, every character in
/// the code page (others become `_`). `None` if `name` is already valid.
fn r12_name(name: &str, code_page: CodePage) -> Option<String> {
    let fits = |c: char| c.is_ascii() || code_page.encode(&c.to_string()).1 == 0;
    if name.chars().count() <= 31 && name.chars().all(fits) {
        return None;
    }
    Some(
        name.chars()
            .map(|c| if fits(c) { c } else { '_' })
            .take(31)
            .collect(),
    )
}

/// Renames table entries and blocks whose names R12 cannot hold, keeping
/// names unique, and returns the renames (old name, upper case -> new).
fn renames<'a>(
    names: impl Iterator<Item = &'a str>,
    code_page: CodePage,
) -> HashMap<String, String> {
    let names: Vec<&str> = names.collect();
    let mut taken: Vec<String> = names.iter().map(|n| n.to_ascii_uppercase()).collect();
    let mut map = HashMap::new();
    for name in names {
        let Some(mut new) = r12_name(name, code_page) else {
            continue;
        };
        let base: String = new.chars().take(27).collect();
        let mut n = 1;
        while taken.iter().any(|t| names_equal(t, &new)) {
            new = format!("{base}~{n}");
            n += 1;
        }
        taken.push(new.to_ascii_uppercase());
        map.insert(name.to_ascii_uppercase(), new);
    }
    map
}

fn renamed(map: &HashMap<String, String>, name: &mut String) {
    if let Some(new) = map.get(&name.to_ascii_uppercase()) {
        *name = new.clone();
    }
}

/// Gives layers, linetypes, text styles, dimension styles and blocks valid
/// R12 names, and updates every reference to them.
pub(super) fn r12_names(d: &mut Drawing, code_page: CodePage, notes: &mut Notes) {
    let t = &d.tables;
    let layers = renames(t.layers.iter().map(|e| e.name.as_str()), code_page);
    let linetypes = renames(t.linetypes.iter().map(|e| e.name.as_str()), code_page);
    let styles = renames(t.text_styles.iter().map(|e| e.name.as_str()), code_page);
    let dim_styles = renames(t.dim_styles.iter().map(|e| e.name.as_str()), code_page);
    let blocks = renames(d.blocks.iter().map(|b| b.name.as_str()), code_page);
    let count = layers.len() + linetypes.len() + styles.len() + dim_styles.len() + blocks.len();
    if count == 0 {
        return;
    }
    notes.add_n(
        "{n} name(s) R12 cannot hold (too long, or characters outside the code page) renamed",
        count,
    );
    let t = &mut d.tables;
    t.layers.entries.iter_mut().for_each(|e| {
        renamed(&layers, &mut e.name);
        renamed(&linetypes, &mut e.linetype);
    });
    t.linetypes
        .entries
        .iter_mut()
        .for_each(|e| renamed(&linetypes, &mut e.name));
    t.text_styles
        .entries
        .iter_mut()
        .for_each(|e| renamed(&styles, &mut e.name));
    t.dim_styles
        .entries
        .iter_mut()
        .for_each(|e| renamed(&dim_styles, &mut e.name));
    fn entity(e: &mut Entity, maps: &[&HashMap<String, String>; 5]) {
        let [layers, linetypes, styles, dim_styles, blocks] = maps;
        renamed(layers, &mut e.layer);
        renamed(linetypes, &mut e.linetype);
        match &mut e.kind {
            EntityKind::Text(t) => renamed(styles, &mut t.style),
            EntityKind::MText(m) => renamed(styles, &mut m.style),
            EntityKind::Attrib(a) => renamed(styles, &mut a.text.style),
            EntityKind::AttDef(a) => renamed(styles, &mut a.text.style),
            EntityKind::Insert(i) => {
                renamed(blocks, &mut i.block);
                i.attributes.iter_mut().for_each(|a| entity(a, maps));
            }
            EntityKind::Dimension(dim) => {
                renamed(blocks, &mut dim.block);
                renamed(dim_styles, &mut dim.style);
            }
            EntityKind::Polyline(p) => p.vertices.iter_mut().for_each(|v| entity(v, maps)),
            _ => {}
        }
    }
    let maps = [&layers, &linetypes, &styles, &dim_styles, &blocks];
    for e in &mut d.entities {
        entity(e, &maps);
    }
    for b in &mut d.blocks.entries {
        renamed(&blocks, &mut b.name);
        renamed(&layers, &mut b.layer);
        for e in &mut b.entities {
            entity(e, &maps);
        }
    }
    for var in &mut d.header.variables {
        let map = match var.name.to_ascii_uppercase().as_str() {
            "$CLAYER" => &layers,
            "$CELTYPE" => &linetypes,
            "$TEXTSTYLE" => &styles,
            "$DIMSTYLE" => &dim_styles,
            _ => continue,
        };
        for p in &mut var.pairs {
            if let Some(new) = map.get(&p.raw.trim().to_ascii_uppercase()) {
                p.raw = new.clone();
            }
        }
    }
}

#[cfg(test)]
mod name_tests {
    use super::*;

    #[test]
    fn r12_names_fit_the_code_page_and_length() {
        let cp = CodePage::ANSI_1252;
        assert_eq!(r12_name("Wände", cp), None);
        assert_eq!(r12_name("配管", cp).as_deref(), Some("__"));
        assert_eq!(r12_name(&"x".repeat(40), cp).map(|n| n.len()), Some(31));
        assert_eq!(r12_name("配管", CodePage(932)), None);
        let map = renames(["配管", "__", "管配"].into_iter(), cp);
        assert_eq!(map["配管"], "__~1");
        assert_eq!(map["管配"], "__~2");
    }
}
