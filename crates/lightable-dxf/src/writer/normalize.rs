//! Turning a drawing into one that is valid for the target version: the
//! writer formats the result of this pass without further decisions.

use std::collections::{BTreeMap, HashSet};

use crate::Warning;
use crate::encoding::{CodePage, FileEncoding};
use crate::entity::*;
use crate::model::{Block, Drawing, Handle, HeaderVariable, Source, Version};
use crate::table::{
    BlockRecord, DimStyle, Layer, Linetype, RawTable, Table, TextStyle, names_equal,
};
use crate::tokenizer::Pair;

use super::downgrade;
use super::objects;

/// Warnings collected while writing, with repeats counted.
#[derive(Default)]
pub(super) struct Notes {
    counts: BTreeMap<String, usize>,
    order: Vec<String>,
}

impl Notes {
    /// Records one occurrence of `message` (a format with `{n}` for the count).
    pub fn add(&mut self, message: impl Into<String>) {
        self.add_n(message, 1);
    }

    pub fn add_n(&mut self, message: impl Into<String>, n: usize) {
        if n == 0 {
            return;
        }
        let message = message.into();
        if !self.counts.contains_key(&message) {
            self.order.push(message.clone());
        }
        *self.counts.entry(message).or_default() += n;
    }

    pub fn into_warnings(self) -> Vec<Warning> {
        self.order
            .into_iter()
            .map(|m| {
                let n = self.counts[&m];
                Warning::new(None, m.replace("{n}", &n.to_string()))
            })
            .collect()
    }
}

/// Handle allocation: existing handles are kept unless already taken; new
/// ones come from above every handle present in the input.
pub(super) struct Handles {
    taken: HashSet<u64>,
    next: u64,
    pub duplicates: usize,
}

impl Handles {
    fn new(existing: &[u64]) -> Self {
        let max = existing.iter().copied().max().unwrap_or(0);
        Self {
            taken: HashSet::new(),
            next: max.max(0x1F) + 1,
            duplicates: 0,
        }
    }

    pub fn fresh(&mut self) -> Handle {
        while self.taken.contains(&self.next) {
            self.next += 1;
        }
        self.taken.insert(self.next);
        self.next += 1;
        Handle(self.next - 1)
    }

    /// Keeps `handle` if it is free, otherwise gives a fresh one.
    pub fn claim(&mut self, handle: Option<Handle>) -> Handle {
        match handle {
            Some(h) if h.0 != 0 && self.taken.insert(h.0) => h,
            Some(_) => {
                self.duplicates += 1;
                self.fresh()
            }
            None => self.fresh(),
        }
    }

    /// Marks a handle as used without checking (raw objects keep theirs).
    pub fn reserve(&mut self, handle: Handle) {
        self.taken.insert(handle.0);
    }

    /// The `$HANDSEED`: above every handle written. (Handles of data that is
    /// not written, such as R12's dropped tables, don't count, so writing
    /// the output again gives the same seed.)
    pub fn seed(&self) -> Handle {
        Handle(self.taken.iter().copied().max().map_or(1, |m| m + 1))
    }
}

pub(super) struct Normalized {
    pub drawing: Drawing,
    pub code_page: CodePage,
    pub notes: Notes,
}

const MODEL: &str = "*Model_Space";
const PAPER: &str = "*Paper_Space";
const MODEL_R12: &str = "$MODEL_SPACE";
const PAPER_R12: &str = "$PAPER_SPACE";

fn is_model(name: &str) -> bool {
    names_equal(name, MODEL) || names_equal(name, MODEL_R12)
}

fn is_paper(name: &str) -> bool {
    names_equal(name, PAPER) || names_equal(name, PAPER_R12)
}

/// Other layouts' blocks (`*Paper_Space0`, `*Paper_Space1`, ...).
fn is_layout_block(name: &str) -> bool {
    match (name.get(..PAPER.len()), name.get(PAPER.len()..)) {
        (Some(head), Some(tail)) => {
            !tail.is_empty() && names_equal(head, PAPER) && tail.bytes().all(|b| b.is_ascii_digit())
        }
        _ => false,
    }
}

pub(super) fn normalize(src: &Drawing, version: Version) -> Normalized {
    let r12 = version == Version::R12;
    let mut d = src.clone();
    d.source = Source::default();
    let mut notes = Notes::default();
    let code_page = output_code_page(&d);

    drop_unwritable(&mut d, &mut notes);
    if d.header.version() != Some(version) {
        drop_version_specific(&mut d, &mut notes);
    }
    space_blocks(&mut d, r12, &mut notes);
    if r12 {
        downgrade::entities(&mut d.entities, &mut notes);
        for b in &mut d.blocks.entries {
            downgrade::entities(&mut b.entities, &mut notes);
        }
        downgrade::tables(&mut d, &mut notes);
        if !d.objects.is_empty() || !d.classes.is_empty() || !d.other_sections.is_empty() {
            notes.add("CLASSES, OBJECTS and other sections are not part of R12; not written");
        }
        d.objects.clear();
        d.classes.clear();
        d.other_sections.clear();
        d.tables.block_records = Table::default();
    }
    // Defaults first, so entities on layer 0 don't count as "missing".
    ensure_defaults(&mut d, r12);
    ensure_referenced(&mut d, &mut notes);
    if r12 {
        downgrade::r12_names(&mut d, code_page, &mut notes);
    } else {
        drop_unknown_types(&mut d, &mut notes);
    }
    let mut handles = Handles::new(&existing_handles(&d));
    if r12 {
        assign_r12(&mut d, &mut handles);
    } else {
        assign_r2018(&mut d, &mut handles, &mut notes);
    }
    if handles.duplicates > 0 {
        notes.add_n(
            "{n} duplicate handle(s) replaced with new ones",
            handles.duplicates,
        );
    }
    header(&mut d, version, code_page, handles.seed(), &mut notes);
    d.encoding = if r12 {
        FileEncoding::CodePage(code_page)
    } else {
        FileEncoding::Utf8
    };
    Normalized {
        drawing: d,
        code_page,
        notes,
    }
}

/// The code page for R12 output (and for `%%nnn` in any output).
fn output_code_page(d: &Drawing) -> CodePage {
    match d.encoding {
        FileEncoding::CodePage(page) => page,
        FileEncoding::Utf8 => d
            .header
            .codepage()
            .and_then(CodePage::from_dwgcodepage)
            .unwrap_or(CodePage::ANSI_1252),
    }
}

fn is_orphan(e: &Entity) -> bool {
    matches!(e.kind, EntityKind::Vertex(_) | EntityKind::Attrib(_))
        || e.kind.type_name().eq_ignore_ascii_case("SEQEND")
}

/// Removes what a valid file cannot contain.
fn drop_unwritable(d: &mut Drawing, notes: &mut Notes) {
    notes.add_n(
        "{n} stray record(s) not written (only an unchanged drawing saves them back)",
        d.stray.len(),
    );
    d.stray.clear();
    if d.unparsed_tail.take().is_some() {
        notes.add("unparsed text from the end of the file not written");
    }
    let mut orphans = 0;
    for list in
        std::iter::once(&mut d.entities).chain(d.blocks.entries.iter_mut().map(|b| &mut b.entities))
    {
        let before = list.len();
        list.retain(|e| !is_orphan(e));
        orphans += before - list.len();
    }
    notes.add_n(
        "{n} VERTEX/ATTRIB/SEQEND record(s) outside their parent not written",
        orphans,
    );
    let unparsed = d.blocks.unparsed.len()
        + d.tables.layers.unparsed.len()
        + d.tables.linetypes.unparsed.len()
        + d.tables.text_styles.unparsed.len()
        + d.tables.dim_styles.unparsed.len()
        + d.tables.block_records.unparsed.len();
    notes.add_n(
        "{n} unparsed table entry or block definition(s) not written",
        unparsed,
    );
    d.blocks.unparsed.clear();
    d.tables.layers.unparsed.clear();
    d.tables.linetypes.unparsed.clear();
    d.tables.text_styles.unparsed.clear();
    d.tables.dim_styles.unparsed.clear();
    d.tables.block_records.unparsed.clear();
}

/// Drops entities of types defined in the CLASSES section (MULTILEADER,
/// IMAGE, ...): their record layout depends on the file version, so a copy
/// written into another version is not valid there. (Writing the same
/// version keeps them; save-back keeps everything.)
fn drop_version_specific(d: &mut Drawing, notes: &mut Notes) {
    let mut classes = Vec::new();
    let mut in_class = false;
    for p in &d.classes {
        if p.code == 0 {
            in_class = p.raw.trim() == "CLASS";
        } else if in_class && p.code == 1 {
            classes.push(p.raw.trim().to_ascii_uppercase());
        }
    }
    if classes.is_empty() {
        return;
    }
    let from = d
        .header
        .acadver()
        .unwrap_or("an unknown version")
        .to_owned();
    for list in
        std::iter::once(&mut d.entities).chain(d.blocks.entries.iter_mut().map(|b| &mut b.entities))
    {
        list.retain(|e| match &e.kind {
            EntityKind::Unknown(u) if classes.contains(&u.type_name.to_ascii_uppercase()) => {
                notes.add(format!(
                    "{{n}} {} entit(ies) from {from} not written: their format depends on the file version",
                    u.type_name
                ));
                false
            }
            _ => true,
        });
    }
}

/// Entity types an R2018 file may contain without a CLASSES entry.
const R2018_TYPES: [&str; 50] = [
    "3DFACE",
    "3DSOLID",
    "ACAD_PROXY_ENTITY",
    "ACAD_TABLE",
    "ARC",
    "ARC_DIMENSION",
    "ATTDEF",
    "ATTRIB",
    "BODY",
    "CIRCLE",
    "DGNUNDERLAY",
    "DIMENSION",
    "DWFUNDERLAY",
    "ELLIPSE",
    "EXTRUDEDSURFACE",
    "HATCH",
    "HELIX",
    "IMAGE",
    "INSERT",
    "LARGE_RADIAL_DIMENSION",
    "LEADER",
    "LIGHT",
    "LINE",
    "LOFTEDSURFACE",
    "LWPOLYLINE",
    "MESH",
    "MLINE",
    "MTEXT",
    "MULTILEADER",
    "NURBSURFACE",
    "OLE2FRAME",
    "OLEFRAME",
    "PDFUNDERLAY",
    "PLANESURFACE",
    "POINT",
    "POLYLINE",
    "RAY",
    "REGION",
    "REVOLVEDSURFACE",
    "SECTION",
    "SEQEND",
    "SHAPE",
    "SOLID",
    "SPLINE",
    "SUN",
    "SWEPTSURFACE",
    "TEXT",
    "TOLERANCE",
    "TRACE",
    "VIEWPORT",
];
const R2018_MORE_TYPES: [&str; 4] = ["VERTEX", "WIPEOUT", "XLINE", "TABLE"];

/// Drops entities whose type is neither a standard DXF entity nor defined
/// in the CLASSES section: no program can read them.
fn drop_unknown_types(d: &mut Drawing, notes: &mut Notes) {
    let mut classes = Vec::new();
    let mut in_class = false;
    for p in &d.classes {
        if p.code == 0 {
            in_class = p.raw.trim() == "CLASS";
        } else if in_class && p.code == 1 {
            classes.push(p.raw.trim().to_ascii_uppercase());
        }
    }
    let known = |t: &str| {
        let t = t.to_ascii_uppercase();
        R2018_TYPES.contains(&t.as_str())
            || R2018_MORE_TYPES.contains(&t.as_str())
            || classes.contains(&t)
    };
    for list in
        std::iter::once(&mut d.entities).chain(d.blocks.entries.iter_mut().map(|b| &mut b.entities))
    {
        list.retain(|e| match &e.kind {
            EntityKind::Unknown(u) if !known(&u.type_name) => {
                notes.add(format!(
                    "{{n}} {} entit(ies) not written: not a known entity type",
                    u.type_name
                ));
                false
            }
            _ => true,
        });
    }
}

/// Names the model and paper space blocks for the target version and makes
/// sure both exist (first, in that order). R12 has a single paper space, so
/// other layouts' blocks are dropped there.
fn space_blocks(d: &mut Drawing, r12: bool, notes: &mut Notes) {
    let (model, paper) = if r12 {
        (MODEL_R12, PAPER_R12)
    } else {
        (MODEL, PAPER)
    };
    let mut blocks = std::mem::take(&mut d.blocks.entries);
    if r12 {
        let before = blocks.len();
        blocks.retain(|b| !is_layout_block(&b.name));
        notes.add_n(
            "{n} extra layout(s) not written to R12",
            before - blocks.len(),
        );
    }
    let take = |blocks: &mut Vec<Block>, test: fn(&str) -> bool| {
        blocks
            .iter()
            .position(|b| test(&b.name))
            .map(|i| blocks.remove(i))
    };
    let mut model_block = take(&mut blocks, is_model).unwrap_or_else(|| Block::new(model));
    let mut paper_block = take(&mut blocks, is_paper).unwrap_or_else(|| Block::new(paper));
    model_block.name = model.into();
    paper_block.name = paper.into();
    let mut ordered = vec![model_block, paper_block];
    ordered.extend(blocks);
    d.blocks.entries = ordered;
}

/// Calls `f` on every entity (and child) in the drawing.
fn for_each_entity(d: &mut Drawing, mut f: impl FnMut(&mut Entity)) {
    fn visit(e: &mut Entity, f: &mut impl FnMut(&mut Entity)) {
        f(e);
        match &mut e.kind {
            EntityKind::Polyline(p) => {
                p.vertices.iter_mut().for_each(|v| visit(v, f));
                if let Some(s) = p.seqend.as_deref_mut() {
                    visit(s, f);
                }
            }
            EntityKind::Insert(i) => {
                i.attributes.iter_mut().for_each(|a| visit(a, f));
                if let Some(s) = i.seqend.as_deref_mut() {
                    visit(s, f);
                }
            }
            _ => {}
        }
    }
    for e in &mut d.entities {
        visit(e, &mut f);
    }
    for b in &mut d.blocks.entries {
        for e in &mut b.entities {
            visit(e, &mut f);
        }
    }
}

/// Adds table entries and blocks that entities refer to but the drawing
/// lacks, so the file does not point at nothing.
fn ensure_referenced(d: &mut Drawing, notes: &mut Notes) {
    let mut layers = Vec::new();
    let mut linetypes = Vec::new();
    let mut styles = Vec::new();
    let mut blocks = Vec::new();
    let mut dim_styles = Vec::new();
    for_each_entity(d, |e| {
        layers.push(e.layer.clone());
        linetypes.push(e.linetype.clone());
        match &e.kind {
            EntityKind::Text(t) => styles.push(t.style.clone()),
            EntityKind::Attrib(a) => styles.push(a.text.style.clone()),
            EntityKind::AttDef(a) => styles.push(a.text.style.clone()),
            EntityKind::MText(m) => styles.push(m.style.clone()),
            EntityKind::Insert(i) => blocks.push(i.block.clone()),
            EntityKind::Dimension(dim) => {
                if !dim.block.is_empty() {
                    blocks.push(dim.block.clone());
                }
                dim_styles.push(dim.style.clone());
            }
            _ => {}
        }
    });
    layers.extend(d.blocks.iter().map(|b| b.layer.clone()));
    linetypes.extend(d.tables.layers.iter().map(|l| l.linetype.clone()));
    let mut added = 0;
    for name in layers {
        if !name.is_empty() && !d.tables.layers.contains(&name) {
            d.tables.layers.entries.push(Layer::new(name));
            added += 1;
        }
    }
    notes.add_n(
        "{n} missing layer(s) added because entities use them",
        added,
    );
    let mut added = 0;
    for name in linetypes {
        let builtin = ["BYLAYER", "BYBLOCK", "CONTINUOUS"]
            .iter()
            .any(|b| names_equal(&name, b));
        if !name.is_empty() && !builtin && !d.tables.linetypes.contains(&name) {
            d.tables.linetypes.entries.push(simple_linetype(&name, ""));
            added += 1;
        }
    }
    notes.add_n("{n} missing linetype(s) added as continuous lines", added);
    let mut added = 0;
    for name in styles {
        if !name.is_empty() && !d.tables.text_styles.contains(&name) {
            d.tables.text_styles.entries.push(text_style(&name));
            added += 1;
        }
    }
    notes.add_n("{n} missing text style(s) added", added);
    let mut added = 0;
    for name in blocks {
        if !d.blocks.contains(&name) {
            d.blocks.entries.push(Block::new(name));
            added += 1;
        }
    }
    notes.add_n("{n} missing block definition(s) added (empty)", added);
    let mut added = 0;
    for name in dim_styles {
        if !name.is_empty() && !d.tables.dim_styles.contains(&name) {
            d.tables.dim_styles.entries.push(dim_style(&name));
            added += 1;
        }
    }
    notes.add_n("{n} missing dimension style(s) added", added);
}

pub(super) fn simple_linetype(name: &str, description: &str) -> Linetype {
    Linetype {
        name: name.into(),
        handle: None,
        owner: None,
        flags: 0,
        description: description.into(),
        alignment: 65,
        pattern_length: 0.0,
        elements: Vec::new(),
        extra: Vec::new(),
    }
}

fn text_style(name: &str) -> TextStyle {
    TextStyle {
        name: name.into(),
        handle: None,
        owner: None,
        flags: 0,
        height: 0.0,
        width_factor: 1.0,
        oblique: 0.0,
        generation_flags: 0,
        last_height: 2.5,
        font: "txt".into(),
        big_font: String::new(),
        extra: Vec::new(),
    }
}

fn dim_style(name: &str) -> DimStyle {
    DimStyle {
        name: name.into(),
        handle: None,
        owner: None,
        flags: 0,
        variables: Vec::new(),
        extra: Vec::new(),
    }
}

/// The table entries every file needs.
fn ensure_defaults(d: &mut Drawing, r12: bool) {
    let t = &mut d.tables;
    if r12 {
        t.linetypes
            .entries
            .retain(|l| !names_equal(&l.name, "BYBLOCK") && !names_equal(&l.name, "BYLAYER"));
    } else {
        for (i, name) in ["ByBlock", "ByLayer"].into_iter().enumerate() {
            if !t.linetypes.contains(name) {
                t.linetypes.entries.insert(i, simple_linetype(name, ""));
            }
        }
    }
    if !t.linetypes.contains("Continuous") {
        let at = if r12 { 0 } else { 2.min(t.linetypes.len()) };
        t.linetypes
            .entries
            .insert(at, simple_linetype("Continuous", "Solid line"));
    }
    if !t.layers.contains("0") {
        t.layers.entries.insert(0, Layer::new("0"));
    }
    if !t.text_styles.contains("Standard") {
        t.text_styles.entries.insert(0, text_style("Standard"));
    }
    if !t.dim_styles.contains("Standard") {
        t.dim_styles.entries.insert(0, dim_style("Standard"));
    }
    // Raw tables: VPORT, VIEW, UCS and APPID always exist, in that order.
    let mut appids: Vec<String> = vec!["ACAD".into()];
    for_each_entity(d, |e| {
        let xdata_apps = e
            .extra
            .iter()
            .filter(|p| p.code == 1001)
            .map(|p| p.raw.trim().to_owned());
        appids.extend(xdata_apps);
        if let EntityKind::Unknown(u) = &e.kind {
            appids.extend(
                u.pairs
                    .iter()
                    .filter(|p| p.code == 1001)
                    .map(|p| p.raw.trim().to_owned()),
            );
        }
    });
    let t = &mut d.tables;
    for name in ["VPORT", "VIEW", "UCS", "APPID"] {
        if !t.other.iter().any(|r| r.name.eq_ignore_ascii_case(name)) {
            t.other.push(RawTable {
                name: name.into(),
                pairs: Vec::new(),
            });
        }
    }
    let order = |name: &str| match name.to_ascii_uppercase().as_str() {
        "VPORT" => 0,
        "VIEW" => 1,
        "UCS" => 2,
        "APPID" => 3,
        _ => 4,
    };
    t.other.sort_by_key(|r| order(&r.name));
    for raw in &mut t.other {
        let (_, entries) = split_raw_table(&raw.pairs);
        let names: Vec<String> = entries
            .iter()
            .filter_map(|e| {
                e.iter()
                    .find(|p| p.code == 2)
                    .map(|p| p.raw.trim().to_owned())
            })
            .collect();
        if raw.name.eq_ignore_ascii_case("VPORT")
            && !names.iter().any(|n| names_equal(n, "*ACTIVE"))
        {
            raw.pairs.extend(objects::default_vport(r12));
        }
        if raw.name.eq_ignore_ascii_case("APPID") {
            let mut seen: Vec<String> = names.clone();
            for app in &appids {
                if !seen.iter().any(|n| names_equal(n, app)) {
                    raw.pairs.push(Pair::new(0, "APPID"));
                    raw.pairs.push(Pair::new(2, app.clone()));
                    raw.pairs.push(Pair::new(70, "0"));
                    seen.push(app.clone());
                }
            }
        }
    }
}

/// Splits a raw table's pairs into its own pairs and its entries (each
/// starting with its `0` pair).
pub(super) fn split_raw_table(pairs: &[Pair]) -> (Vec<Pair>, Vec<Vec<Pair>>) {
    let first = pairs
        .iter()
        .position(|p| p.code == 0)
        .unwrap_or(pairs.len());
    let header = pairs[..first].to_vec();
    let mut entries: Vec<Vec<Pair>> = Vec::new();
    for p in &pairs[first..] {
        if p.code == 0 || entries.is_empty() {
            entries.push(Vec::new());
        }
        if let Some(e) = entries.last_mut() {
            e.push(p.clone());
        }
    }
    (header, entries)
}

/// Every handle present in the input, so fresh ones are above them all.
fn existing_handles(d: &Drawing) -> Vec<u64> {
    let mut out = Vec::new();
    let pair_handles = |pairs: &[Pair], out: &mut Vec<u64>| {
        out.extend(
            pairs
                .iter()
                .filter(|p| p.code == 5 || p.code == 105)
                .filter_map(|p| Handle::parse(&p.raw).map(|h| h.0)),
        );
    };
    pair_handles(&d.objects, &mut out);
    for raw in &d.tables.other {
        pair_handles(&raw.pairs, &mut out);
    }
    let t = &d.tables;
    for pairs in [
        &t.layers.header,
        &t.linetypes.header,
        &t.text_styles.header,
        &t.dim_styles.header,
        &t.block_records.header,
    ] {
        pair_handles(pairs, &mut out);
    }
    out.extend(t.layers.iter().filter_map(|e| e.handle.map(|h| h.0)));
    out.extend(t.linetypes.iter().filter_map(|e| e.handle.map(|h| h.0)));
    out.extend(t.text_styles.iter().filter_map(|e| e.handle.map(|h| h.0)));
    out.extend(t.dim_styles.iter().filter_map(|e| e.handle.map(|h| h.0)));
    out.extend(t.block_records.iter().filter_map(|e| e.handle.map(|h| h.0)));
    for b in d.blocks.iter() {
        out.extend(b.handle.map(|h| h.0));
        pair_handles(&b.end, &mut out);
    }
    fn visit(e: &Entity, out: &mut Vec<u64>) {
        out.extend(e.handle.map(|h| h.0));
        let children: Vec<&Entity> = match &e.kind {
            EntityKind::Polyline(p) => p.vertices.iter().chain(p.seqend.as_deref()).collect(),
            EntityKind::Insert(i) => i.attributes.iter().chain(i.seqend.as_deref()).collect(),
            _ => Vec::new(),
        };
        for c in children {
            visit(c, out);
        }
    }
    for e in d
        .entities
        .iter()
        .chain(d.blocks.iter().flat_map(|b| b.entities.iter()))
    {
        visit(e, &mut out);
    }
    out
}

/// Sets the first `code` pair outside `102` groups to `value`, or inserts
/// it at `at` (clamped).
pub(super) fn set_pair(pairs: &mut Vec<Pair>, code: i16, value: String, at: usize) {
    let mut depth = 0usize;
    for p in pairs.iter_mut() {
        if p.code == 102 {
            let v = p.raw.trim();
            if v.starts_with('{') {
                depth += 1;
            } else if v == "}" {
                depth = depth.saturating_sub(1);
            }
            continue;
        }
        if depth == 0 && p.code == code {
            p.raw = value;
            return;
        }
    }
    let at = at.min(pairs.len());
    pairs.insert(at, Pair::new(code, value));
}

/// Index just after the leading `5` pair and any `102` groups.
fn after_handle_and_groups(pairs: &[Pair]) -> usize {
    let mut i = usize::from(pairs.first().is_some_and(|p| p.code == 5));
    let mut depth = 0usize;
    while let Some(p) = pairs.get(i) {
        if p.code == 102 {
            let v = p.raw.trim();
            if v.starts_with('{') {
                depth += 1;
            } else if v == "}" {
                depth = depth.saturating_sub(1);
            }
            i += 1;
        } else if depth > 0 {
            i += 1;
        } else {
            break;
        }
    }
    i
}

fn claim_entity(e: &mut Entity, handles: &mut Handles, owner: Option<Handle>) {
    let handle = handles.claim(e.handle);
    e.handle = Some(handle);
    e.owner = owner;
    if let EntityKind::Unknown(u) = &mut e.kind {
        set_pair(&mut u.pairs, 5, handle.to_string(), 0);
        match owner {
            Some(owner) => {
                let at = after_handle_and_groups(&u.pairs);
                set_pair(&mut u.pairs, 330, owner.to_string(), at);
                downgrade::upgrade_unknown_markers(&u.type_name, &mut u.pairs);
            }
            None => u.pairs.retain(|p| p.code != 330),
        }
    }
    let children_owner = owner.map(|_| handle);
    let (children, seqend): (&mut Vec<Entity>, &mut Option<Box<Entity>>) = match &mut e.kind {
        EntityKind::Polyline(p) => (&mut p.vertices, &mut p.seqend),
        EntityKind::Insert(i) if !i.attributes.is_empty() => (&mut i.attributes, &mut i.seqend),
        EntityKind::Insert(i) => {
            i.seqend = None;
            return;
        }
        _ => return,
    };
    for c in children.iter_mut() {
        claim_entity(c, handles, children_owner);
    }
    let layer = e.layer.clone();
    let end = seqend.get_or_insert_with(|| {
        let mut s = Entity::new(EntityKind::Unknown(Unknown {
            type_name: "SEQEND".into(),
            pairs: Vec::new(),
        }));
        s.layer = layer;
        Box::new(s)
    });
    let end_handle = handles.claim(end.handle);
    end.handle = Some(end_handle);
    end.owner = children_owner;
    // The SEQEND is written from its fields, not its pairs.
    if let EntityKind::Unknown(u) = &mut end.kind {
        u.pairs.clear();
    }
}

/// The handle of the `ENDBLK` record, kept in `Block::end`.
fn end_handle(block: &Block) -> Option<Handle> {
    block
        .end
        .iter()
        .find(|p| p.code == 5)
        .and_then(|p| Handle::parse(&p.raw))
}

/// Rewrites `Block::end` in its canonical form: handle (R2018: and owner),
/// layer, and the paper-space flag if it was there.
fn set_end(block: &mut Block, handle: Handle, owner: Option<Handle>) {
    let layer = block
        .end
        .iter()
        .find(|p| p.code == 8)
        .map_or_else(|| block.layer.clone(), |p| p.raw.trim().to_owned());
    let paper = block
        .end
        .iter()
        .any(|p| p.code == 67 && p.raw.trim() == "1");
    let mut end = vec![Pair::new(5, handle.to_string())];
    if let Some(owner) = owner {
        end.push(Pair::new(330, owner.to_string()));
    }
    if paper {
        end.push(Pair::new(67, "1"));
    }
    end.push(Pair::new(8, layer));
    block.end = end;
}

fn assign_r12(d: &mut Drawing, handles: &mut Handles) {
    for b in &mut d.blocks.entries {
        b.handle = None;
        b.owner = None;
        let h = handles.claim(end_handle(b));
        for e in &mut b.entities {
            claim_entity(e, handles, None);
        }
        set_end(b, h, None);
    }
    for e in &mut d.entities {
        claim_entity(e, handles, None);
    }
    let t = &mut d.tables;
    t.layers
        .entries
        .iter_mut()
        .for_each(|e| (e.handle, e.owner) = (None, None));
    t.linetypes
        .entries
        .iter_mut()
        .for_each(|e| (e.handle, e.owner) = (None, None));
    t.text_styles
        .entries
        .iter_mut()
        .for_each(|e| (e.handle, e.owner) = (None, None));
    t.dim_styles
        .entries
        .iter_mut()
        .for_each(|e| (e.handle, e.owner) = (None, None));
    for table in [
        &mut t.layers.header,
        &mut t.linetypes.header,
        &mut t.text_styles.header,
        &mut t.dim_styles.header,
    ] {
        table.clear();
    }
    for raw in &mut t.other {
        let (_, entries) = split_raw_table(&raw.pairs);
        raw.pairs = entries.into_iter().flatten().collect();
    }
}

/// Claims a typed table's own handle (kept in its header pairs) and its
/// entries' handles, and points the entries at the table.
macro_rules! claim_table {
    ($table:expr, $handles:expr) => {{
        let own = $table
            .header
            .iter()
            .find(|p| p.code == 5)
            .and_then(|p| Handle::parse(&p.raw));
        let own = $handles.claim(own);
        $table.header = vec![Pair::new(5, own.to_string())];
        for e in &mut $table.entries {
            e.handle = Some($handles.claim(e.handle));
            e.owner = Some(own);
        }
        own
    }};
}

fn assign_r2018(d: &mut Drawing, handles: &mut Handles, notes: &mut Notes) {
    // Objects are written as they are, so their handles are fixed.
    for p in d.objects.iter().filter(|p| p.code == 5) {
        if let Some(h) = Handle::parse(&p.raw) {
            handles.reserve(h);
        }
    }
    // Raw tables: own handle, entry handles, owners and subclass markers.
    for raw in &mut d.tables.other {
        let (header, entries) = split_raw_table(&raw.pairs);
        let own = handles.claim(
            header
                .iter()
                .find(|p| p.code == 5)
                .and_then(|p| Handle::parse(&p.raw)),
        );
        let mut pairs = vec![Pair::new(5, own.to_string())];
        for mut entry in entries {
            let mut body = entry.split_off(1);
            let handle = handles.claim(
                body.iter()
                    .find(|p| p.code == 5)
                    .and_then(|p| Handle::parse(&p.raw)),
            );
            body.retain(|p| p.code != 5);
            body.insert(0, Pair::new(5, handle.to_string()));
            let at = after_handle_and_groups(&body);
            set_pair(&mut body, 330, own.to_string(), at);
            if !body.iter().any(|p| p.code == 100) {
                let at = after_handle_and_groups(&body) + 1;
                let marker = objects::table_record_marker(&raw.name);
                body.insert(at, Pair::new(100, "AcDbSymbolTableRecord"));
                body.insert(at + 1, Pair::new(100, marker));
            }
            pairs.extend(entry);
            pairs.extend(body);
        }
        raw.pairs = pairs;
    }
    let t = &mut d.tables;
    claim_table!(t.linetypes, handles);
    claim_table!(t.layers, handles);
    claim_table!(t.text_styles, handles);
    claim_table!(t.dim_styles, handles);

    // One block record per block, in block order.
    let mut records = std::mem::take(&mut t.block_records.entries);
    let mut ordered = Vec::with_capacity(d.blocks.len());
    for b in d.blocks.iter() {
        let record = match records.iter().position(|r| names_equal(&r.name, &b.name)) {
            Some(i) => {
                let mut r = records.remove(i);
                r.name = b.name.clone();
                r
            }
            None => BlockRecord {
                name: b.name.clone(),
                handle: None,
                owner: None,
                layout: None,
                units: 0,
                explodable: true,
                scalable: false,
                extra: Vec::new(),
            },
        };
        ordered.push(record);
    }
    notes.add_n(
        "{n} block record(s) without a block definition not written",
        records.len(),
    );
    t.block_records.entries = ordered;
    claim_table!(t.block_records, handles);

    let record_handles: Vec<Handle> = t.block_records.iter().filter_map(|r| r.handle).collect();
    for (b, record) in d.blocks.entries.iter_mut().zip(&record_handles) {
        b.handle = Some(handles.claim(b.handle));
        b.owner = Some(*record);
        for e in &mut b.entities {
            claim_entity(e, handles, Some(*record));
        }
        let h = handles.claim(end_handle(b));
        set_end(b, h, Some(*record));
    }
    let (model, paper) = (record_handles[0], record_handles[1]);
    for e in &mut d.entities {
        let owner = if e.paper_space { paper } else { model };
        claim_entity(e, handles, Some(owner));
    }
    objects::ensure_layouts(d, handles, model, paper);
}

/// Header variables allowed in R12 output when downgrading a newer file.
const R12_VARIABLES: [&str; 93] = [
    "$ACADVER",
    "$INSBASE",
    "$EXTMIN",
    "$EXTMAX",
    "$LIMMIN",
    "$LIMMAX",
    "$ORTHOMODE",
    "$REGENMODE",
    "$FILLMODE",
    "$QTEXTMODE",
    "$MIRRTEXT",
    "$DRAGMODE",
    "$LTSCALE",
    "$OSMODE",
    "$ATTMODE",
    "$TEXTSIZE",
    "$TRACEWID",
    "$TEXTSTYLE",
    "$CLAYER",
    "$CELTYPE",
    "$CECOLOR",
    "$DIMSCALE",
    "$DIMASZ",
    "$DIMEXO",
    "$DIMDLI",
    "$DIMEXE",
    "$DIMTP",
    "$DIMTM",
    "$DIMTXT",
    "$DIMCEN",
    "$DIMTSZ",
    "$DIMTOL",
    "$DIMLIM",
    "$DIMTIH",
    "$DIMTOH",
    "$DIMSE1",
    "$DIMSE2",
    "$DIMTAD",
    "$DIMZIN",
    "$DIMBLK",
    "$DIMASO",
    "$DIMSHO",
    "$DIMPOST",
    "$DIMAPOST",
    "$DIMALT",
    "$DIMALTD",
    "$DIMALTF",
    "$DIMLFAC",
    "$DIMTOFL",
    "$DIMTVP",
    "$DIMTIX",
    "$DIMSOXD",
    "$DIMSAH",
    "$DIMBLK1",
    "$DIMBLK2",
    "$DIMSTYLE",
    "$DIMCLRD",
    "$DIMCLRE",
    "$DIMCLRT",
    "$DIMTFAC",
    "$DIMGAP",
    "$LUNITS",
    "$LUPREC",
    "$SKETCHINC",
    "$FILLETRAD",
    "$AUNITS",
    "$AUPREC",
    "$MENU",
    "$ELEVATION",
    "$PELEVATION",
    "$THICKNESS",
    "$LIMCHECK",
    "$CHAMFERA",
    "$CHAMFERB",
    "$SKPOLY",
    "$ANGBASE",
    "$ANGDIR",
    "$PDMODE",
    "$PDSIZE",
    "$PLINEWID",
    "$COORDS",
    "$SPLFRAME",
    "$SPLINETYPE",
    "$SPLINESEGS",
    "$ATTDIA",
    "$ATTREQ",
    "$SURFTAB1",
    "$SURFTAB2",
    "$TILEMODE",
    "$PLINEGEN",
    "$PSLTSCALE",
    "$UNITMODE",
    "$VISRETAIN",
];

fn set_var(header: &mut Vec<HeaderVariable>, name: &str, code: i16, value: String) {
    let pairs = vec![Pair::new(code, value)];
    match header
        .iter_mut()
        .find(|v| v.name.eq_ignore_ascii_case(name))
    {
        Some(var) => var.pairs = pairs,
        None => {
            let at = usize::from(
                header
                    .first()
                    .is_some_and(|v| v.name.eq_ignore_ascii_case("$ACADVER")),
            );
            header.insert(
                at.min(header.len()),
                HeaderVariable {
                    name: name.into(),
                    pairs,
                },
            );
        }
    }
}

fn header(d: &mut Drawing, version: Version, code_page: CodePage, seed: Handle, notes: &mut Notes) {
    let source_r12 = d.header.version() == Some(Version::R12);
    let vars = &mut d.header.variables;
    if version == Version::R12 && !source_r12 {
        let before = vars.len();
        vars.retain(|v| R12_VARIABLES.iter().any(|n| v.name.eq_ignore_ascii_case(n)));
        notes.add_n(
            "{n} header variable(s) not in R12 not written",
            before - vars.len(),
        );
    }
    // $ACADVER always comes first.
    vars.retain(|v| !v.name.eq_ignore_ascii_case("$ACADVER"));
    vars.insert(
        0,
        HeaderVariable {
            name: "$ACADVER".into(),
            pairs: vec![Pair::new(1, version.acadver())],
        },
    );
    let page = if version == Version::R12 {
        code_page.dwgcodepage()
    } else {
        d.header
            .codepage()
            .map_or_else(|| code_page.dwgcodepage(), str::to_owned)
    };
    let vars = &mut d.header.variables;
    set_var(vars, "$DWGCODEPAGE", 3, page);
    if version == Version::R12 {
        set_var(vars, "$HANDLING", 70, "1".into());
    }
    set_var(vars, "$HANDSEED", 5, seed.to_string());
}
