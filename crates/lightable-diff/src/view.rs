//! Scenes for showing a [`Diff`]: each entity recoloured by what happened
//! to it. Pure data, so the app only uploads and draws them.
//!
//! - [`overlay`]: B on top of A. Added green, deleted red (from A),
//!   changed yellow (B's version, with A's version in dark yellow),
//!   unchanged grey.
//! - [`side`]: one drawing on its own, coloured the same way, for the
//!   side-by-side view.
//!
//! Changes the viewer cannot draw (hatches, unknown types) get a dashed box
//! around their extent in their colour, so they are still visible.

use std::collections::HashMap;

use lightable_render::{Bounds, Item, LayerInfo, Rgb, Scene, Shape};

use crate::{ChangeKind, Diff, Place};

pub const ADDED: Rgb = Rgb(0x3c, 0xd0, 0x5a);
pub const DELETED: Rgb = Rgb(0xf0, 0x48, 0x48);
pub const CHANGED: Rgb = Rgb(0xf5, 0xd0, 0x2a);
/// A changed entity as it was.
pub const CHANGED_BEFORE: Rgb = Rgb(0x8a, 0x76, 0x1c);
pub const UNCHANGED: Rgb = Rgb(0x6a, 0x6a, 0x6a);

/// Which drawing a scene shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    /// Before.
    A,
    /// After.
    B,
}

/// The colour of each entity (model space index) on one side.
fn colors(diff: &Diff, side: Side) -> HashMap<usize, Rgb> {
    let mut out = HashMap::new();
    for c in diff.changes.iter().filter(|c| c.place == Place::Model) {
        let (r, color) = match (side, c.kind) {
            (Side::A, ChangeKind::Deleted) => (&c.old, DELETED),
            (Side::A, ChangeKind::Changed) => (&c.old, CHANGED_BEFORE),
            (Side::B, ChangeKind::Added) => (&c.new, ADDED),
            (Side::B, ChangeKind::Changed) => (&c.new, CHANGED),
            _ => continue,
        };
        if let Some(r) = r {
            out.insert(r.index, color);
        }
    }
    // References to changed blocks show the change too.
    for c in diff
        .changes
        .iter()
        .filter(|c| matches!(c.place, Place::Block(_)))
    {
        let (refs, color) = match side {
            Side::A => (&c.used_by_old, CHANGED_BEFORE),
            Side::B => (&c.used_by_new, CHANGED),
        };
        for r in refs {
            out.entry(r.index).or_insert(color);
        }
    }
    out
}

/// Dashed boxes around changes on `side` that are not drawn.
fn markers(diff: &Diff, side: Side, kinds: &[ChangeKind], layer: u32, dash: u32) -> Vec<Item> {
    let mut out = Vec::new();
    for c in diff
        .changes
        .iter()
        .filter(|c| c.place == Place::Model && !c.shown && kinds.contains(&c.kind))
    {
        let color = match (side, c.kind) {
            (Side::A, ChangeKind::Deleted) => DELETED,
            (Side::B, ChangeKind::Added) => ADDED,
            (_, ChangeKind::Changed) => CHANGED,
            _ => continue,
        };
        let Some(b) = c.bounds else { continue };
        let entity = match side {
            Side::A => c.old.as_ref(),
            Side::B => c.new.as_ref(),
        }
        .map_or(u32::MAX, |r| r.index as u32);
        out.push(Item {
            layer,
            color,
            dash: Some(dash),
            shape: Shape::Path {
                points: vec![b.min, [b.max[0], b.min[1]], b.max, [b.min[0], b.max[1]]],
                bulges: Vec::new(),
                closed: true,
            },
            entity,
        });
    }
    out
}

/// Adds the marker layer and its dash pattern (scaled to the drawing).
fn marker_layer(scene: &mut Scene, size: f64) -> (u32, u32) {
    scene.layers.push(LayerInfo {
        name: "(changes not drawn)".into(),
        color: CHANGED,
        visible: true,
    });
    let unit = (size / 200.0).max(1e-9);
    scene.dashes.push(vec![unit, -unit * 0.6]);
    (
        (scene.layers.len() - 1) as u32,
        (scene.dashes.len() - 1) as u32,
    )
}

fn size(scenes: &[&Scene]) -> f64 {
    let mut out: Option<Bounds> = None;
    for s in scenes {
        if let Some(b) = s.bounds(None) {
            match &mut out {
                Some(o) => o.union(&b),
                None => out = Some(b),
            }
        }
    }
    out.map_or(1.0, |b| b.size()[0].max(b.size()[1]))
}

/// Fills (hatches, solids) take a dim tint of the colour, so lines and
/// text over them stay readable; everything else takes the colour itself.
fn paint(item: &mut Item, color: Rgb) {
    item.color = match item.shape {
        Shape::Region { .. } | Shape::Fill { .. } => {
            let bg = lightable_render::svg::BACKGROUND;
            let mix = |c: u8, b: u8| (f64::from(c) * 0.3 + f64::from(b) * 0.7).round() as u8;
            Rgb(mix(color.0, bg.0), mix(color.1, bg.1), mix(color.2, bg.2))
        }
        _ => color,
    };
}

/// `scene` (model space of one drawing, built with
/// [`lightable_render::build`]) recoloured for `side`.
pub fn side(scene: &Scene, diff: &Diff, side: Side) -> Scene {
    let colors = colors(diff, side);
    let mut out = scene.clone();
    for item in &mut out.items {
        let color = colors
            .get(&(item.entity as usize))
            .copied()
            .unwrap_or(UNCHANGED);
        paint(item, color);
    }
    let (layer, dash) = marker_layer(&mut out, size(&[scene]));
    out.items.extend(markers(
        diff,
        side,
        &[ChangeKind::Added, ChangeKind::Deleted, ChangeKind::Changed],
        layer,
        dash,
    ));
    out
}

/// A's deleted and changed entities under all of B, recoloured. Layers are
/// merged by name, so hiding a layer hides it in both.
pub fn overlay(a: &Scene, b: &Scene, diff: &Diff) -> Scene {
    let (colors_a, colors_b) = (colors(diff, Side::A), colors(diff, Side::B));
    let mut out = Scene {
        layers: b.layers.clone(),
        dashes: b.dashes.clone(),
        items: Vec::new(),
        skipped: b.skipped.clone(),
    };
    let mut layer_map = Vec::with_capacity(a.layers.len());
    for l in &a.layers {
        let at = match out
            .layers
            .iter()
            .position(|x| lightable_dxf::names_equal(&x.name, &l.name))
        {
            Some(i) => i,
            None => {
                out.layers.push(l.clone());
                out.layers.len() - 1
            }
        };
        layer_map.push(at as u32);
    }
    let dash_base = out.dashes.len() as u32;
    out.dashes.extend(a.dashes.iter().cloned());
    // A first, so B's versions are drawn on top.
    for item in &a.items {
        if let Some(&color) = colors_a.get(&(item.entity as usize)) {
            let mut item = item.clone();
            paint(&mut item, color);
            item.layer = layer_map.get(item.layer as usize).copied().unwrap_or(0);
            item.dash = item.dash.map(|d| d + dash_base);
            out.items.push(item);
        }
    }
    for item in &b.items {
        let mut item = item.clone();
        let color = colors_b
            .get(&(item.entity as usize))
            .copied()
            .unwrap_or(UNCHANGED);
        paint(&mut item, color);
        out.items.push(item);
    }
    let (layer, dash) = marker_layer(&mut out, size(&[a, b]));
    out.items
        .extend(markers(diff, Side::A, &[ChangeKind::Deleted], layer, dash));
    out.items.extend(markers(
        diff,
        Side::B,
        &[ChangeKind::Added, ChangeKind::Changed],
        layer,
        dash,
    ));
    out
}

/// A box worth zooming to for `bounds`: at least `min_fraction` of the
/// drawing's size, so a point or a short line is shown with its
/// surroundings.
pub fn zoom_box(bounds: Bounds, drawing: Option<Bounds>, min_fraction: f64) -> Bounds {
    let size = drawing.map_or(1.0, |d| d.size()[0].max(d.size()[1]));
    let min = size * min_fraction;
    let c = bounds.center();
    let half = [
        (bounds.size()[0].max(min)) / 2.0,
        (bounds.size()[1].max(min)) / 2.0,
    ];
    Bounds {
        min: [c[0] - half[0], c[1] - half[1]],
        max: [c[0] + half[0], c[1] + half[1]],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Change, DiffOptions, EntityRef};
    use lightable_dxf::{Drawing, Entity, EntityKind, Line, Point3};

    fn line(handle: u64, y: f64) -> Entity {
        let mut e = Entity::new(EntityKind::Line(Line {
            start: Point3::new(0.0, y, 0.0),
            end: Point3::new(10.0, y, 0.0),
            thickness: 0.0,
            extrusion: lightable_dxf::Z_AXIS,
        }));
        e.handle = Some(lightable_dxf::Handle(handle));
        e
    }

    fn drawing(entities: Vec<Entity>) -> Drawing {
        Drawing {
            entities,
            ..Default::default()
        }
    }

    #[test]
    fn overlay_colours_each_kind_of_change() {
        // 1 unchanged, 2 deleted, 3 moved, 4 added.
        let a = drawing(vec![line(1, 0.0), line(2, 1.0), line(3, 2.0)]);
        let b = drawing(vec![line(1, 0.0), line(3, 2.5), line(4, 3.0)]);
        let d = crate::diff(&a, &b, &DiffOptions::default());
        let (sa, sb) = (lightable_render::build(&a), lightable_render::build(&b));
        let o = overlay(&sa, &sb, &d);
        let mut colours: Vec<Rgb> = o.items.iter().map(|i| i.color).collect();
        colours.sort_by_key(|c| (c.0, c.1, c.2));
        let mut want = vec![DELETED, CHANGED_BEFORE, UNCHANGED, CHANGED, ADDED];
        want.sort_by_key(|c| (c.0, c.1, c.2));
        assert_eq!(colours, want);
        // Side by side: A shows the deletion, B the addition.
        assert!(
            side(&sa, &d, Side::A)
                .items
                .iter()
                .any(|i| i.color == DELETED)
        );
        assert!(
            side(&sb, &d, Side::B)
                .items
                .iter()
                .any(|i| i.color == ADDED)
        );
        assert!(
            !side(&sb, &d, Side::B)
                .items
                .iter()
                .any(|i| i.color == DELETED)
        );
    }

    #[test]
    fn changes_not_drawn_get_a_box() {
        let s = Scene::default();
        let mut d = Diff::default();
        d.changes.push(Change {
            kind: ChangeKind::Added,
            place: Place::Model,
            entity_type: "HATCH".into(),
            details: vec!["not shown".into()],
            geometry: true,
            shown: false,
            certain: true,
            old: None,
            new: Some(EntityRef {
                handle: Some("A".into()),
                index: 0,
                layer: "0".into(),
            }),
            bounds: Some(Bounds {
                min: [0.0, 0.0],
                max: [2.0, 1.0],
            }),
            used_by_old: Vec::new(),
            used_by_new: Vec::new(),
        });
        let o = side(&s, &d, Side::B);
        assert_eq!(o.items.len(), 1);
        assert_eq!(o.items[0].color, ADDED);
        assert!(o.items[0].dash.is_some());
        assert!(side(&s, &d, Side::A).items.is_empty());
    }

    #[test]
    fn zoom_boxes_have_a_minimum_size() {
        let p = Bounds::point([5.0, 5.0]);
        let z = zoom_box(
            p,
            Some(Bounds {
                min: [0.0, 0.0],
                max: [100.0, 50.0],
            }),
            0.05,
        );
        assert_eq!(z.size(), [5.0, 5.0]);
        assert_eq!(z.center(), [5.0, 5.0]);
    }
}
