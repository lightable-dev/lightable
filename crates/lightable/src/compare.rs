//! The compare view: two drawings, their changes as an overlay or side by
//! side, a change list and a "next change" button. The comparison itself
//! is `lightable_diff`; this only displays it.

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
}

pub struct Compare {
    a_name: String,
    b_name: String,
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
}

/// The PNG report: shown for a few frames, then captured.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Report {
    Off,
    Preparing(u32),
    Capturing,
}

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

/// A change's one-line title.
fn title(c: &Change) -> String {
    let mut t = format!("{} {}", c.kind.as_str(), c.entity_type);
    if let Some(h) = c.handle() {
        t.push_str(&format!(" {h}"));
    }
    if !c.layer().is_empty() && c.place != Place::Layers {
        t.push_str(&format!(" · {}", c.layer()));
    }
    if c.place != Place::Model {
        t.push_str(&format!(" · {}", c.place));
    }
    t
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

impl Compare {
    pub fn new(
        gl: Option<&Arc<glow::Context>>,
        (a_name, a): (String, Arc<lightable_dxf::Drawing>),
        (b_name, b): (String, Arc<lightable_dxf::Drawing>),
    ) -> Result<Self, String> {
        let diff = lightable_diff::diff(&a, &b, &lightable_diff::DiffOptions::default());
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
        Ok(Self {
            a_name,
            b_name,
            diff,
            overlay,
            left,
            right,
            side_by_side: false,
            current: None,
            extent,
            gap: 4.0,
            report: Report::Off,
        })
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
        Some(dview::zoom_box(c.bounds?, self.extent, 0.05))
    }

    fn select(&mut self, i: usize) -> Option<Bounds> {
        self.current = Some(i);
        self.zoom_for(i)
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
        ui.horizontal(|ui| {
            ui.strong(short_name(&self.a_name));
            ui.label("vs");
            ui.strong(short_name(&self.b_name));
            ui.separator();
            ui.selectable_value(&mut self.side_by_side, false, "Overlay");
            ui.selectable_value(&mut self.side_by_side, true, "Side by side");
            ui.separator();
            let has = !self.diff.changes.is_empty();
            if ui
                .add_enabled(has, egui::Button::new("◀ Previous"))
                .clicked()
            {
                action.zoom = self.step(false);
            }
            if ui
                .add_enabled(has, egui::Button::new("Next change ▶"))
                .on_hover_text("Zoom to the next change (N)")
                .clicked()
                || (has && ui.input(|i| i.key_pressed(egui::Key::N)))
            {
                action.zoom = self.step(true);
            }
            if ui.button("Zoom extents").clicked() {
                action.fit = true;
            }
            ui.separator();
            if ui.button("Export JSON").clicked() {
                action.message = Some(self.export());
            }
            if ui
                .button("Export PNG report")
                .on_hover_text("The overlay and the change list as one image")
                .clicked()
            {
                self.start_report();
                action.fit = true;
            }
            if ui.button("Close compare").clicked() {
                action.close = true;
            }
            ui.separator();
            if ui
                .button("Feedback")
                .on_hover_text("Opens a pre-filled issue on GitHub in your browser; nothing is sent until you submit it there")
                .clicked()
            {
                action.message = crate::feedback::open(ui.ctx());
            }
        });
        action
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

    /// Called once per report frame; true when it is time to capture.
    pub fn report_frame(&mut self) -> bool {
        match self.report {
            Report::Preparing(n) if n >= 3 => {
                self.report = Report::Capturing;
                true
            }
            Report::Preparing(n) => {
                self.report = Report::Preparing(n + 1);
                false
            }
            _ => false,
        }
    }

    pub fn waiting_for_capture(&self) -> bool {
        self.report == Report::Capturing
    }

    /// Saves the captured report and leaves the report layout.
    pub fn finish_report(&mut self, image: &egui::ColorImage) -> String {
        self.report = Report::Off;
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
        ui.horizontal(|ui| {
            ui.heading(format!(
                "Changes from {} to {}",
                short_name(&self.a_name),
                short_name(&self.b_name)
            ));
        });
        ui.horizontal(|ui| {
            ui.colored_label(rgb(dview::ADDED), "■ added");
            ui.colored_label(rgb(dview::DELETED), "■ deleted");
            ui.colored_label(rgb(dview::CHANGED), "■ changed");
            ui.colored_label(rgb(dview::UNCHANGED), "■ unchanged");
            ui.separator();
            ui.label(self.status());
        });
        ui.add_space(4.0);
    }

    /// The change list for the report: no scrolling, as many changes as
    /// fit, then a count of the rest.
    pub fn report_list(&self, ui: &mut egui::Ui) {
        let d = &self.diff;
        ui.heading("Changes");
        ui.horizontal_wrapped(|ui| {
            ui.colored_label(
                rgb(dview::ADDED),
                format!("{} added", d.count(ChangeKind::Added)),
            );
            ui.colored_label(
                rgb(dview::DELETED),
                format!("{} deleted", d.count(ChangeKind::Deleted)),
            );
            ui.colored_label(
                rgb(dview::CHANGED),
                format!("{} changed", d.count(ChangeKind::Changed)),
            );
        });
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
                ui.label(egui::RichText::new(title(c)).strong());
            });
            let details: Vec<&str> = c
                .details
                .iter()
                .map(String::as_str)
                .filter(|d| *d != "not shown")
                .collect();
            if !details.is_empty() {
                ui.indent(i, |ui| ui.small(details.join("; ")));
            }
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

    /// The change list; returns where to zoom when a change is clicked.
    pub fn change_list(&mut self, ui: &mut egui::Ui) -> Option<Bounds> {
        let mut zoom = None;
        let d = &self.diff;
        ui.heading("Changes");
        ui.horizontal_wrapped(|ui| {
            ui.colored_label(
                rgb(dview::ADDED),
                format!("{} added", d.count(ChangeKind::Added)),
            );
            ui.colored_label(
                rgb(dview::DELETED),
                format!("{} deleted", d.count(ChangeKind::Deleted)),
            );
            ui.colored_label(
                rgb(dview::CHANGED),
                format!("{} changed", d.count(ChangeKind::Changed)),
            );
        });
        let not_shown = d.changes.iter().filter(|c| !c.shown).count();
        let uncertain = d.changes.iter().filter(|c| !c.certain).count();
        if not_shown + uncertain > 0 {
            ui.label(format!("{not_shown} not shown, {uncertain} uncertain"));
        }
        ui.separator();
        let mut clicked = None;
        egui::ScrollArea::vertical().show(ui, |ui| {
            for (i, c) in d.changes.iter().enumerate() {
                let selected = self.current == Some(i);
                let title = title(c);
                let response = ui
                    .horizontal(|ui| {
                        let (rect, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
                        ui.painter().rect_filled(rect, 2.0, kind_color(c));
                        ui.selectable_label(selected, title)
                    })
                    .inner;
                if response.clicked() {
                    clicked = Some(i);
                }
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
                let details: Vec<&str> = c.details.iter().map(String::as_str).filter(|d| *d != "not shown").collect();
                ui.indent(i, |ui| {
                    if !details.is_empty() {
                        ui.small(details.join("; "));
                    }
                    if !notes.is_empty() {
                        ui.small(egui::RichText::new(notes.join(", ")).italics().color(Color32::GRAY));
                    }
                    if let Some(text) = block_refs(c) {
                        ui.small(egui::RichText::new(text).color(rgb(dview::CHANGED)));
                    }
                });
                if selected && response.clicked() {
                    response.scroll_to_me(None);
                }
            }
            if !d.not_compared.is_empty() {
                ui.separator();
                egui::CollapsingHeader::new(egui::RichText::new("Not compared yet").strong())
                    .default_open(true)
                    .show(ui, |ui| {
                        ui.small("These parts of the files are not compared, so differences there are not listed above:");
                        for n in &d.not_compared {
                            ui.small(format!("• {n}"));
                        }
                    });
            }
            if !d.redrawn.is_empty() {
                ui.separator();
                egui::CollapsingHeader::new(format!("Same drawing, other entities ({})", d.redrawn.len()))
                    .show(ui, |ui| {
                        ui.small("Drawn identically by other entities in the other file (for example a polyline saved as separate lines). Not counted as changes.");
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
        });
        if let Some(i) = clicked {
            zoom = self.select(i);
        }
        zoom
    }

    pub fn status(&self) -> String {
        let d = &self.diff;
        let mut s = format!("{} changes ({} unchanged", d.changes.len(), d.unchanged);
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
            vec![(&self.left, "A (before)"), (&self.right, "B (after)")]
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
                    FontId::proportional(13.0),
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
                painter.rect_stroke(
                    r,
                    3.0,
                    Stroke::new(1.5, Color32::WHITE),
                    StrokeKind::Outside,
                );
            }
        }
        if panes.len() == 2 {
            let gap = Rect::from_min_max(
                egui::pos2(panes[0].max.x, panes[0].min.y),
                egui::pos2(panes[1].min.x, panes[1].max.y),
            );
            ui.painter().rect_filled(gap, 0.0, Color32::from_gray(60));
        }
    }
}
