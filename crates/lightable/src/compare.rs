//! The compare view: two drawings, their changes as an overlay or side by
//! side, a change list and a "next change" button. The comparison itself
//! is `lightable_diff`; this only displays it.
//!
//! Complexity comes in layers:
//! - layer 1, no clicks: the overlay, one summary line and the colour
//!   legend;
//! - layer 2: the change list in plain words (a click zooms to the change);
//! - layer 3: the side-by-side toggle;
//! - layers 4–5, behind "Details": exact values, uncertain and not-shown
//!   notes, what is not compared yet, layers, exports.
//!
//! One hint at a time points at the next layer, and goes once it is used.

use std::sync::Arc;

use eframe::egui::{self, Align2, Color32, FontId, Rect, Stroke, StrokeKind};
use eframe::glow;

use lightable_diff::view::{self as dview, Side};
use lightable_diff::{Change, ChangeKind, Diff, Place};
use lightable_render::{Bounds, View};

use crate::app::Doc;

/// What the toolbar asks the app to do.
#[derive(Default)]
pub struct Action {
    pub close: bool,
    pub fit: bool,
    pub zoom: Option<Bounds>,
    pub message: Option<String>,
    /// An example chosen in the picker.
    pub example: Option<&'static str>,
    /// Open the file dialog.
    pub choose: bool,
    /// Swap before and after.
    pub swap: bool,
}

pub struct Compare {
    /// The two drawings, kept for swapping them.
    drawings: (Arc<lightable_dxf::Drawing>, Arc<lightable_dxf::Drawing>),
    /// Notes on a comparison that is probably not what was meant (the same
    /// file twice, unrelated drawings).
    pub notes: Vec<String>,
    /// Narrow or portrait screen: the list goes under the drawing and rows
    /// wrap.
    pub narrow: bool,
    /// Little height (a phone held sideways): a compact summary.
    pub short: bool,
    /// The change list and the summary are shown (the demo hides them
    /// in some steps).
    pub show_list: bool,
    /// Show only this many changes, then "… and N more" (the demo's
    /// "found" step).
    pub list_limit: Option<usize>,
    /// With `list_limit`: whether "… and N more" was fully on screen in the
    /// last frame (the recorder's checks).
    pub list_tail_visible: Option<bool>,
    pub show_summary: bool,
    /// Recording a video: only the view toggle in the toolbar, no legend
    /// or Previous/Next.
    pub record: bool,
    a_name: String,
    b_name: String,
    /// What the summary calls the two drawings ("Rev B", "Rev C").
    a_label: String,
    b_label: String,
    /// The built-in example shown, if any.
    pub example: Option<&'static str>,
    diff: Diff,
    overlay: Doc,
    left: Doc,
    right: Doc,
    pub side_by_side: bool,
    current: Option<usize>,
    /// Both drawings' extent (model space).
    extent: Option<Bounds>,
    /// Gap between the side-by-side panes, in points.
    gap: f32,
    report: Report,
    /// Layers 4–5 shown.
    pub details: bool,
    /// Scroll the change list to "Not compared yet" on the next frame.
    pub reveal_not_compared: bool,
    /// Which hints have been acted on: clicking a change, side by side,
    /// details.
    used: [bool; 3],
    /// Where the controls the hints point at were drawn last frame.
    anchors: [Option<Rect>; 3],
    /// The built-in example these files are (its files were dropped or
    /// chosen): its note is shown as for the example itself.
    pub files_of: Option<&'static str>,
    /// Where things were drawn last frame, for the demo's gestures: the
    /// colour legend, "Export PNG report", and each change row fully in
    /// view.
    pub legend_rect: Option<Rect>,
    pub export_rect: Option<Rect>,
    pub json_rect: Option<Rect>,
    pub summary_rect: Option<Rect>,
    pub swap_rect: Option<Rect>,
    pub not_compared_rect: Option<Rect>,
    /// The "objects redrawn the same" line in Details (the demo highlights
    /// it), and whether to scroll it into view this frame.
    pub redrawn_rect: Option<Rect>,
    pub reveal_redrawn: bool,
    /// On the brand stage: seconds since the comparison appeared; the rows
    /// of the change list fade in one after another (all of them real).
    pub reveal_age: Option<f64>,
    pub privacy_rect: Option<Rect>,
    /// How long reading and comparing the user's files took, and (the
    /// large layout's video) the network requests since; shown under the
    /// summary.
    pub timing_line: Option<String>,
    pub network_line: Option<String>,
    pub timing_rect: Option<Rect>,
    pub network_rect: Option<Rect>,
    /// Recording: the privacy line in the toolbar, and the file names with
    /// Swap even in 9:16 (the feature videos that show them).
    pub show_privacy: bool,
    pub show_files: bool,
    pub row_rects: Vec<(usize, Rect)>,
    /// The part of the change list in view (rows outside it scroll in).
    pub list_rect: Option<Rect>,
    /// Frames left to bring the selected row into view.
    reveal_selected: u8,
    /// Change rows in view whose headline was cut off ("…") last frame.
    pub cut_rows: usize,
}

/// The PNG report: shown for a few frames, then captured (or, in the demo,
/// only shown).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Report {
    Off,
    Preparing(u32),
    Capturing,
    Preview,
}

const HINTS: [&str; 3] = [
    "Click a change to zoom in.",
    "Try Side by side to see both revisions next to each other.",
    "Open Details for exact values, exports and what is not compared yet.",
];

/// For a change inside a block: which references show it.
fn block_refs(c: &Change) -> Option<String> {
    if !matches!(c.place, Place::Block(_)) {
        return None;
    }
    let refs: Vec<String> = c
        .used_by_new
        .iter()
        .map(|r| format!("INSERT {}", r.handle.as_deref().unwrap_or("?")))
        .collect();
    Some(if refs.is_empty() {
        "not used in the drawing".to_owned()
    } else {
        format!("shown by {} reference(s): {}", refs.len(), refs.join(", "))
    })
}

fn rgb(c: lightable_render::Rgb) -> Color32 {
    Color32::from_rgb(c.0, c.1, c.2)
}

fn kind_color(c: &Change) -> Color32 {
    match c.kind {
        ChangeKind::Added => rgb(dview::ADDED),
        ChangeKind::Deleted => rgb(dview::DELETED),
        ChangeKind::Changed => rgb(dview::CHANGED),
    }
}

/// Changes with a place in model space: model entities, and changes inside
/// blocks (zoomed to the block's references).
fn zoomable(c: &Change) -> bool {
    matches!(c.place, Place::Model | Place::Block(_))
}

fn short_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

/// The colour legend; returns where it was drawn.
fn legend(ui: &mut egui::Ui) -> Rect {
    let mut area = Rect::NOTHING;
    for (color, label) in [
        (dview::ADDED, "added"),
        (dview::DELETED, "deleted"),
        (dview::CHANGED, "changed"),
        (dview::UNCHANGED, "unchanged"),
    ] {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(11.0, 11.0), egui::Sense::hover());
        ui.painter().rect_filled(rect, 2.0, rgb(color));
        let text = ui.label(egui::RichText::new(label).color(Color32::from_gray(200)));
        area = area.union(rect).union(text.rect);
        ui.add_space(6.0);
    }
    area
}

impl Compare {
    /// Compares `a` (before) with `b` (after). `labels` name them in the
    /// summary; by default their file names.
    pub fn new(
        gl: Option<&Arc<glow::Context>>,
        (a_name, a): (String, Arc<lightable_dxf::Drawing>),
        (b_name, b): (String, Arc<lightable_dxf::Drawing>),
        labels: Option<(String, String)>,
        example: Option<&'static str>,
    ) -> Result<Self, String> {
        let diff = lightable_diff::diff(&a, &b, &lightable_diff::DiffOptions::default());
        let drawings = (a.clone(), b.clone());
        let (sa, sb) = (lightable_render::build(&a), lightable_render::build(&b));
        let mut extent = sa.bounds(None);
        if let Some(bb) = sb.bounds(None) {
            match &mut extent {
                Some(e) => e.union(&bb),
                None => extent = Some(bb),
            }
        }
        let overlay = Doc::new(gl, "overlay".into(), dview::overlay(&sa, &sb, &diff), 0)?;
        let left = Doc::new(gl, a_name.clone(), dview::side(&sa, &diff, Side::A), 0)?;
        let right = Doc::new(gl, b_name.clone(), dview::side(&sb, &diff, Side::B), 0)?;
        let (a_label, b_label) = labels.unwrap_or_else(|| {
            (
                short_name(&a_name).to_owned(),
                short_name(&b_name).to_owned(),
            )
        });
        Ok(Self {
            drawings,
            notes: Vec::new(),
            narrow: false,
            short: false,
            show_list: true,
            list_limit: None,
            list_tail_visible: None,
            show_summary: true,
            record: false,
            a_name,
            b_name,
            a_label,
            b_label,
            example,
            diff,
            overlay,
            left,
            right,
            side_by_side: false,
            current: None,
            extent,
            gap: 4.0,
            report: Report::Off,
            details: false,
            reveal_not_compared: false,
            used: [false; 3],
            anchors: [None; 3],
            files_of: None,
            legend_rect: None,
            export_rect: None,
            json_rect: None,
            summary_rect: None,
            swap_rect: None,
            not_compared_rect: None,
            redrawn_rect: None,
            reveal_redrawn: false,
            reveal_age: None,
            privacy_rect: None,
            timing_line: None,
            network_line: None,
            timing_rect: None,
            network_rect: None,
            show_privacy: false,
            show_files: false,
            row_rects: Vec::new(),
            list_rect: None,
            reveal_selected: 0,
            cut_rows: 0,
        })
    }

    /// The inputs with before and after exchanged, to compare again.
    #[allow(clippy::type_complexity)]
    pub fn swapped(
        &self,
    ) -> (
        (String, Arc<lightable_dxf::Drawing>),
        (String, Arc<lightable_dxf::Drawing>),
        Option<(String, String)>,
    ) {
        (
            (self.b_name.clone(), self.drawings.1.clone()),
            (self.a_name.clone(), self.drawings.0.clone()),
            Some((self.b_label.clone(), self.a_label.clone())),
        )
    }

    /// The comparison's result.
    pub fn diff(&self) -> &Diff {
        &self.diff
    }

    pub fn destroy(&self, gl: Option<&Arc<glow::Context>>) {
        for d in [&self.overlay, &self.left, &self.right] {
            d.destroy(gl);
        }
    }

    pub fn bounds(&self) -> Option<Bounds> {
        self.extent
    }

    /// The panes to draw in: one, or two side by side.
    pub fn panes(&self, rect: Rect) -> Vec<Rect> {
        if !self.side_by_side || self.reporting() {
            return vec![rect];
        }
        // Portrait (a phone, a 9:16 frame): one above the other.
        if rect.height() > rect.width() * 1.2 {
            let half = (rect.height() - self.gap) / 2.0;
            let top = Rect::from_min_size(rect.min, egui::vec2(rect.width(), half));
            let bottom = Rect::from_min_size(
                egui::pos2(rect.min.x, rect.min.y + half + self.gap),
                egui::vec2(rect.width(), half),
            );
            return vec![top, bottom];
        }
        let half = (rect.width() - self.gap) / 2.0;
        let left = Rect::from_min_size(rect.min, egui::vec2(half, rect.height()));
        let right = Rect::from_min_size(
            egui::pos2(rect.min.x + half + self.gap, rect.min.y),
            egui::vec2(half, rect.height()),
        );
        vec![left, right]
    }

    /// Where to zoom for change `i`, if it has a place in model space.
    fn zoom_for(&self, i: usize) -> Option<Bounds> {
        let c = self.diff.changes.get(i)?;
        if !zoomable(c) {
            return None;
        }
        // At least a quarter of the drawing, so the change is seen in its
        // surroundings; a tenth for text, so the words can be read.
        let text = matches!(c.entity_type.as_str(), "TEXT" | "MTEXT" | "ATTRIB");
        let fraction = if text { 0.1 } else { 0.25 };
        Some(dview::zoom_box(c.bounds?, self.extent, fraction))
    }

    /// Selects change `i`; returns where to zoom.
    pub fn select(&mut self, i: usize) -> Option<Bounds> {
        self.current = Some(i);
        self.reveal_selected = 2;
        self.used[0] = true;
        self.zoom_for(i)
    }

    /// The later drawing's objects (model space entities).
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    pub fn object_count(&self) -> usize {
        self.drawings.1.entities.len()
    }

    /// The change selected in the list.
    pub fn selected(&self) -> Option<usize> {
        self.current
    }

    /// The two files' names, earlier first.
    pub fn file_names(&self) -> (&str, &str) {
        (&self.a_name, &self.b_name)
    }

    /// The headline of change `i`.
    #[cfg(test)]
    pub fn headline(&self, i: usize) -> &str {
        self.diff.changes.get(i).map_or("", |c| c.headline.as_str())
    }

    /// Where the revision cloud is, if the changes include one, with the
    /// revision markup right next to it (the revision triangle and its
    /// letter), so a zoom shows the whole callout.
    pub fn cloud_bounds(&self) -> Option<Bounds> {
        // The cloud and a little around it.
        Some(dview::zoom_box(
            self.cloud_bounds_tight()?,
            self.extent,
            0.2,
        ))
    }

    /// The built-in example's note on how it was drawn ("Example: the
    /// drafter clouded only the bolt note."); never for the user's files.
    pub fn example_note(&self) -> Option<&'static str> {
        let id = self.example.or(self.files_of)?;
        crate::examples::EXAMPLES.iter().find(|e| e.id == id)?.note
    }

    /// How many design changes (not revision markup) were found.
    pub fn design_count(&self) -> usize {
        self.diff.design_changes().count()
    }

    /// Design changes whose centre is inside an added revision cloud. For
    /// the demo's narration only: the product does not judge what a cloud
    /// is meant to cover.
    fn clouded(&self) -> Vec<usize> {
        let clouds: Vec<Bounds> = self
            .diff
            .changes
            .iter()
            .filter(|c| c.revision_cloud)
            .filter_map(|c| c.bounds)
            .collect();
        let inside = |b: &Bounds| {
            let (x, y) = ((b.min[0] + b.max[0]) / 2.0, (b.min[1] + b.max[1]) / 2.0);
            clouds
                .iter()
                .any(|k| (k.min[0]..=k.max[0]).contains(&x) && (k.min[1]..=k.max[1]).contains(&y))
        };
        (0..self.diff.changes.len())
            .filter(|&i| {
                let c = &self.diff.changes[i];
                !c.markup && c.bounds.as_ref().is_some_and(inside)
            })
            .collect()
    }

    /// The headlines of the design changes inside a revision cloud (demo
    /// and voice-over checks only).
    #[cfg(test)]
    pub fn clouded_headlines(&self) -> Vec<&str> {
        self.clouded()
            .into_iter()
            .map(|i| self.diff.changes[i].headline.as_str())
            .collect()
    }

    /// The revision cloud and the markup next to it, without the usual
    /// minimum zoom box: close enough to read the clouded text (demo).
    pub fn cloud_bounds_tight(&self) -> Option<Bounds> {
        let cloud = self.diff.changes.iter().find(|c| c.revision_cloud)?;
        let mut b = cloud.bounds?;
        let size = b.size();
        let reach = 0.5 * size[0].max(size[1]);
        for c in &self.diff.changes {
            if let Some(m) = c.bounds.filter(|_| c.markup && !c.revision_cloud) {
                let centre = [(m.min[0] + m.max[0]) / 2.0, (m.min[1] + m.max[1]) / 2.0];
                if centre[0] >= b.min[0] - reach
                    && centre[0] <= b.max[0] + reach
                    && centre[1] >= b.min[1] - reach
                    && centre[1] <= b.max[1] + reach
                {
                    b.union(&m);
                }
            }
        }
        Some(b)
    }

    /// The box around change `i` (model space), if it has one.
    pub fn change_bounds(&self, i: usize) -> Option<Bounds> {
        self.diff
            .changes
            .get(i)
            .filter(|c| zoomable(c))
            .and_then(|c| c.bounds)
    }

    /// Where the change list's first row, "Side by side" and "Details"
    /// were drawn last frame.
    pub fn anchors(&self) -> [Option<Rect>; 3] {
        self.anchors
    }

    /// No change selected.
    pub fn clear_selection(&mut self) {
        self.current = None;
    }

    /// How many design changes the revision cloud surrounds (demo only).
    pub fn clouded_count(&self) -> usize {
        self.clouded().len()
    }

    /// Up to three design changes outside the clouds for the demo to click:
    /// a different kind each where it can (circle, dimension, text), then
    /// whatever comes next.
    pub fn demo_picks(&self) -> Vec<usize> {
        let clouded = self.clouded();
        let open: Vec<usize> = (0..self.diff.changes.len())
            .filter(|&i| {
                let c = &self.diff.changes[i];
                !c.markup && zoomable(c) && !clouded.contains(&i)
            })
            .collect();
        let mut picks: Vec<usize> = ["CIRCLE", "DIMENSION", "TEXT"]
            .iter()
            .filter_map(|t| {
                open.iter()
                    .copied()
                    .find(|&i| self.diff.changes[i].entity_type == *t)
            })
            .collect();
        for i in open {
            if picks.len() >= 3 {
                break;
            }
            if !picks.contains(&i) {
                picks.push(i);
            }
        }
        picks.truncate(3);
        picks
    }

    /// "10 changes found between Rev B and Rev C".
    pub fn summary(&self) -> String {
        let n = self.diff.design_changes().count();
        let what = match n {
            0 => "No changes found".to_owned(),
            1 => "1 change found".to_owned(),
            n => format!("{n} changes found"),
        };
        format!("{what} between {} and {}", self.a_label, self.b_label)
    }

    pub fn labels(&self) -> (&str, &str) {
        (&self.a_label, &self.b_label)
    }

    fn step(&mut self, forward: bool) -> Option<Bounds> {
        let n = self.diff.changes.len();
        if n == 0 {
            return None;
        }
        let i = match (self.current, forward) {
            (None, true) => 0,
            (None, false) => n - 1,
            (Some(i), true) => (i + 1) % n,
            (Some(i), false) => (i + n - 1) % n,
        };
        self.select(i)
    }

    pub fn toolbar(&mut self, ui: &mut egui::Ui) -> Action {
        let mut action = Action::default();
        let narrow = self.narrow || self.short;
        // On phones: files and examples, then the view buttons, then the
        // privacy line, each on its own row.
        let files = |ui: &mut egui::Ui, this: &mut Self, action: &mut Action| {
            if crate::app::CAN_CHOOSE_FILES
                && ui
                    .button("Choose files…")
                    .on_hover_text("Pick the two revisions (DXF) to compare")
                    .clicked()
            {
                action.choose = true;
            }
            let current = match (this.example, narrow) {
                (Some(_), true) => "Examples".to_owned(),
                (Some(id), false) => format!("Example: {}", crate::examples::find(id).title),
                (None, _) => "Examples".to_owned(),
            };
            egui::ComboBox::from_id_salt("example")
                .selected_text(current)
                .show_ui(ui, |ui| {
                    for e in &crate::examples::EXAMPLES {
                        if ui
                            .selectable_label(this.example == Some(e.id), e.title)
                            .clicked()
                        {
                            action.example = Some(e.id);
                        }
                    }
                })
                .response
                .on_hover_text("Built-in examples");
        };
        let privacy = |ui: &mut egui::Ui| {
            if crate::app::CAN_CHOOSE_FILES {
                ui.label(
                    egui::RichText::new(crate::app::PRIVACY)
                        .small()
                        .color(Color32::from_gray(160)),
                );
            }
        };
        let view = |ui: &mut egui::Ui, this: &mut Self, action: &mut Action| {
            let side = ui
                .selectable_label(this.side_by_side, "Side by side")
                .on_hover_text("Show both drawings next to each other");
            if side.clicked() {
                this.side_by_side = !this.side_by_side;
                this.used[1] = true;
            }
            this.anchors[1] = Some(side.rect);
            let details = ui.selectable_label(this.details, "Details").on_hover_text(
                "Exact values, uncertain items, what is not compared yet, layers and exports",
            );
            if details.clicked() {
                this.details = !this.details;
                this.used[2] = true;
            }
            this.anchors[2] = Some(details.rect);
            if this.details {
                ui.separator();
                if ui.button("Zoom extents").clicked() {
                    action.fit = true;
                }
                let json = ui.button("Export JSON");
                this.json_rect = Some(json.rect);
                if json.clicked() {
                    action.message = Some(this.export());
                }
                let export = ui
                    .button("Export PNG report")
                    .on_hover_text("The overlay and the change list as one image");
                this.export_rect = Some(export.rect);
                if export.clicked() {
                    this.start_report();
                    action.fit = true;
                }
            }
        };
        let end = |ui: &mut egui::Ui, action: &mut Action| {
            if ui
                .button("Close")
                .on_hover_text("Close this comparison")
                .clicked()
            {
                action.close = true;
            }
            if ui
                .button("Feedback")
                .on_hover_text("Opens a pre-filled issue on GitHub in your browser; nothing is sent until you submit it there")
                .clicked()
            {
                action.message = crate::feedback::open(ui.ctx());
            }
        };
        self.export_rect = None;
        // Recording a video: only the view buttons (they work as usual),
        // and the privacy line where a video is about it.
        if self.record {
            ui.horizontal_wrapped(|ui| view(ui, self, &mut action));
            if self.show_privacy && crate::app::CAN_CHOOSE_FILES {
                let line = ui.label(
                    egui::RichText::new(crate::app::PRIVACY)
                        .small()
                        .color(Color32::from_gray(200)),
                );
                self.privacy_rect = Some(line.rect);
            }
            return action;
        }
        if narrow {
            ui.horizontal_wrapped(|ui| files(ui, self, &mut action));
            ui.horizontal_wrapped(|ui| {
                view(ui, self, &mut action);
                ui.separator();
                end(ui, &mut action);
            });
            if !self.short {
                privacy(ui);
            }
        } else {
            ui.horizontal_wrapped(|ui| {
                files(ui, self, &mut action);
                privacy(ui);
                ui.separator();
                view(ui, self, &mut action);
                // With the Details buttons there may be no room on the
                // right: then Close and Feedback follow inline.
                if self.details {
                    ui.separator();
                    end(ui, &mut action);
                } else {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        end(ui, &mut action)
                    });
                }
            });
        }
        action
    }

    /// Layer 1: the summary line and the legend, with previous/next; for
    /// the user's own files, which is before and which after.
    pub fn summary_bar(&mut self, ui: &mut egui::Ui, action: &mut Action) -> Option<Bounds> {
        let mut zoom = None;
        ui.add_space(6.0);
        let has = !self.diff.changes.is_empty();
        let (next_key, previous_key) = ui.input(|i| {
            let plain = !i.modifiers.any();
            (
                plain && i.key_pressed(egui::Key::N),
                plain && i.key_pressed(egui::Key::P),
            )
        });
        // Previous then Next on screen; `rtl` adds them in reverse for a
        // right-to-left layout.
        let mut step = |ui: &mut egui::Ui, this: &mut Self, rtl: bool| {
            for forward in if rtl { [true, false] } else { [false, true] } {
                let (label, tip, key) = if forward {
                    ("Next ▶", "Zoom to the next change (N)", next_key)
                } else {
                    (
                        "◀ Previous",
                        "Zoom to the previous change (P)",
                        previous_key,
                    )
                };
                if ui
                    .add_enabled(has, egui::Button::new(label))
                    .on_hover_text(tip)
                    .clicked()
                    || (has && key)
                {
                    zoom = this.step(forward);
                }
            }
        };
        ui.horizontal_wrapped(|ui| {
            self.summary_rect = Some(
                ui.label(
                    egui::RichText::new(self.summary())
                        .size(if self.short || (self.record && self.narrow) {
                            16.0
                        } else {
                            20.0
                        })
                        .strong()
                        .color(Color32::WHITE),
                )
                .rect,
            );
            if let Some(markup) = self.diff.markup_summary().filter(|_| !self.record) {
                ui.label(egui::RichText::new(markup).color(Color32::from_gray(150)));
            }
            if self.record {
                // On its own row, so it reads (and is highlighted) as one.
                ui.end_row();
            } else {
                ui.add_space(16.0);
            }
            // A 9:16 video about the file names leaves the legend out.
            if !(self.record && self.narrow && self.show_files) {
                self.legend_rect = Some(legend(ui));
            }
            // Recording: no Previous / Next.
            if self.record {
                return;
            }
            if self.narrow || self.short {
                ui.add_space(8.0);
                step(ui, self, false);
            } else {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    step(ui, self, true);
                });
            }
        });
        if let Some(note) = self
            .example_note()
            .filter(|_| !self.short && !(self.record && self.narrow))
        {
            ui.label(
                egui::RichText::new(note)
                    .italics()
                    .color(Color32::from_gray(185)),
            );
        }
        // In a 9:16 recording the summary line names the files already.
        if self.example.is_none() && !(self.record && self.narrow && !self.show_files) {
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    egui::RichText::new(format!(
                        "Before: {}  →  After: {}",
                        short_name(&self.a_name),
                        short_name(&self.b_name)
                    ))
                    .color(Color32::from_gray(200)),
                );
                let swap = ui
                    .button("Swap")
                    .on_hover_text("Compare the other way round");
                self.swap_rect = Some(swap.rect);
                if swap.clicked() {
                    action.swap = true;
                }
            });
        }
        self.timing_rect = None;
        self.network_rect = None;
        if let Some(t) = &self.timing_line {
            self.timing_rect = Some(
                ui.label(egui::RichText::new(t).color(Color32::from_gray(185)))
                    .rect,
            );
        }
        if let Some(n) = &self.network_line {
            self.network_rect = Some(
                ui.label(egui::RichText::new(n).color(Color32::from_gray(185)))
                    .rect,
            );
        }
        for note in &self.notes {
            ui.label(egui::RichText::new(note).color(Color32::from_rgb(0xf5, 0xd0, 0x2a)));
        }
        ui.add_space(6.0);
        zoom
    }

    /// Shows the first hint not yet acted on, next to what it is about.
    pub fn hints(&self, ctx: &egui::Context) {
        if self.reporting() {
            return;
        }
        let Some(k) = (0..3).find(|&k| !self.used[k]) else {
            return;
        };
        let Some(anchor) = self.anchors[k] else {
            return;
        };
        // Only next to something that is on screen; on narrow screens only
        // the list hint (the others would cover the summary line).
        if !ctx.content_rect().contains_rect(anchor) || (self.narrow && k > 0) {
            return;
        }
        // The change-list hint sits to the left of the list; the others
        // in the toolbar. On narrow screens: above the list's first row,
        // under the toolbar button.
        let (pos, pivot) = if self.narrow {
            if k == 0 {
                (
                    anchor.center_top() - egui::vec2(0.0, 6.0),
                    Align2::CENTER_BOTTOM,
                )
            } else {
                (
                    anchor.center_bottom() + egui::vec2(0.0, 6.0),
                    Align2::CENTER_TOP,
                )
            }
        } else if k == 0 {
            (
                anchor.left_center() - egui::vec2(10.0, 0.0),
                Align2::RIGHT_CENTER,
            )
        } else {
            // After the Details button, so no button is covered.
            let right = self.anchors[2].map_or(anchor.right_center(), |d| d.right_center());
            (right + egui::vec2(10.0, 0.0), Align2::LEFT_CENTER)
        };
        egui::Area::new(egui::Id::new(("hint", k)))
            .order(egui::Order::Foreground)
            .fixed_pos(pos)
            .pivot(pivot)
            .interactable(false)
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(Color32::from_rgb(0x2b, 0x5f, 0xd9))
                    .corner_radius(6.0)
                    .inner_margin(egui::Margin::symmetric(10, 6))
                    .show(ui, |ui| {
                        ui.label(egui::RichText::new(HINTS[k]).color(Color32::WHITE).strong());
                    });
            });
    }

    /// Saves the JSON change report (next to drawing B, or downloaded on
    /// the web).
    fn export(&self) -> String {
        let json = self
            .diff
            .to_json(short_name(&self.a_name), short_name(&self.b_name));
        let name = crate::files::export_name(&self.b_name, "json");
        crate::files::save(&self.b_name, &name, json.as_bytes(), "application/json")
    }

    /// Starts the PNG report: the next frames show the report layout,
    /// then the window is captured.
    pub fn start_report(&mut self) {
        self.report = Report::Preparing(0);
    }

    pub fn reporting(&self) -> bool {
        self.report != Report::Off
    }

    /// Shows (or hides) the report layout without saving it (the demo).
    pub fn preview_report(&mut self, on: bool) {
        self.report = if on { Report::Preview } else { Report::Off };
    }

    /// Called once per report frame; true when it is time to capture.
    /// `new_frame`: false when the same frame is drawn again (a recording
    /// draws some frames more than once), which does not count.
    pub fn report_frame(&mut self, new_frame: bool) -> bool {
        match self.report {
            Report::Preparing(n) if n >= 3 => {
                self.report = Report::Capturing;
                true
            }
            Report::Preparing(n) if new_frame => {
                self.report = Report::Preparing(n + 1);
                false
            }
            _ => false,
        }
    }

    pub fn waiting_for_capture(&self) -> bool {
        self.report == Report::Capturing
    }

    /// Saves the captured report and leaves the report layout. `save`
    /// false (the live demo): made, but not downloaded onto a viewer's
    /// computer.
    pub fn finish_report(&mut self, image: &egui::ColorImage, save: bool) -> String {
        self.report = Report::Off;
        if !save {
            return format!(
                "Report made ({} × {}); the demo does not save it.",
                image.size[0], image.size[1]
            );
        }
        let rgba: Vec<u8> = image.pixels.iter().flat_map(|p| p.to_array()).collect();
        match crate::files::png(image.size[0], image.size[1], &rgba) {
            Ok(png) => {
                let name = crate::files::export_name(&self.b_name, "png");
                crate::files::save(&self.b_name, &name, &png, "image/png")
            }
            Err(e) => format!("PNG report: {e}"),
        }
    }

    /// The report's title bar.
    pub fn report_header(&self, ui: &mut egui::Ui) {
        ui.add_space(4.0);
        // Wrapped, so a narrow (9:16) report keeps every word.
        ui.horizontal_wrapped(|ui| {
            ui.heading(self.summary());
            if let Some(markup) = self.diff.markup_summary() {
                ui.label(egui::RichText::new(markup).color(Color32::from_gray(150)));
            }
        });
        if let Some(note) = self.example_note() {
            ui.label(egui::RichText::new(note).italics());
        }
        ui.horizontal_wrapped(|ui| {
            let _ = legend(ui);
            ui.separator();
            ui.label(format!(
                "{} to {}",
                short_name(&self.a_name),
                short_name(&self.b_name)
            ));
        });
        ui.add_space(4.0);
    }

    /// The change list for the report: no scrolling, as many changes as
    /// fit, then a count of the rest.
    pub fn report_list(&self, ui: &mut egui::Ui) {
        let d = &self.diff;
        ui.heading("What changed");
        ui.separator();
        // Room for the "not compared" note at the bottom.
        let reserve = 30.0 + 16.0 * d.not_compared.len() as f32;
        for (i, c) in d.changes.iter().enumerate() {
            if ui.available_height() < reserve + 40.0 {
                ui.label(format!(
                    "… and {} more (see the JSON report)",
                    d.changes.len() - i
                ));
                break;
            }
            ui.horizontal(|ui| {
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
                ui.painter().rect_filled(rect, 2.0, kind_color(c));
                let mut t = egui::RichText::new(&c.headline).strong();
                if c.markup {
                    t = t.color(Color32::from_gray(170));
                }
                // Wrapped to the report's column, so long headlines are not
                // cut off.
                ui.add(egui::Label::new(t).wrap());
            });
            if let Some(text) = block_refs(c) {
                ui.indent((i, "refs"), |ui| {
                    ui.small(egui::RichText::new(text).color(rgb(dview::CHANGED)))
                });
            }
        }
        ui.separator();
        ui.small(egui::RichText::new("Not compared yet:").strong());
        for n in &d.not_compared {
            ui.small(format!("• {n}"));
        }
    }

    pub fn layer_panel(&mut self, ui: &mut egui::Ui) {
        let doc = &mut self.overlay;
        let mut changed = false;
        ui.horizontal(|ui| {
            if ui.button("All on").clicked() {
                doc.visible.iter_mut().for_each(|v| *v = true);
                changed = true;
            }
            if ui.button("All off").clicked() {
                doc.visible.iter_mut().for_each(|v| *v = false);
                changed = true;
            }
        });
        egui::ScrollArea::vertical().show(ui, |ui| {
            for (i, layer) in doc.scene.layers.iter().enumerate() {
                changed |= ui.checkbox(&mut doc.visible[i], &layer.name).changed();
            }
        });
        if changed {
            // The same layers, by name, in the side-by-side panes.
            for side in [&mut self.left, &mut self.right] {
                for (i, layer) in side.scene.layers.iter().enumerate() {
                    if let Some(j) = self
                        .overlay
                        .scene
                        .layers
                        .iter()
                        .position(|l| lightable_dxf::names_equal(&l.name, &layer.name))
                    {
                        side.visible[i] = self.overlay.visible[j];
                    }
                }
            }
        }
    }

    /// Layer 2 (and 4–5 with details): the change list; returns where to
    /// zoom when a change is clicked.
    pub fn change_list(&mut self, ui: &mut egui::Ui) -> Option<Bounds> {
        let mut zoom = None;
        let details = self.details;
        let d = &self.diff;
        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            ui.heading("What changed");
            if !d.changes.is_empty() && !self.narrow && !self.record {
                ui.label(
                    egui::RichText::new("click a line to zoom · N / P keys step")
                        .small()
                        .color(Color32::from_gray(150)),
                );
            }
        });
        ui.add_space(4.0);
        let mut clicked = None;
        let mut first_row = None;
        let mut reveal = false;
        let mut tail_visible = None;
        let mut rows = Vec::new();
        let mut cut_rows = 0;
        let mut view = None;
        let mut not_compared = None;
        let mut redrawn = None;
        let bring = self.reveal_selected > 0;
        self.reveal_selected = self.reveal_selected.saturating_sub(1);
        // A short list (a 9:16 recording) scrolls within its panel instead
        // of running past the bottom (egui's default minimum is 64 points).
        egui::ScrollArea::vertical()
            .min_scrolled_height(24.0)
            .show(ui, |ui| {
            view = Some(ui.clip_rect());
            let markup_start = d.changes.iter().position(|c| c.markup);
            let design = d.design_changes().count();
            for (i, c) in d.changes.iter().enumerate() {
                if let Some(limit) = self.list_limit
                    && i >= limit
                {
                    // The rest as a real count: design changes not listed.
                    let rest = design.saturating_sub(limit);
                    if rest > 0 {
                        let tail = ui.label(
                            egui::RichText::new(format!("… and {rest} more"))
                                .size(15.0)
                                .color(Color32::from_gray(200)),
                        );
                        tail_visible = Some(ui.clip_rect().contains_rect(tail.rect));
                    }
                    break;
                }
                if Some(i) == markup_start {
                    ui.add_space(8.0);
                    ui.label(egui::RichText::new("Revision markup").color(Color32::from_gray(150)));
                }
                let selected = self.current == Some(i);
                let mut text = egui::RichText::new(&c.headline).size(15.0);
                if c.markup {
                    text = text.color(Color32::from_gray(170));
                }
                // Row by row on the stage: 0.15 s apart, each over 0.3 s.
                let alpha = self.reveal_age.map_or(1.0, |age| {
                    ((age - 0.15 * i as f64) / 0.3).clamp(0.0, 1.0) as f32
                });
                let response = ui
                    .horizontal(|ui| {
                        ui.multiply_opacity(alpha);
                        let (rect, _) =
                            ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
                        ui.painter().rect_filled(rect, 2.0, kind_color(c));
                        // Long headlines wrap, never cut off: laid out here
                        // so a cut-off row would be known (the recorder's
                        // check).
                        let width = ui.available_width() - 2.0 * ui.spacing().button_padding.x;
                        let galley = egui::WidgetText::from(text).into_galley(
                            ui,
                            Some(egui::TextWrapMode::Wrap),
                            width,
                            egui::TextStyle::Button,
                        );
                        let cut = galley.elided;
                        (ui.add(egui::Button::selectable(selected, galley)), cut)
                    })
                    .inner;
                let (response, cut) = response;
                if cut && ui.clip_rect().intersects(response.rect) {
                    cut_rows += 1;
                }
                if first_row.is_none() {
                    first_row = Some(response.rect);
                }
                rows.push((i, response.rect));
                if response.clicked() {
                    clicked = Some(i);
                }
                if details {
                    let mut notes = Vec::new();
                    if !c.shown {
                        notes.push("changed but not shown");
                    }
                    if !c.certain {
                        notes.push("uncertain");
                    }
                    if !c.geometry && c.kind == ChangeKind::Changed {
                        notes.push("properties only");
                    }
                    // Plain labels, arrows and units (the JSON keeps the
                    // raw details).
                    let (object, exact) =
                        lightable_diff::words::exact(c, d.units.as_deref());
                    ui.indent(i, |ui| {
                        ui.small(egui::RichText::new(object).color(Color32::from_gray(150)));
                        if !exact.is_empty() {
                            ui.small(exact);
                        }
                        if !notes.is_empty() {
                            ui.small(egui::RichText::new(notes.join(", ")).italics().color(Color32::GRAY));
                        }
                        if let Some(text) = block_refs(c) {
                            ui.small(egui::RichText::new(text).color(rgb(dview::CHANGED)));
                        }
                    });
                }
                // A newly selected row comes fully into view (for two
                // frames: in a recording it grows to its full headline).
                if selected && (response.clicked() || bring) {
                    response.scroll_to_me(None);
                }
            }
            if d.changes.is_empty() {
                ui.label("The two drawings are the same in everything compared.");
            }
            if !details && !d.not_compared.is_empty() {
                ui.add_space(10.0);
                let link = ui
                    .link(egui::RichText::new("What isn't compared?").small())
                    .on_hover_text("Opens Details at the list of what this comparison skips");
                not_compared = Some(link.rect);
                if link.clicked() {
                    reveal = true;
                }
            }
            if details {
                let not_shown = d.changes.iter().filter(|c| !c.shown).count();
                let uncertain = d.changes.iter().filter(|c| !c.certain).count();
                ui.separator();
                ui.small(format!(
                    "{} added, {} deleted, {} changed; {not_shown} not shown, {uncertain} uncertain",
                    d.count(ChangeKind::Added),
                    d.count(ChangeKind::Deleted),
                    d.count(ChangeKind::Changed)
                ));
                if !d.not_compared.is_empty() {
                    let shown = egui::CollapsingHeader::new(egui::RichText::new("Not compared yet").strong())
                        .default_open(true)
                        .show(ui, |ui| {
                            ui.small("These parts of the files are not compared, so differences there are not listed above:");
                            for n in &d.not_compared {
                                ui.small(format!("• {n}"));
                            }
                        });
                    // The demo scrolls here once when it opens Details.
                    if std::mem::take(&mut self.reveal_not_compared) {
                        shown.header_response.scroll_to_me(Some(egui::Align::TOP));
                    }
                }
                if !d.redrawn.is_empty() {
                    // A wrapping line (a collapsing header's title cannot
                    // wrap and would widen the panel), then the list.
                    ui.add_space(6.0);
                    let line = ui.label(format!(
                        "{} objects redrawn the same: not changes",
                        d.redrawn.len()
                    ));
                    redrawn = Some(line.rect);
                    if self.reveal_redrawn {
                        line.scroll_to_me(Some(egui::Align::Center));
                    }
                    egui::CollapsingHeader::new("Which objects")
                        .id_salt("redrawn")
                        .show(ui, |ui| {
                            ui.small("Drawn identically by other objects in the other file (for example a polyline saved as separate lines). Not counted as changes.");
                            for c in &d.redrawn {
                                ui.small(format!(
                                    "{} {}{}",
                                    if c.kind == ChangeKind::Deleted { "in A:" } else { "in B:" },
                                    c.entity_type,
                                    c.handle().map(|h| format!(" {h}")).unwrap_or_default()
                                ));
                            }
                        });
                }
            }
        });
        self.anchors[0] = first_row;
        self.list_tail_visible = tail_visible;
        self.row_rects = rows;
        self.cut_rows = cut_rows;
        self.list_rect = view;
        self.not_compared_rect = not_compared;
        self.redrawn_rect = redrawn;
        if reveal {
            self.details = true;
            self.used[2] = true;
            self.reveal_not_compared = true;
        }
        if let Some(i) = clicked {
            zoom = self.select(i);
        }
        zoom
    }

    pub fn status(&self) -> String {
        let d = &self.diff;
        let design = d.design_changes().count();
        let mut s = format!("{design} change{}", if design == 1 { "" } else { "s" });
        let markup = d.markup_count();
        if markup > 0 {
            s.push_str(&format!(
                " + {markup} revision markup item{}",
                if markup == 1 { "" } else { "s" }
            ));
        }
        s.push_str(&format!(" ({} unchanged", d.unchanged));
        if d.equivalent > 0 {
            s.push_str(&format!(", {} redrawn the same", d.equivalent));
        }
        s.push(')');
        if !d.handles_trusted {
            s.push_str("; matched by content (handles differ)");
        }
        if let Some(i) = self.current {
            s.push_str(&format!(" — change {} of {}", i + 1, d.changes.len()));
        }
        s
    }

    pub fn paint(&self, ui: &egui::Ui, panes: &[Rect], view: &View) {
        let docs: Vec<(&Doc, &str)> = if self.side_by_side && !self.reporting() {
            vec![
                (&self.left, self.a_label.as_str()),
                (&self.right, self.b_label.as_str()),
            ]
        } else {
            vec![(&self.overlay, "")]
        };
        let highlight = self
            .current
            .and_then(|i| self.diff.changes.get(i))
            .filter(|c| zoomable(c))
            .and_then(|c| c.bounds);
        for (&rect, (doc, label)) in panes.iter().zip(docs) {
            doc.paint(ui, rect, view);
            let painter = ui.painter_at(rect);
            if !label.is_empty() {
                painter.text(
                    rect.left_top() + egui::vec2(8.0, 6.0),
                    Align2::LEFT_TOP,
                    label,
                    FontId::proportional(16.0),
                    Color32::LIGHT_GRAY,
                );
            }
            if let Some(b) = highlight {
                let p0 = view.to_screen([b.min[0], b.max[1]]);
                let p1 = view.to_screen([b.max[0], b.min[1]]);
                let r = Rect::from_min_max(
                    egui::pos2(rect.min.x + p0[0] as f32, rect.min.y + p0[1] as f32),
                    egui::pos2(rect.min.x + p1[0] as f32, rect.min.y + p1[1] as f32),
                )
                .expand(6.0);
                // Thick enough to see in a small video: a white frame on a
                // dark halo, wider on bigger screens.
                let width = (rect.height() / 250.0).clamp(2.0, 6.0);
                painter.rect_stroke(
                    r.expand(width),
                    3.0 + width,
                    Stroke::new(width + 3.0, Color32::from_black_alpha(170)),
                    StrokeKind::Middle,
                );
                painter.rect_stroke(
                    r,
                    3.0,
                    Stroke::new(width, Color32::WHITE),
                    StrokeKind::Outside,
                );
            }
        }
        if panes.len() == 2 {
            // Between the panes: a column, or a row when stacked.
            let gap = if panes[1].min.y >= panes[0].max.y {
                Rect::from_min_max(
                    egui::pos2(panes[0].min.x, panes[0].max.y),
                    egui::pos2(panes[1].max.x, panes[1].min.y),
                )
            } else {
                Rect::from_min_max(
                    egui::pos2(panes[0].max.x, panes[0].min.y),
                    egui::pos2(panes[1].min.x, panes[1].max.y),
                )
            };
            ui.painter().rect_filled(gap, 0.0, Color32::from_gray(60));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn example(id: &str) -> Compare {
        let e = crate::examples::find(id);
        let read = |b: &[u8]| Arc::new(lightable_dxf::read_bytes(b).unwrap().0);
        Compare::new(
            None,
            (e.a_name.into(), read(e.a)),
            (e.b_name.into(), read(e.b)),
            Some((e.a_label.into(), e.b_label.into())),
            Some(e.id),
        )
        .unwrap()
    }

    /// The demo's captions for the bracket, from the comparison itself.
    #[test]
    fn the_demo_narrates_the_bracket_from_real_results() {
        let c = example("bracket");
        assert_eq!(c.design_count(), 10);
        assert_eq!(c.clouded_count(), 1);
        let picks: Vec<&str> = c.demo_picks().iter().map(|&i| c.headline(i)).collect();
        assert_eq!(picks.len(), 3);
        assert!(picks[0].starts_with("Circle diameter changed from 13.5 mm to 17.5 mm"));
        assert!(picks[1].starts_with("Dimension changed from"));
        assert_eq!(picks[2], "Text changed from 4x Ø13.5 THRU to 4x Ø17.5 THRU");
    }

    /// The cloud zoom takes in the revision triangle beside the cloud (the
    /// 9:16 demo clipped it).
    #[test]
    fn the_cloud_zoom_includes_the_revision_triangle() {
        let c = example("bracket");
        let zoom = c.cloud_bounds().unwrap();
        let triangle = c
            .diff
            .changes
            .iter()
            .filter(|x| x.markup && x.layer() == "REVISION" && !x.revision_cloud)
            .filter_map(|x| x.bounds)
            .collect::<Vec<_>>();
        assert_eq!(triangle.len(), 2, "triangle and its letter");
        for t in triangle {
            assert!(
                zoom.min[0] <= t.min[0]
                    && zoom.max[0] >= t.max[0]
                    && zoom.min[1] <= t.min[1]
                    && zoom.max[1] >= t.max[1],
                "{t:?} outside {zoom:?}"
            );
        }
        // The title block is far away and stays out.
        assert!(zoom.max[0] < 187.0, "{zoom:?}");
    }

    #[test]
    fn the_plan_has_no_cloud_and_still_has_picks() {
        let c = example("plan");
        assert!(c.cloud_bounds().is_none());
        assert_eq!(c.clouded_count(), 0);
        assert_eq!(c.demo_picks().len(), 3);
    }
}
