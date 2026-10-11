//! The built-in examples (crates/lightable/assets, made by
//! tools/gen_example.py) say exactly what changed, in plain words.

use std::path::PathBuf;

use lightable_diff::{ChangeKind, DiffOptions};

fn diff(a: &str, b: &str) -> lightable_diff::Diff {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../lightable/assets");
    let (a, _) = lightable_dxf::read_file(dir.join(a)).unwrap();
    let (b, _) = lightable_dxf::read_file(dir.join(b)).unwrap();
    lightable_diff::diff(&a, &b, &DiffOptions::default())
}

/// The bracket's expected changes were written down from the engineering
/// change (ECN-017, bolts M12 -> M16) before the files were compared:
/// `crates/lightable/assets/bracket-rev-c-inventory.json`. Every expected
/// item must be found, and every difference found must be an expected
/// item: nothing missed, nothing unexplained.
#[test]
fn bracket_rev_c_matches_the_expected_change_inventory() {
    let d = diff("bracket-rev-b.dxf", "bracket-rev-c.dxf");
    assert_eq!(d.units.as_deref(), Some("mm"));
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../lightable/assets");
    let inventory: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(dir.join("bracket-rev-c-inventory.json")).unwrap(),
    )
    .unwrap();
    let mut used = vec![false; d.changes.len()];
    let mut check = |items: &serde_json::Value, markup: bool, many: bool| {
        for item in items.as_array().unwrap() {
            let words: Vec<&str> = item["words"]
                .as_array()
                .unwrap()
                .iter()
                .map(|w| w.as_str().unwrap())
                .collect();
            let fits = |c: &lightable_diff::Change| {
                c.markup == markup
                    && item["type"].as_str().is_none_or(|t| t == c.entity_type)
                    && item["layer"].as_str().is_none_or(|l| l == c.layer())
                    && words.iter().all(|w| c.headline.contains(w))
            };
            let found: Vec<usize> = (0..d.changes.len())
                .filter(|&i| !used[i] && fits(&d.changes[i]))
                .collect();
            assert!(!found.is_empty(), "{} not found: {item}", item["id"]);
            // Markup items without a type may be several entities (a table
            // row is lines and texts); design items are one entity each.
            let take = if many && item["type"].is_null() {
                found
            } else {
                vec![found[0]]
            };
            for i in take {
                used[i] = true;
            }
        }
    };
    check(&inventory["design"], false, false);
    check(&inventory["markup"], true, true);
    let unexplained: Vec<&str> = d
        .changes
        .iter()
        .zip(&used)
        .filter(|(_, u)| !**u)
        .map(|(c, _)| c.headline.as_str())
        .collect();
    assert!(
        unexplained.is_empty(),
        "differences not in the inventory: {unexplained:#?}"
    );
    // One engineering intention, ten design-content differences.
    assert_eq!(d.design_changes().count(), 10);
    assert_eq!(
        d.markup_summary().as_deref(),
        Some("plus revision markup (8 items, including 1 revision cloud)")
    );
    assert!(
        d.changes
            .iter()
            .all(|c| c.certain && c.shown && c.bounds.is_some())
    );
}

/// The demo says "The revision cloud marks 1 change": true for this
/// example. (A check of the demo's narration; the product does not check
/// what clouds cover.) Text boxes are drawn wide on both sides of their
/// anchor, so a change counts as clouded when its centre is inside.
#[test]
fn the_bracket_cloud_surrounds_exactly_one_design_change() {
    let d = diff("bracket-rev-b.dxf", "bracket-rev-c.dxf");
    let cloud = d
        .changes
        .iter()
        .find(|c| c.headline == "Revision cloud added")
        .unwrap()
        .bounds
        .unwrap();
    let clouded: Vec<&str> = d
        .design_changes()
        .filter(|c| {
            let b = c.bounds.unwrap();
            let (x, y) = ((b.min[0] + b.max[0]) / 2.0, (b.min[1] + b.max[1]) / 2.0);
            (cloud.min[0]..=cloud.max[0]).contains(&x) && (cloud.min[1]..=cloud.max[1]).contains(&y)
        })
        .map(|c| c.headline.as_str())
        .collect();
    assert_eq!(
        clouded,
        ["Text changed from BOLTS: 4x M12 ISO 4017 - 8.8 to BOLTS: 4x M16 ISO 4017 - 8.8"]
    );
}

/// Headlines (the first layers of the UI) never contain coordinates; those
/// are in the details.
#[test]
fn headlines_contain_no_coordinates() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut pairs: Vec<(PathBuf, PathBuf)> = vec![
        (
            root.join("crates/lightable/assets/bracket-rev-b.dxf"),
            root.join("crates/lightable/assets/bracket-rev-c.dxf"),
        ),
        (
            root.join("crates/lightable/assets/plan-rev-a.dxf"),
            root.join("crates/lightable/assets/plan-rev-b.dxf"),
        ),
    ];
    for e in std::fs::read_dir(root.join("tests/diff")).unwrap() {
        let p = e.unwrap().path();
        if p.join("b.dxf").exists() {
            pairs.push((p.join("a.dxf"), p.join("b.dxf")));
        }
    }
    let coordinate = |h: &str| {
        h.contains('(')
            && h.chars()
                .skip_while(|c| *c != '(')
                .nth(1)
                .is_some_and(|c| c.is_ascii_digit() || c == '-')
    };
    for (a, b) in pairs {
        let (a, _) = lightable_dxf::read_file(&a).unwrap();
        let (b, _) = lightable_dxf::read_file(&b).unwrap();
        for c in lightable_diff::diff(&a, &b, &DiffOptions::default()).changes {
            assert!(
                !coordinate(&c.headline),
                "coordinates in headline: {}",
                c.headline
            );
        }
    }
}

/// A chamfered corner is named without coordinates; where it is goes in
/// the details.
#[test]
fn a_chamfer_headline_names_the_change_not_the_place() {
    use lightable_dxf::{Entity, EntityKind, LwPolyline, LwVertex};
    let poly = |pts: &[(f64, f64)]| {
        let mut e = Entity::new(EntityKind::LwPolyline(LwPolyline {
            flags: 1,
            constant_width: None,
            elevation: 0.0,
            thickness: 0.0,
            extrusion: lightable_dxf::Z_AXIS,
            vertices: pts
                .iter()
                .map(|&(x, y)| LwVertex {
                    x,
                    y,
                    ..Default::default()
                })
                .collect(),
        }));
        e.handle = Some(lightable_dxf::Handle(7));
        e
    };
    let mut header = lightable_dxf::Header::default();
    header.variables.push(lightable_dxf::HeaderVariable {
        name: "$INSUNITS".into(),
        pairs: vec![lightable_dxf::Pair::new(70, "4")],
    });
    let a = lightable_dxf::Drawing {
        header: header.clone(),
        entities: vec![poly(&[
            (0.0, 0.0),
            (100.0, 0.0),
            (100.0, 50.0),
            (0.0, 50.0),
        ])],
        ..Default::default()
    };
    let b = lightable_dxf::Drawing {
        header,
        entities: vec![poly(&[
            (0.0, 0.0),
            (100.0, 0.0),
            (100.0, 40.0),
            (90.0, 50.0),
            (0.0, 50.0),
        ])],
        ..Default::default()
    };
    let d = lightable_diff::diff(&a, &b, &DiffOptions::default());
    assert_eq!(d.changes[0].headline, "Corner chamfered 10 × 10 mm");
    assert!(
        d.changes[0]
            .details
            .iter()
            .any(|x| x == "corner at (100, 50) chamfered 10 × 10 mm"),
        "{:?}",
        d.changes[0].details
    );
}

#[test]
fn plan_example_headlines_use_millimetres() {
    let d = diff("plan-rev-a.dxf", "plan-rev-b.dxf");
    let all: Vec<&str> = d.changes.iter().map(|c| c.headline.as_str()).collect();
    for want in [
        "Arc R900 moved 600 mm up",
        "Text changed from BEDROOM to MASTER BEDROOM",
        "Line deleted",
        "In block CHAIR: Polyline added",
        "Layer FURNITURE: color changed from 8 to 6",
    ] {
        assert!(all.contains(&want), "missing {want:?} in {all:#?}");
    }
    assert_eq!(d.count(ChangeKind::Deleted), 1);
}

#[test]
fn without_insunits_lengths_have_no_unit() {
    // The test pairs are in metres ($INSUNITS = 6); a drawing without
    // $INSUNITS gets bare numbers.
    let line = |y: f64| {
        let mut e =
            lightable_dxf::Entity::new(lightable_dxf::EntityKind::Line(lightable_dxf::Line {
                start: lightable_dxf::Point3::new(0.0, y, 0.0),
                end: lightable_dxf::Point3::new(10.0, y, 0.0),
                thickness: 0.0,
                extrusion: lightable_dxf::Z_AXIS,
            }));
        e.handle = Some(lightable_dxf::Handle(1));
        e
    };
    let a = lightable_dxf::Drawing {
        entities: vec![line(0.0)],
        ..Default::default()
    };
    let b = lightable_dxf::Drawing {
        entities: vec![line(2.0)],
        ..Default::default()
    };
    let d = lightable_diff::diff(&a, &b, &DiffOptions::default());
    assert_eq!(d.units, None);
    assert_eq!(d.changes[0].headline, "Line moved 2 up");
}
