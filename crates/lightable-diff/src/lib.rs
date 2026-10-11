//! Compares two drawings on their data, not their rendering.
//!
//! [`diff`] matches the entities of drawing A (before) with those of
//! drawing B (after) and reports what was added, deleted or changed, in
//! plain words ("moved by (10, 5)", "layer changed from WALLS to DOORS").
//! Entities are matched by handle when the two files share handles, and
//! otherwise (or for entities without handles) by type, layer and geometry
//! within a tolerance. Handles, `$HANDSEED`, timestamps, entity order and
//! the way numbers are written are never reported.
//!
//! Matching runs in phases, each only on what the previous ones left:
//!
//! 1. identical entities (preferring equal handles);
//! 2. if handles are trusted (most identical pairs share them): same handle
//!    and type, reported as changed;
//! 3. moved: same content shifted by one vector;
//! 4. same geometry, different properties (layer, colour, ...);
//! 5. nearby: same type and layer, overlapping extents;
//! 6. equivalent: leftovers on both sides that draw the same lines and arcs
//!    (a polyline saved as separate lines by another program). These are
//!    not changes and are only counted.
//!
//! Phases 3–6 only run when handles are not trusted, or for entities
//! without handles. What is left is added or deleted.
//!
//! The crate is UI-free; the app only displays a [`Diff`].

pub mod coverage;
pub mod features;
mod json;
pub mod view;
pub mod words;

use std::collections::{HashMap, HashSet};
use std::fmt;

use lightable_dxf::{Drawing, Entity, EntityKind};
use lightable_render::{Bounds, Scene};

use features::{Features, Fields, Tolerance, Value, describe, features};

/// Settings for [`diff`].
#[derive(Debug, Clone)]
pub struct DiffOptions {
    /// Values closer than this fraction of the drawings' size count as
    /// equal.
    pub relative_tolerance: f64,
    /// An absolute tolerance in drawing units, instead of the relative one.
    pub absolute_tolerance: Option<f64>,
}

impl Default for DiffOptions {
    fn default() -> Self {
        Self {
            relative_tolerance: 1e-6,
            absolute_tolerance: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ChangeKind {
    Added,
    Deleted,
    Changed,
}

impl ChangeKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ChangeKind::Added => "added",
            ChangeKind::Deleted => "deleted",
            ChangeKind::Changed => "changed",
        }
    }
}

/// Where a change is.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Place {
    Model,
    Paper,
    /// Inside a named block definition.
    Block(String),
    /// The layer table.
    Layers,
    /// Drawing settings (units, linetype scale).
    Header,
}

impl fmt::Display for Place {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Place::Model => f.write_str("model"),
            Place::Paper => f.write_str("paper"),
            Place::Block(name) => write!(f, "block {name}"),
            Place::Layers => f.write_str("layers"),
            Place::Header => f.write_str("settings"),
        }
    }
}

/// One side of a change.
#[derive(Debug, Clone, PartialEq)]
pub struct EntityRef {
    pub handle: Option<String>,
    /// Index in the entity list of its place (`Drawing::entities` for
    /// model and paper space, the block's entities for a block).
    pub index: usize,
    pub layer: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Change {
    pub kind: ChangeKind,
    pub place: Place,
    /// DXF type (`LINE`), or `LAYER`, `BLOCK`, `HEADER`.
    pub entity_type: String,
    /// Plain words, e.g. "moved by (10, 5)".
    pub details: Vec<String>,
    /// True if what is drawn changed (not only properties such as layer
    /// or colour).
    pub geometry: bool,
    /// False for types the viewer cannot draw yet ("changed but not shown").
    pub shown: bool,
    /// False when the change could not be fully interpreted, or the
    /// match between A and B was a guess between several candidates.
    pub certain: bool,
    /// In drawing A (deleted, changed).
    pub old: Option<EntityRef>,
    /// In drawing B (added, changed).
    pub new: Option<EntityRef>,
    /// Where to zoom to, in world coordinates (model and paper space; for
    /// a change inside a block, the block's references in model space).
    pub bounds: Option<Bounds>,
    /// For a change inside a block definition: the top-level entities of
    /// A that show the block (references to it, also through other
    /// blocks).
    pub used_by_old: Vec<EntityRef>,
    /// The same in B.
    pub used_by_new: Vec<EntityRef>,
    /// The change in one line of plain words, e.g. "Circle Ø12 moved 5 mm
    /// right" (see [`words`]).
    pub headline: String,
    /// Revision markup (a revision cloud, title block text) rather than a
    /// change to the design; see [`words::is_markup`].
    pub markup: bool,
    /// A revision cloud (a closed polyline made only of arcs) was added,
    /// deleted or changed; see [`words::is_revision_cloud`]. Always markup.
    pub revision_cloud: bool,
}

impl Change {
    pub fn layer(&self) -> &str {
        self.new
            .as_ref()
            .or(self.old.as_ref())
            .map_or("", |r| r.layer.as_str())
    }

    pub fn handle(&self) -> Option<&str> {
        let r = if self.kind == ChangeKind::Added {
            &self.new
        } else {
            &self.old
        };
        r.as_ref().and_then(|r| r.handle.as_deref())
    }
}

impl fmt::Display for Change {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.kind.as_str(), self.entity_type)?;
        if let Some(h) = self.handle() {
            write!(f, " {h}")?;
        }
        if !self.layer().is_empty() && self.place != Place::Layers {
            write!(f, " on {}", self.layer())?;
        }
        write!(f, " ({})", self.place)?;
        if !self.certain {
            f.write_str(" [uncertain]")?;
        }
        if !self.details.is_empty() {
            write!(f, ": {}", self.details.join("; "))?;
        }
        if !self.used_by_new.is_empty() {
            let handles: Vec<&str> = self
                .used_by_new
                .iter()
                .filter_map(|r| r.handle.as_deref())
                .collect();
            write!(
                f,
                " [shown by {} reference(s): {}]",
                self.used_by_new.len(),
                handles.join(", ")
            )?;
        }
        Ok(())
    }
}

/// The result of [`diff`].
#[derive(Debug, Clone, Default)]
pub struct Diff {
    pub changes: Vec<Change>,
    /// Entities matched with no change.
    pub unchanged: usize,
    /// Entities that changed representation only (phase 6), on both sides.
    pub equivalent: usize,
    /// Those entities, as deleted (from A) or added (to B) with
    /// `geometry: false`. Not changes to the drawing, but listed so nothing
    /// is silently ignored.
    pub redrawn: Vec<Change>,
    /// Whether matching by handle was used.
    pub handles_trusted: bool,
    /// The length tolerance used, in drawing units.
    pub tolerance: f64,
    /// What this diff does not compare yet, in plain words, with a hint
    /// when the two files differ there.
    pub not_compared: Vec<String>,
    /// The length unit from `$INSUNITS` (of B, else A), if it names one;
    /// headlines use it.
    pub units: Option<String>,
}

impl Diff {
    /// Changes to the design: everything but revision markup.
    pub fn design_changes(&self) -> impl Iterator<Item = &Change> {
        self.changes.iter().filter(|c| !c.markup)
    }

    /// Number of revision markup changes.
    pub fn markup_count(&self) -> usize {
        self.changes.iter().filter(|c| c.markup).count()
    }

    /// Number of revision clouds among the markup changes.
    pub fn cloud_count(&self) -> usize {
        self.changes.iter().filter(|c| c.revision_cloud).count()
    }

    /// The markup part of the summary: "plus revision markup (8 items,
    /// including 1 revision cloud)"; `None` without markup.
    pub fn markup_summary(&self) -> Option<String> {
        let m = self.markup_count();
        if m == 0 {
            return None;
        }
        let items = if m == 1 {
            "1 item".to_owned()
        } else {
            format!("{m} items")
        };
        Some(match self.cloud_count() {
            0 => format!("plus revision markup ({items})"),
            1 => format!("plus revision markup ({items}, including 1 revision cloud)"),
            k => format!("plus revision markup ({items}, including {k} revision clouds)"),
        })
    }

    pub fn count(&self, kind: ChangeKind) -> usize {
        self.changes.iter().filter(|c| c.kind == kind).count()
    }

    /// The machine-readable report (format `lightable-diff/1`).
    pub fn to_json(&self, a_name: &str, b_name: &str) -> String {
        json::report(self, a_name, b_name)
    }
}

/// An entity with what it is compared on.
struct Ent<'a> {
    index: usize,
    entity: &'a Entity,
    type_name: String,
    handle: Option<lightable_dxf::Handle>,
    /// Computed when first needed: most entities match an identical one
    /// by handle and never need it.
    features: std::cell::OnceCell<Features>,
}

impl<'a> Ent<'a> {
    fn new(index: usize, entity: &'a Entity) -> Self {
        Self {
            index,
            entity,
            type_name: entity.kind.type_name().to_uppercase(),
            handle: entity.handle,
            features: std::cell::OnceCell::new(),
        }
    }

    fn f(&self) -> &Features {
        self.features.get_or_init(|| features(self.entity))
    }

    /// Names and texts, which must be equal for most matches.
    fn names(fields: &Fields) -> String {
        let mut s = String::new();
        for (_, v) in fields {
            if let Value::Name(x) | Value::Text(x) = v {
                s.push_str(x);
                s.push('\u{1}');
            }
        }
        s
    }

    /// A hash of the type, names and texts: equal for entities that can
    /// be identical or moved copies of each other.
    fn content_key(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        self.type_name.hash(&mut h);
        for (_, v) in self.f().common.iter().chain(&self.f().shape) {
            if let Value::Name(x) | Value::Text(x) = v {
                x.hash(&mut h);
            }
        }
        h.finish()
    }

    fn first_pos(&self) -> Option<[f64; 3]> {
        self.f().shape.iter().find_map(|(_, v)| match v {
            Value::Pos(p) => Some(*p),
            _ => None,
        })
    }

    /// The box around the entity's points (and circles' radii).
    fn feature_bounds(&self) -> Option<Bounds> {
        let mut b: Option<Bounds> = None;
        let mut add = |p: [f64; 2]| match &mut b {
            Some(b) => b.add(p),
            None => b = Some(Bounds::point(p)),
        };
        let radius = self.f().shape.iter().find_map(|(l, v)| match v {
            Value::Num(r, _) if l == "radius" => Some(r.abs()),
            _ => None,
        });
        for (_, v) in &self.f().shape {
            if let Value::Pos(p) = v {
                let r = radius.unwrap_or(0.0);
                add([p[0] - r, p[1] - r]);
                add([p[0] + r, p[1] + r]);
            }
        }
        if let EntityKind::Unknown(u) = &self.entity.kind {
            let mut x: Option<(i16, f64)> = None;
            // A hatch's extent is its boundary (from group 91 to group 75):
            // its first point is the elevation, later ones seed points.
            let hatch = u.type_name.eq_ignore_ascii_case("HATCH");
            let mut in_boundary = !hatch;
            for p in &u.pairs {
                if hatch {
                    match p.code {
                        91 => in_boundary = true,
                        75 => in_boundary = false,
                        _ => {}
                    }
                }
                if !in_boundary {
                    continue;
                }
                let v = lightable_dxf::value::parse_f64(&p.raw);
                match (p.code, x, v) {
                    (10..=18, _, Some(v)) => x = Some((p.code, v)),
                    (c, Some((xc, xv)), Some(v)) if c == xc + 10 => {
                        add([xv, v]);
                        x = None;
                    }
                    _ => {}
                }
            }
        }
        b
    }
}

/// Same type and equal values.
fn identical(tol: &Tolerance, x: &Ent, y: &Ent) -> bool {
    // Equal records need no further comparison.
    if x.entity == y.entity {
        return true;
    }
    x.type_name == y.type_name
        && tol.fields_equal(&x.f().common, &y.f().common)
        && tol.fields_equal(&x.f().shape, &y.f().shape)
}

/// Matching state for one place.
struct PlaceDiff<'a> {
    place: Place,
    a: Vec<Ent<'a>>,
    b: Vec<Ent<'a>>,
    match_a: Vec<Option<usize>>,
    match_b: Vec<Option<usize>>,
    /// (a, b, certain) pairs that are not identical.
    changed: Vec<(usize, usize, bool)>,
    identical: usize,
    identical_same_handle: usize,
    equivalent_a: Vec<usize>,
    equivalent_b: Vec<usize>,
}

/// Pairs above this are not compared one by one in phases 3–5.
const PAIRWISE_LIMIT: usize = 4_000_000;

impl<'a> PlaceDiff<'a> {
    fn new(place: Place, a: &'a [Entity], b: &'a [Entity], keep: impl Fn(&Entity) -> bool) -> Self {
        let a: Vec<Ent> = a
            .iter()
            .enumerate()
            .filter(|(_, e)| keep(e))
            .map(|(i, e)| Ent::new(i, e))
            .collect();
        let b: Vec<Ent> = b
            .iter()
            .enumerate()
            .filter(|(_, e)| keep(e))
            .map(|(i, e)| Ent::new(i, e))
            .collect();
        Self {
            place,
            match_a: vec![None; a.len()],
            match_b: vec![None; b.len()],
            a,
            b,
            changed: Vec::new(),
            identical: 0,
            identical_same_handle: 0,
            equivalent_a: Vec::new(),
            equivalent_b: Vec::new(),
        }
    }

    fn pair(&mut self, i: usize, j: usize) {
        self.match_a[i] = Some(j);
        self.match_b[j] = Some(i);
    }

    fn identical(&self, tol: &Tolerance, i: usize, j: usize) -> bool {
        identical(tol, &self.a[i], &self.b[j])
    }

    /// Phase 1: identical entities, by handle first, then by content.
    fn match_identical(&mut self, tol: &Tolerance) {
        let by_handle: HashMap<lightable_dxf::Handle, usize> = self
            .b
            .iter()
            .enumerate()
            .filter_map(|(j, e)| Some((e.handle?, j)))
            .collect();
        for i in 0..self.a.len() {
            let Some(&j) = self.a[i].handle.and_then(|h| by_handle.get(&h)) else {
                continue;
            };
            if self.match_b[j].is_none() && self.identical(tol, i, j) {
                self.pair(i, j);
                self.identical += 1;
                self.identical_same_handle += 1;
            }
        }
        // Content buckets: type, names and texts, and a grid cell of the
        // first point (neighbouring cells are searched too).
        let cell = (tol.length * 1000.0).max(1e-9);
        let cell_of = |p: Option<[f64; 3]>| {
            p.map_or((0, 0), |p| {
                ((p[0] / cell).floor() as i64, (p[1] / cell).floor() as i64)
            })
        };
        let mut buckets: HashMap<(u64, i64, i64), Vec<usize>> = HashMap::new();
        for (j, e) in self.b.iter().enumerate() {
            if self.match_b[j].is_none() {
                let (cx, cy) = cell_of(e.first_pos());
                buckets
                    .entry((e.content_key(), cx, cy))
                    .or_default()
                    .push(j);
            }
        }
        for i in 0..self.a.len() {
            if self.match_a[i].is_some() {
                continue;
            }
            let k = self.a[i].content_key();
            let (cx, cy) = cell_of(self.a[i].first_pos());
            let mut found = None;
            'search: for dx in -1..=1 {
                for dy in -1..=1 {
                    let Some(list) = buckets.get_mut(&(k, cx + dx, cy + dy)) else {
                        continue;
                    };
                    if let Some(at) = list.iter().position(|&j| {
                        self.b[j].entity == self.a[i].entity
                            || identical(tol, &self.a[i], &self.b[j])
                    }) {
                        // Matched entities leave the bucket, so many
                        // duplicates do not make this quadratic.
                        found = Some(list.swap_remove(at));
                        break 'search;
                    }
                }
            }
            if let Some(j) = found {
                self.pair(i, j);
                self.identical += 1;
                if self.a[i].handle.is_some() && self.a[i].handle == self.b[j].handle {
                    self.identical_same_handle += 1;
                }
            }
        }
    }

    /// Unmatched entities of A and B that phases 3–6 may use.
    fn open(&self, trusted: bool) -> (Vec<usize>, Vec<usize>) {
        let a = (0..self.a.len())
            .filter(|&i| self.match_a[i].is_none() && (!trusted || self.a[i].handle.is_none()))
            .collect();
        let b = (0..self.b.len())
            .filter(|&j| self.match_b[j].is_none() && (!trusted || self.b[j].handle.is_none()))
            .collect();
        (a, b)
    }

    /// Candidate pairs among open entities with the same `key`.
    fn candidates<K: std::hash::Hash + Eq>(
        &self,
        trusted: bool,
        key: impl Fn(&Ent) -> K,
        mut test: impl FnMut(usize, usize) -> bool,
    ) -> Vec<(usize, usize)> {
        let (oa, ob) = self.open(trusted);
        let mut groups: HashMap<K, Vec<usize>> = HashMap::new();
        for &j in &ob {
            groups.entry(key(&self.b[j])).or_default().push(j);
        }
        let mut work = 0usize;
        let mut out = Vec::new();
        for &i in &oa {
            if let Some(list) = groups.get(&key(&self.a[i])) {
                work += list.len();
                if work > PAIRWISE_LIMIT {
                    return out;
                }
                out.extend(list.iter().filter(|&&j| test(i, j)).map(|&j| (i, j)));
            }
        }
        out
    }

    /// Pairs greedily in the given order; `certain` is false when either
    /// side had more than one candidate.
    fn take(&mut self, ranked: Vec<(usize, usize)>) {
        let mut count_a: HashMap<usize, usize> = HashMap::new();
        let mut count_b: HashMap<usize, usize> = HashMap::new();
        for &(i, j) in &ranked {
            *count_a.entry(i).or_default() += 1;
            *count_b.entry(j).or_default() += 1;
        }
        for (i, j) in ranked {
            if self.match_a[i].is_none() && self.match_b[j].is_none() {
                self.pair(i, j);
                self.changed
                    .push((i, j, count_a[&i] == 1 && count_b[&j] == 1));
            }
        }
    }

    /// Phases 2–6.
    fn match_rest(&mut self, tol: &Tolerance, trusted: bool, scenes: Option<(&Scene, &Scene)>) {
        if trusted {
            let by_handle: HashMap<lightable_dxf::Handle, usize> = self
                .b
                .iter()
                .enumerate()
                .filter_map(|(j, e)| Some((e.handle?, j)))
                .collect();
            for i in 0..self.a.len() {
                let Some(&j) = self.a[i].handle.and_then(|h| by_handle.get(&h)) else {
                    continue;
                };
                if self.match_a[i].is_none()
                    && self.match_b[j].is_none()
                    && self.a[i].type_name == self.b[j].type_name
                {
                    self.pair(i, j);
                    self.changed.push((i, j, true));
                }
            }
        }
        // Phase 3: moved by one vector.
        let names = |e: &Ent| e.content_key();
        let mut moves: Vec<(usize, usize, [f64; 3])> = Vec::new();
        let pairs = self.candidates(trusted, names, |i, j| {
            if !tol.fields_equal(&self.a[i].f().common, &self.b[j].f().common) {
                return false;
            }
            match tol.moved_by(&self.a[i].f().shape, &self.b[j].f().shape) {
                Some(d) => {
                    moves.push((i, j, d));
                    true
                }
                None => false,
            }
        });
        drop(pairs);
        // Prefer the most common vector (a group moved together), then the
        // shortest.
        let grid = (tol.length * 1000.0).max(1e-9);
        let round = |d: [f64; 3]| {
            (
                (d[0] / grid).round() as i64,
                (d[1] / grid).round() as i64,
                (d[2] / grid).round() as i64,
            )
        };
        let mut freq: HashMap<(i64, i64, i64), usize> = HashMap::new();
        for m in &moves {
            *freq.entry(round(m.2)).or_default() += 1;
        }
        moves.sort_by(|x, y| {
            freq[&round(y.2)]
                .cmp(&freq[&round(x.2)])
                .then((x.2[0].hypot(x.2[1])).total_cmp(&y.2[0].hypot(y.2[1])))
        });
        self.take(moves.into_iter().map(|(i, j, _)| (i, j)).collect());

        // Phase 4: same geometry, other properties.
        let shape_key = |e: &Ent| (e.type_name.clone(), Ent::names(&e.f().shape));
        let same_shape = self.candidates(trusted, shape_key, |i, j| {
            tol.fields_equal(&self.a[i].f().shape, &self.b[j].f().shape)
        });
        self.take(same_shape);

        // Phase 5: nearby, same type and layer, ranked by equal values.
        let layer_key = |e: &Ent| (e.type_name.clone(), e.entity.layer.clone());
        let (oa, ob) = self.open(trusted);
        let mut bounds_a: Vec<Option<Bounds>> = vec![None; self.a.len()];
        let mut bounds_b: Vec<Option<Bounds>> = vec![None; self.b.len()];
        for &i in &oa {
            bounds_a[i] = self.a[i].feature_bounds();
        }
        for &j in &ob {
            bounds_b[j] = self.b[j].feature_bounds();
        }
        let overlaps = |i: usize, j: usize| match (bounds_a[i], bounds_b[j]) {
            (Some(x), Some(y)) => {
                let grow = |b: Bounds| Bounds {
                    min: [b.min[0] - tol.length, b.min[1] - tol.length],
                    max: [b.max[0] + tol.length, b.max[1] + tol.length],
                };
                grow(x).intersects(&grow(y))
            }
            _ => false,
        };
        let near = self.candidates(trusted, layer_key, overlaps);
        let score = |i: usize, j: usize| {
            let (x, y) = (&self.a[i].f().shape, &self.b[j].f().shape);
            x.iter()
                .filter(|(l, v)| y.iter().any(|(m, w)| l == m && tol.value_equal(v, w)))
                .count()
        };
        let mut near: Vec<(usize, usize, usize)> =
            near.into_iter().map(|(i, j)| (i, j, score(i, j))).collect();
        near.sort_by_key(|x| std::cmp::Reverse(x.2));
        self.take(near.into_iter().map(|(i, j, _)| (i, j)).collect());

        // Phase 6: the leftovers draw the same thing.
        if let Some((sa, sb)) = scenes {
            self.match_equivalent(tol, sa, sb);
        }
    }

    fn match_equivalent(&mut self, tol: &Tolerance, sa: &Scene, sb: &Scene) {
        use coverage::{Index, Piece, pieces};
        fn group(scene: &Scene) -> HashMap<u32, Vec<&lightable_render::Item>> {
            let mut m: HashMap<u32, Vec<&lightable_render::Item>> = HashMap::new();
            for item in &scene.items {
                m.entry(item.entity).or_default().push(item);
            }
            m
        }
        if self.match_a.iter().all(Option::is_some) || self.match_b.iter().all(Option::is_some) {
            return;
        }
        let (ga, gb) = (group(sa), group(sb));
        let left = |m: &[Option<usize>],
                    ents: &[Ent],
                    g: &HashMap<u32, Vec<&lightable_render::Item>>|
         -> Vec<(usize, Vec<Piece>)> {
            (0..ents.len())
                .filter(|&i| m[i].is_none())
                .map(|i| {
                    (
                        i,
                        g.get(&(ents[i].index as u32))
                            .map(|v| pieces(v))
                            .unwrap_or_default(),
                    )
                })
                .filter(|(_, p)| !p.is_empty())
                .collect()
        };
        let mut la = left(&self.match_a, &self.a, &ga);
        let mut lb = left(&self.match_b, &self.b, &gb);
        let reach = tol.length * 10.0;
        let cell = (reach * 1000.0).max(1e-9);
        // Keep only what the other side covers, until nothing changes.
        loop {
            let ia = Index::new(
                la.iter().flat_map(|(_, p)| p.iter().copied()).collect(),
                cell,
            );
            let ib = Index::new(
                lb.iter().flat_map(|(_, p)| p.iter().copied()).collect(),
                cell,
            );
            let (na, nb) = (la.len(), lb.len());
            la.retain(|(_, p)| ib.covers(p, reach));
            lb.retain(|(_, p)| ia.covers(p, reach));
            if la.len() == na && lb.len() == nb {
                break;
            }
        }
        self.equivalent_a = la.into_iter().map(|(i, _)| i).collect();
        self.equivalent_b = lb.into_iter().map(|(j, _)| j).collect();
    }

    fn changes(&self, tol: &Tolerance, scenes: Option<(&Scene, &Scene)>, out: &mut Diff) {
        // What each entity draws: its box, once per scene.
        fn drawn(scene: Option<&Scene>) -> HashMap<usize, Option<Bounds>> {
            let mut m: HashMap<usize, Option<Bounds>> = HashMap::new();
            for item in scene.map_or(&[][..], |s| &s.items[..]) {
                let slot = m.entry(item.entity as usize).or_default();
                if let Some(ib) = item.shape.bounds() {
                    match slot {
                        Some(b) => b.union(&ib),
                        None => *slot = Some(ib),
                    }
                }
            }
            m
        }
        let (da, db) = (drawn(scenes.map(|s| s.0)), drawn(scenes.map(|s| s.1)));
        let zoomable = matches!(self.place, Place::Model | Place::Paper);
        let bounds_of = |m: &HashMap<usize, Option<Bounds>>, e: &Ent| -> Option<Bounds> {
            if !zoomable {
                return None;
            }
            m.get(&e.index)
                .copied()
                .flatten()
                .or_else(|| e.feature_bounds())
        };
        let shown = |m: &HashMap<usize, Option<Bounds>>, e: &Ent| {
            !matches!(e.entity.kind, EntityKind::Unknown(_)) || m.contains_key(&e.index)
        };
        let r = |e: &Ent| EntityRef {
            handle: e.handle.map(|h| h.to_string()),
            index: e.index,
            layer: e.entity.layer.clone(),
        };
        let union = |x: Option<Bounds>, y: Option<Bounds>| match (x, y) {
            (Some(mut x), Some(y)) => {
                x.union(&y);
                Some(x)
            }
            (x, y) => x.or(y),
        };
        out.unchanged += self.identical;
        out.equivalent += self.equivalent_a.len() + self.equivalent_b.len();
        let eq_a: HashSet<usize> = self.equivalent_a.iter().copied().collect();
        let eq_b: HashSet<usize> = self.equivalent_b.iter().copied().collect();
        for &(i, j, certain) in &self.changed {
            let (x, y) = (&self.a[i], &self.b[j]);
            let is_shown = shown(&da, x) || shown(&db, y);
            let mut details = Vec::new();
            describe(tol, &x.f().common, &y.f().common, &mut details);
            describe(tol, &x.f().shape, &y.f().shape, &mut details);
            if details.is_empty() {
                out.unchanged += 1;
                continue;
            }
            let interpreted = x.f().interpreted && y.f().interpreted;
            if !interpreted {
                details.push("content not interpreted, compared as raw data".into());
            }
            if !is_shown {
                details.insert(0, "not shown".into());
            } else if y.type_name == "HATCH" && details.iter().any(|d| d.starts_with("pattern")) {
                // The fill is drawn, the pattern itself is not.
                details.push("patterns are not drawn yet".into());
            }
            let words = words::changed(
                tol,
                x.entity,
                &x.f().common,
                &x.f().shape,
                y.entity,
                &y.f().common,
                &y.f().shape,
                out.units.as_deref(),
            );
            details.extend(words.details);
            let headline = words.headline;
            let markup = words::is_markup(x.entity) || words::is_markup(y.entity);
            out.changes.push(Change {
                kind: ChangeKind::Changed,
                place: self.place.clone(),
                entity_type: y.type_name.clone(),
                details,
                geometry: !tol.fields_equal(&x.f().shape, &y.f().shape),
                shown: is_shown,
                certain: certain && interpreted,
                old: Some(r(x)),
                new: Some(r(y)),
                bounds: union(bounds_of(&da, x), bounds_of(&db, y)),
                used_by_old: Vec::new(),
                used_by_new: Vec::new(),
                headline,
                markup,
                revision_cloud: words::is_revision_cloud(x.entity)
                    || words::is_revision_cloud(y.entity),
            });
        }
        let single =
            |kind: ChangeKind, e: &Ent, m: &HashMap<usize, Option<Bounds>>, redrawn: bool| {
                let is_shown = shown(m, e);
                let mut details = Vec::new();
                if !is_shown {
                    details.push("not shown".into());
                }
                if redrawn {
                    details
                        .push("drawn the same by other entities (representation changed)".into());
                } else if !e.f().interpreted {
                    details.push("content not interpreted".into());
                }
                let (old, new) = if kind == ChangeKind::Added {
                    (None, Some(r(e)))
                } else {
                    (Some(r(e)), None)
                };
                Change {
                    kind,
                    place: self.place.clone(),
                    entity_type: e.type_name.clone(),
                    details,
                    geometry: !redrawn,
                    shown: is_shown,
                    certain: e.f().interpreted,
                    old,
                    new,
                    bounds: bounds_of(m, e),
                    used_by_old: Vec::new(),
                    used_by_new: Vec::new(),
                    headline: words::added_or_deleted(e.entity, kind == ChangeKind::Added),
                    markup: words::is_markup(e.entity),
                    revision_cloud: words::is_revision_cloud(e.entity),
                }
            };
        for (i, e) in self.a.iter().enumerate() {
            if self.match_a[i].is_none() {
                let redrawn = eq_a.contains(&i);
                let c = single(ChangeKind::Deleted, e, &da, redrawn);
                if redrawn {
                    out.redrawn.push(c)
                } else {
                    out.changes.push(c)
                }
            }
        }
        for (j, e) in self.b.iter().enumerate() {
            if self.match_b[j].is_none() {
                let redrawn = eq_b.contains(&j);
                let c = single(ChangeKind::Added, e, &db, redrawn);
                if redrawn {
                    out.redrawn.push(c)
                } else {
                    out.changes.push(c)
                }
            }
        }
    }
}

/// Block definitions worth comparing: named ones (anonymous blocks such as
/// dimension pictures `*D1` are regenerated by every program, and model or
/// paper space are compared as places).
fn compared_block(name: &str) -> bool {
    !name.starts_with('*') && !name.starts_with('$')
}

fn layer_fields(l: &lightable_dxf::Layer) -> Fields {
    let n = |label: &'static str, v: String| (label.into(), Value::Name(v));
    let yes = |b: bool| if b { "yes".to_owned() } else { "no".to_owned() };
    vec![
        n("color", features::color_name(l.color)),
        n("linetype", l.linetype.to_uppercase()),
        n("lineweight", format!("{:?}", l.lineweight)),
        n("on", yes(l.on)),
        n("frozen", yes(l.frozen())),
        n("locked", yes(l.locked())),
        n("plot", yes(l.plot)),
    ]
}

fn header_value(d: &Drawing, name: &str) -> String {
    d.header
        .get(name)
        .map(|v| {
            v.pairs
                .iter()
                .map(|p| p.raw.trim().to_owned())
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_default()
}

/// Compares drawing `a` (before) with drawing `b` (after).
pub fn diff(a: &Drawing, b: &Drawing, options: &DiffOptions) -> Diff {
    let (scene_a, scene_b) = (
        lightable_render::build_with(a, true),
        lightable_render::build_with(b, true),
    );
    let size = [scene_a.bounds(None), scene_b.bounds(None)]
        .into_iter()
        .flatten()
        .map(|b| b.size()[0].hypot(b.size()[1]))
        .fold(0.0, f64::max);
    let length = options
        .absolute_tolerance
        .unwrap_or((options.relative_tolerance * size).max(1e-12));
    let tol = Tolerance { length };

    let mut places = vec![
        PlaceDiff::new(Place::Model, &a.entities, &b.entities, |e| !e.paper_space),
        PlaceDiff::new(Place::Paper, &a.entities, &b.entities, |e| e.paper_space),
    ];
    let mut names: Vec<&str> = a
        .blocks
        .iter()
        .chain(b.blocks.iter())
        .map(|bl| bl.name.as_str())
        .filter(|n| compared_block(n))
        .collect();
    names.sort_by_key(|n| n.to_uppercase());
    names.dedup_by(|x, y| x.eq_ignore_ascii_case(y));
    let mut out = Diff {
        tolerance: length,
        units: words::units_of(b).or(words::units_of(a)).map(str::to_owned),
        ..Diff::default()
    };
    let mut block_scenes = Vec::new();
    for name in &names {
        match (a.blocks.get(name), b.blocks.get(name)) {
            (Some(x), Some(y)) => {
                places.push(PlaceDiff::new(
                    Place::Block(x.name.clone()),
                    &x.entities,
                    &y.entities,
                    |_| true,
                ));
                block_scenes.push((
                    lightable_render::build_block(a, name),
                    lightable_render::build_block(b, name),
                ));
            }
            (x, y) => {
                let kind = if x.is_some() {
                    ChangeKind::Deleted
                } else {
                    ChangeKind::Added
                };
                let bl = x.or(y).expect("in one of the drawings");
                out.changes.push(Change {
                    kind,
                    place: Place::Block(bl.name.clone()),
                    entity_type: "BLOCK".into(),
                    details: vec![format!("block {} {}", bl.name, kind.as_str())],
                    geometry: false,
                    shown: true,
                    certain: true,
                    old: None,
                    new: None,
                    bounds: None,
                    used_by_old: Vec::new(),
                    used_by_new: Vec::new(),
                    headline: String::new(),
                    markup: false,
                    revision_cloud: false,
                });
            }
        }
    }
    for p in &mut places {
        p.match_identical(&tol);
    }
    let identical: usize = places.iter().map(|p| p.identical).sum();
    let same: usize = places.iter().map(|p| p.identical_same_handle).sum();
    out.handles_trusted = identical > 0 && same * 2 >= identical;
    for (k, p) in places.iter_mut().enumerate() {
        let scenes = match k {
            0 | 1 => Some((&scene_a, &scene_b)),
            _ => match &block_scenes[k - 2] {
                (Some(x), Some(y)) => Some((x, y)),
                _ => None,
            },
        };
        p.match_rest(&tol, out.handles_trusted, scenes);
    }
    for (k, p) in places.iter().enumerate() {
        let scenes = match k {
            0 | 1 => Some((&scene_a, &scene_b)),
            _ => match &block_scenes[k - 2] {
                (Some(x), Some(y)) => Some((x, y)),
                _ => None,
            },
        };
        p.changes(&tol, scenes, &mut out);
    }
    link_block_uses(a, b, &scene_a, &scene_b, &mut out.changes);
    out.not_compared = not_compared(a, b);
    // Layers.
    for la in a.tables.layers.iter() {
        match b.tables.layers.get(&la.name) {
            None => out
                .changes
                .push(table_change(ChangeKind::Deleted, &la.name, Vec::new())),
            Some(lb) => {
                let mut details = Vec::new();
                describe(&tol, &layer_fields(la), &layer_fields(lb), &mut details);
                if !details.is_empty() {
                    out.changes
                        .push(table_change(ChangeKind::Changed, &la.name, details));
                }
            }
        }
    }
    for lb in b.tables.layers.iter() {
        if !a.tables.layers.contains(&lb.name) {
            out.changes
                .push(table_change(ChangeKind::Added, &lb.name, Vec::new()));
        }
    }
    // Settings that change what the numbers mean. Everything else in the
    // header ($HANDSEED, timestamps, current layer, ...) is not a change to
    // the drawing.
    for (var, label) in [("$INSUNITS", "units"), ("$LTSCALE", "linetype scale")] {
        let (x, y) = (header_value(a, var), header_value(b, var));
        if x != y && !(var == "$LTSCALE" && x.parse::<f64>().ok() == y.parse::<f64>().ok()) {
            let show = |s: &str| {
                if s.is_empty() {
                    "(not set)".to_owned()
                } else {
                    s.to_owned()
                }
            };
            out.changes.push(Change {
                kind: ChangeKind::Changed,
                place: Place::Header,
                entity_type: "HEADER".into(),
                details: vec![format!(
                    "{label} ({var}) changed from {} to {}",
                    show(&x),
                    show(&y)
                )],
                geometry: false,
                shown: true,
                certain: true,
                old: None,
                new: None,
                bounds: None,
                used_by_old: Vec::new(),
                used_by_new: Vec::new(),
                headline: String::new(),
                markup: false,
                revision_cloud: false,
            });
        }
    }
    for c in out.changes.iter_mut() {
        if c.headline.is_empty() {
            c.headline = match &c.place {
                Place::Layers => format!(
                    "Layer {}: {}",
                    c.layer(),
                    c.details.first().map_or("changed", String::as_str)
                ),
                _ => words::sentence(c.details.first().map_or("changed", String::as_str)),
            };
            if c.kind != ChangeKind::Changed && c.place == Place::Layers {
                c.headline = words::sentence(&c.details.join("; "));
            }
        }
        if let Place::Block(name) = &c.place
            && c.entity_type != "BLOCK"
        {
            c.headline = format!("In block {name}: {}", c.headline);
        }
    }

    // Design changes first, revision markup after (stable otherwise).
    out.changes.sort_by_key(|c| c.markup);
    out
}

/// What is not compared yet. Each entry says whether the files differ
/// there (definitions compared without handles), so a skipped difference
/// is pointed out rather than hidden.
fn not_compared(a: &Drawing, b: &Drawing) -> Vec<String> {
    fn hint<T: Clone + PartialEq>(x: &[T], y: &[T], strip: impl Fn(&mut T)) -> String {
        let clean = |v: &[T]| -> Vec<T> {
            v.iter()
                .cloned()
                .map(|mut t| {
                    strip(&mut t);
                    t
                })
                .collect()
        };
        if x.len() != y.len() {
            format!("A has {}, B has {}", x.len(), y.len())
        } else if clean(x) == clean(y) {
            "the same in both files".into()
        } else {
            "they differ between the files".into()
        }
    }
    let (ta, tb) = (&a.tables, &b.tables);
    fn collect<T: lightable_dxf::Named + Clone>(t: &lightable_dxf::Table<T>) -> Vec<T> {
        t.iter().cloned().collect()
    }
    let mut out = vec![
        format!(
            "text styles ({})",
            hint(
                &collect(&ta.text_styles),
                &collect(&tb.text_styles),
                |s: &mut lightable_dxf::TextStyle| {
                    s.handle = None;
                    s.owner = None;
                    s.extra.clear();
                }
            )
        ),
        format!(
            "dimension styles ({})",
            hint(
                &collect(&ta.dim_styles),
                &collect(&tb.dim_styles),
                |s: &mut lightable_dxf::DimStyle| {
                    s.handle = None;
                    s.owner = None;
                    s.extra.clear();
                    s.variables
                        .retain(|p| !matches!(p.code, 5 | 105 | 330 | 340..=349));
                }
            )
        ),
        format!(
            "linetype definitions ({}); the linetype names entities use are compared",
            hint(
                &collect(&ta.linetypes),
                &collect(&tb.linetypes),
                |s: &mut lightable_dxf::Linetype| {
                    s.handle = None;
                    s.owner = None;
                    s.extra.clear();
                    s.elements.iter_mut().for_each(|e| e.style = None);
                }
            )
        ),
    ];
    let others: Vec<&str> = ta
        .other
        .iter()
        .chain(&tb.other)
        .map(|t| t.name.as_str())
        .collect();
    let mut others: Vec<String> = others.iter().map(|n| n.trim().to_uppercase()).collect();
    others.sort();
    others.dedup();
    if !others.is_empty() {
        out.push(format!("other tables: {}", others.join(", ")));
    }
    out.push("drawing settings other than units ($INSUNITS) and linetype scale ($LTSCALE)".into());
    out.push("objects (layouts and page setups, dictionaries, groups, ...)".into());
    out.push(
        "anonymous blocks (dimension pictures, ...); the dimensions themselves are compared".into(),
    );
    if !a.other_sections.is_empty() || !b.other_sections.is_empty() {
        out.push("other sections (thumbnail image, ...)".into());
    }
    out
}

/// The top-level entities that show block `name`: references to it, and
/// references to blocks that contain it (at any depth).
pub fn block_users(d: &Drawing, name: &str) -> Vec<usize> {
    let refers_to = |e: &Entity, set: &HashSet<String>| match &e.kind {
        EntityKind::Insert(i) => set.contains(&i.block.to_uppercase()),
        _ => false,
    };
    let mut set: HashSet<String> = HashSet::from([name.to_uppercase()]);
    loop {
        let before = set.len();
        for bl in d.blocks.iter() {
            if !set.contains(&bl.name.to_uppercase())
                && bl.entities.iter().any(|e| refers_to(e, &set))
            {
                set.insert(bl.name.to_uppercase());
            }
        }
        if set.len() == before {
            break;
        }
    }
    (0..d.entities.len())
        .filter(|&i| refers_to(&d.entities[i], &set))
        .collect()
}

/// Fills `used_by_old`/`used_by_new` of changes inside blocks, and makes
/// them zoomable to the block's references in model space.
fn link_block_uses(
    a: &Drawing,
    b: &Drawing,
    scene_a: &Scene,
    scene_b: &Scene,
    changes: &mut [Change],
) {
    /// References in A, in B, and their extent.
    type Uses = (Vec<EntityRef>, Vec<EntityRef>, Option<Bounds>);
    let mut cache: HashMap<String, Uses> = HashMap::new();
    for c in changes.iter_mut() {
        let Place::Block(name) = &c.place else {
            continue;
        };
        let entry = cache.entry(name.to_uppercase()).or_insert_with(|| {
            let mut bounds: Option<Bounds> = None;
            let mut refs = |d: &Drawing, scene: &Scene| -> Vec<EntityRef> {
                let users = block_users(d, name);
                let set: HashSet<usize> = users.iter().copied().collect();
                for item in &scene.items {
                    let i = item.entity as usize;
                    if set.contains(&i)
                        && !d.entities[i].paper_space
                        && let Some(ib) = item.shape.bounds()
                    {
                        match &mut bounds {
                            Some(b) => b.union(&ib),
                            None => bounds = Some(ib),
                        }
                    }
                }
                users
                    .into_iter()
                    .map(|i| EntityRef {
                        handle: d.entities[i].handle.map(|h| h.to_string()),
                        index: i,
                        layer: d.entities[i].layer.clone(),
                    })
                    .collect()
            };
            let old = refs(a, scene_a);
            let new = refs(b, scene_b);
            (old, new, bounds)
        });
        c.used_by_old = entry.0.clone();
        c.used_by_new = entry.1.clone();
        if c.bounds.is_none() {
            c.bounds = entry.2;
        }
    }
}

fn table_change(kind: ChangeKind, name: &str, mut details: Vec<String>) -> Change {
    if details.is_empty() {
        details.push(format!("layer {name} {}", kind.as_str()));
    }
    let r = EntityRef {
        handle: None,
        index: 0,
        layer: name.to_owned(),
    };
    Change {
        kind,
        place: Place::Layers,
        entity_type: "LAYER".into(),
        details,
        geometry: false,
        shown: true,
        certain: true,
        old: (kind != ChangeKind::Added).then(|| r.clone()),
        new: (kind != ChangeKind::Deleted).then_some(r),
        bounds: None,
        used_by_old: Vec::new(),
        used_by_new: Vec::new(),
        headline: String::new(),
        markup: false,
        revision_cloud: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_markup_summary_counts_items_and_clouds() {
        let change = |markup: bool, cloud: bool| Change {
            markup,
            revision_cloud: cloud,
            ..table_change(ChangeKind::Added, "X", Vec::new())
        };
        let summary = |c: Vec<Change>| {
            Diff {
                changes: c,
                ..Default::default()
            }
            .markup_summary()
        };
        assert_eq!(summary(vec![change(false, false)]), None);
        assert_eq!(
            summary(vec![change(true, false)]).as_deref(),
            Some("plus revision markup (1 item)")
        );
        assert_eq!(
            summary(vec![change(true, true), change(true, false)]).as_deref(),
            Some("plus revision markup (2 items, including 1 revision cloud)")
        );
        assert_eq!(
            summary(vec![
                change(true, true),
                change(true, true),
                change(false, false)
            ])
            .as_deref(),
            Some("plus revision markup (2 items, including 2 revision clouds)")
        );
    }
    use lightable_dxf::{Circle, Line, Point3};

    fn line(handle: u64, layer: &str, a: (f64, f64), b: (f64, f64)) -> Entity {
        let mut e = Entity::new(EntityKind::Line(Line {
            start: Point3::new(a.0, a.1, 0.0),
            end: Point3::new(b.0, b.1, 0.0),
            thickness: 0.0,
            extrusion: lightable_dxf::Z_AXIS,
        }));
        e.handle = Some(lightable_dxf::Handle(handle));
        e.layer = layer.into();
        e
    }

    fn circle(handle: u64, c: (f64, f64), r: f64) -> Entity {
        let mut e = Entity::new(EntityKind::Circle(Circle {
            center: Point3::new(c.0, c.1, 0.0),
            radius: r,
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
    fn same_drawing_has_no_changes() {
        let a = drawing(vec![
            line(1, "0", (0.0, 0.0), (10.0, 0.0)),
            circle(2, (5.0, 5.0), 1.0),
        ]);
        let d = diff(&a, &a.clone(), &DiffOptions::default());
        assert!(d.changes.is_empty());
        assert_eq!(d.unchanged, 2);
        assert!(d.handles_trusted);
    }

    #[test]
    fn without_handles_moves_and_layer_changes_are_found_by_content() {
        let mut a = drawing(vec![
            line(1, "A", (0.0, 0.0), (10.0, 0.0)),
            circle(2, (5.0, 5.0), 1.0),
            circle(3, (50.0, 5.0), 2.0),
        ]);
        let mut b = drawing(vec![
            circle(9, (5.0, 6.0), 1.0),
            line(8, "B", (0.0, 0.0), (10.0, 0.0)),
            circle(7, (50.0, 5.0), 2.0),
        ]);
        for e in a.entities.iter_mut().chain(b.entities.iter_mut()) {
            e.handle = None;
        }
        let d = diff(&a, &b, &DiffOptions::default());
        let words: Vec<String> = d.changes.iter().map(|c| c.to_string()).collect();
        assert_eq!(d.changes.len(), 2, "{words:#?}");
        assert!(words.iter().any(|w| w.contains("moved by (0, 1)")));
        assert!(
            words
                .iter()
                .any(|w| w.contains("layer changed from A to B"))
        );
        assert!(d.changes.iter().all(|c| c.kind == ChangeKind::Changed));
    }

    #[test]
    fn a_polyline_split_into_lines_is_equivalent() {
        let mut p = Entity::new(EntityKind::LwPolyline(lightable_dxf::LwPolyline {
            vertices: [(0.0, 0.0), (10.0, 0.0), (10.0, 5.0)]
                .iter()
                .map(|&(x, y)| lightable_dxf::LwVertex {
                    x,
                    y,
                    ..Default::default()
                })
                .collect(),
            flags: 0,
            constant_width: None,
            elevation: 0.0,
            thickness: 0.0,
            extrusion: lightable_dxf::Z_AXIS,
        }));
        p.handle = Some(lightable_dxf::Handle(1));
        let a = drawing(vec![p]);
        let b = drawing(vec![
            line(5, "0", (10.0, 5.0), (10.0, 0.0)),
            line(6, "0", (0.0, 0.0), (10.0, 0.0)),
        ]);
        let d = diff(&a, &b, &DiffOptions::default());
        assert!(d.changes.is_empty(), "{:#?}", d.changes);
        assert_eq!(d.equivalent, 3);
        // A gap is a real change.
        let c = drawing(vec![
            line(5, "0", (10.0, 5.0), (10.0, 0.0)),
            line(6, "0", (0.0, 0.0), (9.0, 0.0)),
        ]);
        assert!(!diff(&a, &c, &DiffOptions::default()).changes.is_empty());
    }

    #[test]
    fn unknown_entities_are_uncertain_and_not_shown() {
        let mut u = Entity::new(EntityKind::Unknown(lightable_dxf::Unknown {
            type_name: "MULTILEADER".into(),
            pairs: vec![
                lightable_dxf::Pair::new(5, "1"),
                lightable_dxf::Pair::new(10, "1.0"),
                lightable_dxf::Pair::new(20, "2.0"),
            ],
        }));
        u.handle = Some(lightable_dxf::Handle(1));
        let a = drawing(vec![u.clone(), line(2, "0", (0.0, 0.0), (1.0, 0.0))]);
        if let EntityKind::Unknown(x) = &mut u.kind {
            x.pairs[1].raw = "3.0".into();
        }
        let b = drawing(vec![u, line(2, "0", (0.0, 0.0), (1.0, 0.0))]);
        let d = diff(&a, &b, &DiffOptions::default());
        assert_eq!(d.changes.len(), 1);
        let c = &d.changes[0];
        assert_eq!(c.kind, ChangeKind::Changed);
        assert!(!c.shown && !c.certain);
        assert!(c.details.iter().any(|x| x == "not shown"));
        assert!(c.bounds.is_some(), "zoomable from its raw points");
    }

    #[test]
    fn block_users_follow_nested_blocks() {
        let insert = |block: &str| {
            Entity::new(EntityKind::Insert(lightable_dxf::Insert {
                block: block.into(),
                insert: Point3::new(0.0, 0.0, 0.0),
                scale: Point3::new(1.0, 1.0, 1.0),
                rotation: 0.0,
                columns: 1,
                rows: 1,
                column_spacing: 0.0,
                row_spacing: 0.0,
                extrusion: lightable_dxf::Z_AXIS,
                attributes: Vec::new(),
                seqend: None,
            }))
        };
        let mut d = drawing(vec![
            insert("PANEL"),
            line(2, "0", (0.0, 0.0), (1.0, 0.0)),
            insert("bolt"),
            insert("OTHER"),
        ]);
        let mut bolt = lightable_dxf::Block::new("BOLT");
        bolt.entities.push(line(10, "0", (0.0, 0.0), (1.0, 1.0)));
        let mut panel = lightable_dxf::Block::new("PANEL");
        panel.entities.push(insert("BOLT"));
        d.blocks.insert(bolt);
        d.blocks.insert(panel);
        d.blocks.insert(lightable_dxf::Block::new("OTHER"));
        // Direct (case-insensitive) and through PANEL.
        assert_eq!(block_users(&d, "BOLT"), vec![0, 2]);
        assert_eq!(block_users(&d, "PANEL"), vec![0]);
    }

    #[test]
    fn handseed_and_timestamps_are_ignored() {
        let mut a = drawing(vec![line(1, "0", (0.0, 0.0), (10.0, 0.0))]);
        let mut b = a.clone();
        a.header.variables.push(lightable_dxf::HeaderVariable {
            name: "$HANDSEED".into(),
            pairs: vec![lightable_dxf::Pair::new(5, "20")],
        });
        b.header.variables.push(lightable_dxf::HeaderVariable {
            name: "$HANDSEED".into(),
            pairs: vec![lightable_dxf::Pair::new(5, "99")],
        });
        b.header.variables.push(lightable_dxf::HeaderVariable {
            name: "$TDUPDATE".into(),
            pairs: vec![lightable_dxf::Pair::new(40, "2460000.5")],
        });
        assert!(diff(&a, &b, &DiffOptions::default()).changes.is_empty());
    }
}
