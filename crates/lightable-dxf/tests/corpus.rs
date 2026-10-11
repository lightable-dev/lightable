//! M1 acceptance: every corpus file loads without panicking, plus spot
//! checks of what was read and a cheap robustness sweep over damaged input.

use std::path::{Path, PathBuf};

use lightable_dxf::{Color, Drawing, EntityKind, LineWeight, Point3, Version, Warning, read_bytes};

fn corpus_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/corpus")
}

/// Git-ignored folder for local files that can't be redistributed.
fn private_corpus_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/corpus-private")
}

/// The `.dxf` files directly in `dir`, sorted; empty if `dir` doesn't exist.
fn dxf_files(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .map(|e| e.expect("readable dir entry").path())
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("dxf")))
        .collect();
    files.sort();
    files
}

/// Corpus files converted by ODA File Converter, or by LibreDWG from a DWG
/// that ODA made, are not in the public repository; they are kept in
/// `tests/corpus-private/converted/` and tested when present.
fn converted_dir() -> PathBuf {
    private_corpus_dir().join("converted")
}

fn have_converted() -> bool {
    !dxf_files(&converted_dir()).is_empty()
}

/// The public corpus plus, when present, the converted files.
fn corpus_files() -> Vec<PathBuf> {
    let mut files = dxf_files(&corpus_dir());
    files.extend(dxf_files(&converted_dir()));
    files.sort_by_key(|p| p.file_name().map(|n| n.to_owned()));
    files
}

fn load(name: &str) -> (Drawing, Vec<Warning>) {
    let public = corpus_dir().join(name);
    let path = if public.exists() {
        public
    } else {
        converted_dir().join(name)
    };
    lightable_dxf::read_file(&path).unwrap_or_else(|e| panic!("{name} failed to load: {e}"))
}

fn kinds(d: &Drawing) -> Vec<&str> {
    d.entities.iter().map(|e| e.kind.type_name()).collect()
}

fn parsed(d: &Drawing) -> usize {
    d.entities
        .iter()
        .filter(|e| !matches!(e.kind, EntityKind::Unknown(_)))
        .count()
}

#[test]
fn at_least_ten_corpus_files_load() {
    let files = corpus_files();
    assert!(files.len() >= 10, "only {} corpus files", files.len());
    for path in files {
        if let Err(err) = lightable_dxf::read_file(&path) {
            panic!("{} failed to load: {err}", path.display());
        }
    }
}

/// Files in `tests/corpus-private/`, when present, must load too.
#[test]
fn private_corpus_files_load() {
    let files = dxf_files(&private_corpus_dir());
    if files.is_empty() {
        eprintln!("no private corpus files; skipping");
    }
    for path in files {
        match lightable_dxf::read_file(&path) {
            Ok((_, warnings)) => {
                for w in warnings {
                    eprintln!("{}: {w}", path.display());
                }
            }
            Err(err) => panic!("{} failed to load: {err}", path.display()),
        }
    }
}

/// LibreDWG 0.14 writes some records twice in R14/R2000 output: VERTEX and
/// ATTRIB records repeated after their SEQEND, and an extra ENDBLK, with
/// duplicate handles. These files load, but those defects are reported.
const LIBREDWG_DUPLICATE_RECORDS: [&str; 18] = [
    "ellipses_splines__libredwg__R14.dxf",
    "ellipses_splines__libredwg__R2000.dxf",
    "hatch_solids__libredwg__R14.dxf",
    "hatch_solids__libredwg__R2000.dxf",
    "layers_linetypes__libredwg__R14.dxf",
    "layers_linetypes__libredwg__R2000.dxf",
    "nested_blocks__libredwg__R14.dxf",
    "nested_blocks__libredwg__R2000.dxf",
    "paperspace_layout__libredwg__R14.dxf",
    "paperspace_layout__libredwg__R2000.dxf",
    "polylines__libredwg__R14.dxf",
    "polylines__libredwg__R2000.dxf",
    "site_mixed__libredwg__R14.dxf",
    "site_mixed__libredwg__R2000.dxf",
    "text_styles__libredwg__R14.dxf",
    "text_styles__libredwg__R2000.dxf",
    "xdata_groups__libredwg__R14.dxf",
    "xdata_groups__libredwg__R2000.dxf",
];

fn is_duplicate_record_warning(w: &Warning) -> bool {
    w.message.contains("repeats handle")
        || w.message == "ENDBLK outside a block definition; kept as a stray record"
}

/// Hand-written files that exist to produce warnings.
const DELIBERATELY_DAMAGED: [&str; 2] = ["damaged_entities.dxf", "entities_only_no_eof.dxf"];

/// M2 acceptance: every corpus file loads with zero warnings, apart from the
/// known LibreDWG duplicate records and the deliberately damaged fixtures.
#[test]
fn corpus_files_have_no_warnings() {
    let mut checked = 0;
    let mut problems = Vec::new();
    for path in corpus_files() {
        let name = path.file_name().unwrap().to_str().unwrap().to_owned();
        if DELIBERATELY_DAMAGED.contains(&name.as_str()) {
            continue;
        }
        let (_, warnings) = lightable_dxf::read_file(&path).unwrap();
        let known_bad = LIBREDWG_DUPLICATE_RECORDS.contains(&name.as_str());
        for w in warnings {
            if !(known_bad && is_duplicate_record_warning(&w)) {
                problems.push(format!("{name}: {w}"));
            }
        }
        checked += 1;
    }
    assert!(
        checked >= if have_converted() { 196 } else { 61 },
        "only {checked} files"
    );
    assert!(
        problems.is_empty(),
        "{} warnings:\n{}",
        problems.len(),
        problems.join("\n")
    );
}

/// The special characters in the `text_styles` drawing decode to the same
/// text whatever the writer, version or encoding (code page, UTF-8, \U+),
/// for every file that has text.
#[test]
fn special_characters_decode_the_same_in_every_file() {
    let wanted = "Wände Ø25 ±0.1 °C µm 配管";
    let mut checked = 0;
    let mut problems = Vec::new();
    for path in corpus_files() {
        let name = path.file_name().unwrap().to_str().unwrap().to_owned();
        // FreeCAD's exporter writes no text entities at all.
        if !name.starts_with("text_styles__") || name.contains("__freecad__") {
            continue;
        }
        let (d, _) = lightable_dxf::read_file(&path).unwrap();
        let found = d.entities.iter().any(|e| match &e.kind {
            EntityKind::Text(t) => t.value == wanted,
            _ => false,
        });
        if !found {
            let texts: Vec<String> = d
                .entities
                .iter()
                .filter_map(|e| match &e.kind {
                    EntityKind::Text(t) if t.value.starts_with('W') => Some(t.value.clone()),
                    _ => None,
                })
                .collect();
            problems.push(format!("{name}: {texts:?}"));
        }
        checked += 1;
    }
    assert!(
        checked >= if have_converted() { 10 } else { 2 },
        "only {checked} files"
    );
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// The known-bad list stays accurate: each listed file really has the defect.
#[test]
fn libredwg_duplicate_records_are_reported() {
    if !have_converted() {
        eprintln!("no converted corpus files; skipping");
        return;
    }
    for name in LIBREDWG_DUPLICATE_RECORDS {
        let (_, warnings) = load(name);
        assert!(
            warnings.iter().any(is_duplicate_record_warning),
            "{name} no longer reports duplicate records"
        );
    }
}

/// Every v0.1 entity type is parsed somewhere in the corpus, and the only
/// unknown types left are ones outside v0.1.
#[test]
fn v01_entities_are_parsed_across_the_corpus() {
    use std::collections::BTreeSet;
    let mut typed = BTreeSet::new();
    let mut unknown = BTreeSet::new();
    let mut visit = |e: &lightable_dxf::Entity| match &e.kind {
        EntityKind::Unknown(u) => {
            unknown.insert(u.type_name.clone());
        }
        kind => {
            typed.insert(kind.type_name().to_owned());
        }
    };
    for path in corpus_files() {
        let (d, _) = lightable_dxf::read_file(&path).unwrap();
        let block_entities = d.blocks.iter().flat_map(|b| b.entities.iter());
        for e in d.entities.iter().chain(block_entities) {
            visit(e);
            match &e.kind {
                EntityKind::Polyline(p) => p.vertices.iter().for_each(&mut visit),
                EntityKind::Insert(i) => i.attributes.iter().for_each(&mut visit),
                _ => {}
            }
        }
    }
    for kind in [
        "LINE",
        "CIRCLE",
        "ARC",
        "LWPOLYLINE",
        "ELLIPSE",
        "POINT",
        "TEXT",
        "MTEXT",
        "INSERT",
        "ATTRIB",
        "DIMENSION",
        "SPLINE",
        "POLYLINE",
        "VERTEX",
    ] {
        assert!(typed.contains(kind), "{kind} never parsed");
    }
    // Damaged hand-written records and the LibreDWG repeats may also be
    // unknown or orphaned; real v0.1 types must never be unknown otherwise.
    let allowed_unknown = [
        "3DFACE",
        "ACME_CUSTOM_THING",
        "ATTDEF",
        "HATCH",
        "LEADER",
        "MULTILEADER",
        "RAY",
        "SEQEND",
        "SOLID",
        "TRACE",
        "VIEWPORT",
        "XLINE",
        "LINE",
        "CIRCLE",
        "ARC",
    ];
    let unexpected: Vec<_> = unknown
        .iter()
        .filter(|t| !allowed_unknown.contains(&t.as_str()))
        .collect();
    assert!(unexpected.is_empty(), "unparsed v0.1 types: {unexpected:?}");
}

/// Dimension types present in the corpus. There is no true aligned (type 1)
/// dimension: ezdxf writes `add_aligned_dim` as a rotated linear dimension,
/// and no converter changes that. Type 1 is covered by unit tests.
#[test]
fn corpus_dimensions_cover_linear_radial_and_angular() {
    use lightable_dxf::DimensionKind as K;
    let mut seen = [false; 4];
    for path in corpus_files() {
        let (d, _) = lightable_dxf::read_file(&path).unwrap();
        for e in &d.entities {
            if let EntityKind::Dimension(dim) = &e.kind {
                let i = match dim.kind {
                    K::Linear { .. } => 0,
                    K::Radius { .. } => 1,
                    K::Diameter { .. } => 2,
                    K::Angular { .. } | K::Angular3Point { .. } => 3,
                    K::Aligned { .. } | K::Ordinate { .. } => continue,
                };
                seen[i] = true;
            }
        }
    }
    assert_eq!(seen, [true; 4], "linear, radius, diameter, angular");
}

#[test]
fn r12_single_line() {
    let (d, w) = load("r12_single_line.dxf");
    assert!(w.is_empty(), "{w:?}");
    assert_eq!(d.header.version(), Some(Version::R12));
    assert_eq!(d.header.extmax(), Some(Point3::new(100.0, 50.0, 0.0)));
    let EntityKind::Line(line) = &d.entities[0].kind else {
        panic!()
    };
    assert_eq!(line.end, Point3::new(100.0, 50.0, 0.0));
}

#[test]
fn r12_circle_arc_layers() {
    let (d, w) = load("r12_circle_arc_layers.dxf");
    assert!(w.is_empty(), "{w:?}");
    assert_eq!(kinds(&d), ["CIRCLE", "ARC"]);
    assert_eq!(d.header.current_layer(), Some("GEOMETRY"));
    assert_eq!(
        d.tables.layers.get("geometry").unwrap().color,
        Color::Index(3)
    );
    assert!(d.tables.linetypes.get("continuous").is_some());
}

#[test]
fn ac1015_floor_plan() {
    let (d, w) = load("ac1015_floor_plan.dxf");
    assert!(w.is_empty(), "{w:?}");
    assert_eq!(d.header.version(), Some(Version::R2000));
    assert_eq!(d.header.codepage(), Some("ANSI_1252"));
    assert_eq!(d.header.insunits(), Some(4));
    assert_eq!(d.header.text_style(), Some("Standard"));
    assert_eq!(d.header.dim_style(), Some("Standard"));
    let layers = &d.tables.layers;
    assert_eq!(layers.len(), 4);
    assert!(!layers.get("DOORS").unwrap().on);
    let walls = layers.get("WALLS").unwrap();
    assert_eq!(walls.lineweight, LineWeight::Hundredths(50));
    assert_eq!(
        walls.extra.len(),
        1,
        "plot style handle kept: {:?}",
        walls.extra
    );
    let hidden = layers.get("HIDDEN_REF").unwrap();
    assert!(hidden.frozen() && hidden.locked() && !hidden.plot);
    let on_walls = d.entities.iter().filter(|e| e.layer == "WALLS").count();
    assert_eq!(on_walls, 4);
    assert_eq!(parsed(&d), 6);
}

#[test]
fn ac1032_colors_and_lineweights() {
    let (d, w) = load("ac1032_colors_lineweights.dxf");
    assert!(w.is_empty(), "{w:?}");
    assert_eq!(d.header.version(), Some(Version::R2018));
    assert_eq!(d.header.extmin(), Some(Point3::new(-5.0, -5.0, 0.0)));
    assert_eq!(
        d.tables.layers.get("site").unwrap().color,
        Color::Rgb(255, 128, 0)
    );
    let e = &d.entities;
    assert_eq!(e[0].color, Color::Rgb(255, 0, 0));
    assert_eq!(e[0].lineweight, LineWeight::Hundredths(35));
    assert_eq!(e[1].color, Color::ByBlock);
    assert_eq!(e[1].linetype, "BYBLOCK");
    assert_eq!(e[1].lineweight, LineWeight::ByBlock);
    assert_eq!(e[2].color, Color::ByLayer);
    // A zero-radius circle is degenerate but valid.
    assert!(matches!(&e[3].kind, EntityKind::Circle(c) if c.radius == 0.0));
}

#[test]
fn crlf_and_padded_files_match_plain_ones() {
    let (d, w) = load("crlf_line_endings.dxf");
    assert!(w.is_empty(), "{w:?}");
    assert_eq!(kinds(&d), ["LINE", "CIRCLE", "ARC"]);
    assert_eq!(parsed(&d), 3);

    let (d, w) = load("padded_group_codes.dxf");
    assert!(w.is_empty(), "{w:?}");
    assert_eq!(d.header.insunits(), Some(1));
    let EntityKind::Circle(c) = &d.entities[1].kind else {
        panic!()
    };
    assert_eq!(c.center, Point3::new(10.0, -0.1, 0.0));
    assert_eq!(c.radius, 0.5);
}

#[test]
fn hand_written_mix_of_entities() {
    let (d, w) = load("unknown_entities.dxf");
    assert!(w.is_empty(), "{w:?}");
    // POLYLINE + 2 VERTEX + SEQEND are one entity.
    assert_eq!(
        kinds(&d),
        [
            "LWPOLYLINE",
            "TEXT",
            "POINT",
            "LINE",
            "POLYLINE",
            "ACME_CUSTOM_THING"
        ]
    );
    assert_eq!(parsed(&d), 5);
    let EntityKind::LwPolyline(poly) = &d.entities[0].kind else {
        panic!()
    };
    assert!(poly.is_closed());
    assert_eq!(poly.vertices.len(), 4);
    assert_eq!(poly.vertices[1].bulge, 0.414213562373095);
    let EntityKind::Polyline(p) = &d.entities[4].kind else {
        panic!()
    };
    assert_eq!(p.vertices.len(), 2);
    assert!(p.seqend.is_some());
    let EntityKind::Unknown(custom) = &d.entities[5].kind else {
        panic!()
    };
    assert_eq!(custom.pairs.len(), 4);
    assert_eq!(d.entities[1].layer, "ANNOT");
}

#[test]
fn all_sections_are_captured() {
    let (d, w) = load("all_sections.dxf");
    assert!(w.is_empty(), "{w:?}");
    assert_eq!(d.header.version(), Some(Version::R2004));
    assert!(!d.classes.is_empty());
    assert!(!d.blocks.is_empty());
    assert!(!d.objects.is_empty());
    assert_eq!(d.tables.other[0].name, "VPORT");
    assert_eq!(d.other_sections[0].name, "ACDSDATA");
    assert_eq!(parsed(&d), 1);
}

#[test]
fn entities_only_file_without_eof() {
    let (d, w) = load("entities_only_no_eof.dxf");
    assert_eq!(parsed(&d), 2);
    assert_eq!(d.header.version(), None);
    assert_eq!(w.len(), 1);
    assert!(w[0].message.contains("EOF"));
}

#[test]
fn arcs_and_extras() {
    let (d, w) = load("arcs_and_extras.dxf");
    assert!(w.is_empty(), "{w:?}");
    assert_eq!(parsed(&d), 5);
    let EntityKind::Arc(wrap) = &d.entities[0].kind else {
        panic!()
    };
    assert_eq!((wrap.start_angle, wrap.end_angle), (350.0, 10.0));
    let EntityKind::Arc(far) = &d.entities[2].kind else {
        panic!()
    };
    assert_eq!(far.center.x, -1_000_000.5);
    assert_eq!(far.thickness, 2.0);
    let extra_codes: Vec<i16> = d.entities[3].extra.iter().map(|p| p.code).collect();
    assert_eq!(extra_codes, [102, 330, 102, 1001, 1000, 1040, 1070]);
    let EntityKind::Arc(flipped) = &d.entities[4].kind else {
        panic!()
    };
    assert_eq!(flipped.extrusion.z, -1.0);
}

#[test]
fn empty_values() {
    let (d, w) = load("empty_values.dxf");
    assert!(w.is_empty(), "{w:?}");
    assert_eq!(d.header.text_style(), Some(""));
    assert_eq!(d.entities.len(), 2);
    assert_eq!(parsed(&d), 2);
    assert!(matches!(&d.entities[0].kind, EntityKind::Text(t) if t.value.is_empty()));
}

#[test]
fn damaged_entities_degrade_to_unknown_with_warnings() {
    let (d, w) = load("damaged_entities.dxf");
    assert_eq!(d.entities.len(), 5);
    assert_eq!(parsed(&d), 1);
    assert_eq!(w.len(), 4, "{w:?}");
    assert!(w.iter().all(|w| w.line.is_some()));
}

#[test]
fn utf8_names() {
    let (d, w) = load("utf8_names.dxf");
    assert!(w.is_empty(), "{w:?}");
    assert_eq!(d.header.current_layer(), Some("Wände"));
    assert!(d.tables.layers.contains("WÄNDE"));
    assert!(d.tables.layers.contains(&d.entities[0].layer));
    assert_eq!(d.entities[1].layer, "配管");
}

/// Truncating the small hand-written corpus files at every byte, and
/// corrupting bytes, must never panic. A cheap stand-in until cargo-fuzz
/// arrives in M2. (Real-writer files, named `<drawing>__<tool>__<version>`,
/// are too large for an exhaustive sweep.)
#[test]
fn damaged_input_never_panics() {
    let mut seed: u64 = 0x2545_F491_4F6C_DD1D;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    let hand_written = corpus_files().into_iter().filter(|p| {
        !p.file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.contains("__"))
    });
    for path in hand_written {
        let bytes = std::fs::read(&path).unwrap();
        for cut in 0..=bytes.len() {
            let _ = read_bytes(&bytes[..cut]);
        }
        for _ in 0..300 {
            let mut damaged = bytes.clone();
            for _ in 0..1 + next() % 4 {
                let at = (next() % damaged.len() as u64) as usize;
                damaged[at] = match next() % 4 {
                    0 => b'\n',
                    1 => b'-',
                    2 => b'0' + (next() % 10) as u8,
                    _ => next() as u8,
                };
            }
            let _ = read_bytes(&damaged);
        }
    }
}
