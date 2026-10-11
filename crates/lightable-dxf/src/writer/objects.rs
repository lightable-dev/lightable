//! Structure R2018 files need: the layout objects that tie model and paper
//! space to their block records, and default table records.

use crate::model::{Drawing, Handle};
use crate::table::names_equal;
use crate::tokenizer::Pair;

use super::normalize::Handles;

fn p(code: i16, value: impl Into<String>) -> Pair {
    Pair::new(code, value)
}

/// The subclass marker of a raw table's records.
pub(super) fn table_record_marker(table: &str) -> &'static str {
    match table.to_ascii_uppercase().as_str() {
        "VPORT" => "AcDbViewportTableRecord",
        "VIEW" => "AcDbViewTableRecord",
        "UCS" => "AcDbUCSTableRecord",
        "APPID" => "AcDbRegAppTableRecord",
        _ => "AcDbSymbolTableRecord",
    }
}

/// A default `*ACTIVE` viewport record (as pairs, starting with `0 VPORT`).
pub(super) fn default_vport(r12: bool) -> Vec<Pair> {
    let mut pairs = vec![
        p(0, "VPORT"),
        p(2, if r12 { "*ACTIVE" } else { "*Active" }),
        p(70, "0"),
    ];
    let values: [(i16, &str); 27] = [
        (10, "0.0"),
        (20, "0.0"),
        (11, "1.0"),
        (21, "1.0"),
        (12, "0.0"),
        (22, "0.0"),
        (13, "0.0"),
        (23, "0.0"),
        (14, "0.5"),
        (24, "0.5"),
        (15, "0.5"),
        (25, "0.5"),
        (16, "0.0"),
        (26, "0.0"),
        (36, "1.0"),
        (17, "0.0"),
        (27, "0.0"),
        (37, "0.0"),
        (40, "1000.0"),
        (41, "1.34"),
        (42, "50.0"),
        (43, "0.0"),
        (44, "0.0"),
        (50, "0.0"),
        (51, "0.0"),
        (71, "0"),
        (72, "100"),
    ];
    pairs.extend(values.iter().map(|&(c, v)| p(c, v)));
    pairs.extend(
        [
            (73, "1"),
            (74, "3"),
            (75, "0"),
            (76, "0"),
            (77, "0"),
            (78, "0"),
        ]
        .map(|(c, v)| p(c, v)),
    );
    pairs
}

/// A `LAYOUT` object for `name`, owned by `dictionary`, for block record
/// `record`.
fn layout(handle: Handle, dictionary: Handle, name: &str, tab: i32, record: Handle) -> Vec<Pair> {
    let mut v = vec![
        p(0, "LAYOUT"),
        p(5, handle.to_string()),
        p(102, "{ACAD_REACTORS"),
        p(330, dictionary.to_string()),
        p(102, "}"),
        p(330, dictionary.to_string()),
        p(100, "AcDbPlotSettings"),
        p(1, ""),
        p(4, "A3"),
        p(6, ""),
    ];
    for (code, value) in [
        (40, "7.5"),
        (41, "20.0"),
        (42, "7.5"),
        (43, "20.0"),
        (44, "420.0"),
        (45, "297.0"),
        (46, "0.0"),
        (47, "0.0"),
        (48, "0.0"),
        (49, "0.0"),
        (140, "0.0"),
        (141, "0.0"),
        (142, "1.0"),
        (143, "1.0"),
    ] {
        v.push(p(code, value));
    }
    let flags = if name == "Model" { "1024" } else { "0" };
    v.extend([
        p(70, flags),
        p(72, "1"),
        p(73, "0"),
        p(74, "5"),
        p(7, ""),
        p(75, "16"),
        p(76, "0"),
        p(77, "2"),
        p(78, "300"),
    ]);
    v.extend([
        p(147, "1.0"),
        p(148, "0.0"),
        p(149, "0.0"),
        p(100, "AcDbLayout"),
        p(1, name),
        p(70, "1"),
        p(71, tab.to_string()),
    ]);
    for (code, value) in [
        (10, "0.0"),
        (20, "0.0"),
        (11, "420.0"),
        (21, "297.0"),
        (12, "0.0"),
        (22, "0.0"),
        (32, "0.0"),
        (14, "1E20"),
        (24, "1E20"),
        (34, "1E20"),
        (15, "-1E20"),
        (25, "-1E20"),
        (35, "-1E20"),
        (146, "0.0"),
        (13, "0.0"),
        (23, "0.0"),
        (33, "0.0"),
        (16, "1.0"),
        (26, "0.0"),
        (36, "0.0"),
        (17, "0.0"),
        (27, "1.0"),
        (37, "0.0"),
        (76, "1"),
    ] {
        v.push(p(code, value));
    }
    v.push(p(330, record.to_string()));
    v
}

fn dictionary(handle: Handle, owner: Handle, entries: &[(&str, Handle)]) -> Vec<Pair> {
    let mut v = vec![
        p(0, "DICTIONARY"),
        p(5, handle.to_string()),
        p(102, "{ACAD_REACTORS"),
        p(330, owner.to_string()),
        p(102, "}"),
        p(330, owner.to_string()),
        p(100, "AcDbDictionary"),
        p(281, "1"),
    ];
    for (name, h) in entries {
        v.push(p(3, *name));
        v.push(p(350, h.to_string()));
    }
    v
}

/// Makes sure the objects contain a layout dictionary with layouts for
/// model and paper space, linked from their block records. Files without an
/// OBJECTS section (R12) get a minimal one; files without layouts (R13/R14)
/// get them added to their root dictionary. Files that have them are left
/// alone.
pub(super) fn ensure_layouts(d: &mut Drawing, handles: &mut Handles, model: Handle, paper: Handle) {
    let has_layouts = d
        .objects
        .iter()
        .any(|p| p.code == 3 && p.raw.trim() == "ACAD_LAYOUT");
    if has_layouts {
        return;
    }
    let root = d
        .objects
        .first()
        .filter(|p| p.code == 0 && p.raw.trim() == "DICTIONARY")
        .and_then(|_| d.objects.iter().find(|p| p.code == 5))
        .and_then(|p| Handle::parse(&p.raw));
    let layouts = handles.fresh();
    let model_layout = handles.fresh();
    let paper_layout = handles.fresh();
    let mut added = Vec::new();
    let root = match root {
        Some(root) => {
            // Add the entry to the existing root dictionary (its first record).
            let end = d.objects[1..]
                .iter()
                .position(|p| p.code == 0)
                .map_or(d.objects.len(), |i| i + 1);
            d.objects
                .splice(end..end, [p(3, "ACAD_LAYOUT"), p(350, layouts.to_string())]);
            root
        }
        None => {
            let root = handles.fresh();
            let groups = handles.fresh();
            let mut root_dict = vec![
                p(0, "DICTIONARY"),
                p(5, root.to_string()),
                p(330, "0"),
                p(100, "AcDbDictionary"),
                p(281, "1"),
                p(3, "ACAD_GROUP"),
                p(350, groups.to_string()),
                p(3, "ACAD_LAYOUT"),
                p(350, layouts.to_string()),
            ];
            root_dict.extend(dictionary(groups, root, &[]));
            added.extend(root_dict);
            root
        }
    };
    added.extend(dictionary(
        layouts,
        root,
        &[("Layout1", paper_layout), ("Model", model_layout)],
    ));
    added.extend(layout(model_layout, layouts, "Model", 0, model));
    added.extend(layout(paper_layout, layouts, "Layout1", 1, paper));
    if d.objects.is_empty() {
        d.objects = added;
    } else {
        d.objects.extend(added);
    }
    for r in &mut d.tables.block_records.entries {
        if r.handle == Some(model) || names_equal(&r.name, "*Model_Space") {
            r.layout = Some(model_layout);
        } else if r.handle == Some(paper) {
            r.layout = Some(paper_layout);
        }
    }
}
