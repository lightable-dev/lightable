//! Emitting the TABLES and BLOCKS sections.

use crate::model::{Block, Color, Drawing, Handle, LineWeight, Version};
use crate::table::RawTable;
use crate::tokenizer::Pair;

use super::entity::{EntityWriter, split_extra};
use super::out::Out;

fn handle_in(pairs: &[Pair], code: i16) -> Option<Handle> {
    pairs
        .iter()
        .find(|p| p.code == code)
        .and_then(|p| Handle::parse(&p.raw))
}

fn table_start(out: &mut Out, name: &str, own: Option<Handle>, count: usize, r12: bool) {
    out.pair(0, "TABLE");
    out.pair(2, name);
    if !r12 {
        if let Some(h) = own {
            out.handle(5, h);
        }
        out.pair(330, "0");
        out.pair(100, "AcDbSymbolTable");
    }
    out.int(70, count);
    if !r12 && name == "DIMSTYLE" {
        out.pair(100, "AcDbDimStyleTable");
        out.int(71, count);
    }
}

/// The start of a table record: type, handle, owner and subclass markers.
fn record_start(
    out: &mut Out,
    type_name: &str,
    handle_code: i16,
    (handle, owner): (Option<Handle>, Option<Handle>),
    marker: &str,
    extra: &[Pair],
    r12: bool,
) {
    out.pair(0, type_name);
    if r12 {
        return;
    }
    if let Some(h) = handle {
        out.handle(handle_code, h);
    }
    for p in split_extra(extra).groups {
        out.pair(p.code, &p.raw);
    }
    if let Some(o) = owner {
        out.handle(330, o);
    }
    out.pair(100, "AcDbSymbolTableRecord");
    out.pair(100, marker);
}

/// Leftover pairs of a table record: other codes, then XDATA.
fn record_end(out: &mut Out, extra: &[Pair], r12: bool) {
    let parts = split_extra(extra);
    if !r12 {
        for p in parts.entity_level.iter().chain(&parts.data) {
            out.pair(p.code, &p.raw);
        }
    }
    for p in parts.xdata {
        out.pair(p.code, &p.raw);
    }
}

fn raw_table(out: &mut Out, raw: &RawTable, r12: bool) {
    let first_record = raw
        .pairs
        .iter()
        .position(|p| p.code == 0)
        .unwrap_or(raw.pairs.len());
    let count = raw.pairs.iter().filter(|p| p.code == 0).count();
    let name = raw.name.to_ascii_uppercase();
    table_start(
        out,
        &name,
        handle_in(&raw.pairs[..first_record], 5),
        count,
        r12,
    );
    out.raw(&raw.pairs[first_record..]);
    out.pair(0, "ENDTAB");
}

fn color_value(color: Color, on: bool) -> (i64, Option<i64>) {
    let (index, rgb) = match color {
        Color::Index(i) => (i64::from(i), None),
        Color::Rgb(r, g, b) => (
            i64::from(Color::nearest_aci(r, g, b)),
            Some((i64::from(r) << 16) | (i64::from(g) << 8) | i64::from(b)),
        ),
        Color::ByLayer | Color::ByBlock => (7, None),
    };
    (if on { index } else { -index }, rgb)
}

pub(super) fn tables(out: &mut Out, d: &Drawing, version: Version) {
    let r12 = version == Version::R12;
    let t = &d.tables;
    let raw = |name: &str| t.other.iter().find(|r| r.name.eq_ignore_ascii_case(name));
    out.pair(0, "SECTION");
    out.pair(2, "TABLES");

    if let Some(vport) = raw("VPORT") {
        raw_table(out, vport, r12);
    }

    table_start(
        out,
        "LTYPE",
        handle_in(&t.linetypes.header, 5),
        t.linetypes.len(),
        r12,
    );
    for lt in t.linetypes.iter() {
        record_start(
            out,
            "LTYPE",
            5,
            (lt.handle, lt.owner),
            "AcDbLinetypeTableRecord",
            &lt.extra,
            r12,
        );
        out.pair(2, &lt.name);
        out.int(70, lt.flags);
        out.pair(3, &lt.description);
        out.int(72, lt.alignment);
        out.int(73, lt.elements.len());
        out.float(40, lt.pattern_length);
        for e in &lt.elements {
            out.float(49, e.length);
            if r12 {
                continue;
            }
            out.int(74, e.flags);
            if e.flags != 0 {
                out.int(75, e.shape_number);
                if let Some(style) = e.style {
                    out.handle(340, style);
                }
                out.float(46, e.scale);
                out.float(50, e.rotation);
                out.float(44, e.offset.0);
                out.float(45, e.offset.1);
                if let Some(text) = &e.text {
                    out.pair(9, text);
                }
            }
        }
        record_end(out, &lt.extra, r12);
    }
    out.pair(0, "ENDTAB");

    table_start(
        out,
        "LAYER",
        handle_in(&t.layers.header, 5),
        t.layers.len(),
        r12,
    );
    for l in t.layers.iter() {
        record_start(
            out,
            "LAYER",
            5,
            (l.handle, l.owner),
            "AcDbLayerTableRecord",
            &l.extra,
            r12,
        );
        out.pair(2, &l.name);
        out.int(70, l.flags);
        let (index, rgb) = color_value(l.color, l.on);
        out.int(62, index);
        if !r12 && let Some(rgb) = rgb {
            out.int(420, rgb);
        }
        out.pair(6, &l.linetype);
        if !r12 {
            if !l.plot {
                out.int(290, 0);
            }
            let lw = match l.lineweight {
                LineWeight::ByLayer => -1,
                LineWeight::ByBlock => -2,
                LineWeight::Default => -3,
                LineWeight::Hundredths(n) => i32::from(n),
            };
            out.int(370, lw);
        }
        record_end(out, &l.extra, r12);
    }
    out.pair(0, "ENDTAB");

    table_start(
        out,
        "STYLE",
        handle_in(&t.text_styles.header, 5),
        t.text_styles.len(),
        r12,
    );
    for s in t.text_styles.iter() {
        record_start(
            out,
            "STYLE",
            5,
            (s.handle, s.owner),
            "AcDbTextStyleTableRecord",
            &s.extra,
            r12,
        );
        out.pair(2, &s.name);
        out.int(70, s.flags);
        out.float(40, s.height);
        out.float(41, s.width_factor);
        out.float(50, s.oblique);
        out.int(71, s.generation_flags);
        out.float(42, s.last_height);
        out.pair(3, &s.font);
        out.pair(4, &s.big_font);
        record_end(out, &s.extra, r12);
    }
    out.pair(0, "ENDTAB");

    for name in ["VIEW", "UCS", "APPID"] {
        if let Some(table) = raw(name) {
            raw_table(out, table, r12);
        }
    }

    table_start(
        out,
        "DIMSTYLE",
        handle_in(&t.dim_styles.header, 5),
        t.dim_styles.len(),
        r12,
    );
    for s in t.dim_styles.iter() {
        record_start(
            out,
            "DIMSTYLE",
            105,
            (s.handle, s.owner),
            "AcDbDimStyleTableRecord",
            &s.extra,
            r12,
        );
        out.pair(2, &s.name);
        out.int(70, s.flags);
        out.raw(&s.variables);
        record_end(out, &s.extra, r12);
    }
    out.pair(0, "ENDTAB");

    if !r12 {
        table_start(
            out,
            "BLOCK_RECORD",
            handle_in(&t.block_records.header, 5),
            t.block_records.len(),
            r12,
        );
        for r in t.block_records.iter() {
            record_start(
                out,
                "BLOCK_RECORD",
                5,
                (r.handle, r.owner),
                "AcDbBlockTableRecord",
                &r.extra,
                r12,
            );
            out.pair(2, &r.name);
            match r.layout {
                Some(h) => out.handle(340, h),
                None => out.pair(340, "0"),
            }
            out.int(70, r.units);
            out.int(280, i32::from(r.explodable));
            out.int(281, i32::from(r.scalable));
            record_end(out, &r.extra, r12);
        }
        out.pair(0, "ENDTAB");
    }

    // Any other raw tables, after the standard ones.
    let standard = ["VPORT", "VIEW", "UCS", "APPID"];
    for table in t
        .other
        .iter()
        .filter(|r| !standard.iter().any(|s| r.name.eq_ignore_ascii_case(s)))
    {
        raw_table(out, table, r12);
    }
    out.pair(0, "ENDSEC");
}

pub(super) fn blocks(out: &mut Out, d: &Drawing, writer: &mut EntityWriterFactory) {
    let r12 = writer.version == Version::R12;
    out.pair(0, "SECTION");
    out.pair(2, "BLOCKS");
    for b in d.blocks.iter() {
        block(out, b, r12);
        let mut w = EntityWriter {
            out,
            version: writer.version,
            code_page: writer.code_page,
        };
        for e in &b.entities {
            w.entity(e);
        }
        block_end(out, b, r12);
    }
    out.pair(0, "ENDSEC");
}

/// Version and code page for writing entities.
pub(super) struct EntityWriterFactory {
    pub version: Version,
    pub code_page: crate::encoding::CodePage,
}

fn block(out: &mut Out, b: &Block, r12: bool) {
    let parts = split_extra(&b.extra);
    let paper = b.extra.iter().find(|p| p.code == 67);
    out.pair(0, "BLOCK");
    if !r12 {
        if let Some(h) = b.handle {
            out.handle(5, h);
        }
        for p in &parts.groups {
            out.pair(p.code, &p.raw);
        }
        if let Some(o) = b.owner {
            out.handle(330, o);
        }
        out.pair(100, "AcDbEntity");
    }
    if let Some(p) = paper {
        out.pair(67, &p.raw);
    }
    out.pair(8, &b.layer);
    if !r12 {
        out.pair(100, "AcDbBlockBegin");
    }
    out.pair(2, &b.name);
    out.int(70, b.flags);
    out.point(10, b.base_point);
    out.pair(3, &b.name);
    out.pair(1, &b.xref_path);
    if !b.description.is_empty() && !r12 {
        out.pair(4, &b.description);
    }
    if !r12 {
        for p in parts
            .entity_level
            .iter()
            .chain(&parts.data)
            .filter(|p| p.code != 67)
        {
            out.pair(p.code, &p.raw);
        }
    }
    for p in &parts.xdata {
        out.pair(p.code, &p.raw);
    }
}

fn block_end(out: &mut Out, b: &Block, r12: bool) {
    out.pair(0, "ENDBLK");
    if let Some(h) = handle_in(&b.end, 5) {
        out.handle(5, h);
    }
    if !r12 {
        if let Some(o) = handle_in(&b.end, 330) {
            out.handle(330, o);
        }
        out.pair(100, "AcDbEntity");
    }
    if let Some(p) = b.end.iter().find(|p| p.code == 67) {
        out.pair(67, &p.raw);
    }
    let layer = b
        .end
        .iter()
        .find(|p| p.code == 8)
        .map_or(b.layer.as_str(), |p| p.raw.as_str());
    out.pair(8, layer);
    if !r12 {
        out.pair(100, "AcDbBlockEnd");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layer_colour_sign_means_off() {
        assert_eq!(color_value(Color::Index(3), false), (-3, None));
        assert_eq!(
            color_value(Color::Rgb(255, 0, 0), true),
            (1, Some(0xFF0000))
        );
    }
}
