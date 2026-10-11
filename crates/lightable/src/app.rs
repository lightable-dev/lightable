//! The viewer: a canvas with pan and zoom, a layer panel and a status bar.

use std::sync::{Arc, Mutex};

use eframe::egui::{
    self, Align2, Color32, FontId, PointerButton, Pos2, Rect, Sense, epaint::TextShape,
};
use eframe::{egui_glow, glow};

use lightable_render::{HAlign, Scene, Shape, VAlign, View, tess};

use crate::compare::Compare;
use crate::gpu::{DrawStats, GpuScene};

/// Grid size for GPU chunks (per layer): finer means better culling and
/// precision, coarser means fewer draw calls.
const GRID: usize = 16;
/// Text smaller than this on screen is not drawn.
const MIN_TEXT_PX: f64 = 4.0;
/// At most this many texts per frame (zoomed far out over dense text).
const MAX_TEXTS: usize = 5000;
pub const BACKGROUND: Color32 = Color32::from_rgb(0x21, 0x21, 0x21);

pub(crate) struct Doc {
    pub(crate) name: String,
    pub(crate) scene: Arc<Scene>,
    /// Per chunk: layer, bounds and the text items in it.
    texts: Vec<(u32, Option<lightable_render::Bounds>, Vec<u32>)>,
    gpu: Option<Arc<Mutex<GpuScene>>>,
    pub(crate) visible: Vec<bool>,
    warnings: usize,
    stats: Arc<Mutex<DrawStats>>,
    vertices: usize,
    /// The drawing, for comparing it with another one.
    pub(crate) drawing: Option<Arc<lightable_dxf::Drawing>>,
}

impl Doc {
    /// Tessellates `scene` and uploads it (when there is a GL context).
    pub(crate) fn new(
        gl: Option<&Arc<glow::Context>>,
        name: String,
        scene: Scene,
        warnings: usize,
    ) -> Result<Self, String> {
        let geometry = tess::tessellate(&scene, GRID);
        let gpu = match gl.map(|gl| GpuScene::new(gl, &geometry)) {
            Some(Ok(g)) => Some(Arc::new(Mutex::new(g))),
            Some(Err(e)) => return Err(format!("graphics error: {e}")),
            None => None,
        };
        let texts = geometry
            .chunks
            .iter()
            .filter(|c| !c.texts.is_empty())
            .map(|c| (c.layer, c.bounds, c.texts.clone()))
            .collect();
        Ok(Doc {
            name,
            visible: scene.default_visibility(),
            vertices: geometry.vertex_count(),
            scene: Arc::new(scene),
            texts,
            gpu,
            warnings,
            stats: Arc::default(),
            drawing: None,
        })
    }

    /// Frees the GPU copy.
    pub(crate) fn destroy(&self, gl: Option<&Arc<glow::Context>>) {
        if let (Some(gl), Some(gpu)) = (gl, &self.gpu) {
            gpu.lock().map(|g| g.destroy(gl)).ok();
        }
    }

    /// Draws the drawing in `rect` (screen points) with `view`.
    pub(crate) fn paint(&self, ui: &egui::Ui, rect: Rect, view: &View) {
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, BACKGROUND);
        if let Some(gpu) = &self.gpu {
            let gpu = gpu.clone();
            let view = *view;
            let visible = self.visible.clone();
            let stats = self.stats.clone();
            let ppp = ui.ctx().pixels_per_point();
            let callback = egui_glow::CallbackFn::new(move |_info, painter| {
                if let Ok(gpu) = gpu.lock() {
                    let s = gpu.draw(painter.gl(), &view, &visible, ppp);
                    if let Ok(mut out) = stats.lock() {
                        *out = s;
                    }
                }
            });
            painter.add(egui::PaintCallback {
                rect,
                callback: Arc::new(callback),
            });
        }
        draw_texts(&painter, rect, view, self);
    }
}

/// Animated pan/zoom for `--bench`, reporting frame times at the end.
pub struct Bench {
    pub frames: usize,
    done: usize,
    times: Vec<f64>,
    #[cfg(not(target_arch = "wasm32"))]
    shot: Option<String>,
}

impl Bench {
    #[cfg(not(target_arch = "wasm32"))]
    pub fn new(frames: usize) -> Self {
        Self {
            frames,
            done: 0,
            times: Vec::new(),
            shot: None,
        }
    }

    /// Instead of animating: after a few frames, save a screenshot of the
    /// window (binary PPM) and quit.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn screenshot(path: String) -> Self {
        Self {
            frames: 0,
            done: 0,
            times: Vec::new(),
            shot: Some(path),
        }
    }
}

/// How the app starts.
#[derive(Default)]
pub struct Start {
    /// Files to open (two are compared).
    pub files: Vec<(String, Vec<u8>)>,
    /// A built-in example to open; with no files and no example, the
    /// default example opens.
    pub example: Option<String>,
    pub bench: Option<Bench>,
    /// Open the comparison side by side (`--side`).
    pub side_by_side: bool,
    /// Save the PNG report of the first comparison and quit (`--report`).
    pub report_and_quit: bool,
    /// Play the demo sequence (`--demo`, `?demo`).
    pub demo: bool,
}

/// A smooth move of the view.
#[derive(Clone, Copy)]
struct Anim {
    from: View,
    to: View,
    start: f64,
    seconds: f64,
}

/// The demo sequence (`?demo`): about 30 seconds with captions, then an end
/// card. Times in seconds from the start.
struct Demo {
    start: Option<f64>,
    /// The last step applied.
    step: usize,
}

/// Start time of each demo step; the end card stays after the last.
const DEMO_STEPS: [f64; 10] = [0.0, 3.5, 7.0, 10.5, 13.5, 16.5, 19.5, 23.0, 26.5, 30.0];

/// A file read in the background: name and bytes.
type FileSlot = Arc<Mutex<Vec<(usize, String, Vec<u8>)>>>;

pub struct CadApp {
    doc: Option<Doc>,
    view: View,
    fit_pending: bool,
    message: Option<String>,
    frame_times: Vec<f64>,
    bench: Option<Bench>,
    pending: Vec<(String, Vec<u8>)>,
    /// Files read in the background (the web reads dropped files
    /// asynchronously): the drop's position and the file.
    incoming: FileSlot,
    /// How many dropped files are still being read.
    awaiting: usize,
    compare: Option<Compare>,
    /// The next file received is compared with the open drawing.
    compare_next: bool,
    /// Start compare views side by side (`--side`).
    side_by_side: bool,
    /// `--report`: save the PNG report of the first comparison and quit.
    report_and_quit: bool,
    /// An example to open on the next frame.
    example_pending: Option<String>,
    anim: Option<Anim>,
    /// A demo zoom to start once this frame's pane size is known (a demo
    /// step can switch side by side or Details, which resizes the panes).
    demo_zoom: Option<(lightable_render::Bounds, f64)>,
    /// The view shows everything and nobody has moved it since: when the
    /// panes change size (the change list settling its width, Side by
    /// side), it is fitted again.
    auto_fit: bool,
    demo: Option<Demo>,
    /// The demo caption shown now.
    caption: Option<String>,
    /// The demo's end card is showing.
    end_card: bool,
}

/// egui's two text fonts without its emoji fonts (about 740 kB of the web
/// download). Nothing in the app needs them: every character the app and
/// the egui widgets it uses draw is in Ubuntu-Light or Hack. Symbols only
/// the emoji fonts had (☑ ✔ ★ ⚠ …) now draw as egui's replacement glyph.
fn fonts() -> egui::FontDefinitions {
    use egui::{FontData, FontFamily};
    let mut f = egui::FontDefinitions::empty();
    for (name, data) in [
        ("Ubuntu-Light", epaint_default_fonts::UBUNTU_LIGHT),
        ("Hack", epaint_default_fonts::HACK_REGULAR),
    ] {
        f.font_data
            .insert(name.to_owned(), Arc::new(FontData::from_static(data)));
    }
    // Hack as a fallback: it has the ◀ ▶ of the Previous / Next buttons.
    f.families.insert(
        FontFamily::Proportional,
        vec!["Ubuntu-Light".to_owned(), "Hack".to_owned()],
    );
    f.families.insert(
        FontFamily::Monospace,
        vec!["Hack".to_owned(), "Ubuntu-Light".to_owned()],
    );
    f
}

impl CadApp {
    pub fn new(cc: &eframe::CreationContext<'_>, start: Start) -> Self {
        cc.egui_ctx.set_theme(egui::ThemePreference::Dark);
        cc.egui_ctx.set_fonts(fonts());
        let Start {
            files,
            example,
            bench,
            side_by_side,
            report_and_quit,
            demo,
        } = start;
        // First run, nothing to open: show the default example at once, so
        // the first thing on screen is a comparison.
        let example = if files.is_empty() && (example.is_some() || bench.is_none() || demo) {
            Some(example.unwrap_or_default())
        } else {
            example
        };
        Self {
            doc: None,
            view: View::new([800.0, 600.0]),
            fit_pending: true,
            message: None,
            frame_times: Vec::new(),
            bench,
            pending: files,
            incoming: Arc::default(),
            awaiting: 0,
            compare: None,
            compare_next: false,
            side_by_side,
            report_and_quit,
            example_pending: example,
            anim: None,
            demo_zoom: None,
            auto_fit: false,
            demo: demo.then_some(Demo {
                start: None,
                step: usize::MAX,
            }),
            caption: None,
            end_card: false,
        }
    }

    /// Opens built-in example `id` (the default one if unknown).
    fn open_example(&mut self, gl: Option<&Arc<glow::Context>>, id: &str) {
        let e = crate::examples::find(id);
        let read = |name: &str, bytes: &[u8]| {
            lightable_dxf::read_bytes(bytes)
                .map(|(d, _)| (name.to_owned(), Arc::new(d)))
                .map_err(|err| format!("{name}: {err}"))
        };
        match (read(e.a_name, e.a), read(e.b_name, e.b)) {
            (Ok(a), Ok(b)) => self.start_compare(
                gl,
                a,
                b,
                Some((e.a_label.to_owned(), e.b_label.to_owned())),
                Some(e.id),
            ),
            (Err(err), _) | (_, Err(err)) => self.message = Some(err),
        }
    }

    /// Moves the view smoothly to show `bounds`.
    fn animate_to(
        &mut self,
        bounds: &lightable_render::Bounds,
        margin: f64,
        now: f64,
        seconds: f64,
    ) {
        self.auto_fit = false;
        let mut to = self.view;
        to.fit(bounds, margin);
        self.anim = Some(Anim {
            from: self.view,
            to,
            start: now,
            seconds,
        });
    }

    /// Advances a running view animation.
    fn step_anim(&mut self, now: f64) -> bool {
        let Some(a) = self.anim else { return false };
        let t = ((now - a.start) / a.seconds).clamp(0.0, 1.0);
        let e = t * t * (3.0 - 2.0 * t);
        self.view.center = [
            a.from.center[0] + (a.to.center[0] - a.from.center[0]) * e,
            a.from.center[1] + (a.to.center[1] - a.from.center[1]) * e,
        ];
        self.view.scale = (a.from.scale.ln() + (a.to.scale.ln() - a.from.scale.ln()) * e).exp();
        if t >= 1.0 {
            self.anim = None;
        }
        true
    }

    fn load(&mut self, gl: Option<&Arc<glow::Context>>, name: String, bytes: &[u8]) {
        let (drawing, warnings) = match lightable_dxf::read_bytes(bytes) {
            Ok(r) => r,
            Err(e) => {
                self.message = Some(format!("{name}: {e}"));
                return;
            }
        };
        let scene = lightable_render::build(&drawing);
        if let Some(old) = self.doc.take() {
            old.destroy(gl);
        }
        if let Some(old) = self.compare.take() {
            old.destroy(gl);
        }
        match Doc::new(gl, name, scene, warnings.len()) {
            Ok(mut doc) => {
                doc.drawing = Some(Arc::new(drawing));
                self.doc = Some(doc);
                self.message = None;
            }
            Err(e) => self.message = Some(e),
        }
        self.fit_pending = true;
    }

    /// Opens the compare view: `a` (before) against `b` (after).
    fn start_compare(
        &mut self,
        gl: Option<&Arc<glow::Context>>,
        a: (String, Arc<lightable_dxf::Drawing>),
        b: (String, Arc<lightable_dxf::Drawing>),
        labels: Option<(String, String)>,
        example: Option<&'static str>,
    ) {
        if let Some(old) = self.compare.take() {
            old.destroy(gl);
        }
        match Compare::new(gl, a, b, labels, example) {
            Ok(c) => {
                self.compare = Some(c);
                self.message = None;
                self.fit_pending = true;
            }
            Err(e) => self.message = Some(e),
        }
    }

    /// Files that arrived (dropped, or from the command line). Two files
    /// are compared; one is opened, or compared with the open drawing
    /// after "Compare with…".
    fn receive(&mut self, gl: Option<&Arc<glow::Context>>, mut files: Vec<(String, Vec<u8>)>) {
        let read = |(name, bytes): (String, Vec<u8>)| match lightable_dxf::read_bytes(&bytes) {
            Ok((d, _)) => Ok((name, Arc::new(d))),
            Err(e) => Err(format!("{name}: {e}")),
        };
        if files.len() >= 2 {
            files.truncate(2);
            let b = files.pop().map(read);
            let a = files.pop().map(read);
            match (a, b) {
                (Some(Ok(a)), Some(Ok(b))) => self.start_compare(gl, a, b, None, None),
                (Some(Err(e)), _) | (_, Some(Err(e))) => self.message = Some(e),
                _ => {}
            }
            return;
        }
        let Some(file) = files.pop() else { return };
        let current = self
            .doc
            .as_ref()
            .and_then(|d| Some((d.name.clone(), d.drawing.clone()?)));
        if self.compare_next
            && let Some(a) = current
        {
            self.compare_next = false;
            match read(file) {
                Ok(b) => self.start_compare(gl, a, b, None, None),
                Err(e) => self.message = Some(e),
            }
            return;
        }
        self.load(gl, file.0, &file.1);
    }

    fn fit(&mut self) {
        self.auto_fit = true;
        if let Some(c) = &self.compare {
            if let Some(b) = c.bounds() {
                self.view.fit(&b, 0.03);
            }
            return;
        }
        if let Some(doc) = &self.doc
            && let Some(b) = doc.scene.bounds(Some(&doc.visible))
        {
            self.view.fit(&b, 0.03);
        }
    }

    fn toolbar(&mut self, ui: &mut egui::Ui, gl: Option<&Arc<glow::Context>>) {
        if self.compare.is_some() {
            let mut close = false;
            let mut fit = false;
            let mut example = None;
            let now = ui.input(|i| i.time);
            if let Some(c) = &mut self.compare {
                let action = c.toolbar(ui);
                close = action.close;
                fit = action.fit;
                example = action.example;
                if let Some(b) = action.zoom {
                    self.animate_to(&b, 0.15, now, 0.6);
                }
                if let Some(m) = action.message {
                    self.message = Some(m);
                }
            }
            if close && let Some(c) = self.compare.take() {
                c.destroy(gl);
            }
            if fit {
                self.fit();
            }
            if let Some(id) = example {
                self.open_example(gl, id);
            }
            return;
        }
        let mut example = false;
        ui.horizontal(|ui| {
            example = ui
                .button("Try the example")
                .on_hover_text("Compare two revisions of a sample drawing (built in, made for this app)")
                .clicked();
            ui.separator();
            if self.doc.as_ref().is_some_and(|d| d.drawing.is_some()) {
                let label = if self.compare_next {
                    "Drop the newer drawing…"
                } else {
                    "Compare with…"
                };
                if ui
                    .selectable_label(self.compare_next, label)
                    .on_hover_text("Then drop the drawing to compare this one with. Dropping two files at once also compares them.")
                    .clicked()
                {
                    self.compare_next = !self.compare_next;
                }
                ui.separator();
            }
            match &self.doc {
                Some(doc) => ui.strong(&doc.name),
                None => ui.label("Drop a DXF file here (or pass one on the command line)"),
            };
            if ui.button("Zoom extents").clicked() {
                self.fit();
            }
            if let Some(doc) = &self.doc {
                let stats = doc.stats.lock().map(|s| *s).unwrap_or_default();
                ui.separator();
                ui.label(format!(
                    "{} items, {} vertices",
                    doc.scene.items.len(),
                    doc.vertices
                ));
                ui.label(format!(
                    "drawn: {} chunks, {} vertices",
                    stats.chunks, stats.vertices
                ));
                if doc.warnings > 0 {
                    ui.label(format!("{} read warnings", doc.warnings));
                }
            }
            ui.separator();
            if ui
                .button("Feedback")
                .on_hover_text("Opens a pre-filled issue on GitHub in your browser; nothing is sent until you submit it there")
                .clicked()
            {
                self.message = crate::feedback::open(ui.ctx());
            }
            if !self.frame_times.is_empty() {
                let avg = self.frame_times.iter().sum::<f64>() / self.frame_times.len() as f64;
                ui.separator();
                ui.label(format!("{:.1} ms/frame", avg * 1000.0));
            }
        });
        if example {
            self.open_example(gl, "");
        }
    }

    fn layer_panel(&mut self, ui: &mut egui::Ui) {
        if let Some(c) = &mut self.compare {
            c.layer_panel(ui);
            return;
        }
        let Some(doc) = &mut self.doc else {
            ui.label("No drawing");
            return;
        };
        ui.horizontal(|ui| {
            if ui.button("All on").clicked() {
                doc.visible.iter_mut().for_each(|v| *v = true);
            }
            if ui.button("All off").clicked() {
                doc.visible.iter_mut().for_each(|v| *v = false);
            }
        });
        egui::ScrollArea::vertical().show(ui, |ui| {
            for (i, layer) in doc.scene.layers.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.checkbox(&mut doc.visible[i], "");
                    let (rect, _) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), Sense::hover());
                    let c = layer.color;
                    ui.painter()
                        .rect_filled(rect, 2.0, Color32::from_rgb(c.0, c.1, c.2));
                    ui.label(&layer.name);
                });
            }
        });
    }

    fn canvas(&mut self, ui: &mut egui::Ui) {
        let (rect, response) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
        // Side by side: two panes share one view.
        let panes = match &self.compare {
            Some(c) => c.panes(rect),
            None => vec![rect],
        };
        let pane = panes[0];
        let size = [f64::from(pane.width()), f64::from(pane.height())];
        let resized = size != self.view.size;
        self.view.size = size;
        if resized && self.auto_fit && self.anim.is_none() && rect.width() > 0.0 {
            self.fit();
        }
        if let Some((b, margin)) = self.demo_zoom.take() {
            let now = ui.input(|i| i.time);
            self.animate_to(&b, margin, now, 1.2);
        }
        let has_content = self.doc.is_some() || self.compare.is_some();
        if self.fit_pending && has_content && rect.width() > 0.0 {
            self.fit();
            self.fit_pending = false;
        }
        // Pointer position within the pane it is over.
        let local = |p: Pos2| {
            let r = panes.iter().find(|r| r.contains(p)).unwrap_or(&pane);
            [f64::from(p.x - r.min.x), f64::from(p.y - r.min.y)]
        };
        if response.dragged_by(PointerButton::Middle) || response.dragged_by(PointerButton::Primary)
        {
            let d = response.drag_delta();
            self.view.pan(f64::from(d.x), f64::from(d.y));
            self.auto_fit = false;
        }
        if response.double_clicked_by(PointerButton::Middle) {
            self.fit();
        }
        if let Some(pos) = response.hover_pos() {
            let (scroll, pinch) = ui.input(|i| (i.smooth_scroll_delta.y, i.zoom_delta()));
            let factor = f64::from(scroll * 0.0025).exp() * f64::from(pinch);
            if factor != 1.0 {
                self.view.zoom_at(local(pos), factor);
                self.auto_fit = false;
            }
        }
        if let Some(c) = &self.compare {
            c.paint(ui, &panes, &self.view);
            return;
        }
        let Some(doc) = &self.doc else {
            ui.painter_at(rect).rect_filled(rect, 0.0, BACKGROUND);
            return;
        };
        doc.paint(ui, rect, &self.view);
    }
}

/// Where the text's alignment point lies in its laid-out box (unrotated,
/// from the top left): left/centre/right, and top/middle/bottom or the
/// font's real baseline.
fn anchor_in_galley(t: &lightable_render::TextShape, galley: &egui::Galley) -> egui::Vec2 {
    let r = galley.rect;
    let x = match t.halign {
        HAlign::Left => r.min.x,
        HAlign::Center => r.center().x,
        HAlign::Right => r.max.x,
    };
    let baseline = galley
        .rows
        .first()
        .and_then(|row| row.row.glyphs.first().map(|g| row.pos.y + g.pos.y))
        .unwrap_or(r.min.y + r.height() * 0.8);
    let y = match t.valign {
        VAlign::Top => r.min.y,
        VAlign::Middle => r.center().y,
        VAlign::Bottom => r.max.y,
        VAlign::Baseline => baseline,
    };
    egui::vec2(x, y)
}

/// Draws the texts that are on screen and large enough to read.
pub(crate) fn draw_texts(painter: &egui::Painter, rect: Rect, view: &View, doc: &Doc) {
    let shown = view.world_bounds();
    let mut count = 0;
    for (layer, bounds, items) in &doc.texts {
        if !doc.visible.get(*layer as usize).copied().unwrap_or(true)
            || bounds.is_some_and(|b| !b.intersects(&shown))
        {
            continue;
        }
        for &i in items {
            let item = &doc.scene.items[i as usize];
            let Shape::Text(t) = &item.shape else {
                continue;
            };
            let px = t.height * view.scale;
            if px < MIN_TEXT_PX {
                continue;
            }
            if item.shape.bounds().is_some_and(|b| !b.intersects(&shown)) {
                continue;
            }
            let color = Color32::from_rgb(item.color.0, item.color.1, item.color.2);
            // CAD text height is the cap height, about 0.7 of the font size.
            let size = (px / 0.7).min(2000.0) as f32;
            let galley = painter.layout_no_wrap(t.text.clone(), FontId::proportional(size), color);
            let p = view.to_screen(t.at);
            let at = Pos2::new(rect.min.x + p[0] as f32, rect.min.y + p[1] as f32);
            // egui places a text by its top-left corner and rotates it
            // around that corner; put the alignment point on `at` and turn
            // around it.
            let angle = -t.rotation as f32;
            let a = anchor_in_galley(t, &galley);
            let pos = at - egui::emath::Rot2::from_angle(angle) * a;
            painter.add(TextShape::new(pos, galley, color).with_angle(angle));
            count += 1;
            if count >= MAX_TEXTS {
                return;
            }
        }
    }
}

impl eframe::App for CadApp {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let dt = ui.input(|i| f64::from(i.unstable_dt));
        self.frame_times.push(dt);
        if self.frame_times.len() > 60 {
            self.frame_times.remove(0);
        }
        if !self.pending.is_empty() {
            let files = std::mem::take(&mut self.pending);
            self.receive(frame.gl(), files);
            if self.side_by_side
                && let Some(c) = &mut self.compare
            {
                c.side_by_side = true;
            }
        }
        let dropped = ui.input(|i| i.raw.dropped_files.clone());
        #[cfg(not(target_arch = "wasm32"))]
        if !dropped.is_empty() {
            let mut files = Vec::new();
            for file in dropped {
                let name = file.path().display().to_string();
                match file.bytes() {
                    Ok(bytes) => files.push((name, bytes)),
                    Err(e) => self.message = Some(format!("{name}: {e}")),
                }
            }
            self.receive(frame.gl(), files);
        }
        #[cfg(target_arch = "wasm32")]
        {
            // Browsers read files asynchronously; the bytes arrive in a
            // later frame through `incoming`.
            if !dropped.is_empty() {
                self.awaiting = dropped.len();
                if let Ok(mut slot) = self.incoming.lock() {
                    slot.clear();
                }
            }
            for (order, file) in dropped.into_iter().enumerate() {
                let name = file.path().display().to_string();
                let incoming = self.incoming.clone();
                let ctx = ui.ctx().clone();
                wasm_bindgen_futures::spawn_local(async move {
                    let bytes = file.bytes_async().await.unwrap_or_default();
                    if let Ok(mut slot) = incoming.lock() {
                        slot.push((order, name, bytes));
                        ctx.request_repaint();
                    }
                });
            }
        }
        let arrived = self.incoming.lock().ok().and_then(|mut slot| {
            (self.awaiting > 0 && slot.len() >= self.awaiting).then(|| std::mem::take(&mut *slot))
        });
        if let Some(mut files) = arrived {
            self.awaiting = 0;
            files.sort_by_key(|f| f.0);
            let files = files.into_iter().map(|(_, n, b)| (n, b)).collect();
            self.receive(frame.gl(), files);
        }

        let gl = frame.gl().cloned();
        if let Some(id) = self.example_pending.take() {
            self.open_example(gl.as_ref(), &id);
            if self.side_by_side
                && let Some(c) = &mut self.compare
            {
                c.side_by_side = true;
            }
        }
        let now = ui.input(|i| i.time);
        if self.step_anim(now) {
            ui.ctx().request_repaint();
        }
        if self.demo.is_some() {
            self.run_demo(ui, now);
        }
        if self.report_and_quit
            && let Some(c) = &mut self.compare
            && !c.reporting()
        {
            if self
                .message
                .as_deref()
                .is_some_and(|m| m.starts_with("Saved") || m.contains(".png"))
            {
                println!("{}", self.message.as_deref().unwrap_or_default());
                self.report_and_quit = false;
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            } else {
                c.start_report();
                self.fit_pending = true;
            }
        }
        if self.compare.as_ref().is_some_and(|c| c.reporting()) {
            self.report(ui);
            self.overlays(ui);
            return;
        }
        egui::Panel::top("toolbar").show(ui, |ui| self.toolbar(ui, gl.as_ref()));
        let details = self.compare.as_ref().is_none_or(|c| c.details);
        if let Some(c) = &mut self.compare {
            let mut zoom = None;
            egui::Panel::top("summary").show(ui, |ui| zoom = c.summary_bar(ui));
            egui::Panel::right("changes")
                .resizable(true)
                .default_size(380.0)
                .show(ui, |ui| {
                    if let Some(b) = c.change_list(ui) {
                        zoom = Some(b);
                    }
                });
            if let Some(b) = zoom {
                self.animate_to(&b, 0.15, now, 0.6);
            }
        }
        // The status bar is a detail, except to report an export or a
        // problem.
        if details || self.message.is_some() {
            egui::Panel::bottom("status").show(ui, |ui| {
                ui.horizontal(|ui| {
                    if let Some(m) = &self.message {
                        // Exports report where the file went; anything else is a problem.
                        let ok = m.starts_with("Saved ") || m.starts_with("Downloaded ");
                        let color = if ok {
                            Color32::LIGHT_GREEN
                        } else {
                            Color32::LIGHT_RED
                        };
                        ui.colored_label(color, m);
                    }
                    if let Some(c) = &self.compare {
                        ui.label(c.status());
                    } else if let Some(doc) = &self.doc
                        && !doc.scene.skipped.is_empty()
                    {
                        let skipped: Vec<String> = doc
                            .scene
                            .skipped
                            .iter()
                            .map(|(k, n)| format!("{n} {k}"))
                            .collect();
                        ui.label(format!("not drawn: {}", skipped.join(", ")));
                    }
                    if details && let Some(p) = ui.input(|i| i.pointer.hover_pos()) {
                        let w = self.view.to_world([f64::from(p.x), f64::from(p.y)]);
                        ui.label(format!("{:.4}, {:.4}", w[0], w[1]));
                    }
                });
            });
        }
        if details {
            egui::Panel::left("layers")
                .resizable(true)
                .default_size(200.0)
                .show(ui, |ui| {
                    ui.heading("Layers");
                    self.layer_panel(ui);
                });
        }
        egui::CentralPanel::no_frame().show(ui, |ui| self.canvas(ui));
        if let Some(c) = &self.compare
            && self.demo.is_none()
        {
            c.hints(ui.ctx());
        }
        self.overlays(ui);

        #[cfg(not(target_arch = "wasm32"))]
        if let Some(bench) = &mut self.bench
            && let Some(path) = bench.shot.clone()
        {
            ui.ctx().request_repaint();
            bench.done += 1;
            if bench.done == 5 {
                ui.ctx()
                    .send_viewport_cmd(
                        egui::ViewportCommand::Screenshot(egui::UserData::default()),
                    );
            }
            let image = ui.input(|i| {
                i.raw.events.iter().find_map(|e| match e {
                    egui::Event::Screenshot { image, .. } => Some(image.clone()),
                    _ => None,
                })
            });
            if let Some(image) = image {
                let mut ppm =
                    format!("P6\n{} {}\n255\n", image.size[0], image.size[1]).into_bytes();
                for p in &image.pixels {
                    ppm.extend_from_slice(&[p.r(), p.g(), p.b()]);
                }
                if let Err(e) = std::fs::write(&path, ppm) {
                    eprintln!("{path}: {e}");
                }
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            }
            return;
        }
        if let Some(bench) = &mut self.bench {
            ui.ctx().request_repaint();
            if self.doc.is_some() || self.compare.is_some() {
                bench.done += 1;
                // Skip the first frames (loading, first upload).
                if bench.done > 10 {
                    bench.times.push(dt);
                }
                let t = bench.done as f64;
                let factor = if (bench.done / 60) % 2 == 0 {
                    1.03
                } else {
                    1.0 / 1.03
                };
                let c = [
                    self.view.size[0] / 2.0 + 100.0 * (t / 20.0).sin(),
                    self.view.size[1] / 2.0,
                ];
                self.view.zoom_at(c, factor);
                self.view.pan(3.0 * (t / 15.0).cos(), 2.0);
                if bench.done == bench.frames + 10 {
                    report(bench, self.doc.as_ref());
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }
    }
}

impl CadApp {
    /// Plays the demo: each step sets up the view once when its time
    /// comes; captions say what is on screen.
    fn run_demo(&mut self, ui: &mut egui::Ui, now: f64) {
        ui.ctx().request_repaint();
        let Some(c) = &self.compare else { return };
        let (a, b) = {
            let (a, b) = c.labels();
            (a.to_owned(), b.to_owned())
        };
        let Some(demo) = &mut self.demo else { return };
        let start = *demo.start.get_or_insert(now + 1.0);
        let t = now - start;
        if t < 0.0 {
            return;
        }
        let step = DEMO_STEPS.iter().rposition(|&s| t >= s).unwrap_or(0);
        if step == demo.step {
            return;
        }
        demo.step = step;
        let Some(c) = &mut self.compare else { return };
        let extent = c.bounds();
        let picks = c.demo_picks();
        let changes = |n: usize| match n {
            1 => "1 change".to_owned(),
            n => format!("{n} changes"),
        };
        let mut zoom: Option<(lightable_render::Bounds, f64)> = None;
        // Every number and headline below comes from the comparison.
        self.caption = Some(match step {
            0 => {
                c.side_by_side = true;
                c.details = false;
                c.preview_report(false);
                zoom = extent.map(|e| (e, 0.03));
                format!("Before and after: {a} on the left, {b} on the right.")
            }
            1 => match c.cloud_bounds() {
                Some(cloud) => {
                    zoom = Some((cloud, 0.1));
                    format!("The revision cloud marks {}.", changes(c.clouded_count()))
                }
                None => format!("{b} has no revision cloud."),
            },
            2 => {
                c.side_by_side = false;
                zoom = extent.map(|e| (e, 0.03));
                format!("Lightable found {}.", changes(c.design_count()))
            }
            3..=5 => match picks.get(step - 3) {
                Some(&i) => {
                    zoom = c.select(i).map(|b| (b, 0.15));
                    c.headline(i).to_owned()
                }
                None => String::new(),
            },
            6 => {
                c.details = true;
                c.reveal_not_compared = true;
                zoom = extent.map(|e| (e, 0.03));
                "Details: the technical data, and what was not compared.".to_owned()
            }
            7 => {
                c.details = false;
                c.preview_report(true);
                "Download the review report as PNG or JSON.".to_owned()
            }
            8 => {
                c.preview_report(false);
                "Lightable shows what changed. Whether it is right is still your call.".to_owned()
            }
            _ => {
                self.end_card = true;
                String::new()
            }
        });
        if let Some((b, margin)) = zoom {
            self.demo_zoom = Some((b, margin));
        }
    }

    /// Demo captions and the end card, over everything.
    fn overlays(&self, ui: &mut egui::Ui) {
        let screen = ui.ctx().content_rect();
        let painter = ui.ctx().layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("demo"),
        ));
        if self.end_card {
            painter.rect_filled(
                screen,
                0.0,
                Color32::from_rgba_unmultiplied(12, 12, 14, 245),
            );
            let c = screen.center();
            let line = |y: f32, text: &str, size: f32, color: Color32| {
                painter.text(
                    egui::pos2(c.x, c.y + y),
                    Align2::CENTER_CENTER,
                    text,
                    FontId::proportional(size),
                    color,
                );
            };
            line(-70.0, "Free. Open source.", 40.0, Color32::WHITE);
            line(
                -10.0,
                "Your files never leave your computer.",
                30.0,
                Color32::from_gray(220),
            );
            line(
                60.0,
                "lightable.dev",
                34.0,
                Color32::from_rgb(0x6f, 0x9b, 0xff),
            );
            return;
        }
        if let Some(text) = self.caption.as_deref().filter(|t| !t.is_empty()) {
            let galley =
                painter.layout_no_wrap(text.to_owned(), FontId::proportional(24.0), Color32::WHITE);
            let size = galley.size() + egui::vec2(36.0, 20.0);
            let rect =
                Rect::from_center_size(egui::pos2(screen.center().x, screen.max.y - 60.0), size);
            painter.rect_filled(rect, 10.0, Color32::from_rgba_unmultiplied(0, 0, 0, 215));
            painter.galley(rect.min + egui::vec2(18.0, 10.0), galley, Color32::WHITE);
        }
    }

    /// The PNG report layout: header, overlay, change list and footer,
    /// shown for a few frames and then captured.
    fn report(&mut self, ui: &mut egui::Ui) {
        ui.ctx().request_repaint();
        let Some(c) = &mut self.compare else { return };
        let capture = c.report_frame();
        let image = ui.input(|i| {
            i.raw.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(image) = image
            && c.waiting_for_capture()
        {
            self.message = Some(c.finish_report(&image));
            return;
        }
        egui::Panel::top("report-header").show(ui, |ui| c.report_header(ui));
        egui::Panel::bottom("report-footer").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.small(format!(
                    "Made with {} {} · {}",
                    crate::APP_NAME,
                    env!("CARGO_PKG_VERSION"),
                    crate::REPO_URL
                ));
            });
        });
        egui::Panel::right("report-changes")
            .exact_size(420.0)
            .show(ui, |ui| c.report_list(ui));
        egui::CentralPanel::no_frame().show(ui, |ui| self.canvas(ui));
        if capture {
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
        }
    }
}

fn report(bench: &Bench, doc: Option<&Doc>) {
    let mut times = bench.times.clone();
    times.sort_by(|a, b| a.total_cmp(b));
    let n = times.len().max(1);
    let avg = times.iter().sum::<f64>() / n as f64;
    let p95 = times[(n * 95 / 100).min(n - 1)];
    let worst = times.last().copied().unwrap_or(0.0);
    if let Some(doc) = doc {
        println!(
            "{}: {} items, {} vertices",
            doc.name,
            doc.scene.items.len(),
            doc.vertices
        );
    }
    println!(
        "{} frames of pan/zoom: average {:.2} ms ({:.0} fps), 95th percentile {:.2} ms, worst {:.2} ms",
        n,
        avg * 1000.0,
        1.0 / avg.max(1e-9),
        p95 * 1000.0,
        worst * 1000.0
    );
}

#[cfg(test)]
mod font_tests {
    use skrifa::MetadataProvider as _;

    /// Every character the app writes (outside comments) has a glyph in the
    /// fonts it installs: no replacement boxes after dropping egui's emoji
    /// fonts.
    #[test]
    fn every_character_the_app_writes_has_a_glyph() {
        let sources = [
            include_str!("app.rs"),
            include_str!("compare.rs"),
            include_str!("main.rs"),
            include_str!("feedback.rs"),
            include_str!("files.rs"),
            include_str!("examples.rs"),
            include_str!("../../lightable-diff/src/words.rs"),
            include_str!("../../lightable-diff/src/lib.rs"),
        ];
        let chars: std::collections::BTreeSet<char> = sources
            .iter()
            .flat_map(|s| s.lines())
            .filter(|l| !l.trim_start().starts_with("//"))
            .flat_map(|l| l.chars())
            .filter(|c| !c.is_ascii())
            .collect();
        assert!(chars.contains(&'▶') && chars.contains(&'Ø'));
        // Checked against the fonts' character maps (egui's own
        // `has_glyph` misreports characters of its replacement-glyph font).
        let fonts = super::fonts();
        for (family, names) in &fonts.families {
            let maps: Vec<skrifa::FontRef<'_>> = names
                .iter()
                .map(|n| skrifa::FontRef::new(&fonts.font_data[n].font).unwrap())
                .collect();
            let missing: String = chars
                .iter()
                .filter(|c| !maps.iter().any(|m| m.charmap().map(**c).is_some()))
                .collect();
            assert!(
                missing.is_empty(),
                "{family:?} has no glyph for {missing:?}"
            );
        }
    }
}
