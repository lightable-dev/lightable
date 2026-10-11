//! M4b acceptance: scripted edit pairs (ezdxf) and re-saves.
//!
//! Each `tests/diff/<pair>/` holds a.dxf, b.dxf and expected.json. Every
//! expected change must be reported (same kind, type and handle, details
//! containing the expected words) and nothing else may be. Pairs marked
//! `zero_geometry_changes` (same drawing renumbered, saved as another
//! version, or re-saved by FreeCAD) must report no geometry change.

use std::path::PathBuf;

use lightable_diff::{Change, ChangeKind, Diff, DiffOptions};
use serde_json::Value;

fn pairs() -> Vec<PathBuf> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/diff");
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.is_dir())
        .collect();
    out.sort();
    out
}

fn run(dir: &std::path::Path) -> (Diff, Value) {
    let (a, _) = lightable_dxf::read_file(dir.join("a.dxf")).unwrap();
    let (b, _) = lightable_dxf::read_file(dir.join("b.dxf")).unwrap();
    let expected: Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("expected.json")).unwrap()).unwrap();
    (
        lightable_diff::diff(&a, &b, &DiffOptions::default()),
        expected,
    )
}

fn kind(s: &str) -> ChangeKind {
    match s {
        "added" => ChangeKind::Added,
        "deleted" => ChangeKind::Deleted,
        _ => ChangeKind::Changed,
    }
}

/// The handle an expected change refers to: b's for added, a's otherwise.
fn handle_of(c: &Change) -> Option<String> {
    let r = if c.kind == ChangeKind::Added {
        &c.new
    } else {
        &c.old
    };
    r.as_ref().and_then(|r| r.handle.clone())
}

fn matches(c: &Change, e: &Value) -> bool {
    let words: Vec<&str> = e["details"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w.as_str().unwrap())
        .collect();
    let text = c.details.join("; ");
    // For changes inside blocks: the references that show the block.
    let used_by_ok = e["used_by"].as_array().is_none_or(|want| {
        let want: Vec<&str> = want.iter().map(|w| w.as_str().unwrap()).collect();
        let got: Vec<&str> = c
            .used_by_new
            .iter()
            .filter_map(|r| r.handle.as_deref())
            .collect();
        want == got && c.bounds.is_some()
    });
    used_by_ok
        && c.kind == kind(e["kind"].as_str().unwrap())
        && c.entity_type == e["type"].as_str().unwrap()
        && handle_of(c).as_deref() == e["handle"].as_str()
        && words.iter().all(|w| text.contains(w))
}

#[test]
fn every_change_is_found_and_nothing_else() {
    let (mut hits, mut misses, mut extras) = (0, Vec::new(), Vec::new());
    let mut zero_pairs = 0;
    for dir in pairs() {
        let name = dir.file_name().unwrap().to_string_lossy().into_owned();
        if !dir.join("b.dxf").exists() {
            continue;
        }
        let (diff, expected) = run(&dir);
        if expected["zero_geometry_changes"].as_bool().unwrap() {
            zero_pairs += 1;
            for c in diff.changes.iter().filter(|c| c.geometry) {
                extras.push(format!("{name}: {c}"));
            }
            continue;
        }
        let wanted = expected["changes"].as_array().unwrap();
        let mut used = vec![false; diff.changes.len()];
        for e in wanted {
            match diff
                .changes
                .iter()
                .enumerate()
                .find(|(i, c)| !used[*i] && matches(c, e))
            {
                Some((i, _)) => {
                    used[i] = true;
                    hits += 1;
                }
                None => misses.push(format!(
                    "{name}: expected {e}\n   got: {}",
                    diff.changes
                        .iter()
                        .map(|c| c.to_string())
                        .collect::<Vec<_>>()
                        .join("\n        ")
                )),
            }
        }
        for (i, c) in diff.changes.iter().enumerate() {
            if !used[i] {
                extras.push(format!("{name}: {c}"));
            }
        }
    }
    let precision = hits as f64 / (hits + extras.len()).max(1) as f64;
    let recall = hits as f64 / (hits + misses.len()).max(1) as f64;
    eprintln!(
        "diff accuracy: {hits} expected changes found, {} missed, {} extra; precision {:.3}, recall {:.3}; {zero_pairs} no-change pairs",
        misses.len(),
        extras.len(),
        precision,
        recall
    );
    assert!(hits >= 25, "only {hits} expected changes");
    assert!(zero_pairs >= 3);
    assert!(
        misses.is_empty() && extras.is_empty(),
        "missed:\n{}\nextra:\n{}",
        misses.join("\n"),
        extras.join("\n")
    );
}

#[test]
fn freecad_resave_differs_only_in_representation_and_layer_names() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/diff/freecad_resave");
    let (diff, _) = run(&dir);
    // FreeCAD splits the polyline into lines and an arc: the same geometry.
    assert!(diff.equivalent > 0, "{diff:?}");
    assert_eq!(
        diff.changes.iter().filter(|c| c.geometry).count(),
        0,
        "{:#?}",
        diff.changes
    );
    // It renames every layer: reported, but not as a geometry change.
    assert!(
        diff.changes
            .iter()
            .any(|c| c.details.iter().any(|d| d.starts_with("layer changed")))
    );
}

#[test]
fn renumbered_and_reordered_drawings_report_nothing() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/diff/renumbered");
    let (diff, _) = run(&dir);
    assert!(diff.changes.is_empty(), "{:#?}", diff.changes);
    assert!(
        !diff.handles_trusted,
        "handles differ throughout, so they are not used"
    );
}

#[test]
fn json_report_is_valid_and_complete() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/diff/combined");
    let (diff, _) = run(&dir);
    let json: Value = serde_json::from_str(&diff.to_json("a.dxf", "b.dxf")).expect("valid JSON");
    assert_eq!(json["format"], "lightable-diff/1");
    assert_eq!(
        json["changes"].as_array().unwrap().len(),
        diff.changes.len()
    );
    assert_eq!(json["summary"]["added"], 1);
    let not_compared = json["not_compared"].as_array().unwrap();
    assert!(
        not_compared
            .iter()
            .any(|n| n.as_str().unwrap().starts_with("text styles"))
    );
    let first = &json["changes"][0];
    for key in [
        "kind", "place", "type", "layer", "details", "geometry", "shown", "certain",
    ] {
        assert!(!first[key].is_null(), "missing {key}");
    }
}
