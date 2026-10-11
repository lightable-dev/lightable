//! The machine-readable change report (format `lightable-diff/1`), written by
//! hand to keep the crate free of dependencies.

use std::fmt::Write as _;

use crate::{Change, ChangeKind, Diff, EntityRef};

fn string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn number(v: f64) -> String {
    if v.is_finite() {
        lightable_render::svg::num(v)
    } else {
        "null".into()
    }
}

fn side(r: &Option<EntityRef>) -> String {
    match r {
        None => "null".into(),
        Some(r) => format!(
            "{{\"handle\": {}, \"index\": {}, \"layer\": {}}}",
            r.handle.as_deref().map_or("null".into(), string),
            r.index,
            string(&r.layer)
        ),
    }
}

fn refs(list: &[EntityRef]) -> String {
    list.iter()
        .map(|r| side(&Some(r.clone())))
        .collect::<Vec<_>>()
        .join(", ")
}

fn change(c: &Change) -> String {
    let details: Vec<String> = c.details.iter().map(|d| string(d)).collect();
    let bounds = c.bounds.map_or("null".into(), |b| {
        format!(
            "[{}, {}, {}, {}]",
            number(b.min[0]),
            number(b.min[1]),
            number(b.max[0]),
            number(b.max[1])
        )
    });
    format!(
        "    {{\"kind\": {}, \"place\": {}, \"type\": {}, \"layer\": {}, \"handle\": {}, \"details\": [{}], \"geometry\": {}, \"shown\": {}, \"certain\": {}, \"old\": {}, \"new\": {}, \"bounds\": {}, \"used_by_before\": [{}], \"used_by\": [{}], \"headline\": {}, \"markup\": {}, \"revision_cloud\": {}}}",
        string(c.kind.as_str()),
        string(&c.place.to_string()),
        string(&c.entity_type),
        string(c.layer()),
        c.handle().map_or("null".into(), string),
        details.join(", "),
        c.geometry,
        c.shown,
        c.certain,
        side(&c.old),
        side(&c.new),
        bounds,
        refs(&c.used_by_old),
        refs(&c.used_by_new),
        string(&c.headline),
        c.markup,
        c.revision_cloud
    )
}

pub fn report(d: &Diff, a: &str, b: &str) -> String {
    let mut s = String::new();
    s.push_str("{\n  \"format\": \"lightable-diff/1\",\n");
    let _ = writeln!(s, "  \"a\": {},\n  \"b\": {},", string(a), string(b));
    let _ = writeln!(
        s,
        "  \"tolerance\": {},\n  \"units\": {},\n  \"handles_trusted\": {},",
        number(d.tolerance),
        d.units.as_deref().map_or("null".into(), string),
        d.handles_trusted
    );
    let _ = writeln!(
        s,
        "  \"summary\": {{\"added\": {}, \"deleted\": {}, \"changed\": {}, \"design_changes\": {}, \"markup\": {}, \"revision_clouds\": {}, \"unchanged\": {}, \"equivalent\": {}, \"not_shown\": {}, \"uncertain\": {}}},",
        d.count(ChangeKind::Added),
        d.count(ChangeKind::Deleted),
        d.count(ChangeKind::Changed),
        d.design_changes().count(),
        d.markup_count(),
        d.cloud_count(),
        d.unchanged,
        d.equivalent,
        d.changes.iter().filter(|c| !c.shown).count(),
        d.changes.iter().filter(|c| !c.certain).count()
    );
    let list = |items: &[Change]| -> String {
        let items: Vec<String> = items.iter().map(change).collect();
        if items.is_empty() {
            "[]".into()
        } else {
            format!("[\n{}\n  ]", items.join(",\n"))
        }
    };
    // Entities drawn identically by other entities in the other file:
    // not changes, but listed so nothing is left out.
    let not: Vec<String> = d.not_compared.iter().map(|n| string(n)).collect();
    let _ = writeln!(s, "  \"not_compared\": [{}],", not.join(", "));
    let _ = writeln!(s, "  \"redrawn\": {},", list(&d.redrawn));
    let _ = writeln!(s, "  \"changes\": {}", list(&d.changes));
    s.push_str("}\n");
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strings_are_escaped() {
        assert_eq!(string("a\"b\\c\nd\u{1}"), "\"a\\\"b\\\\c\\nd\\u0001\"");
    }

    #[test]
    fn an_empty_diff_is_valid() {
        let s = report(&Diff::default(), "a.dxf", "b.dxf");
        assert!(s.contains("\"changes\": []"));
        assert!(s.contains("\"redrawn\": []"));
        assert!(s.contains("\"added\": 0"));
    }
}
