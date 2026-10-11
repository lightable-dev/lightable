//! Hatches are drawn as boundary plus fill. The boundaries are checked
//! against ezdxf's own flattening of the same hatches
//! (`tests/hatch/reference.json`, made by `tools/gen_hatch_fixtures.py`):
//! every point of each side must lie on the other side's loops, and the
//! filled area must match the even-odd area of ezdxf's loops.

use std::path::PathBuf;

use lightable_render::{P2, Shape};
use serde_json::Value;

fn distance_to_loops(p: P2, loops: &[Vec<P2>]) -> f64 {
    let mut best = f64::INFINITY;
    for l in loops {
        for i in 0..l.len() {
            let (a, b) = (l[i], l[(i + 1) % l.len()]);
            let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
            let len2 = dx * dx + dy * dy;
            let t = if len2 == 0.0 {
                0.0
            } else {
                (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / len2).clamp(0.0, 1.0)
            };
            best = best.min((p[0] - a[0] - t * dx).hypot(p[1] - a[1] - t * dy));
        }
    }
    best
}

fn area(loops: &[Vec<P2>]) -> f64 {
    lightable_geom::fill::even_odd_triangles(loops)
        .iter()
        .map(lightable_geom::fill::triangle_area)
        .sum()
}

#[test]
fn hatch_boundaries_and_fills_match_ezdxf() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/hatch");
    let (d, warnings) = lightable_dxf::read_file(dir.join("hatches.dxf")).unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
    let reference: Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("reference.json")).unwrap())
            .unwrap();
    let scene = lightable_render::build(&d);
    assert!(scene.skipped.is_empty(), "{:?}", scene.skipped);
    let mut checked = 0;
    for (i, e) in d.entities.iter().enumerate() {
        let handle = e.handle.unwrap().to_string();
        let want = &reference[&handle];
        let want: Vec<Vec<P2>> = want["loops"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| {
                l.as_array()
                    .unwrap()
                    .iter()
                    .map(|p| [p[0].as_f64().unwrap(), p[1].as_f64().unwrap()])
                    .collect()
            })
            .collect();
        let items: Vec<_> = scene
            .items
            .iter()
            .filter(|it| it.entity as usize == i)
            .collect();
        let regions: Vec<&Vec<Vec<P2>>> = items
            .iter()
            .filter_map(|it| match &it.shape {
                Shape::Region { loops } => Some(loops),
                _ => None,
            })
            .collect();
        assert_eq!(regions.len(), 1, "hatch {handle}: one fill");
        let got = regions[0];
        assert_eq!(got.len(), want.len(), "hatch {handle}: loop count");
        let outlines = items
            .iter()
            .filter(|it| matches!(it.shape, Shape::Path { closed: true, .. }))
            .count();
        assert_eq!(outlines, want.len(), "hatch {handle}: one outline per loop");
        // 5° arc steps: the chord is at most r (1 - cos 2.5°) ≈ 0.001 r
        // away from the arc; radii here are up to 8.
        let tol = 0.02;
        for p in got.iter().flatten() {
            let off = distance_to_loops(*p, &want);
            assert!(
                off < tol,
                "hatch {handle}: our point {p:?} is {off} off ezdxf's boundary"
            );
        }
        for p in want.iter().flatten() {
            let off = distance_to_loops(*p, got);
            assert!(
                off < tol,
                "hatch {handle}: ezdxf point {p:?} is {off} off our boundary"
            );
        }
        let (a, b) = (area(got), area(&want));
        assert!(
            (a - b).abs() <= 0.01 * b,
            "hatch {handle}: area {a} vs ezdxf {b}"
        );
        // Pattern hatches are filled dimmed, solid ones in full colour.
        let fill = items
            .iter()
            .find(|it| matches!(it.shape, Shape::Region { .. }))
            .unwrap()
            .color;
        let line = items
            .iter()
            .find(|it| matches!(it.shape, Shape::Path { .. }))
            .unwrap()
            .color;
        assert_eq!(
            fill == line,
            want_solid(&reference[&handle]),
            "hatch {handle}: fill colour"
        );
        checked += 1;
    }
    assert_eq!(checked, 8);
}

fn want_solid(v: &Value) -> bool {
    v["solid"].as_bool().unwrap()
}
