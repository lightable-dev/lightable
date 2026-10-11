//! Writer tests: handles and owners, R12 downgrades, encodings, save-back.

use std::collections::{HashMap, HashSet};

use crate::entity::*;
use crate::model::*;
use crate::{CodePage, FileEncoding, Warning, read_bytes, read_str};

fn dxf(pairs: &[(i16, &str)]) -> String {
    pairs.iter().map(|(c, v)| format!("{c}\n{v}\n")).collect()
}

fn read(text: &str) -> Drawing {
    read_str(text).expect("test input loads").0
}

/// Always generates the file (never reuses the source bytes).
fn write(d: &Drawing, version: Version) -> (Vec<u8>, Vec<Warning>) {
    d.generate_bytes(version).expect("writes")
}

fn reread(d: &Drawing, version: Version) -> (Drawing, Vec<Warning>, Vec<Warning>) {
    let (bytes, write_warnings) = write(d, version);
    let (back, read_warnings) = read_bytes(&bytes).expect("written file loads");
    (back, write_warnings, read_warnings)
}

/// A small drawing with no handles at all: a block with a nested insert, an
/// insert with attributes, a polyline with vertices, and plain entities.
fn handleless() -> Drawing {
    read(&dxf(&[
        (0, "SECTION"),
        (2, "HEADER"),
        (9, "$ACADVER"),
        (1, "AC1009"),
        (0, "ENDSEC"),
        (0, "SECTION"),
        (2, "TABLES"),
        (0, "TABLE"),
        (2, "LAYER"),
        (0, "LAYER"),
        (2, "WALLS"),
        (70, "0"),
        (62, "1"),
        (6, "CONTINUOUS"),
        (0, "ENDTAB"),
        (0, "ENDSEC"),
        (0, "SECTION"),
        (2, "BLOCKS"),
        (0, "BLOCK"),
        (2, "BOLT"),
        (70, "0"),
        (10, "0"),
        (20, "0"),
        (0, "CIRCLE"),
        (8, "0"),
        (10, "0"),
        (20, "0"),
        (40, "1"),
        (0, "ENDBLK"),
        (0, "BLOCK"),
        (2, "PLATE"),
        (70, "2"),
        (10, "0"),
        (20, "0"),
        (0, "INSERT"),
        (8, "0"),
        (2, "BOLT"),
        (10, "5"),
        (20, "5"),
        (0, "ATTDEF"),
        (8, "0"),
        (10, "0"),
        (20, "0"),
        (40, "1"),
        (1, ""),
        (3, "No?"),
        (2, "NO"),
        (70, "0"),
        (0, "ENDBLK"),
        (0, "ENDSEC"),
        (0, "SECTION"),
        (2, "ENTITIES"),
        (0, "LINE"),
        (8, "WALLS"),
        (10, "0"),
        (20, "0"),
        (11, "10"),
        (21, "0"),
        (0, "INSERT"),
        (8, "0"),
        (66, "1"),
        (2, "PLATE"),
        (10, "20"),
        (20, "0"),
        (0, "ATTRIB"),
        (8, "0"),
        (10, "20"),
        (20, "0"),
        (40, "1"),
        (1, "P-1"),
        (2, "NO"),
        (70, "0"),
        (0, "SEQEND"),
        (8, "0"),
        (0, "POLYLINE"),
        (8, "0"),
        (66, "1"),
        (70, "1"),
        (0, "VERTEX"),
        (8, "0"),
        (10, "0"),
        (20, "0"),
        (42, "1"),
        (0, "VERTEX"),
        (8, "0"),
        (10, "4"),
        (20, "0"),
        (0, "SEQEND"),
        (8, "0"),
        (0, "CIRCLE"),
        (8, "0"),
        (67, "1"),
        (10, "0"),
        (20, "0"),
        (40, "3"),
        (0, "ENDSEC"),
        (0, "EOF"),
    ]))
}

/// Every handle written, from the parsed model (entities, children, blocks,
/// block ends, table entries) and the raw objects.
fn all_handles(d: &Drawing) -> Vec<u64> {
    let mut out = Vec::new();
    let entity = |e: &Entity, out: &mut Vec<u64>| {
        out.extend(e.handle.map(|h| h.0));
        let children: Vec<&Entity> = match &e.kind {
            EntityKind::Polyline(p) => p.vertices.iter().chain(p.seqend.as_deref()).collect(),
            EntityKind::Insert(i) => i.attributes.iter().chain(i.seqend.as_deref()).collect(),
            _ => Vec::new(),
        };
        out.extend(children.iter().filter_map(|c| c.handle.map(|h| h.0)));
    };
    for e in &d.entities {
        entity(e, &mut out);
    }
    for b in d.blocks.iter() {
        out.extend(b.handle.map(|h| h.0));
        out.extend(
            b.end
                .iter()
                .filter(|p| p.code == 5)
                .filter_map(|p| Handle::parse(&p.raw).map(|h| h.0)),
        );
        for e in &b.entities {
            entity(e, &mut out);
        }
    }
    let t = &d.tables;
    out.extend(t.layers.iter().filter_map(|e| e.handle.map(|h| h.0)));
    out.extend(t.linetypes.iter().filter_map(|e| e.handle.map(|h| h.0)));
    out.extend(t.text_styles.iter().filter_map(|e| e.handle.map(|h| h.0)));
    out.extend(t.dim_styles.iter().filter_map(|e| e.handle.map(|h| h.0)));
    out.extend(t.block_records.iter().filter_map(|e| e.handle.map(|h| h.0)));
    out.extend(
        d.objects
            .iter()
            .filter(|p| p.code == 5)
            .filter_map(|p| Handle::parse(&p.raw).map(|h| h.0)),
    );
    out
}

// ---- handles and owners -----------------------------------------------------

#[test]
fn r2018_output_has_unique_handles_for_everything() {
    let (d, ww, rw) = reread(&handleless(), Version::R2018);
    assert!(rw.is_empty(), "{rw:?}");
    assert!(ww.is_empty(), "{ww:?}");
    let handles = all_handles(&d);
    let unique: HashSet<u64> = handles.iter().copied().collect();
    assert_eq!(
        unique.len(),
        handles.len(),
        "duplicate handles: {handles:?}"
    );
    // Every entity, child and block has one.
    for e in &d.entities {
        assert!(e.handle.is_some(), "{:?}", e.kind.type_name());
    }
    let seed = d.header.get("$HANDSEED").expect("$HANDSEED written");
    let seed = Handle::parse(&seed.pairs[0].raw).unwrap().0;
    assert!(
        handles.iter().all(|&h| h < seed),
        "$HANDSEED {seed:X} not above all handles"
    );
}

#[test]
fn r2018_owners_follow_the_structure() {
    let (d, _, _) = reread(&handleless(), Version::R2018);
    let records: HashMap<String, Handle> = d
        .tables
        .block_records
        .iter()
        .map(|r| (r.name.to_ascii_uppercase(), r.handle.unwrap()))
        .collect();
    let model = records["*MODEL_SPACE"];
    let paper = records["*PAPER_SPACE"];
    for e in &d.entities {
        let expected = if e.paper_space { paper } else { model };
        assert_eq!(e.owner, Some(expected), "{}", e.kind.type_name());
        let children: Vec<&Entity> = match &e.kind {
            EntityKind::Polyline(p) => p.vertices.iter().chain(p.seqend.as_deref()).collect(),
            EntityKind::Insert(i) => i.attributes.iter().chain(i.seqend.as_deref()).collect(),
            _ => Vec::new(),
        };
        for c in children {
            assert_eq!(c.owner, e.handle, "child of {}", e.kind.type_name());
        }
    }
    for b in d.blocks.iter() {
        let record = records[&b.name.to_ascii_uppercase()];
        assert_eq!(b.owner, Some(record), "block {}", b.name);
        for e in &b.entities {
            assert_eq!(e.owner, Some(record), "entity in {}", b.name);
        }
    }
    // Table entries are owned by their table, whose handle is in its header.
    let layer_table = d
        .tables
        .layers
        .header
        .iter()
        .find(|p| p.code == 5)
        .and_then(|p| Handle::parse(&p.raw))
        .expect("LAYER table handle");
    for l in d.tables.layers.iter() {
        assert_eq!(l.owner, Some(layer_table), "layer {}", l.name);
    }
}

#[test]
fn r2018_block_records_link_to_layouts() {
    let (d, _, _) = reread(&handleless(), Version::R2018);
    let objects = &d.objects;
    let object_handles: HashSet<Handle> = objects
        .iter()
        .filter(|p| p.code == 5)
        .filter_map(|p| Handle::parse(&p.raw))
        .collect();
    for name in ["*Model_Space", "*Paper_Space"] {
        let record = d.tables.block_records.get(name).expect(name);
        let layout = record.layout.expect("layout link");
        assert!(
            object_handles.contains(&layout),
            "{name} layout {layout} missing"
        );
    }
    assert!(
        objects
            .iter()
            .any(|p| p.code == 3 && p.raw == "ACAD_LAYOUT")
    );
    assert!(objects.iter().any(|p| p.code == 0 && p.raw == "LAYOUT"));
}

#[test]
fn existing_handles_are_kept_and_duplicates_reassigned() {
    let d = read(&dxf(&[
        (0, "SECTION"),
        (2, "ENTITIES"),
        (0, "LINE"),
        (5, "2A"),
        (10, "0"),
        (20, "0"),
        (11, "1"),
        (21, "0"),
        (0, "LINE"),
        (5, "2A"),
        (10, "0"),
        (20, "0"),
        (11, "2"),
        (21, "0"),
        (0, "ENDSEC"),
        (0, "EOF"),
    ]));
    let (back, ww, _) = reread(&d, Version::R2018);
    assert_eq!(back.entities[0].handle, Some(Handle(0x2A)));
    assert_ne!(back.entities[1].handle, Some(Handle(0x2A)));
    assert!(
        ww.iter().any(|w| w.message.contains("duplicate handle")),
        "{ww:?}"
    );
}

#[test]
fn r12_output_has_entity_handles_and_no_owners() {
    let (bytes, ww) = write(&handleless(), Version::R12);
    assert!(ww.is_empty(), "{ww:?}");
    let text = String::from_utf8(bytes).unwrap();
    assert!(!text.contains("\n330\r\n"), "R12 has no owner links");
    assert!(!text.contains("AcDb"), "R12 has no subclass markers");
    let (d, rw) = read_bytes(text.as_bytes()).unwrap();
    assert!(rw.is_empty(), "{rw:?}");
    assert_eq!(d.header.version(), Some(Version::R12));
    let handling = d.header.get("$HANDLING").expect("$HANDLING");
    assert_eq!(handling.pairs[0].raw.trim(), "1");
    let handles: Vec<u64> = d
        .entities
        .iter()
        .filter_map(|e| e.handle.map(|h| h.0))
        .collect();
    assert_eq!(handles.len(), d.entities.len());
    assert!(d.blocks.contains("$MODEL_SPACE") && d.blocks.contains("$PAPER_SPACE"));
}

// ---- R12 downgrades -----------------------------------------------------------

fn modern(entities: &[(i16, &str)]) -> Drawing {
    let mut pairs = vec![
        (0, "SECTION"),
        (2, "HEADER"),
        (9, "$ACADVER"),
        (1, "AC1032"),
        (0, "ENDSEC"),
        (0, "SECTION"),
        (2, "ENTITIES"),
    ];
    pairs.extend_from_slice(entities);
    pairs.extend_from_slice(&[(0, "ENDSEC"), (0, "EOF")]);
    read(&dxf(&pairs))
}

fn r12_entities(d: &Drawing) -> (Vec<Entity>, Vec<Warning>) {
    let (back, ww, rw) = reread(d, Version::R12);
    assert!(rw.is_empty(), "{rw:?}");
    (back.entities, ww)
}

#[test]
fn r12_lwpolyline_becomes_polyline_with_bulges_and_widths() {
    let d = modern(&[
        (0, "LWPOLYLINE"),
        (90, "3"),
        (70, "1"),
        (38, "2"),
        (10, "0"),
        (20, "0"),
        (42, "1"),
        (10, "10"),
        (20, "0"),
        (40, "0.5"),
        (41, "1"),
        (10, "10"),
        (20, "10"),
    ]);
    let (entities, ww) = r12_entities(&d);
    assert!(ww.is_empty(), "{ww:?}");
    let EntityKind::Polyline(p) = &entities[0].kind else {
        panic!("{:?}", entities[0].kind)
    };
    assert!(p.is_closed());
    assert_eq!(p.location.z, 2.0, "elevation");
    let vertices: Vec<&Vertex> = p
        .vertices
        .iter()
        .map(|v| match &v.kind {
            EntityKind::Vertex(v) => v,
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(vertices.len(), 3);
    assert_eq!(vertices[0].bulge, 1.0);
    assert_eq!((vertices[1].start_width, vertices[1].end_width), (0.5, 1.0));
    assert_eq!(
        (vertices[2].location.x, vertices[2].location.y),
        (10.0, 10.0)
    );
}

#[test]
fn r12_ellipse_and_spline_become_polylines_on_the_curve() {
    let d = modern(&[
        (0, "ELLIPSE"),
        (10, "5"),
        (20, "5"),
        (11, "4"),
        (21, "0"),
        (40, "0.5"),
        (0, "SPLINE"),
        (70, "8"),
        (71, "3"),
        (72, "8"),
        (73, "4"),
        (40, "0"),
        (40, "0"),
        (40, "0"),
        (40, "0"),
        (40, "1"),
        (40, "1"),
        (40, "1"),
        (40, "1"),
        (10, "0"),
        (20, "0"),
        (10, "1"),
        (20, "2"),
        (10, "3"),
        (20, "2"),
        (10, "4"),
        (20, "0"),
    ]);
    let (entities, ww) = r12_entities(&d);
    assert_eq!(entities.len(), 2);
    assert!(ww.iter().any(|w| w.message.contains("ELLIPSE")), "{ww:?}");
    let points = |e: &Entity| -> Vec<Point3> {
        let EntityKind::Polyline(p) = &e.kind else {
            panic!("{:?}", e.kind)
        };
        p.vertices
            .iter()
            .map(|v| match &v.kind {
                EntityKind::Vertex(v) => v.location,
                other => panic!("{other:?}"),
            })
            .collect()
    };
    // Every ellipse vertex satisfies (x-5)²/4² + (y-5)²/2² = 1.
    let ellipse = points(&entities[0]);
    assert!(ellipse.len() >= 32);
    for p in &ellipse {
        let v = ((p.x - 5.0) / 4.0).powi(2) + ((p.y - 5.0) / 2.0).powi(2);
        assert!((v - 1.0).abs() < 1e-9, "{p:?}");
    }
    // A clamped spline starts and ends at its end control points.
    let spline = points(&entities[1]);
    let (first, last) = (spline[0], spline[spline.len() - 1]);
    assert!(first.x.abs() < 1e-9 && first.y.abs() < 1e-9);
    assert!((last.x - 4.0).abs() < 1e-9 && last.y.abs() < 1e-9);
    assert!(spline.len() > 8);
}

#[test]
fn r12_mtext_becomes_text_lines_without_formatting() {
    let d = modern(&[
        (0, "MTEXT"),
        (5, "2A"),
        (10, "0"),
        (20, "100"),
        (40, "2.5"),
        (3, "{\\fArial|b1;NOTES:}\\P1. \\C1;ALL\\C256; DIMS"),
        (1, "\\P2. A\\\\B"),
    ]);
    let (entities, ww) = r12_entities(&d);
    assert!(ww.iter().any(|w| w.message.contains("MTEXT")), "{ww:?}");
    let lines: Vec<(String, f64)> = entities
        .iter()
        .map(|e| match &e.kind {
            EntityKind::Text(t) => (t.value.clone(), t.insert.y),
            other => panic!("{other:?}"),
        })
        .collect();
    let texts: Vec<&str> = lines.iter().map(|(t, _)| t.as_str()).collect();
    assert_eq!(texts, ["NOTES:", "1. ALL DIMS", "2. A\\B"]);
    assert!(
        !ww.iter().any(|w| w.message.contains("duplicate")),
        "{ww:?}"
    );
    assert!(
        lines[0].1 > lines[1].1 && lines[1].1 > lines[2].1,
        "lines go down"
    );
}

#[test]
fn r12_drops_what_it_cannot_express_with_warnings() {
    let d = modern(&[
        (0, "HATCH"),
        (8, "0"),
        (2, "SOLID"),
        (0, "XLINE"),
        (10, "0"),
        (20, "0"),
        (11, "1"),
        (21, "0"),
        (0, "SOLID"),
        (100, "AcDbEntity"),
        (8, "0"),
        (100, "AcDbTrace"),
        (10, "0"),
        (20, "0"),
        (11, "1"),
        (21, "0"),
        (12, "0"),
        (22, "1"),
        (13, "1"),
        (23, "1"),
        (0, "LINE"),
        (420, "16711680"),
        (370, "50"),
        (10, "0"),
        (20, "0"),
        (11, "1"),
        (21, "0"),
    ]);
    let (entities, ww) = r12_entities(&d);
    let kinds: Vec<&str> = entities.iter().map(|e| e.kind.type_name()).collect();
    assert_eq!(kinds, ["SOLID", "LINE"]);
    assert!(ww.iter().any(|w| w.message.contains("HATCH")), "{ww:?}");
    assert!(ww.iter().any(|w| w.message.contains("XLINE")), "{ww:?}");
    // The SOLID kept its geometry but lost its subclass markers.
    let EntityKind::Unknown(solid) = &entities[0].kind else {
        panic!()
    };
    assert!(solid.pairs.iter().all(|p| p.code != 100));
    assert_eq!(solid.pairs.iter().filter(|p| p.code == 13).count(), 1);
    // True colour becomes the nearest index colour; lineweight is dropped.
    assert_eq!(entities[1].color, Color::Index(1));
    assert_eq!(entities[1].lineweight, LineWeight::ByLayer);
}

// ---- encodings ------------------------------------------------------------------

fn text_entity(value: &str) -> Drawing {
    modern(&[(0, "TEXT"), (10, "0"), (20, "0"), (40, "1"), (1, value)])
}

fn first_text(d: &Drawing) -> &Text {
    match &d.entities[0].kind {
        EntityKind::Text(t) => t,
        other => panic!("{other:?}"),
    }
}

#[test]
fn r12_output_uses_the_code_page_and_escapes_the_rest() {
    let d = text_entity("Wände ⌀25 配管");
    let (bytes, _) = write(&d, Version::R12);
    let needle = b"W\xe4nde \\U+230025 \\U+914D\\U+7BA1";
    assert!(
        bytes.windows(needle.len()).any(|w| w == needle),
        "cp1252 bytes and escapes"
    );
    let (back, rw) = read_bytes(&bytes).unwrap();
    assert!(rw.is_empty(), "{rw:?}");
    assert_eq!(first_text(&back).value, "Wände ⌀25 配管");
    assert_eq!(back.encoding, FileEncoding::CodePage(CodePage(1252)));
}

#[test]
fn r2018_output_is_utf8() {
    let d = text_entity("Wände ⌀25 配管");
    let (bytes, _) = write(&d, Version::R2018);
    let text = std::str::from_utf8(&bytes).expect("UTF-8");
    assert!(text.contains("Wände ⌀25 配管"));
    let (back, _) = read_bytes(&bytes).unwrap();
    assert_eq!(first_text(&back).value, "Wände ⌀25 配管");
    assert_eq!(back.encoding, FileEncoding::Utf8);
}

#[test]
fn original_text_is_written_when_it_still_matches() {
    let d = text_entity("%%c25 \\U+00B0");
    assert_eq!(first_text(&d).value, "⌀25 °");
    for version in [Version::R12, Version::R2018] {
        let (back, _, _) = reread(&d, version);
        let t = first_text(&back);
        assert_eq!(
            t.raw_value.as_deref(),
            Some("%%c25 \\U+00B0"),
            "{version:?}"
        );
        assert_eq!(t.value, "⌀25 °");
    }
    // After an edit the new value is written, not the stale original.
    let mut edited = d.clone();
    if let EntityKind::Text(t) = &mut edited.entities[0].kind {
        t.value = "50%".into();
    }
    let (back, _, _) = reread(&edited, Version::R2018);
    assert_eq!(first_text(&back).value, "50%");
    assert_eq!(first_text(&back).raw_value, None);
}

#[test]
fn undecodable_bytes_survive_a_code_page_round_trip() {
    let mut file = b"0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1009\n9\n$DWGCODEPAGE\n3\nANSI_932\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n0\nTEXT\n10\n0\n20\n0\n40\n1\n1\n".to_vec();
    file.extend_from_slice(b"\x94\x7a\x8a\xc7 \x94");
    file.extend_from_slice(b"\n0\nENDSEC\n0\nEOF\n");
    let (d, _) = read_bytes(&file).unwrap();
    assert_eq!(first_text(&d).value, "配管 \u{F794}");
    let (bytes, _) = write(&d, Version::R12);
    let needle = b"\x94\x7a\x8a\xc7 \x94\r\n";
    assert!(
        bytes.windows(needle.len()).any(|w| w == needle),
        "raw byte written back"
    );
    let (back, _) = read_bytes(&bytes).unwrap();
    assert_eq!(first_text(&back).value, "配管 \u{F794}");
}

// ---- save-back ------------------------------------------------------------------

#[test]
fn unedited_drawing_saves_back_byte_for_byte() {
    let original = b"  0\r\nSECTION\r\n  2\r\nENTITIES\r\n  0\r\nLINE\r\n  8\r\n0\r\n 10\r\n0.0\r\n 20\r\n0\r\n 11\r\n1.000\r\n 21\r\n1\r\n999\r\nodd but kept\r\n  0\r\nENDSEC\r\n  0\r\nEOF\r\n  0\r\nJUNK\r\n";
    let (d, _) = read_bytes(original).unwrap();
    let (saved, warnings) = d.save_bytes();
    assert_eq!(saved, original.to_vec());
    assert!(warnings.is_empty());
}

#[test]
fn edited_drawing_is_regenerated_and_says_so() {
    let (mut d, _) = read_bytes(b"0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1015\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n0\nPOINT\n10\n1\n20\n2\n0\nENDSEC\n0\nEOF\n").unwrap();
    if let EntityKind::Point(p) = &mut d.entities[0].kind {
        p.location.x = 9.0;
    }
    let (saved, warnings) = d.save_bytes();
    let (back, _) = read_bytes(&saved).unwrap();
    assert_eq!(back.header.version(), Some(Version::R2018));
    assert!(matches!(&back.entities[0].kind, EntityKind::Point(p) if p.location.x == 9.0));
    assert!(
        warnings.iter().any(|w| w.message.contains("AC1015")),
        "{warnings:?}"
    );
}

#[test]
fn writing_the_same_version_of_an_unedited_file_is_identical() {
    let original = dxf(&[
        (0, "SECTION"),
        (2, "HEADER"),
        (9, "$ACADVER"),
        (1, "AC1009"),
        (0, "ENDSEC"),
        (0, "SECTION"),
        (2, "ENTITIES"),
        (0, "POINT"),
        (10, "1"),
        (20, "2"),
        (0, "ENDSEC"),
        (0, "EOF"),
    ]);
    let d = read(&original);
    assert_eq!(d.to_bytes(Version::R12).unwrap().0, original.as_bytes());
    // Generating instead reformats it.
    assert_ne!(write(&d, Version::R12).0, original.as_bytes());
}

#[test]
fn only_r12_and_r2018_are_written() {
    let d = handleless();
    assert!(d.to_bytes(Version::R2000).is_err());
    assert!(d.generate_bytes(Version::R2004).is_err());
}

#[test]
fn conversions_are_a_fixpoint() {
    for version in [Version::R12, Version::R2018] {
        let (first, _) = write(&handleless(), version);
        let (d1, _) = read_bytes(&first).unwrap();
        let (second, _) = write(&d1, version);
        let (d2, _) = read_bytes(&second).unwrap();
        assert_eq!(first, second, "{version:?}: write is not stable");
        assert_eq!(d1, d2, "{version:?}");
    }
}
