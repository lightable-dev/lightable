//! The viewer: a canvas with pan and zoom, a layer panel and a status bar.

use std::sync::{Arc, Mutex};

use eframe::egui::{self, Color32, FontId, PointerButton, Pos2, Rect, Sense, epaint::TextShape};
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
    /// When the next step starts (the first a second after the app is up).
    next_at: Option<f64>,
    /// The step showing now; `usize::MAX` before the first.
    step: usize,
}

/// The demo's steps; the last is the end card, which stays.
const DEMO_LAST_STEP: usize = 9;

/// How long a demo caption stays: reading time at 3 words a second, plus
/// a second for the zoom to settle; at least 3 seconds.
pub fn caption_seconds(caption: &str) -> f64 {
    (caption.split_whitespace().count() as f64 / 3.0 + 1.0).max(3.0)
}

/// Caption text size for a screen: readable in a 16:9 or 9:16 video.
pub fn caption_size(width: f32, height: f32) -> f32 {
    (width.min(height) / 26.0).clamp(20.0, 48.0)
}

/// The file dialog exists (web); the native app takes files by drag and
/// drop or on the command line.
pub const CAN_CHOOSE_FILES: bool = cfg!(target_arch = "wasm32");
/// The privacy promise, next to "Choose files…".
pub const PRIVACY: &str = "Files are read in this browser, never uploaded.";

pub struct CadApp {
    doc: Option<Doc>,
    view: View,
    fit_pending: bool,
    message: Option<String>,
    frame_times: Vec<f64>,
    bench: Option<Bench>,
    pending: Vec<(String, Vec<u8>)>,
    /// Files read in the background (the web reads dropped and chosen
    /// files asynchronously).
    incoming: crate::files::FileSlot,
    /// Files to read once "Reading…" has been shown (a big file can take
    /// seconds): the files, whether to put them in revision order, and
    /// frames left to wait.
    to_read: Option<(Vec<crate::files::Arrived>, bool, u8)>,
    /// A file that could not be used, shown in its own window.
    problem: Option<crate::intake::Problem>,
    /// Two files waiting for the user to say which is the earlier: the
    /// files and whether their contents are identical.
    #[allow(clippy::type_complexity)]
    ask_order: Option<(
        (String, Arc<lightable_dxf::Drawing>),
        (String, Arc<lightable_dxf::Drawing>),
        bool,
    )>,
    compare: Option<Compare>,
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
    /// Where the drawing was drawn last frame (demo captions sit at its
    /// bottom).
    canvas_rect: Option<Rect>,
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
        // Keyboard focus (egui draws a focused widget in its "active" style)
        // gets a bright 2-point outline, visible from across the room.
        cc.egui_ctx.all_styles_mut(|style| {
            style.visuals.widgets.active.bg_stroke =
                egui::Stroke::new(2.0, Color32::from_rgb(0x6f, 0x9b, 0xff));
        });
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
        let incoming = crate::files::FileSlot::default();
        #[cfg(target_arch = "wasm32")]
        if let Err(e) = crate::files::listen(&incoming, &cc.egui_ctx) {
            eprintln!("file dialog: {e}");
        }
        Self {
            doc: None,
            view: View::new([800.0, 600.0]),
            fit_pending: true,
            message: None,
            frame_times: Vec::new(),
            bench,
            pending: files,
            incoming,
            to_read: None,
            problem: None,
            ask_order: None,
            compare: None,
            side_by_side,
            report_and_quit,
            example_pending: example,
            anim: None,
            demo_zoom: None,
            auto_fit: false,
            demo: demo.then_some(Demo {
                next_at: None,
                step: usize::MAX,
            }),
            canvas_rect: None,
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

    fn load(
        &mut self,
        gl: Option<&Arc<glow::Context>>,
        name: String,
        drawing: Arc<lightable_dxf::Drawing>,
        warnings: usize,
    ) {
        let scene = lightable_render::build(&drawing);
        if let Some(old) = self.doc.take() {
            old.destroy(gl);
        }
        if let Some(old) = self.compare.take() {
            old.destroy(gl);
        }
        match Doc::new(gl, name, scene, warnings) {
            Ok(mut doc) => {
                doc.drawing = Some(drawing);
                self.doc = Some(doc);
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

    /// Compares two files the user handed over, `a` as the earlier.
    fn compare_files(
        &mut self,
        gl: Option<&Arc<glow::Context>>,
        a: (String, Arc<lightable_dxf::Drawing>),
        b: (String, Arc<lightable_dxf::Drawing>),
        same: bool,
    ) {
        self.start_compare(gl, a, b, None, None);
        if let Some(c) = &mut self.compare {
            c.notes = crate::intake::notes(same, c.diff());
        }
    }

    /// Two files whose names and dates do not say which came first: the
    /// user picks the earlier one.
    fn order_window(&mut self, ui: &mut egui::Ui, gl: Option<&Arc<glow::Context>>) {
        let Some((a, b, same)) = self.ask_order.take() else {
            return;
        };
        let (na, nb) = (
            crate::intake::short(&a.0).to_owned(),
            crate::intake::short(&b.0).to_owned(),
        );
        let mut choice = None;
        let width = (ui.ctx().content_rect().width() - 32.0).clamp(240.0, 480.0);
        let modal = egui::Modal::new(egui::Id::new("order")).show(ui.ctx(), |ui| {
            ui.set_width(width);
            ui.heading("Which file is the earlier revision?");
            ui.add_space(6.0);
            ui.add(
                egui::Label::new(
                    "The file names and dates do not say clearly which came first. \
                     Choose the one to compare from (Before); you can swap them later.",
                )
                .wrap(),
            );
            ui.add_space(10.0);
            if ui.button(format!("{na} is Before")).clicked() {
                choice = Some(true);
            }
            if ui.button(format!("{nb} is Before")).clicked() {
                choice = Some(false);
            }
        });
        match choice {
            Some(true) => self.compare_files(gl, a, b, same),
            Some(false) => self.compare_files(gl, b, a, same),
            // Closing without a choice cancels; the open view stays.
            None if modal.should_close() => {}
            None => self.ask_order = Some((a, b, same)),
        }
    }

    /// Shows "Reading…" now and reads `files` a frame later, so the
    /// message is on screen while a big file is parsed.
    fn stage(&mut self, files: Vec<crate::files::Arrived>, reorder: bool) {
        if files.is_empty() {
            return;
        }
        let total: usize = files
            .iter()
            .map(|f| f.bytes.as_ref().map_or(0, Vec::len))
            .sum();
        let names: Vec<&str> = files
            .iter()
            .map(|f| crate::intake::short(&f.name))
            .collect();
        self.message = Some(format!(
            "Reading {} ({})…",
            names.join(" and "),
            crate::intake::size(total)
        ));
        self.problem = None;
        self.to_read = Some((files, reorder, 2));
    }

    /// Files that arrived (dropped, chosen or from the command line). Two
    /// files are compared, the earlier revision first when `reorder` (the
    /// command line keeps its order); one is opened, or compared with the
    /// drawing already open on its own.
    fn receive(
        &mut self,
        gl: Option<&Arc<glow::Context>>,
        mut files: Vec<crate::files::Arrived>,
        reorder: bool,
    ) {
        use crate::intake;
        self.message = None;
        files.truncate(2);
        let mut read = Vec::new();
        for f in files {
            let bytes = match f.bytes {
                Ok(b) => b,
                Err(e) => {
                    self.problem = Some(intake::Problem {
                        title: format!("{} could not be read", intake::short(&f.name)),
                        body: "The browser could not read the file. Choose it again; if it is on a network drive or in cloud storage, copy it to this device first.".into(),
                        detail: Some(e),
                    });
                    return;
                }
            };
            match lightable_dxf::read_bytes(&bytes) {
                Ok((drawing, warnings)) => {
                    if let Some(p) = intake::incomplete(&f.name, &warnings) {
                        self.problem = Some(p);
                        return;
                    }
                    read.push((f.name, Arc::new(drawing), f.modified, bytes, warnings.len()));
                }
                Err(e) => {
                    self.problem = Some(intake::unreadable(&f.name, &bytes, &e));
                    return;
                }
            }
        }
        if read.len() == 2 {
            let same = read[0].3 == read[1].3;
            let order = if reorder {
                intake::revision_order((&read[0].0, read[0].2), (&read[1].0, read[1].2))
            } else {
                intake::Order::AsGiven
            };
            let b = read.pop().map(|r| (r.0, r.1));
            let a = read.pop().map(|r| (r.0, r.1));
            if let (Some(a), Some(b)) = (a, b) {
                match order {
                    intake::Order::AsGiven => self.compare_files(gl, a, b, same),
                    intake::Order::Swapped => self.compare_files(gl, b, a, same),
                    intake::Order::Ask => self.ask_order = Some((a, b, same)),
                }
            }
            return;
        }
        let Some((name, drawing, _, _, warnings)) = read.pop() else {
            return;
        };
        // One file while another is open on its own: compare them, the open
        // one as the earlier revision (Swap changes that).
        let open = self
            .doc
            .as_ref()
            .and_then(|d| Some((d.name.clone(), d.drawing.clone()?)));
        if self.compare.is_none()
            && let Some(a) = open
        {
            self.start_compare(gl, a, (name, drawing), None, None);
            return;
        }
        self.load(gl, name, drawing, warnings);
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

    /// Opens the file dialog (web). Chosen files arrive in `incoming`.
    fn choose_files(&mut self, ctx: &egui::Context) {
        #[cfg(target_arch = "wasm32")]
        if let Err(e) = crate::files::choose() {
            self.message = Some(format!("Could not open the file dialog: {e}"));
        }
        let _ = ctx;
    }

    /// Does what the compare toolbar or summary asked for.
    fn act(&mut self, action: &crate::compare::Action, gl: Option<&Arc<glow::Context>>) {
        if let Some(m) = &action.message {
            self.message = Some(m.clone());
        }
        if action.close
            && let Some(c) = self.compare.take()
        {
            c.destroy(gl);
        }
        if action.fit {
            self.fit();
        }
        if let Some(id) = action.example {
            self.open_example(gl, id);
        }
        if action.swap
            && let Some(c) = &self.compare
        {
            let (a, b, labels) = c.swapped();
            let notes = c.notes.clone();
            let side = c.side_by_side;
            self.start_compare(gl, a, b, labels, None);
            if let Some(c) = &mut self.compare {
                c.notes = notes;
                c.side_by_side = side;
            }
        }
    }

    fn toolbar(&mut self, ui: &mut egui::Ui, gl: Option<&Arc<glow::Context>>) {
        if let Some(c) = &mut self.compare {
            let action = c.toolbar(ui);
            if let Some(b) = action.zoom {
                let now = ui.input(|i| i.time);
                self.animate_to(&b, 0.15, now, 0.6);
            }
            if action.choose {
                self.choose_files(ui.ctx());
            }
            self.act(&action, gl);
            return;
        }
        let mut example = false;
        let mut choose = false;
        ui.horizontal_wrapped(|ui| {
            if CAN_CHOOSE_FILES {
                choose = ui
                    .button("Choose files…")
                    .on_hover_text("Pick the two revisions (DXF) to compare")
                    .clicked();
                ui.label(egui::RichText::new(PRIVACY).small().color(Color32::from_gray(160)));
                ui.separator();
            }
            example = ui
                .button("Try the example")
                .on_hover_text("Compare two revisions of a sample drawing (built in, made for this app)")
                .clicked();
            if let Some(doc) = &self.doc {
                ui.separator();
                let stats = doc.stats.lock().map(|s| *s).unwrap_or_default();
                ui.strong(&doc.name).on_hover_text(format!(
                    "{} items, {} vertices; drawn in {} chunks{}",
                    doc.scene.items.len(),
                    doc.vertices,
                    stats.chunks,
                    if doc.warnings > 0 {
                        format!("; {} read warnings", doc.warnings)
                    } else {
                        String::new()
                    }
                ));
                ui.label(
                    egui::RichText::new(if CAN_CHOOSE_FILES {
                        "To compare, choose (or drop) the other revision."
                    } else {
                        "To compare, drop the other revision on the window."
                    })
                    .color(Color32::from_rgb(0xf5, 0xd0, 0x2a)),
                );
                if ui.button("Zoom extents").clicked() {
                    self.fit();
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
        });
        if choose {
            self.choose_files(ui.ctx());
        }
        if example {
            self.open_example(gl, "");
        }
    }

    /// Nothing open: what Lightable does and how to start.
    fn welcome(&mut self, ui: &mut egui::Ui, gl: Option<&Arc<glow::Context>>) {
        let mut example = false;
        let mut choose = false;
        ui.vertical_centered(|ui| {
            ui.add_space((ui.available_height() * 0.25).min(160.0));
            ui.heading(egui::RichText::new("Compare two revisions of a drawing").size(24.0));
            ui.add_space(8.0);
            ui.label("Lightable lists what changed between two DXF files, in plain words, and shows where.");
            ui.add_space(16.0);
            if CAN_CHOOSE_FILES {
                choose = ui
                    .add(egui::Button::new(egui::RichText::new("Choose two DXF files…").size(18.0)))
                    .clicked();
                ui.add_space(6.0);
                ui.label(egui::RichText::new(PRIVACY).color(Color32::from_gray(170)));
            } else {
                ui.label("Drop two DXF files on this window.");
            }
            ui.add_space(12.0);
            example = ui.button("Or try the example").clicked();
        });
        if choose {
            self.choose_files(ui.ctx());
        }
        if example {
            self.open_example(gl, "");
        }
    }

    /// A file that could not be used: what happened and what to do.
    fn problem_window(&mut self, ui: &mut egui::Ui, gl: Option<&Arc<glow::Context>>) {
        let Some(p) = self.problem.clone() else {
            return;
        };
        let mut close = false;
        let mut choose = false;
        let width = (ui.ctx().content_rect().width() - 32.0).clamp(240.0, 480.0);
        let modal = egui::Modal::new(egui::Id::new("problem")).show(ui.ctx(), |ui| {
            ui.set_width(width);
            ui.heading(&p.title);
            ui.add_space(6.0);
            ui.add(egui::Label::new(&p.body).wrap());
            if let Some(d) = &p.detail {
                ui.add_space(6.0);
                egui::CollapsingHeader::new(egui::RichText::new("Details").small())
                    .show(ui, |ui| {
                        ui.add(egui::Label::new(egui::RichText::new(d).small()).wrap())
                    });
            }
            ui.add_space(10.0);
            ui.horizontal_wrapped(|ui| {
                if CAN_CHOOSE_FILES && ui.button("Choose other files…").clicked() {
                    choose = true;
                }
                if ui.button("OK").clicked() {
                    close = true;
                }
            });
        });
        if modal.should_close() || close || choose {
            self.problem = None;
        }
        if choose {
            self.choose_files(ui.ctx());
        }
        let _ = gl;
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
        self.canvas_rect = Some(rect);
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

/// The list goes under the drawing: portrait screens (phones held
/// upright, 9:16 video), and narrow windows that are not also short.
pub fn is_narrow(width: f32, height: f32) -> bool {
    height > width * 1.2 || (width < 720.0 && !is_short(height))
}

/// Little height (a phone held sideways): the list stays beside the
/// drawing and the summary is compact.
pub fn is_short(height: f32) -> bool {
    height < 480.0
}

impl eframe::App for CadApp {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let dt = ui.input(|i| f64::from(i.unstable_dt));
        self.frame_times.push(dt);
        if self.frame_times.len() > 60 {
            self.frame_times.remove(0);
        }
        use crate::files::Arrived;
        // Files named on the command line, in the order given.
        if !self.pending.is_empty() {
            let files = std::mem::take(&mut self.pending)
                .into_iter()
                .enumerate()
                .map(|(order, (name, bytes))| Arrived {
                    order,
                    name,
                    bytes: Ok(bytes),
                    modified: None,
                })
                .collect();
            self.stage(files, false);
        }
        let dropped = ui.input(|i| i.raw.dropped_files.clone());
        #[cfg(not(target_arch = "wasm32"))]
        if !dropped.is_empty() {
            let files = dropped
                .into_iter()
                .enumerate()
                .map(|(order, file)| {
                    let path = file.path().to_path_buf();
                    let modified = std::fs::metadata(&path)
                        .and_then(|m| m.modified())
                        .ok()
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map(|d| d.as_secs_f64() * 1000.0);
                    Arrived {
                        order,
                        name: path.display().to_string(),
                        bytes: file.bytes(),
                        modified,
                    }
                })
                .collect();
            self.stage(files, true);
        }
        #[cfg(target_arch = "wasm32")]
        {
            // Browsers read files asynchronously; the bytes arrive in a
            // later frame through `incoming`.
            if !dropped.is_empty()
                && let Ok(mut slot) = self.incoming.lock()
            {
                slot.expect(dropped.len());
            }
            for (order, file) in dropped.into_iter().enumerate() {
                let name = file.path().display().to_string();
                let modified = file.web_file().map(web_sys::File::last_modified);
                let incoming = self.incoming.clone();
                let ctx = ui.ctx().clone();
                wasm_bindgen_futures::spawn_local(async move {
                    let bytes = file.bytes_async().await;
                    if let Ok(mut slot) = incoming.lock() {
                        slot.files.push(Arrived {
                            order,
                            name,
                            bytes,
                            modified,
                        });
                        ctx.request_repaint();
                    }
                });
            }
        }
        let arrived = self
            .incoming
            .lock()
            .ok()
            .and_then(|mut s| s.take_complete());
        if let Some(files) = arrived {
            self.stage(files, true);
        }
        // Read staged files once "Reading…" has been drawn.
        if let Some((files, reorder, wait)) = self.to_read.take() {
            if wait > 0 {
                self.to_read = Some((files, reorder, wait - 1));
                ui.ctx().request_repaint();
            } else {
                self.receive(frame.gl(), files, reorder);
                if self.side_by_side
                    && let Some(c) = &mut self.compare
                {
                    c.side_by_side = true;
                }
            }
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
        // Narrow or portrait screens (phones): the change list goes under
        // the drawing and rows wrap.
        let screen = ui.ctx().content_rect();
        let narrow = is_narrow(screen.width(), screen.height());
        if let Some(c) = &mut self.compare {
            c.narrow = narrow;
            c.short = is_short(screen.height());
        }
        egui::Panel::top("toolbar").show(ui, |ui| self.toolbar(ui, gl.as_ref()));
        // Details: the comparison's toggle; always for a single drawing;
        // nothing to detail when nothing is open.
        let details = self
            .compare
            .as_ref()
            .map_or(self.doc.is_some(), |c| c.details);
        if let Some(c) = &mut self.compare {
            let mut zoom = None;
            let mut action = crate::compare::Action::default();
            egui::Panel::top("summary").show(ui, |ui| zoom = c.summary_bar(ui, &mut action));
            let list = |ui: &mut egui::Ui| c.change_list(ui);
            let picked = if narrow {
                egui::Panel::bottom("changes-below")
                    .resizable(true)
                    .default_size(screen.height() * 0.35)
                    .show(ui, list)
                    .inner
            } else {
                egui::Panel::right("changes")
                    .resizable(true)
                    .default_size(520.0_f32.min(screen.width() * 0.4))
                    .show(ui, list)
                    .inner
            };
            if let Some(b) = picked.or(zoom) {
                self.animate_to(&b, 0.15, now, 0.6);
            }
            self.act(&action, gl.as_ref());
        }
        // The status bar is a detail, except to report an export or that a
        // file is being read.
        if details || self.message.is_some() {
            egui::Panel::bottom("status").show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    if let Some(m) = &self.message {
                        ui.label(egui::RichText::new(m).color(Color32::WHITE));
                    }
                    if !details {
                        return;
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
                    if let Some(p) = ui.input(|i| i.pointer.hover_pos())
                        && (self.doc.is_some() || self.compare.is_some())
                    {
                        let w = self.view.to_world([f64::from(p.x), f64::from(p.y)]);
                        ui.label(format!("{:.4}, {:.4}", w[0], w[1]));
                    }
                });
            });
        }
        if details && (self.doc.is_some() || self.compare.is_some()) && !narrow {
            egui::Panel::left("layers")
                .resizable(true)
                .default_size(200.0)
                .show(ui, |ui| {
                    ui.heading("Layers");
                    self.layer_panel(ui);
                });
        }
        if self.doc.is_none() && self.compare.is_none() {
            egui::CentralPanel::default().show(ui, |ui| self.welcome(ui, gl.as_ref()));
        }
        if self.doc.is_some() || self.compare.is_some() {
            egui::CentralPanel::no_frame().show(ui, |ui| self.canvas(ui));
        }
        if let Some(c) = &self.compare
            && self.demo.is_none()
            && self.problem.is_none()
            && self.ask_order.is_none()
        {
            c.hints(ui.ctx());
        }
        self.problem_window(ui, gl.as_ref());
        self.order_window(ui, gl.as_ref());
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
        let next_at = *demo.next_at.get_or_insert(now + 1.0);
        if now < next_at || demo.step == DEMO_LAST_STEP {
            return;
        }
        let step = demo.step.wrapping_add(1);
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
                format!("Before and after: {a} left, {b} right.")
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
                "Details: exact values, and what is not compared.".to_owned()
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
        let seconds = caption_seconds(self.caption.as_deref().unwrap_or(""));
        if let Some(demo) = &mut self.demo {
            demo.next_at = Some(now + seconds);
        }
    }

    /// Demo captions and the end card, over everything.
    fn overlays(&self, ui: &mut egui::Ui) {
        let screen = ui.ctx().content_rect();
        let painter = ui.ctx().layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("demo"),
        ));
        // Reading files (a big one can take seconds): say so in the middle.
        if self.to_read.is_some()
            && let Some(text) = &self.message
        {
            let galley = painter.layout(
                text.clone(),
                FontId::proportional(18.0),
                Color32::WHITE,
                screen.width() * 0.8,
            );
            let rect =
                Rect::from_center_size(screen.center(), galley.size() + egui::vec2(40.0, 28.0));
            painter.rect_filled(rect, 10.0, Color32::from_rgba_unmultiplied(0, 0, 0, 225));
            painter.galley(rect.min + egui::vec2(20.0, 14.0), galley, Color32::WHITE);
        }
        let m = screen.width().min(screen.height());
        if self.end_card {
            painter.rect_filled(
                screen,
                0.0,
                Color32::from_rgba_unmultiplied(12, 12, 14, 245),
            );
            let c = screen.center();
            let line = |y: f32, text: &str, size: f32, color: Color32| {
                let galley = painter.layout(
                    text.to_owned(),
                    FontId::proportional(size),
                    color,
                    screen.width() * 0.9,
                );
                let at = egui::pos2(c.x - galley.size().x / 2.0, c.y + y - galley.size().y / 2.0);
                painter.galley(at, galley, color);
            };
            line(-m * 0.12, "Free. Open source.", m / 14.0, Color32::WHITE);
            line(
                0.0,
                "Your files never leave your computer.",
                m / 20.0,
                Color32::from_gray(220),
            );
            line(
                m * 0.12,
                "lightable.dev",
                m / 17.0,
                Color32::from_rgb(0x6f, 0x9b, 0xff),
            );
            return;
        }
        if let Some(text) = self.caption.as_deref().filter(|t| !t.is_empty()) {
            let area = self.canvas_rect.unwrap_or(screen);
            let size = caption_size(screen.width(), screen.height());
            let pad = egui::vec2(size * 0.75, size * 0.4);
            let galley = painter.layout(
                text.to_owned(),
                FontId::proportional(size),
                Color32::WHITE,
                area.width() * 0.9 - 2.0 * pad.x,
            );
            let boxed = galley.size() + 2.0 * pad;
            let rect = Rect::from_center_size(
                egui::pos2(area.center().x, area.max.y - boxed.y / 2.0 - size),
                boxed,
            );
            painter.rect_filled(
                rect,
                size * 0.4,
                Color32::from_rgba_unmultiplied(0, 0, 0, 215),
            );
            painter.galley(rect.min + pad, galley, Color32::WHITE);
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
mod demo_tests {
    /// Captions stay long enough to read at 3 words a second.
    #[test]
    fn captions_stay_long_enough_to_read() {
        let headline =
            "Circle diameter changed from 13.5 mm to 17.5 mm; center moved 5 mm right and 5 mm up";
        assert_eq!(headline.split_whitespace().count(), 18);
        assert!(super::caption_seconds(headline) >= 18.0 / 3.0);
        assert_eq!(super::caption_seconds("Lightable found 10 changes."), 3.0);
        assert_eq!(super::caption_seconds(""), 3.0);
    }

    /// Captions are larger in a 1080p video than in a laptop window, and
    /// the same size in 16:9 and 9:16.
    #[test]
    fn caption_size_follows_the_shorter_side() {
        assert_eq!(
            super::caption_size(1920.0, 1080.0),
            super::caption_size(1080.0, 1920.0)
        );
        assert!(super::caption_size(1920.0, 1080.0) >= 40.0);
        assert!((super::caption_size(1400.0, 714.0) - 27.5).abs() < 0.1);
        assert_eq!(super::caption_size(390.0, 844.0), 20.0);
    }
}

#[cfg(test)]
mod layout_tests {
    /// Phones in either orientation, and narrow windows, get the stacked
    /// layout; laptops and tablets in landscape keep the list on the right.
    #[test]
    fn narrow_means_portrait_or_a_narrow_window() {
        assert!(super::is_narrow(390.0, 844.0));
        assert!(super::is_narrow(1080.0, 1920.0));
        assert!(super::is_narrow(600.0, 700.0));
        // A phone held sideways keeps the list beside the drawing.
        assert!(!super::is_narrow(844.0, 390.0));
        assert!(!super::is_narrow(640.0, 300.0));
        assert!(super::is_short(300.0));
        assert!(!super::is_narrow(1366.0, 768.0));
        assert!(!super::is_narrow(1024.0, 768.0));
    }
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
