//! Reader tests.

use super::*;
use crate::entity::*;
use crate::model::*;
use crate::tokenizer::Pair;

/// Builds a DXF document from `(code, value)` pairs.
fn dxf(pairs: &[(i16, &str)]) -> String {
    pairs.iter().map(|(c, v)| format!("{c}\n{v}\n")).collect()
}

/// Wraps entity pairs in an ENTITIES section.
fn entities_doc(body: &[(i16, &str)]) -> String {
    let mut pairs = vec![(0, "SECTION"), (2, "ENTITIES")];
    pairs.extend_from_slice(body);
    pairs.extend_from_slice(&[(0, "ENDSEC"), (0, "EOF")]);
    dxf(&pairs)
}

fn read_ok(text: &str) -> (Drawing, Vec<Warning>) {
    read_str(text).expect("document should load")
}

fn only_entity(text: &str) -> (Entity, Vec<Warning>) {
    let (drawing, warnings) = read_ok(text);
    assert_eq!(drawing.entities.len(), 1);
    (drawing.entities.into_iter().next().unwrap(), warnings)
}

#[test]
fn line_with_common_fields() {
    let (e, warnings) = only_entity(&entities_doc(&[
        (0, "LINE"),
        (5, "2F"),
        (330, "1F"),
        (100, "AcDbEntity"),
        (8, "WALLS"),
        (6, "DASHED"),
        (62, "1"),
        (370, "25"),
        (100, "AcDbLine"),
        (10, "1.5"),
        (20, "-2"),
        (30, "0"),
        (11, "10"),
        (21, "20"),
        (31, "3"),
    ]));
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(e.handle, Some(Handle(0x2F)));
    assert_eq!(e.owner, Some(Handle(0x1F)));
    assert_eq!(e.layer, "WALLS");
    assert_eq!(e.linetype, "DASHED");
    assert_eq!(e.color, Color::Index(1));
    assert_eq!(e.lineweight, LineWeight::Hundredths(25));
    assert!(e.visible);
    assert!(e.extra.is_empty());
    let EntityKind::Line(line) = e.kind else {
        panic!("expected a line, got {:?}", e.kind)
    };
    assert_eq!(line.start, Point3::new(1.5, -2.0, 0.0));
    assert_eq!(line.end, Point3::new(10.0, 20.0, 3.0));
    assert_eq!(line.thickness, 0.0);
    assert_eq!(line.extrusion, Z_AXIS);
}

#[test]
fn line_defaults_without_z_or_layer() {
    let (e, _) = only_entity(&entities_doc(&[
        (0, "LINE"),
        (10, "0"),
        (20, "0"),
        (11, "1"),
        (21, "1"),
    ]));
    assert_eq!(e.layer, "0");
    assert_eq!(e.color, Color::ByLayer);
    let EntityKind::Line(line) = e.kind else {
        panic!()
    };
    assert_eq!(line.end, Point3::new(1.0, 1.0, 0.0));
}

#[test]
fn line_missing_end_point_becomes_unknown_with_warning() {
    let (e, warnings) = only_entity(&entities_doc(&[
        (0, "LINE"),
        (8, "A"),
        (10, "0"),
        (20, "0"),
    ]));
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].message.contains("end point"));
    assert_eq!(warnings[0].line, Some(5));
    let EntityKind::Unknown(u) = e.kind else {
        panic!()
    };
    assert_eq!(u.type_name, "LINE");
    assert_eq!(u.pairs.len(), 3);
    assert_eq!(e.layer, "A");
}

#[test]
fn bad_number_keeps_raw_pairs() {
    let (e, warnings) = only_entity(&entities_doc(&[
        (0, "LINE"),
        (10, "abc"),
        (20, "0"),
        (11, "1"),
        (21, "1"),
    ]));
    assert_eq!(warnings.len(), 1);
    let EntityKind::Unknown(u) = e.kind else {
        panic!()
    };
    assert_eq!(u.pairs[0], Pair::new(10, "abc"));
}

#[test]
fn circle() {
    let (e, warnings) = only_entity(&entities_doc(&[
        (0, "CIRCLE"),
        (8, "0"),
        (10, "5"),
        (20, "6"),
        (40, "2.5"),
        (39, "1"),
    ]));
    assert!(warnings.is_empty());
    let EntityKind::Circle(c) = e.kind else {
        panic!()
    };
    assert_eq!(c.center, Point3::new(5.0, 6.0, 0.0));
    assert_eq!(c.radius, 2.5);
    assert_eq!(c.thickness, 1.0);
}

#[test]
fn circle_without_radius_or_with_negative_radius_is_unknown() {
    for radius in [None, Some("-1")] {
        let mut body = vec![(0, "CIRCLE"), (10, "0"), (20, "0")];
        if let Some(r) = radius {
            body.push((40, r));
        }
        let (e, warnings) = only_entity(&entities_doc(&body));
        assert!(matches!(e.kind, EntityKind::Unknown(_)));
        assert_eq!(warnings.len(), 1);
    }
}

#[test]
fn arc_including_one_crossing_zero_degrees() {
    let (e, warnings) = only_entity(&entities_doc(&[
        (0, "ARC"),
        (10, "1"),
        (20, "2"),
        (40, "3"),
        (50, "350"),
        (51, "10"),
        (210, "0"),
        (220, "0"),
        (230, "-1"),
    ]));
    assert!(warnings.is_empty());
    let EntityKind::Arc(a) = e.kind else { panic!() };
    assert_eq!(a.center, Point3::new(1.0, 2.0, 0.0));
    assert_eq!(a.radius, 3.0);
    assert_eq!(a.start_angle, 350.0);
    assert_eq!(a.end_angle, 10.0);
    assert_eq!(a.extrusion, Point3::new(0.0, 0.0, -1.0));
}

#[test]
fn arc_missing_angle_is_unknown() {
    let (e, warnings) = only_entity(&entities_doc(&[
        (0, "ARC"),
        (10, "1"),
        (20, "2"),
        (40, "3"),
        (50, "0"),
    ]));
    assert!(matches!(e.kind, EntityKind::Unknown(_)));
    assert!(warnings[0].message.contains("end angle"));
}

#[test]
fn true_color_wins_over_index_and_xdata_is_extra() {
    let (e, warnings) = only_entity(&entities_doc(&[
        (0, "CIRCLE"),
        (102, "{ACAD_REACTORS"),
        (330, "AB"),
        (102, "}"),
        (330, "1F"),
        (62, "5"),
        (420, "16711680"),
        (60, "1"),
        (10, "0"),
        (20, "0"),
        (40, "1"),
        (999, "a comment"),
        (1001, "MYAPP"),
        (1000, "hello"),
        (1040, "2.0"),
    ]));
    assert!(warnings.is_empty());
    assert_eq!(e.owner, Some(Handle(0x1F)));
    assert_eq!(e.color, Color::Rgb(255, 0, 0));
    assert!(!e.visible);
    let codes: Vec<i16> = e.extra.iter().map(|p| p.code).collect();
    assert_eq!(codes, vec![102, 330, 102, 999, 1001, 1000, 1040]);
}

#[test]
fn invalid_color_makes_entity_unknown() {
    let (e, warnings) = only_entity(&entities_doc(&[
        (0, "CIRCLE"),
        (62, "300"),
        (10, "0"),
        (20, "0"),
        (40, "1"),
    ]));
    assert!(matches!(e.kind, EntityKind::Unknown(_)));
    assert_eq!(warnings.len(), 1);
}

#[test]
fn unknown_entity_type_is_preserved_without_warning() {
    let body = [
        (0, "HATCH"),
        (8, "PLAN"),
        (91, "2"),
        (10, "0"),
        (20, "0"),
        (10, "1"),
        (20, "1"),
    ];
    let (e, warnings) = only_entity(&entities_doc(&body));
    assert!(warnings.is_empty());
    assert_eq!(e.layer, "PLAN");
    let EntityKind::Unknown(u) = &e.kind else {
        panic!()
    };
    assert_eq!(u.type_name, "HATCH");
    let expected: Vec<Pair> = body[1..].iter().map(|&(c, v)| Pair::new(c, v)).collect();
    assert_eq!(u.pairs, expected);
    assert!(e.extra.is_empty());
    assert_eq!(e.kind.type_name(), "HATCH");
}

#[test]
fn header_variables_and_layers() {
    let text = dxf(&[
        (0, "SECTION"),
        (2, "HEADER"),
        (9, "$ACADVER"),
        (1, "AC1015"),
        (9, "$INSUNITS"),
        (70, "6"),
        (9, "$CLAYER"),
        (8, "Walls"),
        (0, "ENDSEC"),
        (0, "SECTION"),
        (2, "TABLES"),
        (0, "TABLE"),
        (2, "VPORT"),
        (70, "1"),
        (0, "VPORT"),
        (2, "*ACTIVE"),
        (0, "ENDTAB"),
        (0, "TABLE"),
        (2, "LAYER"),
        (5, "2"),
        (70, "3"),
        (0, "LAYER"),
        (5, "10"),
        (2, "0"),
        (70, "0"),
        (62, "7"),
        (6, "CONTINUOUS"),
        (0, "LAYER"),
        (2, "Walls"),
        (70, "5"),
        (62, "-1"),
        (6, "DASHED"),
        (370, "50"),
        (290, "0"),
        (0, "LAYER"),
        (2, "WALLS"),
        (62, "2"),
        (0, "ENDTAB"),
        (0, "ENDSEC"),
        (0, "EOF"),
    ]);
    let (d, warnings) = read_ok(&text);
    assert_eq!(d.header.version(), Some(crate::Version::R2000));
    assert_eq!(d.header.insunits(), Some(6));
    assert_eq!(d.header.current_layer(), Some("Walls"));

    let layers = &d.tables.layers;
    assert_eq!(layers.len(), 2);
    assert_eq!(layers.header.len(), 2);
    let zero = layers.get("0").unwrap();
    assert_eq!(zero.handle, Some(Handle(0x10)));
    assert!(zero.on);
    let walls = layers.get("walls").unwrap();
    assert_eq!(walls.name, "Walls");
    assert!(!walls.on);
    assert_eq!(walls.color, Color::Index(1));
    assert!(walls.frozen());
    assert!(walls.locked());
    assert!(!walls.plot);
    assert_eq!(walls.linetype, "DASHED");
    assert_eq!(walls.lineweight, LineWeight::Hundredths(50));

    // The case-insensitive duplicate is kept verbatim with a warning.
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(warnings[0].message.contains("duplicate"));
    assert_eq!(layers.unparsed.len(), 1);

    assert_eq!(d.tables.other.len(), 1);
    assert_eq!(d.tables.other[0].name, "VPORT");
    assert_eq!(d.tables.other[0].pairs.len(), 3);
}

#[test]
fn layer_without_name_is_kept_unparsed() {
    let text = dxf(&[
        (0, "SECTION"),
        (2, "TABLES"),
        (0, "TABLE"),
        (2, "LAYER"),
        (0, "LAYER"),
        (70, "0"),
        (0, "ENDTAB"),
        (0, "ENDSEC"),
        (0, "EOF"),
    ]);
    let (d, warnings) = read_ok(&text);
    assert!(d.tables.layers.is_empty());
    assert_eq!(d.tables.layers.unparsed.len(), 1);
    assert_eq!(warnings.len(), 1);
}

#[test]
fn unknown_and_raw_sections_are_kept() {
    let text = dxf(&[
        (0, "SECTION"),
        (2, "CLASSES"),
        (0, "CLASS"),
        (1, "X"),
        (0, "ENDSEC"),
        (0, "SECTION"),
        (2, "BLOCKS"),
        (0, "BLOCK"),
        (2, "B"),
        (0, "ENDBLK"),
        (0, "ENDSEC"),
        (0, "SECTION"),
        (2, "THUMBNAILIMAGE"),
        (90, "0"),
        (0, "ENDSEC"),
        (0, "SECTION"),
        (2, "OBJECTS"),
        (0, "DICTIONARY"),
        (0, "ENDSEC"),
        (0, "EOF"),
    ]);
    let (d, warnings) = read_ok(&text);
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(d.classes.len(), 2);
    assert_eq!(d.blocks.len(), 1);
    assert!(d.blocks.get("b").unwrap().entities.is_empty());
    assert_eq!(d.objects.len(), 1);
    assert_eq!(d.other_sections.len(), 1);
    assert_eq!(d.other_sections[0].name, "THUMBNAILIMAGE");
    assert_eq!(d.other_sections[0].pairs, vec![Pair::new(90, "0")]);
}

#[test]
fn missing_eof_and_endsec_are_warnings() {
    let text = dxf(&[
        (0, "SECTION"),
        (2, "ENTITIES"),
        (0, "CIRCLE"),
        (10, "0"),
        (20, "0"),
        (40, "1"),
    ]);
    let (d, warnings) = read_ok(&text);
    assert_eq!(d.entities.len(), 1);
    assert_eq!(warnings.len(), 2, "{warnings:?}");
}

#[test]
fn truncated_file_keeps_what_was_read() {
    let mut text = entities_doc(&[(0, "CIRCLE"), (10, "0"), (20, "0"), (40, "1")]);
    text = text.replace("0\nENDSEC\n0\nEOF\n", "0\nENDSEC\n0");
    let (d, warnings) = read_ok(&text);
    assert_eq!(d.entities.len(), 1);
    assert!(
        warnings
            .iter()
            .any(|w| w.message.contains("rest of the file"))
    );
}

#[test]
fn stray_pairs_outside_sections_are_kept() {
    let text = dxf(&[
        (999, "comments are fine"),
        (5, "stray"),
        (0, "SECTION"),
        (2, "ENTITIES"),
        (0, "ENDSEC"),
        (0, "EOF"),
        (0, "JUNK"),
    ]);
    let (d, warnings) = read_ok(&text);
    assert_eq!(warnings.len(), 2, "{warnings:?}");
    let stray: Vec<(&str, usize, Vec<Pair>)> = d
        .stray
        .iter()
        .map(|r| (r.section.as_str(), r.index, r.pairs.clone()))
        .collect();
    assert_eq!(
        stray,
        [
            ("", 0, vec![Pair::new(999, "comments are fine")]),
            ("", 0, vec![Pair::new(5, "stray")]),
            ("EOF", 0, vec![Pair::new(0, "JUNK")]),
        ]
    );
}

#[test]
fn non_dxf_input_is_an_error() {
    assert!(matches!(read_str(""), Err(DxfError::NotDxf)));
    assert!(matches!(
        read_str("0\nLINE\n0\nEOF\n"),
        Err(DxfError::NotDxf)
    ));
    assert!(read_str("hello world\n").is_err());
}

#[test]
fn binary_dxf_is_rejected_clearly() {
    let mut bytes = b"AutoCAD Binary DXF\r\n\x1a\0".to_vec();
    bytes.extend_from_slice(&[0, 0, 1, 2, 3]);
    assert!(matches!(
        read_bytes(&bytes),
        Err(DxfError::BinaryNotSupported)
    ));
}

#[test]
fn latin1_input_loads_with_warning() {
    let mut bytes = entities_doc(&[(0, "LINE"), (8, "W\u{e4}nde")]).into_bytes();
    // Replace the UTF-8 "ä" (C3 A4) with Latin-1 (E4).
    let at = bytes.windows(2).position(|w| w == [0xC3, 0xA4]).unwrap();
    bytes.splice(at..at + 2, [0xE4]);
    let (d, warnings) = read_bytes(&bytes).unwrap();
    assert_eq!(d.entities[0].layer, "Wände");
    assert!(
        warnings
            .iter()
            .any(|w| w.message.contains("no $DWGCODEPAGE") && w.message.contains("ANSI_1252")),
        "{warnings:?}"
    );
    assert_eq!(
        d.encoding,
        crate::FileEncoding::CodePage(crate::CodePage(1252))
    );
}

#[test]
fn bom_is_ignored() {
    let mut bytes = b"\xEF\xBB\xBF".to_vec();
    bytes.extend_from_slice(entities_doc(&[]).as_bytes());
    let (_, warnings) = read_bytes(&bytes).unwrap();
    assert!(warnings.is_empty());
}

// ---- M2 part A: tricky cases first ----------------------------------------

fn kind_of(text: &str) -> (EntityKind, Vec<Warning>) {
    let (e, w) = only_entity(text);
    (e.kind, w)
}

#[test]
fn lwpolyline_bulges_and_widths_belong_to_their_vertex() {
    let (kind, w) = kind_of(&entities_doc(&[
        (0, "LWPOLYLINE"),
        (100, "AcDbPolyline"),
        (90, "4"),
        (70, "1"),
        (43, "0.0"),
        (38, "2.5"),
        (10, "0"),
        (20, "0"),
        (10, "10"),
        (20, "0"),
        (40, "0.5"),
        (41, "1.5"),
        (42, "1.0"),
        (10, "10"),
        (20, "10"),
        (10, "0"),
        (20, "10"),
        (42, "-0.4142"),
    ]));
    assert!(w.is_empty(), "{w:?}");
    let EntityKind::LwPolyline(p) = kind else {
        panic!("{kind:?}")
    };
    assert!(p.is_closed());
    assert_eq!(p.constant_width, Some(0.0));
    assert_eq!(p.elevation, 2.5);
    let bulges: Vec<f64> = p.vertices.iter().map(|v| v.bulge).collect();
    // The bulge after the last vertex is the closing segment's.
    assert_eq!(bulges, [0.0, 1.0, 0.0, -0.4142]);
    assert_eq!(
        (p.vertices[1].start_width, p.vertices[1].end_width),
        (0.5, 1.5)
    );
    assert_eq!((p.vertices[2].x, p.vertices[2].y), (10.0, 10.0));
    // Vertex 1's bulge is a counter-clockwise semicircle to vertex 2.
    let (a, b) = (&p.vertices[1], &p.vertices[2]);
    let arc = lightable_geom::bulge::arc_from_bulge([a.x, a.y], [b.x, b.y], a.bulge).unwrap();
    assert!((arc.sweep - std::f64::consts::PI).abs() < 1e-12);
    assert_eq!((arc.center, arc.radius), ([10.0, 5.0], 5.0));
    // The closing segment (last vertex back to the first) bulges clockwise.
    let (last, first) = (&p.vertices[3], &p.vertices[0]);
    let closing =
        lightable_geom::bulge::arc_from_bulge([last.x, last.y], [first.x, first.y], last.bulge)
            .unwrap();
    assert!(closing.sweep < 0.0);
}

#[test]
fn lwpolyline_with_vertex_ids() {
    let (kind, w) = kind_of(&entities_doc(&[
        (0, "LWPOLYLINE"),
        (90, "2"),
        (10, "0"),
        (20, "0"),
        (91, "7"),
        (10, "1"),
        (20, "1"),
        (91, "8"),
    ]));
    assert!(w.is_empty());
    let EntityKind::LwPolyline(p) = kind else {
        panic!()
    };
    assert_eq!(p.vertices[1].id, Some(8));
}

#[test]
fn malformed_lwpolylines_become_unknown() {
    let cases: [&[(i16, &str)]; 4] = [
        // count says 3, only 2 vertices
        &[
            (0, "LWPOLYLINE"),
            (90, "3"),
            (10, "0"),
            (20, "0"),
            (10, "1"),
            (20, "1"),
        ],
        // x without y
        &[(0, "LWPOLYLINE"), (10, "0"), (10, "1"), (20, "1")],
        // bulge before any vertex
        &[(0, "LWPOLYLINE"), (42, "1"), (10, "0"), (20, "0")],
        // last vertex without y
        &[(0, "LWPOLYLINE"), (10, "0"), (20, "0"), (10, "1")],
    ];
    for body in cases {
        let (kind, w) = kind_of(&entities_doc(body));
        assert!(matches!(kind, EntityKind::Unknown(_)), "{body:?}");
        assert_eq!(w.len(), 1, "{body:?}");
    }
}

#[test]
fn r12_polyline_sequence_is_grouped() {
    let (d, w) = read_ok(&entities_doc(&[
        (0, "POLYLINE"),
        (8, "P"),
        (66, "1"),
        (10, "0"),
        (20, "0"),
        (30, "5"),
        (70, "1"),
        (0, "VERTEX"),
        (8, "P"),
        (10, "0"),
        (20, "0"),
        (42, "1"),
        (0, "VERTEX"),
        (8, "P"),
        (10, "4"),
        (20, "0"),
        (40, "0.5"),
        (0, "SEQEND"),
        (8, "P"),
        (0, "LINE"),
        (10, "0"),
        (20, "0"),
        (11, "1"),
        (21, "1"),
    ]));
    assert!(w.is_empty(), "{w:?}");
    assert_eq!(d.entities.len(), 2, "POLYLINE+VERTEX+SEQEND is one entity");
    let EntityKind::Polyline(p) = &d.entities[0].kind else {
        panic!()
    };
    assert!(p.is_closed());
    assert_eq!(p.location.z, 5.0);
    assert_eq!(p.vertices.len(), 2);
    let EntityKind::Vertex(v0) = &p.vertices[0].kind else {
        panic!()
    };
    assert_eq!(v0.bulge, 1.0);
    let EntityKind::Vertex(v1) = &p.vertices[1].kind else {
        panic!()
    };
    assert_eq!((v1.location.x, v1.start_width), (4.0, 0.5));
    assert_eq!(p.vertices[1].layer, "P");
    let seqend = p.seqend.as_ref().expect("SEQEND kept");
    assert_eq!(seqend.kind.type_name(), "SEQEND");
    assert_eq!(seqend.layer, "P");
    assert_eq!(d.entities[1].kind.type_name(), "LINE");
}

#[test]
fn polyline_without_seqend_warns_and_keeps_vertices() {
    let (d, w) = read_ok(&entities_doc(&[
        (0, "POLYLINE"),
        (66, "1"),
        (0, "VERTEX"),
        (10, "0"),
        (20, "0"),
        (0, "CIRCLE"),
        (10, "0"),
        (20, "0"),
        (40, "1"),
    ]));
    assert_eq!(d.entities.len(), 2);
    let EntityKind::Polyline(p) = &d.entities[0].kind else {
        panic!()
    };
    assert_eq!(p.vertices.len(), 1);
    assert!(p.seqend.is_none());
    assert_eq!(w.len(), 1);
    assert!(w[0].message.contains("SEQEND"));
}

#[test]
fn polyface_mesh_vertices_keep_face_indices() {
    let (d, w) = read_ok(&entities_doc(&[
        (0, "POLYLINE"),
        (70, "64"),
        (71, "3"),
        (72, "1"),
        (0, "VERTEX"),
        (10, "0"),
        (20, "0"),
        (70, "192"),
        (0, "VERTEX"),
        (10, "0"),
        (20, "0"),
        (70, "128"),
        (71, "1"),
        (72, "2"),
        (73, "-3"),
        (0, "SEQEND"),
    ]));
    assert!(w.is_empty(), "{w:?}");
    let EntityKind::Polyline(p) = &d.entities[0].kind else {
        panic!()
    };
    assert_eq!(p.mesh_counts, (3, 1));
    let EntityKind::Vertex(face) = &p.vertices[1].kind else {
        panic!()
    };
    assert_eq!(face.indices, [1, 2, -3, 0]);
}

#[test]
fn stray_vertex_warns() {
    let (d, w) = read_ok(&entities_doc(&[(0, "VERTEX"), (10, "0"), (20, "0")]));
    assert_eq!(d.entities.len(), 1);
    assert!(w[0].message.contains("without a parent"));
}

#[test]
fn insert_with_attributes() {
    let (d, w) = read_ok(&entities_doc(&[
        (0, "INSERT"),
        (66, "1"),
        (2, "PLATE"),
        (10, "5"),
        (20, "6"),
        (41, "2"),
        (50, "30"),
        (0, "ATTRIB"),
        (10, "5"),
        (20, "8"),
        (40, "2.5"),
        (1, "P-100"),
        (2, "PARTNO"),
        (70, "0"),
        (100, "AcDbAttribute"),
        (74, "2"),
        (0, "ATTRIB"),
        (10, "5"),
        (20, "2"),
        (40, "1.5"),
        (1, " S235 "),
        (2, "MAT"),
        (70, "1"),
        (0, "SEQEND"),
    ]));
    assert!(w.is_empty(), "{w:?}");
    assert_eq!(d.entities.len(), 1);
    let EntityKind::Insert(ins) = &d.entities[0].kind else {
        panic!()
    };
    assert_eq!(ins.block, "PLATE");
    assert_eq!(ins.scale, Point3::new(2.0, 1.0, 1.0));
    assert_eq!(ins.rotation, 30.0);
    assert_eq!((ins.columns, ins.rows), (1, 1));
    assert_eq!(ins.attributes.len(), 2);
    let EntityKind::Attrib(a) = &ins.attributes[0].kind else {
        panic!()
    };
    assert_eq!((a.tag.as_str(), a.text.value.as_str()), ("PARTNO", "P-100"));
    assert_eq!(a.text.vertical_align, 2);
    let EntityKind::Attrib(b) = &ins.attributes[1].kind else {
        panic!()
    };
    assert_eq!(b.text.value, " S235 ", "text values are not trimmed");
    assert_eq!(b.flags, 1);
    assert!(ins.seqend.is_some());
}

#[test]
fn insert_array_without_attributes() {
    let (kind, w) = kind_of(&entities_doc(&[
        (0, "INSERT"),
        (2, "BOLT"),
        (10, "0"),
        (20, "0"),
        (70, "5"),
        (71, "3"),
        (44, "10"),
        (45, "12"),
    ]));
    assert!(w.is_empty());
    let EntityKind::Insert(ins) = kind else {
        panic!()
    };
    assert_eq!((ins.columns, ins.rows), (5, 3));
    assert_eq!((ins.column_spacing, ins.row_spacing), (10.0, 12.0));
    assert!(ins.attributes.is_empty() && ins.seqend.is_none());
}

fn blocks_doc(blocks: &[&[(i16, &'static str)]]) -> String {
    let mut pairs = vec![(0, "SECTION"), (2, "BLOCKS")];
    for b in blocks {
        pairs.extend_from_slice(b);
    }
    pairs.extend_from_slice(&[(0, "ENDSEC"), (0, "EOF")]);
    dxf(&pairs)
}

#[test]
fn nested_blocks() {
    let text = blocks_doc(&[
        &[
            (0, "BLOCK"),
            (5, "20"),
            (8, "0"),
            (2, "Bolt"),
            (70, "0"),
            (10, "0"),
            (20, "0"),
            (30, "0"),
            (3, "Bolt"),
            (1, ""),
            (0, "CIRCLE"),
            (10, "0"),
            (20, "0"),
            (40, "2"),
            (0, "ENDBLK"),
            (5, "21"),
            (8, "0"),
        ],
        &[
            (0, "BLOCK"),
            (2, "PLATE"),
            (70, "2"),
            (10, "1"),
            (20, "2"),
            (0, "INSERT"),
            (2, "BOLT"),
            (10, "5"),
            (20, "5"),
            (0, "POLYLINE"),
            (66, "1"),
            (0, "VERTEX"),
            (10, "0"),
            (20, "0"),
            (0, "VERTEX"),
            (10, "40"),
            (20, "0"),
            (0, "SEQEND"),
            (0, "ATTDEF"),
            (10, "2"),
            (20, "22"),
            (40, "2"),
            (1, ""),
            (3, "Part number?"),
            (2, "PARTNO"),
            (70, "0"),
            (0, "ENDBLK"),
        ],
        &[
            (0, "BLOCK"),
            (2, "FRAME"),
            (10, "0"),
            (20, "0"),
            (0, "INSERT"),
            (2, "plate"),
            (10, "0"),
            (20, "0"),
            (0, "INSERT"),
            (2, "PLATE"),
            (10, "50"),
            (20, "0"),
            (50, "90"),
            (0, "ENDBLK"),
        ],
    ]);
    let (d, w) = read_ok(&text);
    assert!(w.is_empty(), "{w:?}");
    assert_eq!(d.blocks.len(), 3);

    let bolt = d.blocks.get("BOLT").expect("case-insensitive lookup");
    assert_eq!(bolt.name, "Bolt");
    assert_eq!(bolt.handle, Some(Handle(0x20)));
    assert!(
        bolt.extra.is_empty(),
        "group 3 equal to the name is dropped"
    );
    assert_eq!(bolt.end, vec![Pair::new(5, "21"), Pair::new(8, "0")]);

    let plate = d.blocks.get("plate").unwrap();
    assert_eq!(plate.base_point, Point3::new(1.0, 2.0, 0.0));
    assert_eq!(plate.flags, 2);
    let kinds: Vec<&str> = plate.entities.iter().map(|e| e.kind.type_name()).collect();
    assert_eq!(kinds, ["INSERT", "POLYLINE", "ATTDEF"]);
    let EntityKind::Polyline(p) = &plate.entities[1].kind else {
        panic!()
    };
    assert_eq!(p.vertices.len(), 2);

    // Resolve FRAME -> PLATE -> BOLT through the references.
    let frame = d.blocks.get("FRAME").unwrap();
    let mut depth = 0;
    let mut current = frame;
    while let Some(EntityKind::Insert(ins)) = current.entities.first().map(|e| &e.kind) {
        current = d.blocks.get(&ins.block).expect("referenced block exists");
        depth += 1;
    }
    assert_eq!((depth, current.name.as_str()), (2, "Bolt"));
}

#[test]
fn duplicate_or_nameless_blocks_are_kept_verbatim() {
    let text = blocks_doc(&[
        &[
            (0, "BLOCK"),
            (2, "A"),
            (0, "LINE"),
            (10, "0"),
            (20, "0"),
            (11, "1"),
            (21, "1"),
            (0, "ENDBLK"),
            (5, "9"),
        ],
        &[
            (0, "BLOCK"),
            (2, "a"),
            (0, "CIRCLE"),
            (10, "0"),
            (20, "0"),
            (40, "1"),
            (0, "ENDBLK"),
            (5, "A"),
        ],
        &[(0, "BLOCK"), (70, "0"), (0, "ENDBLK")],
    ]);
    let (d, w) = read_ok(&text);
    assert_eq!(d.blocks.len(), 1);
    assert_eq!(d.blocks.unparsed.len(), 2);
    assert_eq!(w.len(), 2, "{w:?}");
    // The duplicate keeps its entity and its ENDBLK record.
    let dup = &d.blocks.unparsed[0];
    assert!(dup.contains(&Pair::new(0, "CIRCLE")));
    assert_eq!(dup.last(), Some(&Pair::new(5, "A")));
}

#[test]
fn block_without_endblk_warns() {
    let text = blocks_doc(&[&[
        (0, "BLOCK"),
        (2, "A"),
        (0, "LINE"),
        (10, "0"),
        (20, "0"),
        (11, "1"),
        (21, "1"),
    ]]);
    let (d, w) = read_ok(&text);
    assert_eq!(d.blocks.get("A").unwrap().entities.len(), 1);
    assert!(w.iter().any(|w| w.message.contains("ENDBLK")));
}

// ---- remaining entity types -------------------------------------------------

#[test]
fn ellipse() {
    let (kind, w) = kind_of(&entities_doc(&[
        (0, "ELLIPSE"),
        (10, "1"),
        (20, "2"),
        (11, "10"),
        (21, "0"),
        (40, "0.5"),
        (41, "0"),
        (42, "2.5"),
    ]));
    assert!(w.is_empty());
    let EntityKind::Ellipse(e) = kind else {
        panic!()
    };
    assert_eq!(e.major_axis, Point3::new(10.0, 0.0, 0.0));
    assert_eq!((e.ratio, e.end_param), (0.5, 2.5));
    let (kind, w) = kind_of(&entities_doc(&[
        (0, "ELLIPSE"),
        (10, "0"),
        (20, "0"),
        (11, "1"),
        (21, "0"),
        (40, "0"),
    ]));
    assert!(matches!(kind, EntityKind::Unknown(_)));
    assert_eq!(w.len(), 1);
}

#[test]
fn full_ellipse_defaults() {
    let (kind, _) = kind_of(&entities_doc(&[
        (0, "ELLIPSE"),
        (10, "0"),
        (20, "0"),
        (11, "1"),
        (21, "0"),
        (40, "1"),
    ]));
    let EntityKind::Ellipse(e) = kind else {
        panic!()
    };
    assert_eq!((e.start_param, e.end_param), (0.0, std::f64::consts::TAU));
}

#[test]
fn point() {
    let (kind, w) = kind_of(&entities_doc(&[
        (0, "POINT"),
        (10, "1"),
        (20, "2"),
        (30, "3"),
        (50, "45"),
    ]));
    assert!(w.is_empty());
    let EntityKind::Point(p) = kind else { panic!() };
    assert_eq!(p.location, Point3::new(1.0, 2.0, 3.0));
    assert_eq!(p.x_axis_angle, 45.0);
}

#[test]
fn text() {
    let (kind, w) = kind_of(&entities_doc(&[
        (0, "TEXT"),
        (100, "AcDbText"),
        (10, "1"),
        (20, "2"),
        (40, "2.5"),
        (1, "  Room 101"),
        (50, "15"),
        (7, "SANS"),
        (72, "1"),
        (11, "3"),
        (21, "2"),
        (100, "AcDbText"),
        (73, "2"),
    ]));
    assert!(w.is_empty(), "{w:?}");
    let EntityKind::Text(t) = kind else { panic!() };
    assert_eq!(t.value, "  Room 101");
    assert_eq!((t.height, t.rotation, t.width_factor), (2.5, 15.0, 1.0));
    assert_eq!(t.style, "SANS");
    assert_eq!((t.horizontal_align, t.vertical_align), (1, 2));
    assert_eq!(t.align_point, Some(Point3::new(3.0, 2.0, 0.0)));
    // Missing height or value: unknown.
    let (kind, _) = kind_of(&entities_doc(&[
        (0, "TEXT"),
        (10, "1"),
        (20, "2"),
        (1, "x"),
    ]));
    assert!(matches!(kind, EntityKind::Unknown(_)));
}

#[test]
fn text_with_empty_value() {
    let (kind, w) = kind_of(&entities_doc(&[
        (0, "TEXT"),
        (10, "1"),
        (20, "2"),
        (40, "1"),
        (1, ""),
    ]));
    assert!(w.is_empty());
    assert!(matches!(kind, EntityKind::Text(t) if t.value.is_empty()));
}

#[test]
fn mtext_joins_chunks_and_ignores_embedded_object() {
    let (e, w) = only_entity(&entities_doc(&[
        (0, "MTEXT"),
        (10, "0"),
        (20, "60"),
        (40, "2.5"),
        (41, "80"),
        (71, "1"),
        (3, "NOTES:\\P1. ALL DIMS"),
        (3, " IN MM\\P"),
        (1, "2. END"),
        (7, "SANS"),
        (11, "1"),
        (21, "0"),
        (101, "Embedded Object"),
        (70, "1"),
        (10, "1"),
        (11, "9"),
        (40, "99"),
    ]));
    assert!(w.is_empty(), "{w:?}");
    let EntityKind::MText(m) = &e.kind else {
        panic!()
    };
    assert_eq!(m.text, "NOTES:\\P1. ALL DIMS IN MM\\P2. END");
    assert_eq!((m.height, m.reference_width, m.attachment), (2.5, 80.0, 1));
    assert_eq!(m.x_direction, Some(Point3::new(1.0, 0.0, 0.0)));
    assert_eq!(m.insert, Point3::new(0.0, 60.0, 0.0));
    let codes: Vec<i16> = e.extra.iter().map(|p| p.code).collect();
    assert_eq!(codes, [101, 70, 10, 11, 40]);
}

fn dimension_of(body: &[(i16, &str)]) -> Dimension {
    let mut pairs = vec![
        (0, "DIMENSION"),
        (2, "*D1"),
        (10, "1"),
        (20, "2"),
        (11, "3"),
        (21, "4"),
        (3, "Standard"),
    ];
    pairs.extend_from_slice(body);
    let (kind, w) = kind_of(&entities_doc(&pairs));
    assert!(w.is_empty(), "{w:?}");
    match kind {
        EntityKind::Dimension(d) => *d,
        other => panic!("{other:?}"),
    }
}

#[test]
fn dimension_types() {
    let d = dimension_of(&[
        (70, "32"),
        (13, "0"),
        (23, "0"),
        (14, "10"),
        (24, "0"),
        (50, "90"),
    ]);
    assert_eq!(d.flags, 32);
    assert_eq!(d.block, "*D1");
    assert_eq!(d.definition_point, Point3::new(1.0, 2.0, 0.0));
    assert!(matches!(d.kind, DimensionKind::Linear { angle: 90.0, .. }));

    let d = dimension_of(&[(70, "33"), (13, "0"), (23, "0"), (14, "3"), (24, "4")]);
    assert!(matches!(d.kind, DimensionKind::Aligned { second, .. } if second.y == 4.0));

    let d = dimension_of(&[
        (70, "34"),
        (13, "0"),
        (23, "0"),
        (14, "1"),
        (24, "0"),
        (15, "0"),
        (25, "0"),
        (16, "1"),
        (26, "1"),
    ]);
    assert!(matches!(d.kind, DimensionKind::Angular { arc_point, .. } if arc_point.x == 1.0));

    let d = dimension_of(&[(70, "3"), (15, "5"), (25, "0"), (40, "2")]);
    assert!(matches!(
        d.kind,
        DimensionKind::Diameter {
            leader_length: 2.0,
            ..
        }
    ));

    let d = dimension_of(&[(70, "4"), (15, "5"), (25, "0")]);
    assert!(matches!(d.kind, DimensionKind::Radius { chord_point, .. } if chord_point.x == 5.0));

    let d = dimension_of(&[
        (70, "5"),
        (13, "1"),
        (23, "0"),
        (14, "0"),
        (24, "1"),
        (15, "0"),
        (25, "0"),
    ]);
    assert!(
        matches!(d.kind, DimensionKind::Angular3Point { vertex, .. } if vertex == Point3::default())
    );

    let d = dimension_of(&[(70, "70"), (13, "1"), (23, "0"), (14, "1"), (24, "5")]);
    assert!(matches!(
        d.kind,
        DimensionKind::Ordinate { x_type: true, .. }
    ));
    assert_eq!(d.flags, 64);
}

#[test]
fn dimension_keeps_codes_of_other_types_and_overrides() {
    let (e, w) = only_entity(&entities_doc(&[
        (0, "DIMENSION"),
        (2, "*D2"),
        (10, "0"),
        (20, "0"),
        (11, "1"),
        (21, "1"),
        (70, "4"),
        (1, "R<>"),
        (42, "5.0"),
        (15, "5"),
        (25, "0"),
        (13, "9"),
        (23, "9"),
        (1001, "ACAD"),
        (1000, "DSTYLE"),
    ]));
    assert!(w.is_empty());
    let EntityKind::Dimension(d) = &e.kind else {
        panic!()
    };
    assert_eq!(
        (d.text_override.as_str(), d.measurement),
        ("R<>", Some(5.0))
    );
    let codes: Vec<i16> = e.extra.iter().map(|p| p.code).collect();
    assert_eq!(
        codes,
        [13, 23, 1001, 1000],
        "13/23 do not belong to a radius dimension"
    );
}

#[test]
fn dimension_missing_points_or_bad_type_is_unknown() {
    for body in [
        &[
            (0, "DIMENSION"),
            (10, "0"),
            (20, "0"),
            (11, "1"),
            (21, "1"),
            (70, "0"),
            (13, "0"),
            (23, "0"),
        ][..],
        &[
            (0, "DIMENSION"),
            (10, "0"),
            (20, "0"),
            (11, "1"),
            (21, "1"),
            (70, "9"),
        ][..],
    ] {
        let (kind, w) = kind_of(&entities_doc(body));
        assert!(matches!(kind, EntityKind::Unknown(_)));
        assert_eq!(w.len(), 1);
    }
}

#[test]
fn spline_with_knots_weights_and_fit_points() {
    let (kind, w) = kind_of(&entities_doc(&[
        (0, "SPLINE"),
        (210, "0"),
        (220, "0"),
        (230, "1"),
        (70, "12"),
        (71, "3"),
        (72, "8"),
        (73, "4"),
        (74, "2"),
        (42, "1e-10"),
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
        (30, "0"),
        (41, "1"),
        (10, "1"),
        (20, "1"),
        (30, "0"),
        (41, "2"),
        (10, "2"),
        (20, "1"),
        (30, "0"),
        (41, "2"),
        (10, "3"),
        (20, "0"),
        (30, "0"),
        (41, "1"),
        (11, "0"),
        (21, "0"),
        (31, "0"),
        (11, "3"),
        (21, "0"),
        (31, "0"),
        (12, "1"),
        (22, "0"),
        (32, "0"),
    ]));
    assert!(w.is_empty(), "{w:?}");
    let EntityKind::Spline(s) = kind else {
        panic!()
    };
    assert_eq!((s.flags, s.degree), (12, 3));
    assert_eq!(s.knots.len(), 8);
    assert_eq!(s.weights, [1.0, 2.0, 2.0, 1.0]);
    assert_eq!(s.control_points[1], Point3::new(1.0, 1.0, 0.0));
    assert_eq!(s.fit_points.len(), 2);
    assert_eq!(s.start_tangent, Some(Point3::new(1.0, 0.0, 0.0)));
    assert_eq!(s.end_tangent, None);
    assert_eq!(s.normal, Some(Point3::new(0.0, 0.0, 1.0)));
    assert_eq!(s.knot_tolerance, Some(1e-10));
}

#[test]
fn spline_count_mismatch_is_unknown() {
    let (kind, w) = kind_of(&entities_doc(&[
        (0, "SPLINE"),
        (73, "3"),
        (10, "0"),
        (20, "0"),
        (10, "1"),
        (20, "1"),
    ]));
    assert!(matches!(kind, EntityKind::Unknown(_)));
    assert!(w[0].message.contains("control point count"));
}

// ---- tables -----------------------------------------------------------------

fn tables_doc(tables: &[&[(i16, &'static str)]]) -> String {
    let mut pairs = vec![(0, "SECTION"), (2, "TABLES")];
    for t in tables {
        pairs.extend_from_slice(t);
    }
    pairs.extend_from_slice(&[(0, "ENDSEC"), (0, "EOF")]);
    dxf(&pairs)
}

#[test]
fn linetypes_with_complex_elements() {
    let text = tables_doc(&[&[
        (0, "TABLE"),
        (2, "LTYPE"),
        (70, "2"),
        (0, "LTYPE"),
        (5, "14"),
        (2, "Continuous"),
        (70, "0"),
        (3, "Solid line"),
        (72, "65"),
        (73, "0"),
        (40, "0.0"),
        (0, "LTYPE"),
        (2, "GAS"),
        (3, "Gas line ----GAS----"),
        (72, "65"),
        (73, "3"),
        (40, "1.5"),
        (49, "1.0"),
        (74, "0"),
        (49, "-0.25"),
        (74, "2"),
        (75, "0"),
        (340, "11"),
        (46, "0.1"),
        (50, "0.0"),
        (44, "-0.1"),
        (45, "-0.05"),
        (9, "GAS"),
        (49, "-0.25"),
        (74, "0"),
        (0, "ENDTAB"),
    ]]);
    let (d, w) = read_ok(&text);
    assert!(w.is_empty(), "{w:?}");
    let lts = &d.tables.linetypes;
    assert_eq!(lts.len(), 2);
    assert!(lts.get("CONTINUOUS").unwrap().elements.is_empty());
    let gas = lts.get("gas").unwrap();
    assert_eq!(gas.pattern_length, 1.5);
    assert_eq!(gas.elements.len(), 3);
    let text = &gas.elements[1];
    assert_eq!((text.length, text.flags), (-0.25, 2));
    assert_eq!(text.style, Some(Handle(0x11)));
    assert_eq!(text.offset, (-0.1, -0.05));
    assert_eq!(text.text.as_deref(), Some("GAS"));
    assert_eq!(gas.elements[2].scale, 1.0);
}

#[test]
fn text_styles_dim_styles_and_block_records() {
    let text = tables_doc(&[
        &[
            (0, "TABLE"),
            (2, "STYLE"),
            (0, "STYLE"),
            (2, "Standard"),
            (70, "0"),
            (40, "0"),
            (41, "1"),
            (3, "txt"),
            (4, ""),
            (0, "STYLE"),
            (2, ""),
            (70, "1"),
            (3, "ltypeshp.shx"),
            (0, "STYLE"),
            (2, ""),
            (70, "1"),
            (3, "other.shx"),
            (0, "ENDTAB"),
        ],
        &[
            (0, "TABLE"),
            (2, "DIMSTYLE"),
            (100, "AcDbSymbolTable"),
            (71, "1"),
            (0, "DIMSTYLE"),
            (105, "27"),
            (100, "AcDbDimStyleTableRecord"),
            (2, "ISO-25"),
            (70, "0"),
            (5, "ARCHTICK"),
            (40, "1.0"),
            (140, "2.5"),
            (340, "11"),
            (0, "ENDTAB"),
        ],
        &[
            (0, "TABLE"),
            (2, "BLOCK_RECORD"),
            (0, "BLOCK_RECORD"),
            (5, "1F"),
            (2, "*Model_Space"),
            (340, "22"),
            (70, "4"),
            (280, "1"),
            (281, "0"),
            (0, "ENDTAB"),
        ],
    ]);
    let (d, w) = read_ok(&text);
    assert!(w.is_empty(), "{w:?}");
    let styles = &d.tables.text_styles;
    assert_eq!(
        styles.len(),
        3,
        "shape-file entries with empty names are kept"
    );
    assert_eq!(styles.get("standard").unwrap().font, "txt");
    assert_eq!(styles.entries[2].font, "other.shx");

    let dims = &d.tables.dim_styles;
    let iso = dims.get("iso-25").unwrap();
    assert_eq!(iso.handle, Some(Handle(0x27)));
    assert_eq!(iso.variable(140), Some("2.5"));
    assert_eq!(
        iso.variable(5),
        Some("ARCHTICK"),
        "group 5 is DIMBLK, not a handle"
    );
    assert_eq!(iso.variables.len(), 4);
    assert_eq!(dims.header.len(), 2);

    let model = d.tables.block_records.get("*MODEL_SPACE").unwrap();
    assert_eq!(model.layout, Some(Handle(0x22)));
    assert_eq!(model.units, 4);
    assert!(model.explodable && !model.scalable);
}

#[test]
fn linetype_count_mismatch_is_kept_unparsed() {
    let text = tables_doc(&[&[
        (0, "TABLE"),
        (2, "LTYPE"),
        (0, "LTYPE"),
        (2, "BAD"),
        (73, "2"),
        (49, "1"),
        (0, "ENDTAB"),
    ]]);
    let (d, w) = read_ok(&text);
    assert!(d.tables.linetypes.is_empty());
    assert_eq!(d.tables.linetypes.unparsed.len(), 1);
    assert_eq!(w.len(), 1);
}

#[test]
fn repeated_children_after_seqend_are_named_as_duplicates() {
    let (d, w) = read_ok(&entities_doc(&[
        (0, "POLYLINE"),
        (5, "8A"),
        (66, "1"),
        (0, "VERTEX"),
        (5, "8C"),
        (10, "0"),
        (20, "0"),
        (0, "SEQEND"),
        (5, "8B"),
        (0, "VERTEX"),
        (5, "8C"),
        (10, "0"),
        (20, "0"),
        (0, "VERTEX"),
        (5, "8C"),
        (10, "0"),
        (20, "0"),
    ]));
    assert_eq!(d.entities.len(), 3, "the repeats are kept, not dropped");
    assert_eq!(w.len(), 2);
    assert!(
        w.iter().all(|w| w.message.contains("repeats handle 8C")),
        "{w:?}"
    );
}

// ---- M2 part B: nothing is dropped -------------------------------------------

#[test]
fn stray_records_in_sections_are_kept_with_their_position() {
    let text = dxf(&[
        (0, "SECTION"),
        (2, "HEADER"),
        (70, "1"),
        (9, "$ACADVER"),
        (1, "AC1015"),
        (0, "ENDSEC"),
        (0, "SECTION"),
        (2, "TABLES"),
        (0, "TABLE"),
        (2, "VPORT"),
        (0, "ENDTAB"),
        (0, "JUNK"),
        (5, "1"),
        (0, "ENDSEC"),
        (0, "SECTION"),
        (2, "BLOCKS"),
        (0, "BLOCK"),
        (2, "A"),
        (0, "ENDBLK"),
        (5, "1D"),
        (0, "ENDBLK"),
        (5, "76"),
        (0, "BLOCK"),
        (2, "B"),
        (0, "ENDBLK"),
        (0, "ENDSEC"),
        (0, "SECTION"),
        (2, "ENTITIES"),
        (8, "early"),
        (0, "POINT"),
        (10, "0"),
        (20, "0"),
        (0, "ENDSEC"),
        (0, "EOF"),
    ]);
    let (d, w) = read_ok(&text);
    assert_eq!(w.len(), 4, "{w:?}");
    assert!(
        w.iter()
            .any(|w| w.message == "ENDBLK outside a block definition; kept as a stray record")
    );
    let kept: Vec<(&str, usize, usize)> = d
        .stray
        .iter()
        .map(|r| (r.section.as_str(), r.index, r.pairs.len()))
        .collect();
    assert_eq!(
        kept,
        [
            ("HEADER", 0, 1),
            ("TABLES", 1, 2),
            ("BLOCKS", 1, 2),
            ("ENTITIES", 0, 1)
        ]
    );
    assert_eq!(
        d.stray[2].pairs,
        vec![Pair::new(0, "ENDBLK"), Pair::new(5, "76")]
    );
    assert_eq!(d.blocks.len(), 2);
    assert_eq!(d.entities.len(), 1);
}

#[test]
fn truncated_file_keeps_the_unparsed_tail() {
    let text = "0\nSECTION\n2\nENTITIES\n0\nPOINT\n10\n0\n20\n0\nnot a code\n1.0\n0\nENDSEC\n";
    let (d, w) = read_ok(text);
    assert_eq!(d.entities.len(), 1);
    assert_eq!(
        d.unparsed_tail.as_deref(),
        Some("not a code\n1.0\n0\nENDSEC\n")
    );
    assert!(
        w.iter()
            .any(|w| w.message.contains("kept as unparsed text"))
    );
}

#[test]
fn attdef_in_a_block() {
    let text = blocks_doc(&[&[
        (0, "BLOCK"),
        (2, "TAGGED"),
        (70, "2"),
        (0, "ATTDEF"),
        (100, "AcDbText"),
        (10, "2"),
        (20, "22"),
        (40, "2"),
        (1, "%%c25"),
        (100, "AcDbAttributeDefinition"),
        (3, "Part number?"),
        (2, "PARTNO"),
        (70, "8"),
        (74, "1"),
        (0, "ENDBLK"),
    ]]);
    let (d, w) = read_ok(&text);
    assert!(w.is_empty(), "{w:?}");
    let block = d.blocks.get("tagged").unwrap();
    let EntityKind::AttDef(a) = &block.entities[0].kind else {
        panic!("{:?}", block.entities[0].kind)
    };
    assert_eq!(
        (a.tag.as_str(), a.prompt.as_str(), a.flags),
        ("PARTNO", "Part number?", 8)
    );
    assert_eq!(a.text.value, "⌀25");
    assert_eq!(a.text.raw_value.as_deref(), Some("%%c25"));
    assert_eq!(a.text.vertical_align, 1);
    // Missing tag: unknown with a warning.
    let (kind, w) = kind_of(&entities_doc(&[
        (0, "ATTDEF"),
        (10, "0"),
        (20, "0"),
        (40, "1"),
        (1, ""),
    ]));
    assert!(matches!(kind, EntityKind::Unknown(_)));
    assert_eq!(w.len(), 1);
}

// ---- M2 part B: encodings ----------------------------------------------------

/// A minimal file with a header and one TEXT, as bytes in some encoding.
fn text_file(acadver: &str, codepage: Option<&str>, text: &[u8]) -> Vec<u8> {
    let mut out = format!("0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\n{acadver}\n").into_bytes();
    if let Some(cp) = codepage {
        out.extend_from_slice(format!("9\n$DWGCODEPAGE\n3\n{cp}\n").as_bytes());
    }
    out.extend_from_slice(
        b"0\nENDSEC\n0\nSECTION\n2\nENTITIES\n0\nTEXT\n8\n0\n10\n0\n20\n0\n40\n1\n1\n",
    );
    out.extend_from_slice(text);
    out.extend_from_slice(b"\n0\nENDSEC\n0\nEOF\n");
    out
}

fn text_value(bytes: &[u8]) -> (String, Option<String>, crate::FileEncoding, Vec<Warning>) {
    let (d, w) = read_bytes(bytes).expect("loads");
    let EntityKind::Text(t) = &d.entities[0].kind else {
        panic!("{:?}", d.entities[0].kind)
    };
    (t.value.clone(), t.raw_value.clone(), d.encoding, w)
}

#[test]
fn pre_2007_code_pages() {
    use crate::{CodePage, FileEncoding};
    let cases: [(&str, &[u8], &str); 7] = [
        ("ANSI_1250", b"Zak\xb3ad \xb9\xe6", "Zakład ąć"),
        ("ANSI_1251", b"\xcf\xf0\xe8\xe2\xe5\xf2", "Привет"),
        ("ANSI_1252", b"W\xe4nde 90\xb0", "Wände 90°"),
        ("ansi_1253", b"\xc1\xe2\xe3", "Αβγ"),
        ("ANSI_932", b"\x94\x7a\x8a\xc7", "配管"),
        ("ANSI_936", b"\xc5\xe4\xb9\xdc", "配管"),
        ("ANSI_949", b"\xb9\xe8\xb0\xfc", "배관"),
    ];
    for (cp, bytes, expected) in cases {
        let (value, raw, encoding, w) = text_value(&text_file("AC1015", Some(cp), bytes));
        assert_eq!(value, expected, "{cp}");
        assert_eq!(raw, None, "byte decoding is not an escape: {cp}");
        assert!(w.is_empty(), "{cp}: {w:?}");
        let page = CodePage::from_dwgcodepage(cp).unwrap();
        assert_eq!(encoding, FileEncoding::CodePage(page));
    }
}

#[test]
fn r2007_and_later_are_utf8_whatever_the_code_page_says() {
    let (value, _, encoding, w) =
        text_value(&text_file("AC1032", Some("ANSI_1252"), "配管 Ø".as_bytes()));
    assert_eq!(value, "配管 Ø");
    assert_eq!(encoding, crate::FileEncoding::Utf8);
    assert!(w.is_empty());
    // Not valid UTF-8 in a UTF-8 version: decoded with the code page, warned.
    let (value, _, _, w) = text_value(&text_file("AC1032", Some("ANSI_1252"), b"W\xe4nde"));
    assert_eq!(value, "Wände");
    assert!(w[0].message.contains("AC1032 files are UTF-8"), "{w:?}");
}

#[test]
fn pre_2007_file_that_is_valid_utf8_reads_as_utf8() {
    let (value, _, encoding, w) =
        text_value(&text_file("AC1015", Some("ANSI_1252"), "Wände".as_bytes()));
    assert_eq!(value, "Wände");
    assert_eq!(encoding, crate::FileEncoding::Utf8);
    assert!(w.is_empty());
    // ASCII-only files report their declared code page.
    let (_, _, encoding, _) = text_value(&text_file("AC1009", Some("ANSI_1251"), b"plain"));
    assert_eq!(
        encoding,
        crate::FileEncoding::CodePage(crate::CodePage(1251))
    );
}

#[test]
fn unsupported_or_missing_code_page_falls_back_with_a_warning() {
    let (value, _, _, w) = text_value(&text_file("AC1015", Some("ANSI_1361"), b"x\xe4"));
    assert_eq!(value, "xä");
    assert!(
        w[0].message.contains("\"ANSI_1361\" is not supported"),
        "{w:?}"
    );
    let (value, _, _, w) = text_value(&text_file("AC1009", None, b"x\xe4"));
    assert_eq!(value, "xä");
    assert!(w[0].message.contains("no $DWGCODEPAGE"), "{w:?}");
    // Bytes the code page cannot decode are kept as private-use characters.
    let (value, _, _, w) = text_value(&text_file("AC1015", Some("ANSI_932"), b"\x94"));
    assert_eq!(value, "\u{F794}", "kept, not replaced");
    assert!(
        w.iter().any(|w| w.message.contains("not valid ANSI_932")),
        "{w:?}"
    );
}

#[test]
fn escapes_are_decoded_and_the_original_kept() {
    let (value, raw, _, _) = text_value(&text_file(
        "AC1015",
        Some("ANSI_1252"),
        b"\\U+914D\\U+7BA1 %%d",
    ));
    assert_eq!(value, "配管 °");
    assert_eq!(raw.as_deref(), Some("\\U+914D\\U+7BA1 %%d"));
    // %%nnn uses the drawing's code page.
    let (value, _, _, _) = text_value(&text_file("AC1015", Some("ANSI_1250"), b"%%185"));
    assert_eq!(value, "ą");
    // read_str also honours $DWGCODEPAGE for %%nnn.
    let text = String::from_utf8(text_file("AC1015", Some("ANSI_1250"), b"%%185")).unwrap();
    let (d, _) = read_str(&text).unwrap();
    assert!(matches!(&d.entities[0].kind, EntityKind::Text(t) if t.value == "ą"));
}

#[test]
fn mtext_and_dimension_text_are_decoded() {
    let (d, w) = read_ok(&entities_doc(&[
        (0, "MTEXT"),
        (10, "0"),
        (20, "0"),
        (40, "1"),
        (3, "\\U+00D8%%c "),
        (1, "\\P\\\\U+0041"),
        (0, "DIMENSION"),
        (10, "0"),
        (20, "0"),
        (11, "0"),
        (21, "0"),
        (70, "3"),
        (15, "1"),
        (25, "0"),
        (1, "%%c<>"),
    ]));
    assert!(w.is_empty(), "{w:?}");
    let EntityKind::MText(m) = &d.entities[0].kind else {
        panic!()
    };
    assert_eq!(m.text, "Ø⌀ \\P\\\\U+0041");
    assert_eq!(m.raw_text.as_deref(), Some("\\U+00D8%%c \\P\\\\U+0041"));
    let EntityKind::Dimension(dim) = &d.entities[1].kind else {
        panic!()
    };
    assert_eq!(dim.text_override, "⌀<>");
    assert_eq!(dim.raw_text_override.as_deref(), Some("%%c<>"));
}

#[test]
fn plain_text_has_no_raw_copy() {
    let (value, raw, _, _) = text_value(&text_file("AC1032", None, b"Room 101"));
    assert_eq!((value.as_str(), raw), ("Room 101", None));
}

#[test]
fn header_value_sniffing() {
    let bytes = b"0\nSECTION\n2\nHEADER\n9\n$ACADVERX\n1\nNO\n9\n$ACADVER\r\n  1\r\nAC1018 \r\n";
    assert_eq!(super::header_value(bytes, "$ACADVER", "1"), Some("AC1018"));
    assert_eq!(super::header_value(bytes, "$DWGCODEPAGE", "3"), None);
    assert_eq!(super::header_value(b"9\n$ACADVER", "$ACADVER", "1"), None);
    assert_eq!(super::line_offset("a\nb\nc", 3), 4);
    assert_eq!(super::line_offset("a\nb", 9), 3);
}
