//! M3 acceptance: round-trips over the whole corpus.
//!
//! - Save-back: an unchanged drawing saves back byte-for-byte.
//! - Generated R12 and R2018 files load with no warnings, and are a
//!   fixpoint: writing what was read gives the same bytes and an equal
//!   drawing.
//! - R2018 output keeps every entity's content.

use std::path::PathBuf;

use lightable_dxf::{Drawing, Entity, EntityKind, Version, read_bytes};

/// The public corpus plus, when present, the files converted by ODA File
/// Converter or LibreDWG, which are kept out of the public repository in
/// `tests/corpus-private/converted/`.
fn corpus_files() -> Vec<PathBuf> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests");
    let mut files: Vec<PathBuf> = Vec::new();
    for dir in [root.join("corpus"), root.join("corpus-private/converted")] {
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        files.extend(
            entries
                .map(|e| e.unwrap().path())
                .filter(|p| p.extension().is_some_and(|e| e == "dxf")),
        );
    }
    files.sort_by_key(|p| p.file_name().map(|n| n.to_owned()));
    files
}

/// 63 public files; 198 with the converted ones.
fn expected_files() -> usize {
    let converted =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/corpus-private/converted");
    if converted.exists() { 198 } else { 63 }
}

fn name(path: &std::path::Path) -> String {
    path.file_name().unwrap().to_string_lossy().into_owned()
}

#[test]
fn every_corpus_file_saves_back_byte_for_byte() {
    let mut checked = 0;
    for path in corpus_files() {
        let original = std::fs::read(&path).unwrap();
        let (d, _) = read_bytes(&original).unwrap();
        let (saved, warnings) = d.save_bytes();
        assert!(saved == original, "{} changed on save-back", name(&path));
        assert!(warnings.is_empty(), "{}: {warnings:?}", name(&path));
        checked += 1;
    }
    assert!(checked >= expected_files(), "only {checked} files");
}

/// What R2018 output must keep for each entity: everything but handles,
/// owners and leftover pairs (whose order the writer normalises).
fn content(e: &Entity) -> String {
    let kind = match &e.kind {
        // Unknown entities get their handle and owner pairs rewritten.
        EntityKind::Unknown(u) => format!("Unknown({})", u.type_name),
        EntityKind::Polyline(p) => {
            let vertices: Vec<String> = p.vertices.iter().map(content).collect();
            format!(
                "Polyline({:?}, {:?}, {:?}) {vertices:?}",
                p.flags, p.location, p.extrusion
            )
        }
        EntityKind::Insert(i) => {
            let attributes: Vec<String> = i.attributes.iter().map(content).collect();
            format!(
                "Insert({}, {:?}, {:?}, {}) {attributes:?}",
                i.block, i.insert, i.scale, i.rotation
            )
        }
        kind => format!("{kind:?}"),
    };
    format!(
        "{kind} layer={} color={:?} linetype={} lw={:?} visible={} paper={}",
        e.layer, e.color, e.linetype, e.lineweight, e.visible, e.paper_space
    )
}

fn top_level(d: &Drawing) -> Vec<&Entity> {
    d.entities
        .iter()
        .filter(|e| !matches!(e.kind.type_name(), "VERTEX" | "ATTRIB" | "SEQEND"))
        .collect()
}

#[test]
fn generated_files_are_clean_and_a_fixpoint() {
    let mut problems = Vec::new();
    for path in corpus_files() {
        let (d0, _) = lightable_dxf::read_file(&path).unwrap();
        for version in [Version::R12, Version::R2018] {
            let label = format!("{} -> {version:?}", name(&path));
            let (w1, write_warnings) = d0.generate_bytes(version).unwrap();
            let (d1, warnings) = match read_bytes(&w1) {
                Ok(r) => r,
                Err(e) => {
                    problems.push(format!("{label}: written file does not load: {e}"));
                    continue;
                }
            };
            // Deliberately damaged records are written as they are, so they
            // still warn; anything else must read back cleanly.
            if !warnings.is_empty() && name(&path) != "damaged_entities.dxf" {
                problems.push(format!("{label}: reading it back warns: {warnings:?}"));
            }
            let (w2, _) = d1.generate_bytes(version).unwrap();
            if w1 != w2 {
                problems.push(format!("{label}: writing it again changes the bytes"));
            }
            let (d2, _) = read_bytes(&w2).unwrap();
            if d1 != d2 {
                problems.push(format!(
                    "{label}: reading it again gives a different drawing"
                ));
            }
            if version == Version::R2018 {
                // Entity types the writer reports as not written (with a
                // warning) are left out of the comparison.
                let dropped: Vec<String> = write_warnings
                    .iter()
                    .filter(|w| w.message.contains("not written"))
                    .filter_map(|w| w.message.split(" entit").next())
                    .filter_map(|m| m.split_whitespace().last().map(str::to_owned))
                    .collect();
                let before: Vec<String> = top_level(&d0)
                    .into_iter()
                    .filter(|e| {
                        !dropped
                            .iter()
                            .any(|t| t.eq_ignore_ascii_case(e.kind.type_name()))
                    })
                    .map(content)
                    .collect();
                let after: Vec<String> = top_level(&d1).into_iter().map(content).collect();
                if before != after {
                    let first = before.iter().zip(&after).position(|(a, b)| a != b);
                    let detail = match first {
                        Some(i) => format!("first difference:\n  {}\n  {}", before[i], after[i]),
                        None => format!("{} entities became {}", before.len(), after.len()),
                    };
                    problems.push(format!("{label}: entity content changed; {detail}"));
                }
            }
        }
    }
    assert!(
        problems.is_empty(),
        "{} problems:\n{}",
        problems.len(),
        problems.join("\n")
    );
}
