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
    /// Demo with voice-over timing (`--narration`, `?demo&narration`).
    pub narration: bool,
    /// Demo driven frame by frame by a recorder (`?demo&record`, web).
    pub record: bool,
    /// Demo steps cut in without zoom animation (`?demo&cut`, quick drafts).
    pub cut: bool,
    /// A short feature video instead of the whole demo (`?demo&feature=find`).
    pub feature: Option<String>,
    /// Open on the start screen ("Choose two DXF files…"), no example
    /// (`?own`, the demo page's "Compare your own files").
    pub own: bool,
    /// The machine a recording is made on (`?machine=`), named next to any
    /// time measured in it; without it, "this device".
    pub machine: Option<String>,
    /// Recording inside the brand stage (`?stage`, tools/stage/): the
    /// stage draws the captions, the pointer and the end card around the
    /// real app, which lays itself out for the window it is shown in.
    pub stage: Option<Stage>,
}

/// How the brand stage shows the app (`?stage&zoom=..&frameh=..&inset=..`,
/// in CSS pixels of the stage's frame).
#[derive(Debug, Clone, Copy, Default)]
pub struct Stage {
    /// The app's zoom (CSS pixels a point), fitted to its window.
    pub zoom: f32,
    /// The height of the whole video frame: captions are sized for it.
    pub frame_height: f32,
    /// How far the app's left edge must keep its controls in, so the
    /// frame's top-left 10 % stays clear.
    pub inset: f32,
}

/// A smooth move of the view.
#[derive(Clone, Copy)]
struct Anim {
    from: View,
    to: View,
    start: f64,
    seconds: f64,
}

/// The demo sequence (`?demo`): about 40 seconds with captions, then an end
/// card. Times in seconds from the start.
struct Demo {
    /// The built-in example whose files are dropped in.
    example: String,
    /// A feature video instead of the whole demo.
    feature: Option<crate::demo::Feature>,
    /// The large layout's plan has its numbers (its files were compared).
    planned_with_compare: bool,
    /// Voice-over timing (`?demo&narration`).
    narration: bool,
    /// Recording: demo time is the frame number the recorder sets, at 30
    /// frames a second (`?demo&record`).
    record: bool,
    /// The plan and when it started (app time), once the example is open.
    plan: Option<(crate::demo::Plan, f64)>,
    /// Shots applied so far.
    done: usize,
    /// Zooms are instant (`?demo&cut`).
    cut: bool,
    /// Playing live (no recorder): the app performs the gestures itself,
    /// through its own input.
    live: Live,
}

/// The live demo's gestures, fed to egui's input like a person's.
#[derive(Default)]
struct Live {
    /// The gesture under way (index into the plan's gestures).
    next: usize,
    /// Where the pointer moves from and to (points), and for a pinch the
    /// zoom factor, once the gesture has started.
    path: Option<(Pos2, Pos2, f64)>,
    /// Where the pointer is.
    pos: Option<Pos2>,
    /// The pinch progress applied so far (0 to 1).
    zoomed: f64,
    /// A click pressed, to release next frame.
    release: bool,
    /// When the pointer started moving to the current target.
    moving_from: f64,
    /// Where the row being scrolled into view was last frame (scrolling
    /// stops when it no longer moves).
    scrolled: Option<f32>,
    /// Frames the row has not moved while scrolling (three: the end).
    unmoved: u8,
}

/// A file the live demo drops: the example's bytes.
#[derive(Debug)]
struct ExampleFile {
    path: std::path::PathBuf,
    bytes: &'static [u8],
}

impl egui::DroppedFile for ExampleFile {
    fn path(&self) -> &std::path::Path {
        &self.path
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn bytes(&self) -> Result<Vec<u8>, String> {
        Ok(self.bytes.to_vec())
    }

    #[cfg(target_arch = "wasm32")]
    fn bytes_async(
        &self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<u8>, String>> + '_>> {
        Box::pin(std::future::ready(Ok(self.bytes.to_vec())))
    }
}

/// The recorder's side of `?demo&record` (`tools/demo.mjs`): it
/// sets `window.__lightableFrame`, the app draws that moment and answers in
/// `window.__lightableRendered`; the demo's length is in
/// `window.__lightableDemoLength`.
mod record {
    #[cfg(target_arch = "wasm32")]
    fn window() -> Option<eframe::web_sys::Window> {
        eframe::web_sys::window()
    }

    /// The frame the recorder asks for, if it has asked.
    pub fn frame() -> Option<f64> {
        #[cfg(target_arch = "wasm32")]
        {
            let w = window()?;
            js_sys::Reflect::get(&w, &"__lightableFrame".into())
                .ok()?
                .as_f64()
        }
        #[cfg(not(target_arch = "wasm32"))]
        None
    }

    fn set(_name: &str, _value: f64) {
        #[cfg(target_arch = "wasm32")]
        if let Some(w) = window() {
            let _ = js_sys::Reflect::set(&w, &_name.into(), &_value.into());
        }
    }

    /// Frame `frame` is drawn, the `draws`-th draw (the recorder waits for
    /// each input event's draw).
    pub fn rendered(frame: f64, draws: u64) {
        set("__lightableDraws", draws as f64);
        set("__lightableRendered", frame.round());
    }

    pub fn publish_length(seconds: f64) {
        set("__lightableDemoLength", seconds);
    }

    fn set_text(_name: &str, _value: &str) {
        #[cfg(target_arch = "wasm32")]
        if let Some(w) = window() {
            let _ = js_sys::Reflect::set(&w, &_name.into(), &_value.into());
        }
    }

    /// The demo's plan for the recorder: its length, the steps
    /// (`__lightableDemoShots`: `[{"at": seconds, "desc": "..."}]`), the
    /// gestures to perform (`__lightableDemoGestures`: `[{"from", "at",
    /// "until", "target"}]`) and the files to drop (`__lightableDemoFiles`).
    pub fn publish_plan(plan: &crate::demo::Plan, example: &crate::examples::Example) {
        publish_length(plan.length);
        let shots: Vec<String> = plan
            .shots
            .iter()
            .map(|s| {
                // The scene and the caption as planned (the end card's
                // words, which the stage shows), for the brand stage.
                let caption = if s.act == crate::demo::Act::EndCard {
                    crate::demo::END_CARD
                } else {
                    s.caption.as_deref().unwrap_or("")
                };
                format!(
                    "{{\"at\":{},\"desc\":{},\"view\":{},\"scene\":{},\"caption\":{},\"spot\":{}}}",
                    s.at,
                    json_string(&format!("{s:?}")),
                    s.view.map_or("null".to_owned(), |v| json_string(v.name())),
                    s.scene.map_or("null".to_owned(), |v| json_string(v.name())),
                    json_string(caption),
                    s.spot().map_or("null".to_owned(), json_string)
                )
            })
            .collect();
        set_text("__lightableDemoShots", &format!("[{}]", shots.join(",")));
        let gestures: Vec<String> = plan
            .gestures
            .iter()
            .map(|g| {
                format!(
                    "{{\"from\":{},\"at\":{},\"until\":{},\"target\":{}}}",
                    g.from,
                    g.at,
                    g.until,
                    json_string(&g.target.name())
                )
            })
            .collect();
        set_text(
            "__lightableDemoGestures",
            &format!("[{}]", gestures.join(",")),
        );
        // Per drop: the files, as copies of the example's files on disk
        // (`copy`) or as bytes (`hex`).
        let mut sets = Vec::new();
        for g in &plan.gestures {
            if let crate::demo::Target::Drop(set) = g.target {
                let files: Vec<String> = set
                    .files(example)
                    .iter()
                    .map(|(name, bytes, copy)| match copy {
                        Some(c) => format!(
                            "{{\"name\":{},\"copy\":{}}}",
                            json_string(name),
                            json_string(c)
                        ),
                        None => format!(
                            "{{\"name\":{},\"hex\":\"{}\"}}",
                            json_string(name),
                            bytes.iter().map(|b| format!("{b:02x}")).collect::<String>()
                        ),
                    })
                    .collect();
                sets.push(format!(
                    "{}:[{}]",
                    json_string(&g.target.name()),
                    files.join(",")
                ));
            }
        }
        set_text("__lightableDemoFiles", &format!("{{{}}}", sets.join(",")));
    }

    /// What is on screen this frame, as JSON (for the recorder's checks and
    /// its hold optimisation).
    pub fn publish_frame_info(json: &str) {
        set_text("__lightableFrameInfo", json);
    }

    /// A string as a JSON string literal.
    pub fn json_string(s: &str) -> String {
        let mut out = String::from("\"");
        for c in s.chars() {
            match c {
                '"' => out.push_str("\\\""),
                '\\' => out.push_str("\\\\"),
                c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
                c => out.push(c),
            }
        }
        out.push('"');
        out
    }
}

/// Frames a second when recording.
const RECORD_FPS: f64 = 30.0;

/// The page's clock in milliseconds (wall time: for how long reading and
/// comparing took).
fn clock_ms() -> f64 {
    #[cfg(target_arch = "wasm32")]
    {
        js_sys::Date::now()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0.0, |d| d.as_secs_f64() * 1000.0)
    }
}

/// How many network requests the page has made so far, as the browser
/// counts them (its Resource Timing list: every fetch, image, script and
/// other resource the page loaded). Native: none.
fn requests_so_far() -> Option<usize> {
    #[cfg(target_arch = "wasm32")]
    {
        let entries = web_sys::window()?
            .performance()?
            .get_entries_by_type("resource");
        Some(entries.length() as usize)
    }
    #[cfg(not(target_arch = "wasm32"))]
    None
}

/// Caption text size for a screen: 4.5 % of its height (readable on a phone
/// in a social feed), shrunk to [`CAPTION_MIN`] at most to fit two lines.
pub fn caption_size(height: f32) -> f32 {
    0.045 * height
}

/// The smallest caption, as a share of the screen height.
const CAPTION_MIN: f32 = 0.04;

/// Where a caption's box ends (points from the top): just above the bottom
/// of the drawing, and never in the bottom 15 % of the frame, where video
/// players put their controls.
fn caption_bottom(screen: Rect, area: Rect, size: f32) -> f32 {
    (area.max.y - size * 0.4).min(screen.max.y - 0.15 * screen.height())
}

/// The file dialog exists (web); the native app takes files by drag and
/// drop or on the command line.
pub const CAN_CHOOSE_FILES: bool = cfg!(target_arch = "wasm32");
/// The privacy promise, next to "Choose files…".
pub const PRIVACY: &str = "Your files are read in your browser, never uploaded.";

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
    /// The zoom and when it starts (the step's scheduled time, so a frame
    /// looks the same however the recorder reaches it).
    demo_zoom: Option<(lightable_render::Bounds, f64, f64)>,
    /// The view shows everything and nobody has moved it since: when the
    /// panes change size (the change list settling its width, Side by
    /// side), it is fitted again.
    auto_fit: bool,
    demo: Option<Demo>,
    /// The demo caption shown now.
    caption: Option<String>,
    /// The caption's key number (use-case videos).
    caption_key: Option<String>,
    /// The demo's end card is showing.
    end_card: bool,
    /// Where the drawing was drawn last frame (demo captions sit at its
    /// bottom).
    canvas_rect: Option<Rect>,
    /// This frame's time (seconds): the app clock, or the recorder's frame
    /// clock when recording.
    now: f64,
    /// The UI scale changes next frame (recording): wait with zooms.
    zoom_pending: bool,
    /// What the demo's current step is about (the change, the cloud, the
    /// sheet), for the recorder's checks.
    demo_focus: Option<lightable_render::Bounds>,
    /// The caption drawn last: text size, box and number of lines.
    caption_box: std::cell::Cell<Option<(f32, Rect, usize)>>,
    /// The previous frame's time: a recording draws some frames twice, and
    /// frame-counted waits count frames, not draws.
    prev_now: f64,
    /// Draws so far (the recorder waits for each input event's draw).
    draws: u64,
    /// The demo shows the pointer (from the first gesture to the phone).
    pointer_shown: bool,
    /// When the primary button was last pressed (demo time), for the tap
    /// ripple.
    last_press: Option<f64>,
    /// Where files are being dragged over the page (web; CSS pixels).
    drag_pos: crate::files::DragPos,
    /// The comparison open now came from files dropped on the page.
    dropped: bool,
    /// A target highlighted from, until (demo time): the colour legend, the
    /// summary line, the privacy line.
    highlight: Option<(&'static str, f64, f64)>,
    /// Recording a feature video that shows the privacy line, or the file
    /// names and Swap (kept across Swap, which compares anew).
    show_privacy: bool,
    show_files: bool,
    /// The two files being compared are a built-in example's (kept while
    /// the user says which is earlier).
    files_of: Option<&'static str>,
    /// Where the order question's two buttons and a problem's OK were
    /// drawn this frame.
    order_rects: [Option<Rect>; 2],
    problem_ok: Option<Rect>,
    /// The demo's phone scene: the layout in a phone outline.
    phone: bool,
    /// Where the demo's gesture targets are on screen (points), from the
    /// last frame, and the pinch onto the revision cloud: the point to zoom
    /// at and the factor.
    targets: Vec<(String, Rect)>,
    pinch: Option<(Pos2, f64)>,
    /// The window (points), this frame.
    screen: Rect,
    /// What the demo's pinch zooms onto: the revision cloud, or one small
    /// change.
    pinch_to: Option<lightable_render::Bounds>,
    /// Files dropped or chosen: when they arrived (ms, the page's clock),
    /// and, once compared, how long reading and comparing took (seconds)
    /// and how many objects each revision has.
    arrived_at: Option<f64>,
    timing: Option<(f64, usize, usize)>,
    /// The page's network requests (the browser's own count) when files
    /// last arrived.
    requests_at_open: Option<usize>,
    /// Show the request count (the large layout's video).
    show_network: bool,
    /// Where times are measured: "this device", or the recording machine.
    machine: String,
    /// Recording inside the brand stage.
    stage: Option<Stage>,
    /// When the comparison appeared (demo time): the stage reveals the
    /// change list row by row from then.
    compare_since: Option<f64>,
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
            narration,
            record,
            cut,
            feature,
            own,
            machine,
            stage,
        } = start;
        // The demo opens its example's Rev C on its own and has the two
        // files dropped in.
        let demo_example = if demo { example.clone() } else { None };
        let example = if demo { None } else { example };
        // First run, nothing to open: show the default example at once, so
        // the first thing on screen is a comparison.
        let example = if files.is_empty() && !demo && !own && (example.is_some() || bench.is_none())
        {
            Some(example.unwrap_or_default())
        } else {
            example
        };
        let incoming = crate::files::FileSlot::default();
        let drag_pos = crate::files::DragPos::default();
        #[cfg(target_arch = "wasm32")]
        if let Err(e) = crate::files::listen(&incoming, &drag_pos, &cc.egui_ctx) {
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
            demo: demo.then(|| Demo {
                example: demo_example.unwrap_or_default(),
                feature: feature.as_deref().and_then(crate::demo::Feature::from_id),
                planned_with_compare: false,
                narration,
                record,
                plan: None,
                done: 0,
                cut,
                live: Live::default(),
            }),
            now: 0.0,
            zoom_pending: false,
            demo_focus: None,
            caption_box: std::cell::Cell::new(None),
            canvas_rect: None,
            caption: None,
            caption_key: None,
            end_card: false,
            prev_now: f64::NAN,
            draws: 0,
            pointer_shown: false,
            last_press: None,
            drag_pos,
            dropped: false,
            highlight: None,
            show_privacy: false,
            show_files: false,
            order_rects: [None; 2],
            problem_ok: None,
            files_of: None,
            phone: false,
            targets: Vec::new(),
            pinch: None,
            screen: Rect::ZERO,
            pinch_to: None,
            arrived_at: None,
            timing: None,
            requests_at_open: None,
            show_network: false,
            machine: machine.unwrap_or_else(|| "this device".to_owned()),
            stage,
            compare_since: None,
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
        self.animate_to_above(bounds, margin, now, seconds, 0.0);
    }

    /// Like `animate_to`, keeping a band of `bottom` points at the bottom of
    /// the view clear (for a caption).
    fn animate_to_above(
        &mut self,
        bounds: &lightable_render::Bounds,
        margin: f64,
        now: f64,
        seconds: f64,
        bottom: f64,
    ) {
        self.auto_fit = false;
        let mut to = self.view;
        to.fit_above(bounds, margin, bottom);
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
        // A zero-length animation lands at once.
        let t = if a.seconds > 0.0 {
            ((now - a.start) / a.seconds).clamp(0.0, 1.0)
        } else {
            1.0
        };
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
        let objects = (a.1.entities.len(), b.1.entities.len());
        self.start_compare(gl, a, b, None, None);
        // How long reading and comparing took, from the files' arrival.
        self.timing = self
            .arrived_at
            .take()
            .map(|t| ((clock_ms() - t) / 1000.0, objects.0, objects.1));
        if let Some(c) = &mut self.compare {
            c.notes = crate::intake::notes(same, c.diff());
            c.files_of = self.files_of.take();
        }
    }

    /// Two files whose names and dates do not say which came first: the
    /// user picks the earlier one.
    fn order_window(&mut self, ui: &mut egui::Ui, gl: Option<&Arc<glow::Context>>) {
        self.order_rects = [None; 2];
        let Some((a, b, same)) = self.ask_order.take() else {
            return;
        };
        let (na, nb) = (
            crate::intake::short(&a.0).to_owned(),
            crate::intake::short(&b.0).to_owned(),
        );
        let mut choice = None;
        let mut rects = [None; 2];
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
            let first = ui.button(format!("{na} is Before"));
            let second = ui.button(format!("{nb} is Before"));
            rects = [Some(first.rect), Some(second.rect)];
            if first.clicked() {
                choice = Some(true);
            }
            if second.clicked() {
                choice = Some(false);
            }
        });
        self.order_rects = rects;
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
            // A built-in example's own files (in either order): its note
            // too, once they are compared.
            self.files_of = crate::examples::EXAMPLES
                .iter()
                .find(|e| {
                    let (x, y) = (read[0].3.as_slice(), read[1].3.as_slice());
                    (e.a, e.b) == (x, y) || (e.b, e.a) == (x, y)
                })
                .map(|e| e.id);
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
        // The demo keeps its caption's band clear (not in the report, which
        // is exported as it looks, nor in the phone, captioned below).
        let band = self.caption_band();
        let bounds = match (&self.compare, &self.doc) {
            (Some(c), _) => c.bounds(),
            (None, Some(doc)) => doc.scene.bounds(Some(&doc.visible)),
            (None, None) => None,
        };
        if let Some(b) = bounds {
            self.view.fit_above(&b, 0.03, band);
        }
    }

    /// Opens the file dialog (web). Chosen files arrive in `incoming`.
    fn choose_files(&mut self, ctx: &egui::Context) {
        self.dropped = false;
        self.arrived_at = Some(clock_ms());
        self.requests_at_open = requests_so_far();
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
            let files_of = c.files_of;
            self.start_compare(gl, a, b, labels, None);
            if let Some(c) = &mut self.compare {
                c.notes = notes;
                c.side_by_side = side;
                c.files_of = files_of;
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
        // Recording: the drawing's name and how to compare it, as usual.
        if self.recording() {
            if let Some(doc) = &self.doc {
                ui.horizontal_wrapped(|ui| {
                    ui.strong(crate::intake::short(&doc.name));
                    ui.label(
                        egui::RichText::new("To compare, choose (or drop) the other revision.")
                            .color(Color32::from_rgb(0xf5, 0xd0, 0x2a)),
                    );
                });
            }
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
        self.problem_ok = None;
        let Some(p) = self.problem.clone() else {
            return;
        };
        let mut ok_rect = None;
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
                let ok = ui.button("OK");
                ok_rect = Some(ok.rect);
                if ok.clicked() {
                    close = true;
                }
            });
        });
        self.problem_ok = ok_rect;
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
        if !self.zoom_pending
            && let Some((b, margin, start)) = self.demo_zoom.take()
        {
            // Keep the change clear of the caption: two lines and padding.
            let band = self.caption_band();
            let seconds = if self.demo.as_ref().is_some_and(|d| d.cut) {
                0.0
            } else {
                1.2
            };
            self.animate_to_above(&b, margin, start.min(self.now), seconds, band);
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

thread_local! {
    /// The changed texts drawn this frame (screen box and text), for the
    /// recorder's check that no old and new text overlap.
    static CHANGED_TEXTS: std::cell::RefCell<Vec<(Rect, String)>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// Pairs of changed texts drawn over each other this frame (more than a
/// third of the smaller box covered, different words): "20" under "25"
/// would read "225". Clears the list for the next frame.
fn text_overlaps() -> usize {
    CHANGED_TEXTS.with(|t| {
        let t = std::mem::take(&mut *t.borrow_mut());
        let mut n = 0;
        for (k, (a, sa)) in t.iter().enumerate() {
            for (b, sb) in &t[k + 1..] {
                let i = a.intersect(*b);
                if sa != sb && i.is_positive() && i.area() > a.area().min(b.area()) / 3.0 {
                    n += 1;
                }
            }
        }
        n
    })
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
            // Changed texts (old and new): where they are on screen.
            use lightable_diff::view::{CHANGED, CHANGED_BEFORE};
            if (item.color == CHANGED || item.color == CHANGED_BEFORE)
                && let Some(b) = item.shape.bounds()
                && b.size()[0] < 1e9
            {
                let p0 = view.to_screen([b.min[0], b.max[1]]);
                let p1 = view.to_screen([b.max[0], b.min[1]]);
                let r = Rect::from_min_max(
                    Pos2::new(rect.min.x + p0[0] as f32, rect.min.y + p0[1] as f32),
                    Pos2::new(rect.min.x + p1[0] as f32, rect.min.y + p1[1] as f32),
                );
                CHANGED_TEXTS.with(|list| list.borrow_mut().push((r, t.text.clone())));
            }
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
    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw: &mut egui::RawInput) {
        self.live_gestures(raw);
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        self.draws += 1;
        // Recording: the recorder's frame clock; otherwise the app's.
        let now = match &self.demo {
            Some(d) if d.record => record::frame().map_or(-1.0, |f| f / RECORD_FPS),
            _ => ui.input(|i| i.time),
        };
        // A recording draws some frames more than once: waits count frames.
        let new_frame = !self.recording() || now != self.prev_now;
        self.prev_now = now;
        if ui.input(|i| i.pointer.primary_pressed()) {
            self.last_press = Some(now);
        }
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
        if !dropped.is_empty() {
            self.dropped = true;
            self.arrived_at = Some(clock_ms());
            self.requests_at_open = requests_so_far();
        }
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
                self.to_read = Some((files, reorder, wait - u8::from(new_frame)));
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
        self.now = now;
        self.screen = ui.ctx().content_rect();
        if self.step_anim(now) && !self.recording() {
            ui.ctx().request_repaint();
        }
        if self.demo.is_some() {
            self.run_demo(ui, now, gl.as_ref());
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
        // Recording a video for phones: everything 1.75x larger.
        // The report layout stays at normal scale: it shows the document as
        // exported. The phone scene: a phone's width (320 points) in the
        // outline.
        self.zoom_pending = false;
        if self.recording() {
            // egui's own animations (fades, list scrolling) and tooltips run
            // on the wall clock: off while recording, so every frame depends
            // only on the demo clock.
            if ui.ctx().global_style().animation_time > 0.0 {
                ui.ctx().all_styles_mut(|style| {
                    style.animation_time = 0.0;
                    style.scroll_animation = egui::style::ScrollAnimation::none();
                    style.interaction.tooltip_delay = f32::INFINITY;
                    style.interaction.show_tooltips_only_when_still = true;
                });
            }
            let r = ui.ctx().content_rect();
            let reporting = self.compare.as_ref().is_some_and(|c| c.reporting());
            let zoom = if reporting {
                1.0
            } else if self.phone {
                let css_height = r.height() * ui.ctx().zoom_factor();
                phone_rects(Rect::from_min_size(Pos2::ZERO, egui::vec2(1.0, css_height)))
                    .1
                    .width()
                    / 320.0
            } else if let Some(s) = self.stage {
                s.zoom
            } else {
                1.75
            };
            if (ui.ctx().zoom_factor() - zoom).abs() > 1e-3 {
                ui.ctx().set_zoom_factor(zoom);
                // Sizes change next frame: fit the demo's zoom then.
                self.zoom_pending = true;
            }
        }
        let network = self
            .requests_since_open()
            .filter(|_| self.show_network)
            .map(|n| {
                // Exactly what is counted: entries in the browser's
                // Resource Timing list (fetches, scripts, images and other
                // resources the page loads).
                format!("Requests the browser logged since the files opened: {n} (Resource Timing)")
            });
        let timing = self
            .timing
            .filter(|_| !self.recording() || self.show_network);
        let reveal_redrawn = self.lit() == Some("redrawn");
        if self.compare.is_none() {
            self.compare_since = None;
        }
        if let Some(c) = &mut self.compare {
            // (In recordings, only the large layout's video shows it: in a
            // 9:16 frame it would take the drawing's room.)

            // (Recordings give the objects in a caption: the line stays
            // short enough for a 9:16 frame.)
            let recording = self.demo.as_ref().is_some_and(|d| d.record);
            c.timing_line = timing.map(|(s, a, b)| {
                let objects = if recording {
                    String::new()
                } else {
                    format!(
                        " ({} and {} objects)",
                        crate::demo::count(a),
                        crate::demo::count(b)
                    )
                };
                format!("Read and compared in {s:.1} s on {}{objects}", self.machine)
            });
            // The demo's "redrawn the same" step brings that line into view.
            c.reveal_redrawn = reveal_redrawn;
            // On the stage the list comes in row by row.
            if self.stage.is_some() && recording {
                let since = *self.compare_since.get_or_insert(self.now);
                c.reveal_age = Some(self.now - since);
            }
            c.network_line = network;
            c.show_privacy = self.show_privacy;
            c.show_files = self.show_files;
            c.row_rects.clear();
            c.legend_rect = None;
            c.list_rect = None;
        }
        if self.compare.as_ref().is_some_and(|c| c.reporting()) {
            self.report(ui, new_frame);
        } else if self.phone {
            // The same comparison, laid out in a phone outline as on a phone.
            let screen = ui.ctx().content_rect();
            let (body, inner) = phone_rects(screen);
            let painter = ui.painter();
            painter.rect_filled(screen, 0.0, Color32::from_rgb(14, 14, 18));
            let round = body.width() * 0.13;
            painter.rect_filled(body, round, Color32::from_gray(40));
            painter.rect_stroke(
                body,
                round,
                egui::Stroke::new(2.0, Color32::from_gray(120)),
                egui::StrokeKind::Inside,
            );
            painter.rect_filled(inner, 0.0, BACKGROUND);
            ui.scope_builder(egui::UiBuilder::new().max_rect(inner), |ui| {
                ui.set_clip_rect(inner);
                self.layout(ui, gl.as_ref(), now, inner);
            });
        } else {
            let screen = ui.ctx().content_rect();
            self.layout(ui, gl.as_ref(), now, screen);
        }
        self.problem_window(ui, gl.as_ref());
        self.order_window(ui, gl.as_ref());
        // Nothing over the report while it is captured for the PNG.
        if !self
            .compare
            .as_ref()
            .is_some_and(|c| c.waiting_for_capture())
        {
            self.overlays(ui);
        }
        if self.demo.is_some() {
            self.find_targets();
        }
        if self.recording() {
            self.publish_frame_info(ui);
            record::rendered(self.now * RECORD_FPS, self.draws);
        }

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
    /// The app's layout in `screen` (the window, or the phone outline's
    /// screen in the demo): toolbar, summary, change list, status bar,
    /// layers and the drawing.
    fn layout(
        &mut self,
        ui: &mut egui::Ui,
        gl: Option<&Arc<glow::Context>>,
        now: f64,
        screen: Rect,
    ) {
        let gl = gl.cloned();
        // Narrow or portrait screens (phones): the change list goes under
        // the drawing and rows wrap.
        let narrow = is_narrow(screen.width(), screen.height());
        let recording = self.recording();
        if let Some(c) = &mut self.compare {
            c.narrow = narrow;
            c.short = is_short(screen.height());
            c.record = recording;
        }
        // Recording: the frame's top-left 10 % kept clear (a video player
        // puts buttons there).
        let inset = match self.stage {
            Some(s) if recording => s.inset / s.zoom,
            _ if recording => screen.width() * 0.1,
            _ => 0.0,
        };
        egui::Panel::top("toolbar").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.add_space(inset);
                ui.vertical(|ui| self.toolbar(ui, gl.as_ref()));
            });
        });
        // Details: the comparison's toggle; always for a single drawing;
        // nothing to detail when nothing is open.
        let details = self
            .compare
            .as_ref()
            .map_or(self.doc.is_some(), |c| c.details);
        if let Some(c) = &mut self.compare {
            let mut zoom = None;
            let mut action = crate::compare::Action::default();
            if c.show_summary {
                egui::Panel::top("summary").show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.add_space(inset);
                        ui.vertical(|ui| zoom = c.summary_bar(ui, &mut action));
                    });
                });
            }
            let show_list = c.show_list;
            let limited = c.list_limit.is_some();
            let list = |ui: &mut egui::Ui| c.change_list(ui);
            // The demo uses fixed shares of the screen, whatever the panel
            // was before.
            let demo = self.demo.is_some();
            let picked = if !show_list {
                None
            } else if narrow {
                let panel = egui::Panel::bottom("changes-below");
                let panel = if demo {
                    // One-line rows; the drawing keeps most of the frame.
                    // Less where a feature video shows extra rows above.
                    let share = if limited {
                        0.33
                    } else if self.show_files {
                        0.12
                    } else if self.show_privacy {
                        0.15
                    } else {
                        0.18
                    };
                    panel.exact_size(screen.height() * share)
                } else {
                    panel.resizable(true).default_size(screen.height() * 0.35)
                };
                panel.show(ui, list).inner
            } else {
                let panel = egui::Panel::right("changes");
                let panel = if demo {
                    // Half the frame at the "found" step, so its four
                    // lines and the count of the rest show in full.
                    panel.exact_size(screen.width() * if limited { 0.5 } else { 0.36 })
                } else {
                    panel
                        .resizable(true)
                        .default_size(520.0_f32.min(screen.width() * 0.4))
                };
                panel.show(ui, list).inner
            };
            if let Some(b) = picked.or(zoom) {
                // Cut drafts: no animation. The demo keeps its caption's
                // band clear.
                let seconds = if self.demo.as_ref().is_some_and(|d| d.cut) {
                    0.0
                } else {
                    0.6
                };
                let band = self.caption_band();
                self.animate_to_above(&b, 0.15, now, seconds, band);
            }
            self.act(&action, gl.as_ref());
        }
        // The status bar is a detail, except to report an export or that a
        // file is being read.
        if (details || self.message.is_some()) && !self.recording() {
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
        if details && (self.doc.is_some() || self.compare.is_some()) && !narrow && !recording {
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
    }

    /// Plays the demo: Rev C opens on its own, then each step sets the
    /// scene when its time comes and captions say what is on screen.
    /// Everything a person does is a gesture through the app's normal input:
    /// browser events from the recorder, or, live, `live_gestures`.
    fn run_demo(&mut self, ui: &mut egui::Ui, now: f64, gl: Option<&Arc<glow::Context>>) {
        // Live, the demo draws continuously; recorded, a frame is drawn when
        // the recorder asks for it.
        if !self.recording() {
            ui.ctx().request_repaint();
        }
        let Some(demo) = &mut self.demo else { return };
        // A recording waits for its first frame.
        if demo.record && now < 0.0 {
            return;
        }
        // The large layout: its plan gets its numbers once its dropped files
        // are compared (the times and gestures stay the same).
        if demo.feature == Some(crate::demo::Feature::Big)
            && !demo.planned_with_compare
            && let (Some(c), Some((plan, _))) = (&self.compare, &mut demo.plan)
        {
            *plan = crate::demo::feature_plan(Some(c), crate::demo::Feature::Big);
            demo.planned_with_compare = true;
        }
        if demo.plan.is_none() {
            let id = demo.feature.map_or(demo.example.as_str(), |f| f.example());
            let e = crate::examples::find(id);
            let read = |name: &str, bytes: &[u8]| {
                lightable_dxf::read_bytes(bytes)
                    .map(|(d, _)| (name.to_owned(), Arc::new(d)))
                    .map_err(|err| format!("{name}: {err}"))
            };
            // The plan comes from comparing the example's two files: the
            // files the demo then drops in.
            let compared = read(e.a_name, e.a).and_then(|a| {
                let b = read(e.b_name, e.b)?;
                let labels = Some((e.a_label.to_owned(), e.b_label.to_owned()));
                Ok((Compare::new(None, a, b.clone(), labels, Some(e.id))?, b))
            });
            let (c, b) = match compared {
                Ok(x) => x,
                Err(err) => {
                    self.message = Some(err);
                    self.demo = None;
                    return;
                }
            };
            let plan = match demo.feature {
                Some(crate::demo::Feature::Big) => {
                    crate::demo::feature_plan(None, crate::demo::Feature::Big)
                }
                Some(f) => crate::demo::feature_plan(Some(&c), f),
                None => crate::demo::plan(&c, demo.narration),
            };
            record::publish_plan(&plan, e);
            let empty = demo.feature.is_some_and(|f| f.starts_empty());
            self.show_network = demo.feature == Some(crate::demo::Feature::Big);
            // Live, a second's pause first; recorded, from the first frame.
            let start = if demo.record { now } else { now + 1.0 };
            demo.plan = Some((plan, start));
            if empty {
                // The start screen, nothing open.
            } else {
                // Rev C on its own, as it came in.
                self.load(gl, b.0, b.1, 0);
            }
            return;
        }
        let Some((plan, start)) = &demo.plan else {
            return;
        };
        let t = now - start;
        let mut due = Vec::new();
        while let Some(shot) = plan.shots.get(demo.done).filter(|s| s.at <= t) {
            due.push(shot.clone());
            demo.done += 1;
        }
        let narration = plan.narration;
        let start = *start;
        for shot in due {
            self.play(shot, narration, start);
        }
    }

    /// Applies one demo moment (`start`: when the demo started). It only
    /// sets the scene; what a person does is a gesture.
    fn play(&mut self, shot: crate::demo::Shot, narration: bool, start: f64) {
        use crate::demo::Act;
        let at = start + shot.at;
        match shot.act {
            Act::Opening => self.pointer_shown = false,
            Act::Drop => self.pointer_shown = true,
            Act::Highlight(name) => self.highlight = Some((name, at + 0.3, at + 3.0)),
            Act::Phone => {
                self.phone = true;
                self.pointer_shown = false;
                if let Some(c) = &mut self.compare {
                    c.preview_report(false);
                    c.side_by_side = false;
                    c.details = false;
                    c.clear_selection();
                }
                self.anim = None;
                self.fit_pending = true;
            }
            Act::EndCard => {
                self.end_card = true;
                self.pointer_shown = false;
            }
            Act::Cloud => {
                self.pinch_to = self.compare.as_ref().and_then(|c| c.cloud_bounds_tight());
            }
            Act::Close(i) => {
                self.pinch_to = self.compare.as_ref().and_then(|c| c.change_bounds(i));
            }
            Act::Tap(_) | Act::SideBySide | Act::Report | Act::Nothing => {}
        }
        // Narration: the voice carries the words. Measured numbers go in as
        // the caption is shown (the large layout).
        if let Some(caption) = shot.caption {
            let caption = self.fill_in(&caption);
            self.caption = Some(caption).filter(|t| !t.is_empty() && !narration);
            // Its key number, filled in the same way (the stage colours it).
            self.caption_key = shot.key.as_deref().map(|k| self.fill_in(k)).filter(|k| {
                self.caption
                    .as_deref()
                    .is_some_and(|c| c.contains(k.as_str()))
            });
        }
        // What the step is about, for the recorder's checks.
        let c = self.compare.as_ref();
        self.demo_focus = match shot.act {
            // (Details' "redrawn the same" line: the drawing stays as it was.)
            Act::Highlight("redrawn") => self.demo_focus,
            Act::Highlight(_) => c.and_then(|c| c.bounds()),
            Act::Cloud => c.and_then(|c| c.cloud_bounds_tight()),
            Act::Close(i) => c.and_then(|c| c.change_bounds(i)),
            Act::Tap(i) => c.and_then(|c| c.change_bounds(i)),
            Act::SideBySide | Act::Nothing => self.demo_focus,
            Act::Opening | Act::Drop | Act::Report | Act::Phone | Act::EndCard => None,
        };
    }

    /// The live demo's gestures (no recorder): the same pointer moves,
    /// clicks, drop and pinch the recorder makes, fed to egui's input.
    fn live_gestures(&mut self, raw: &mut egui::RawInput) {
        use crate::demo::Target;
        use egui::Event;
        let Some(demo) = &mut self.demo else { return };
        let (Some((plan, start)), Some(time)) = (&demo.plan, raw.time) else {
            return;
        };
        if demo.record {
            return;
        }
        let t = time - start;
        let live = &mut demo.live;
        let button = |pos: Pos2, pressed: bool| Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::default(),
        };
        if live.release {
            live.release = false;
            if let Some(pos) = live.pos {
                raw.events.push(button(pos, false));
            }
        }
        let Some(g) = plan.gestures.get(live.next).copied() else {
            return;
        };
        if t < g.from {
            return;
        }
        let find = |name: &str| {
            self.targets
                .iter()
                .find(|(n, _)| n == name)
                .map(|(_, r)| *r)
        };
        // A row outside the list's view: scroll the list first (small wheel
        // steps over the list).
        if live.path.is_none()
            && matches!(g.target, Target::Row(_))
            && let (Some(row), Some(list)) = (find(&g.target.name()), find("list"))
            && !(list.expand(1.0).contains_rect(row) && self.screen.contains_rect(row))
            && live.unmoved < 3
        {
            live.unmoved = if live.scrolled == Some(row.min.y) {
                live.unmoved + 1
            } else {
                0
            };
            live.scrolled = Some(row.min.y);
            let over = list.center();
            live.pos = Some(over);
            raw.events.push(Event::PointerMoved(over));
            let down = if row.max.y > list.max.y { -7.0 } else { 7.0 };
            raw.events.push(Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(0.0, down),
                phase: egui::TouchPhase::Move,
                modifiers: egui::Modifiers::default(),
            });
            return;
        }
        if live.path.is_none() {
            // Where the target is now (last frame).
            let name = g.target.on_screen();
            let to = if g.target == Target::Pinch {
                self.pinch
            } else {
                self.targets
                    .iter()
                    .find(|(n, _)| *n == name)
                    .map(|(_, r)| (r.center(), 1.0))
            };
            let Some((to, factor)) = to else {
                // Not on screen: skip it.
                live.next += 1;
                return;
            };
            // The first gesture comes in from the right-hand edge.
            let s = self.screen;
            let from = live
                .pos
                .unwrap_or(egui::pos2(s.max.x - 2.0, s.center().y + s.height() * 0.1));
            live.path = Some((from, to, factor));
            live.unmoved = 0;
            // Clicks: the pointer takes at most 0.8 s to get there.
            live.moving_from = if matches!(g.target, Target::Drop(_) | Target::Pinch) {
                g.from
            } else {
                t.max(g.at - 0.8)
            };
        }
        let Some((p0, p1, factor)) = live.path else {
            return;
        };
        if t < g.at {
            let m = live.moving_from;
            // Arriving 0.3 s before the click: a pause on the control.
            let k = ((t - m) / (g.at - 0.3 - m).max(1e-3)).clamp(0.0, 1.0);
            let k = (k * k * (3.0 - 2.0 * k)) as f32;
            let pos = p0 + (p1 - p0) * k;
            live.pos = Some(pos);
            raw.events.push(Event::PointerMoved(pos));
            if let Target::Drop(set) = g.target {
                let n = set.files(crate::examples::find(&demo.example)).len();
                raw.hovered_files = vec![egui::HoveredFile::default(); n];
            }
            return;
        }
        live.pos = Some(p1);
        raw.events.push(Event::PointerMoved(p1));
        match g.target {
            Target::Drop(set) => {
                let e = crate::examples::find(&demo.example);
                raw.hovered_files.clear();
                raw.dropped_files = set
                    .files(e)
                    .into_iter()
                    .map(|(name, bytes, _)| {
                        Arc::new(ExampleFile {
                            path: name.into(),
                            bytes,
                        }) as egui::DroppedFileHandle
                    })
                    .collect();
            }
            Target::Pinch => {
                let p = if g.until > g.at {
                    ((t - g.at) / (g.until - g.at)).clamp(0.0, 1.0)
                } else {
                    1.0
                };
                raw.events
                    .push(Event::Zoom(factor.powf(p - live.zoomed) as f32));
                live.zoomed = p;
                if p < 1.0 {
                    return;
                }
                live.zoomed = 0.0;
            }
            _ => {
                raw.events.push(button(p1, true));
                live.release = true;
            }
        }
        live.next += 1;
        live.path = None;
    }

    /// Where the demo's gesture targets are this frame (points), and the
    /// pinch that fits the revision cloud on screen.
    fn find_targets(&mut self) {
        let mut targets = Vec::new();
        // Files are dropped on the drawing, or anywhere on the start screen.
        let open = self.doc.is_some() || self.compare.is_some();
        match self.canvas_rect.filter(|_| open) {
            Some(canvas) => targets.push(("drop".to_owned(), canvas)),
            None => targets.push((
                "drop".to_owned(),
                self.screen.shrink(self.screen.height() * 0.15),
            )),
        }
        self.pinch = None;
        if let Some(c) = self.compare.as_ref().filter(|c| !c.reporting()) {
            let [_, side, details] = c.anchors();
            for (name, rect) in [
                ("legend", c.legend_rect),
                ("side_by_side", side),
                ("details", details),
                ("export_png", c.export_rect),
                ("export_json", c.json_rect),
                ("summary", c.summary_rect),
                ("swap", c.swap_rect),
                ("privacy", c.privacy_rect),
                ("timing", c.timing_rect),
                ("network", c.network_rect),
                ("redrawn", c.redrawn_rect),
            ] {
                if let Some(rect) = rect.filter(|r| self.screen.contains_rect(*r)) {
                    targets.push((name.to_owned(), rect));
                }
            }
            // Every row, in view or not (outside the list's view it is
            // scrolled in first).
            for (i, rect) in &c.row_rects {
                targets.push((format!("row:{i}"), *rect));
            }
            if let Some(list) = c.list_rect.filter(|_| c.show_list) {
                targets.push(("list".to_owned(), list));
            }
            if let Some(link) = c.not_compared_rect {
                targets.push(("not_compared".to_owned(), link));
            }
            // The pinch: zooming about one fixed point turns the current
            // view into the one that fits the cloud above the caption.
            if let (Some(cloud), Some(canvas)) = (
                self.pinch_to.or_else(|| c.cloud_bounds_tight()),
                self.canvas_rect,
            ) && !c.side_by_side
            {
                let pane = c.panes(canvas)[0];
                let band = self.caption_band();
                let from = self.view;
                let mut to = from;
                to.fit_above(&cloud, 0.06, band);
                let (s1, s2) = (from.scale, to.scale);
                if (s2 / s1 - 1.0).abs() > 1e-6 {
                    let fixed = [
                        (to.center[0] * s2 - from.center[0] * s1) / (s2 - s1),
                        (to.center[1] * s2 - from.center[1] * s1) / (s2 - s1),
                    ];
                    let p = from.to_screen(fixed);
                    let at = egui::pos2(pane.min.x + p[0] as f32, pane.min.y + p[1] as f32);
                    if pane.contains(at) {
                        self.pinch = Some((at, s2 / s1));
                    }
                }
            }
        }
        for (k, rect) in self.order_rects.iter().enumerate() {
            if let Some(rect) = rect {
                targets.push((format!("order:{k}"), *rect));
            }
        }
        if let Some(ok) = self.problem_ok {
            targets.push(("problem_ok".to_owned(), ok));
        }
        self.targets = targets;
    }

    /// Publishes what is on screen this frame (points; `zoom` converts to
    /// CSS pixels) for the recorder.
    fn publish_frame_info(&self, ui: &egui::Ui) {
        let r = |r: Rect| format!("[{},{},{},{}]", r.min.x, r.min.y, r.max.x, r.max.y);
        let screen = ui.ctx().content_rect();
        let canvas = self
            .canvas_rect
            .filter(|_| self.compare.as_ref().is_some_and(|c| !c.reporting()));
        let mut focus = Vec::new();
        if let (Some(c), Some(canvas), Some(b)) = (&self.compare, canvas, self.demo_focus) {
            for pane in c.panes(canvas) {
                let mut v = self.view;
                v.size = [f64::from(pane.width()), f64::from(pane.height())];
                let p0 = v.to_screen([b.min[0], b.max[1]]);
                let p1 = v.to_screen([b.max[0], b.min[1]]);
                focus.push(r(Rect::from_min_max(
                    egui::pos2(pane.min.x + p0[0] as f32, pane.min.y + p0[1] as f32),
                    egui::pos2(pane.min.x + p1[0] as f32, pane.min.y + p1[1] as f32),
                )));
            }
        }
        // The selected change, where it is drawn (the first pane), and the
        // files compared: the stage's highlights and file cards must match.
        let mut selected = "null".to_owned();
        let mut selected_rect = "null".to_owned();
        let mut files = "null".to_owned();
        if let Some(c) = self.compare.as_ref() {
            let (a, b) = c.file_names();
            files = format!("[{},{}]", record::json_string(a), record::json_string(b));
            if let Some(i) = c.selected() {
                selected = i.to_string();
                if let (Some(canvas), Some(b)) = (canvas, c.change_bounds(i))
                    && let Some(pane) = c.panes(canvas).first()
                {
                    let mut v = self.view;
                    v.size = [f64::from(pane.width()), f64::from(pane.height())];
                    let p0 = v.to_screen([b.min[0], b.max[1]]);
                    let p1 = v.to_screen([b.max[0], b.min[1]]);
                    selected_rect = r(Rect::from_min_max(
                        egui::pos2(pane.min.x + p0[0] as f32, pane.min.y + p0[1] as f32),
                        egui::pos2(pane.min.x + p1[0] as f32, pane.min.y + p1[1] as f32),
                    ));
                }
            }
        }
        let caption = self
            .caption_box
            .get()
            .map_or("null".to_owned(), |(size, rect, rows)| {
                format!("{{\"size\":{size},\"rect\":{},\"rows\":{rows}}}", r(rect))
            });
        // The next moment something happens: a step, or a gesture starting.
        let (step, next) = self.demo.as_ref().map_or((0, None), |d| {
            let next = d.plan.as_ref().and_then(|(p, start)| {
                let t = self.now - start;
                let shot = p.shots.get(d.done).map(|s| s.at);
                let gesture = p.gestures.iter().map(|g| g.from).find(|&f| f > t);
                match (shot, gesture) {
                    (Some(a), Some(b)) => Some(a.min(b)),
                    (a, b) => a.or(b),
                }
            });
            (d.done, next)
        });
        let busy = self.busy();
        let still = self.anim.is_none()
            && self.demo_zoom.is_none()
            && !self.zoom_pending
            && self.to_read.is_none()
            && !busy
            && ui.input(|i| i.raw.hovered_files.is_empty());
        let c = self.compare.as_ref();
        let targets: Vec<String> = self
            .targets
            .iter()
            .map(|(n, rect)| format!("{}:{}", record::json_string(n), r(*rect)))
            .collect();
        let json = format!(
            "{{\"step\":{step},\"zoom\":{},\"screen\":{},\"canvas\":{},\"focus\":[{}],\"caption\":{caption},\"still\":{still},\"next\":{},\"end_card\":{},\"report\":{},\"list\":{},\"list_tail\":{},\"draws\":{},\"busy\":{busy},\"pointer\":{},\"targets\":{{{}}},\"pinch\":{},\"dropped\":{},\"example_files\":{},\"phone\":{},\"legend\":{},\"message\":{},\"view\":\"{}\",\"text_overlaps\":{},\"cut_rows\":{},\"caption_text\":{},\"selected\":{selected},\"selected_rect\":{selected_rect},\"files\":{files},\"stage\":{},\"caption_key\":{}}}",
            ui.ctx().zoom_factor(),
            r(screen),
            canvas.map_or("null".to_owned(), r),
            focus.join(","),
            next.map_or("null".to_owned(), |t| t.to_string()),
            self.end_card,
            c.is_some_and(|c| c.reporting()),
            c.is_some_and(|c| c.show_list),
            c.and_then(|c| c
                .list_tail_visible
                .filter(|_| c.show_list && c.list_limit.is_some()))
                .map_or("null".to_owned(), |v| v.to_string()),
            self.draws,
            self.pointer_pos(ui)
                .map_or("null".to_owned(), |p| format!("[{},{}]", p.x, p.y)),
            targets.join(","),
            self.pinch
                .map_or("null".to_owned(), |(p, f)| format!("[{},{},{f}]", p.x, p.y)),
            self.dropped && c.is_some(),
            c.is_some_and(|c| c.files_of.is_some()),
            self.phone,
            self.lit().is_some(),
            self.message
                .as_deref()
                .map_or("null".to_owned(), record::json_string),
            self.view_name(),
            text_overlaps(),
            c.filter(|c| c.show_list).map_or(0, |c| c.cut_rows),
            self.caption
                .as_deref()
                .map_or("null".to_owned(), record::json_string),
            self.stage.is_some(),
            self.caption_key
                .as_deref()
                .map_or("null".to_owned(), record::json_string),
        );
        record::publish_frame_info(&json);
    }

    /// What the screen shows, in the plan's terms (`demo::View`).
    fn view_name(&self) -> &'static str {
        if self.end_card {
            "end"
        } else if self.phone {
            "phone"
        } else if let Some(c) = &self.compare {
            if c.reporting() {
                "report"
            } else if c.details {
                "details"
            } else if c.side_by_side {
                "side_by_side"
            } else {
                "overlay"
            }
        } else if self.doc.is_some() {
            "single"
        } else {
            "start"
        }
    }

    /// Something is under way outside the frame clock: files being read
    /// by the browser, or the report being captured. The recorder draws the
    /// same frame again until it is done.
    fn busy(&self) -> bool {
        self.incoming.lock().is_ok_and(|s| s.expected > 0)
            || self
                .compare
                .as_ref()
                .is_some_and(|c| c.waiting_for_capture())
    }

    /// Where the demo's pointer is (points), when it is shown: dragged
    /// files, or the mouse.
    fn pointer_pos(&self, ui: &egui::Ui) -> Option<Pos2> {
        if !self.pointer_shown || self.phone || self.end_card {
            return None;
        }
        let dragging = ui.input(|i| !i.raw.hovered_files.is_empty());
        let drag = self.drag_pos.lock().ok().and_then(|d| *d).map(|(x, y)| {
            let z = ui.ctx().zoom_factor();
            egui::pos2(x / z, y / z)
        });
        let mouse = ui.input(|i| i.pointer.latest_pos());
        if dragging {
            drag.or(mouse)
        } else {
            mouse.or(drag)
        }
    }

    /// What is highlighted now.
    fn lit(&self) -> Option<&'static str> {
        self.highlight
            .filter(|(_, a, b)| self.now >= *a && self.now < *b)
            .map(|(name, _, _)| name)
    }

    /// Recording the demo frame by frame (`?demo&record`).
    fn recording(&self) -> bool {
        self.demo.as_ref().is_some_and(|d| d.record)
    }

    /// The height captions are sized for (points): the screen, or on the
    /// stage the whole video frame (the app's window is smaller).
    fn caption_reference(&self, screen: Rect) -> f32 {
        self.stage
            .map_or(screen.height(), |s| s.frame_height / s.zoom)
    }

    /// How much of the drawing's bottom a caption covers (points): the
    /// demo's fits and zooms keep it clear. Captions sit above the bottom
    /// 15 % of the frame (where a video player puts its controls).
    fn caption_band(&self) -> f64 {
        if !self.captions_possible()
            || self.phone
            || self.compare.as_ref().is_some_and(|c| c.reporting())
        {
            return 0.0;
        }
        let size = caption_size(self.caption_reference(self.screen));
        let Some(canvas) = self.canvas_rect else {
            return f64::from(size * 3.3);
        };
        // Two lines and their padding above the caption's lowest point.
        let top = caption_bottom(self.screen, canvas, size) - 3.1 * size;
        f64::from((canvas.max.y - top).max(0.0))
    }

    /// A caption's measured numbers: objects, seconds to read and compare,
    /// network requests since the files were opened.
    fn fill_in(&self, caption: &str) -> String {
        let mut out = caption.to_owned();
        if let Some((seconds, _, b)) = self.timing {
            out = out
                .replace("{objects}", &crate::demo::count(b))
                .replace("{seconds}", &format!("{seconds:.1}"));
        }
        if let Some(n) = self.requests_since_open() {
            out = out.replace("{requests}", &n.to_string());
        }
        if let Some(c) = &self.compare {
            let n = c.design_count();
            out = out.replace(
                "{changes}",
                &if n == 1 {
                    "1 change".to_owned()
                } else {
                    format!("{n} changes")
                },
            );
        }
        out
    }

    /// Network requests since the files were opened (the browser's count).
    fn requests_since_open(&self) -> Option<usize> {
        Some(requests_so_far()?.saturating_sub(self.requests_at_open?))
    }

    /// Captions can be on screen: the demo, except with voice-over timing
    /// (the voice carries the words).
    fn captions_possible(&self) -> bool {
        self.demo
            .as_ref()
            .is_some_and(|d| !d.plan.as_ref().map_or(d.narration, |(p, _)| p.narration))
    }

    /// Demo captions, the pointer and the end card, over everything; and,
    /// for everyone, the drop target while files are dragged over the page.
    fn overlays(&self, ui: &mut egui::Ui) {
        self.caption_box.set(None);
        let screen = ui.ctx().content_rect();
        let painter = ui.ctx().layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("demo"),
        ));
        let m = screen.width().min(screen.height());
        let blue = Color32::from_rgb(0x6f, 0x9b, 0xff);
        // Files dragged over the page: where to drop them.
        let hovered = ui.input(|i| i.raw.hovered_files.len());
        if hovered > 0 {
            painter.rect_filled(
                screen,
                0.0,
                Color32::from_rgba_unmultiplied(30, 60, 130, 90),
            );
            painter.rect_stroke(
                screen.shrink(m * 0.02),
                m * 0.02,
                egui::Stroke::new(m * 0.008, blue),
                egui::StrokeKind::Inside,
            );
            let galley = painter.layout(
                "Drop two DXF files to compare them".to_owned(),
                FontId::proportional(m * 0.055),
                Color32::WHITE,
                screen.width() * 0.8,
            );
            let rect = Rect::from_center_size(
                screen.center(),
                galley.size() + egui::vec2(m * 0.06, m * 0.04),
            );
            painter.rect_filled(
                rect,
                m * 0.02,
                Color32::from_rgba_unmultiplied(0, 0, 0, 200),
            );
            painter.galley(
                rect.min + egui::vec2(m * 0.03, m * 0.02),
                galley,
                Color32::WHITE,
            );
        }
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
        // The stage shows its own end card.
        if self.end_card && self.stage.is_some() {
            return;
        }
        if self.end_card {
            painter.rect_filled(
                screen,
                0.0,
                Color32::from_rgba_unmultiplied(12, 12, 14, 245),
            );
            let c = screen.center();
            // Each line centred, also when one wraps (the privacy line on a
            // 9:16 screen).
            let line = |y: f32, text: &str, size: f32, color: Color32| {
                let mut job = egui::text::LayoutJob::simple(
                    text.to_owned(),
                    FontId::proportional(size),
                    color,
                    screen.width() * 0.9,
                );
                job.halign = egui::Align::Center;
                let galley = painter.layout_job(job);
                let at = egui::pos2(c.x, c.y + y - galley.size().y / 2.0);
                painter.galley(at, galley, color);
            };
            line(-m * 0.12, "Free. Open source.", m / 14.0, Color32::WHITE);
            line(
                0.0,
                "Your files are read in your browser, never uploaded.",
                m / 20.0,
                Color32::from_gray(220),
            );
            line(m * 0.12, "lightable.dev", m / 17.0, blue);
            return;
        }
        // The colour legend (or the summary, or the privacy line), briefly
        // highlighted.
        let lit = self.lit().and_then(|name| {
            let c = self.compare.as_ref()?;
            match name {
                "legend" => c.legend_rect,
                "summary" => c.summary_rect,
                "privacy" => c.privacy_rect,
                "timing" => c.timing_rect,
                "network" => c.network_rect,
                "redrawn" => c.redrawn_rect,
                _ => None,
            }
        });
        if let Some(lit) = lit.filter(|_| self.stage.is_none()) {
            let rect = lit.expand(5.0);
            painter.rect_filled(rect, 6.0, Color32::from_rgba_unmultiplied(255, 224, 96, 28));
            painter.rect_stroke(
                rect,
                6.0,
                egui::Stroke::new(2.5, Color32::from_rgb(0xff, 0xe0, 0x60)),
                egui::StrokeKind::Outside,
            );
        }
        if let Some(text) = self.caption.as_deref().filter(|t| !t.is_empty()) {
            // Under the drawing; in the phone scene, under the phone.
            let area = if self.phone {
                screen
            } else {
                self.canvas_rect.unwrap_or(screen)
            };
            // Two lines at most: smaller only if it does not fit.
            let mut size = caption_size(self.caption_reference(screen));
            let layout = |size: f32| {
                let pad = egui::vec2(size * 0.35, size * 0.3);
                let galley = painter.layout(
                    text.to_owned(),
                    FontId::proportional(size),
                    Color32::WHITE,
                    area.width() * 0.98 - 2.0 * pad.x,
                );
                (galley, pad)
            };
            let (mut galley, mut pad) = layout(size);
            let smallest = CAPTION_MIN * self.caption_reference(screen);
            while galley.rows.len() > 2 && size > smallest {
                size = (size * 0.95).max(smallest);
                (galley, pad) = layout(size);
            }
            let boxed = galley.size() + 2.0 * pad;
            // At the bottom of the drawing (above the frame's bottom 15 %),
            // in the band the demo's zooms keep clear.
            let bottom = caption_bottom(screen, area, caption_size(self.caption_reference(screen)));
            let rect =
                Rect::from_center_size(egui::pos2(area.center().x, bottom - boxed.y / 2.0), boxed);
            let rows = galley.rows.len();
            // On the stage, only where: the stage sets the words itself.
            if self.stage.is_none() {
                painter.rect_filled(
                    rect,
                    size * 0.3,
                    Color32::from_rgba_unmultiplied(0, 0, 0, 240),
                );
                painter.galley(rect.min + pad, galley, Color32::WHITE);
            }
            self.caption_box.set(Some((size, rect, rows)));
        }
        // The pointer: an arrow in 16:9, a fingertip in 9:16 (on the stage,
        // the stage's own).
        if let Some(pos) = self.pointer_pos(ui).filter(|_| self.stage.is_none()) {
            let pressed = ui.input(|i| i.pointer.primary_down());
            let since = self.last_press.map_or(f64::INFINITY, |t| self.now - t);
            if screen.height() > screen.width() * 1.2 {
                let radius = m * 0.055;
                let tap = pressed || since < 0.2;
                if (0.0..0.4).contains(&since) {
                    // The tap's ripple.
                    let k = (since / 0.4) as f32;
                    painter.circle_stroke(
                        pos,
                        radius * (1.0 + k),
                        egui::Stroke::new(
                            3.0,
                            Color32::from_white_alpha((200.0 * (1.0 - k)) as u8),
                        ),
                    );
                }
                let r = if tap { radius * 0.8 } else { radius };
                painter.circle_filled(
                    pos,
                    r,
                    Color32::from_white_alpha(if tap { 150 } else { 90 }),
                );
                painter.circle_stroke(pos, r, egui::Stroke::new(2.5, Color32::WHITE));
            } else {
                let s = m * 0.045 / 20.0;
                let shape: Vec<Pos2> = [
                    (0.0, 0.0),
                    (0.0, 17.0),
                    (4.2, 13.2),
                    (7.0, 19.6),
                    (9.6, 18.5),
                    (6.9, 12.2),
                    (12.4, 12.2),
                ]
                .iter()
                .map(|&(x, y)| pos + egui::vec2(x, y) * s)
                .collect();
                if (0.0..0.4).contains(&since) {
                    let k = (since / 0.4) as f32;
                    painter.circle_stroke(
                        pos,
                        s * (6.0 + 14.0 * k),
                        egui::Stroke::new(
                            2.5,
                            Color32::from_rgba_unmultiplied(
                                0x6f,
                                0x9b,
                                0xff,
                                (230.0 * (1.0 - k)) as u8,
                            ),
                        ),
                    );
                }
                painter.add(egui::Shape::convex_polygon(
                    shape,
                    Color32::WHITE,
                    egui::Stroke::new(s * 1.3, Color32::BLACK),
                ));
            }
            // Files being dragged: how many.
            if hovered > 0 {
                let galley = painter.layout_no_wrap(
                    match hovered {
                        1 => "1 file".to_owned(),
                        n => format!("{n} files"),
                    },
                    FontId::proportional(m * 0.035),
                    Color32::WHITE,
                );
                let at = pos + egui::vec2(m * 0.04, m * 0.05);
                let rect = Rect::from_min_size(at, galley.size() + egui::vec2(m * 0.03, m * 0.016));
                painter.rect_filled(rect, m * 0.01, blue);
                painter.galley(
                    at + egui::vec2(m * 0.015, m * 0.008),
                    galley,
                    Color32::WHITE,
                );
            }
        }
    }

    /// The PNG report layout: header, overlay, change list and footer,
    /// shown for a few frames and then captured. In the demo the exported
    /// report stays on screen until the next step.
    fn report(&mut self, ui: &mut egui::Ui, new_frame: bool) {
        if !self.recording() {
            ui.ctx().request_repaint();
        }
        let demo = self.demo.is_some();
        // The live demo does not download files onto a viewer's computer;
        // a recording saves into the recorder's folder.
        let save = !demo || self.recording();
        let Some(c) = &mut self.compare else { return };
        let image = ui.input(|i| {
            i.raw.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(image) = image
            && c.waiting_for_capture()
        {
            self.message = Some(c.finish_report(&image, save));
            if !demo {
                return;
            }
            c.preview_report(true);
        }
        let capture = c.report_frame(new_frame);
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
        // Narrow or portrait (a 9:16 frame): the list under the drawing.
        let screen = ui.ctx().content_rect();
        if is_narrow(screen.width(), screen.height()) {
            egui::Panel::bottom("report-changes-below")
                .exact_size(screen.height() * 0.45)
                .show(ui, |ui| c.report_list(ui));
        } else {
            egui::Panel::right("report-changes")
                .exact_size(420.0)
                .show(ui, |ui| c.report_list(ui));
        }
        egui::CentralPanel::no_frame().show(ui, |ui| self.canvas(ui));
        if capture {
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
        }
    }
}

/// The demo's phone outline in `screen`: the body and its screen, about
/// 9 by 19.5, upright in the middle, above the caption.
fn phone_rects(screen: Rect) -> (Rect, Rect) {
    let h = screen.height();
    let inner_h = h * 0.74;
    let inner = Rect::from_center_size(
        egui::pos2(screen.center().x, screen.min.y + h * 0.065 + inner_h / 2.0),
        egui::vec2(inner_h * 9.0 / 19.5, inner_h),
    );
    let body = Rect::from_min_max(
        inner.min - egui::vec2(h * 0.014, h * 0.03),
        inner.max + egui::vec2(h * 0.014, h * 0.03),
    );
    (body, inner)
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
mod record_tests {
    #[test]
    fn json_strings_are_escaped() {
        assert_eq!(
            super::record::json_string("a\"b\\c\n"),
            r#""a\"b\\c\u000a""#
        );
    }
}

#[cfg(test)]
mod demo_tests {
    /// Captions stay long enough to read at 3 words a second.
    #[test]
    fn captions_stay_long_enough_to_read() {
        let headline =
            "Circle diameter changed from 13.5 mm to 17.5 mm; centre moved 5 mm right and 5 mm up";
        assert_eq!(headline.split_whitespace().count(), 18);
        assert!(crate::demo::caption_seconds(headline) >= 18.0 / 3.0);
        assert_eq!(
            crate::demo::caption_seconds("Lightable found 10 changes."),
            3.0
        );
        assert_eq!(crate::demo::caption_seconds(""), 3.0);
    }

    /// Captions start at 4.5 % of the screen height.
    #[test]
    fn caption_size_is_a_share_of_the_height() {
        assert!((super::caption_size(1080.0) - 48.6).abs() < 1e-3);
        assert!((super::caption_size(714.0) - 32.13).abs() < 1e-2);
        const { assert!(super::CAPTION_MIN < 0.045) };
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
