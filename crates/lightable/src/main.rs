//! The drafting app (working name). With no command, opens the viewer:
//!
//!     lightable [FILE.dxf]                       open the viewer
//!     lightable --bench FILE.dxf [FRAMES]        animated pan/zoom, print frame times
//!     lightable --shot FILE.dxf OUT.ppm          save a screenshot of the window and quit
//!     lightable info FILE.dxf                    summary and warnings
//!     lightable convert IN.dxf OUT.dxf r12|r2018 write as R12 or R2018
//!     lightable generate IN.dxf OUT.dxf r12|r2018 always regenerate (never reuse bytes)
//!     lightable save IN.dxf OUT.dxf              save back (unchanged = identical)
//!     lightable diff A.dxf B.dxf [--json OUT | --svg OUT]  list changes from A to B
//!                                                (--svg: the overlay as SVG)
//!     lightable --diff A.dxf B.dxf [--side]      compare A and B in the viewer
//!     lightable --example [bracket|plan] [--side]  compare a built-in example
//!     lightable --demo [--narration] [--feature ID]  play the demo (voice-over timing, a feature video)
//!     lightable demo-cues OUT_DIR               subtitles (SRT, WebVTT), voice-over cue sheet,
//!                                                and the large layout (OUT_DIR/usecases)
//!     lightable --diff A B --report              save the PNG change report and quit
//!     lightable --diff A.dxf B.dxf --shot OUT.ppm   ... save a screenshot and quit
//!
//! On the web it runs in the canvas with id `cad_canvas` (see `web/` and
//! `tools/build_web.sh`); drop one file to view it or two to compare them,
//! or add `?example` to the address to open the built-in example.

mod app;
mod compare;
mod demo;
#[cfg(not(target_arch = "wasm32"))]
mod depot;
mod examples;
mod feedback;
mod files;
mod gpu;
mod intake;

/// The product name shown in the app and its reports.
pub const APP_NAME: &str = "Lightable";
/// Where the project lives (feedback, report footers).
pub const REPO_URL: &str = "https://github.com/lightable-dev/lightable";

#[cfg(not(target_arch = "wasm32"))]
mod cli {
    use lightable_dxf::{Drawing, Version, Warning};

    fn print_warnings(path: &str, warnings: &[Warning]) {
        for w in warnings {
            println!("warning: {path}: {w}");
        }
    }

    fn load(path: &str) -> Result<Drawing, String> {
        let (drawing, warnings) =
            lightable_dxf::read_file(path).map_err(|e| format!("{path}: {e}"))?;
        print_warnings(path, &warnings);
        Ok(drawing)
    }

    /// The overlay of two drawings as SVG, made by the comparison itself.
    fn overlay_svg(a: &Drawing, b: &Drawing) -> String {
        let diff = lightable_diff::diff(a, b, &lightable_diff::DiffOptions::default());
        let (sa, sb) = (lightable_render::build(a), lightable_render::build(b));
        lightable_render::svg::to_svg(&lightable_diff::view::overlay(&sa, &sb, &diff), None)
    }

    /// Runs a command-line command; `None` if `args` are not one.
    pub fn run(args: &[String]) -> Option<Result<(), String>> {
        Some(match args {
            [cmd, input, output, version] if cmd == "convert" || cmd == "generate" => (|| {
                let version = match version.to_ascii_lowercase().as_str() {
                    "r12" => Version::R12,
                    "r2018" => Version::R2018,
                    other => return Err(format!("unknown version {other:?}; use r12 or r2018")),
                };
                let drawing = load(input)?;
                let warnings = if cmd == "generate" {
                    let (bytes, warnings) =
                        drawing.generate_bytes(version).map_err(|e| e.to_string())?;
                    std::fs::write(output, bytes).map_err(|e| format!("{output}: {e}"))?;
                    warnings
                } else {
                    drawing
                        .write_file(output, version)
                        .map_err(|e| format!("{output}: {e}"))?
                };
                print_warnings(output, &warnings);
                Ok(())
            })(
            ),
            [cmd, input, output] if cmd == "save" => (|| {
                let drawing = load(input)?;
                let warnings = drawing
                    .save_file(output)
                    .map_err(|e| format!("{output}: {e}"))?;
                print_warnings(output, &warnings);
                Ok(())
            })(),
            [cmd, a, b, rest @ ..] if cmd == "diff" && matches!(rest, [] | [_, _]) => (|| {
                let (da, db) = (load(a)?, load(b)?);
                let diff = lightable_diff::diff(&da, &db, &lightable_diff::DiffOptions::default());
                for c in &diff.changes {
                    let markup = if c.markup { " (revision markup)" } else { "" };
                    println!("{}{markup}\n    {c}", c.headline);
                }
                println!(
                    "{} added, {} deleted, {} changed, {} unchanged, {} equivalent",
                    diff.count(lightable_diff::ChangeKind::Added),
                    diff.count(lightable_diff::ChangeKind::Deleted),
                    diff.count(lightable_diff::ChangeKind::Changed),
                    diff.unchanged,
                    diff.equivalent
                );
                for n in &diff.not_compared {
                    println!("not compared yet: {n}");
                }
                match rest {
                    [flag, out] if flag == "--json" => {
                        std::fs::write(out, diff.to_json(a, b)).map_err(|e| format!("{out}: {e}"))
                    }
                    [flag, out] if flag == "--svg" => std::fs::write(out, overlay_svg(&da, &db))
                        .map_err(|e| format!("{out}: {e}")),
                    [flag, _] => Err(format!("unknown option {flag}")),
                    _ => Ok(()),
                }
            })(
            ),
            [cmd, out] if cmd == "demo-cues" => (|| {
                // Subtitles and the voice-over cue sheet for the bracket demo,
                // from the same plan the app plays.
                let e = crate::examples::find("bracket");
                let read = |b: &[u8]| {
                    lightable_dxf::read_bytes(b)
                        .map(|(d, _)| std::sync::Arc::new(d))
                        .map_err(|e| e.to_string())
                };
                let c = crate::compare::Compare::new(
                    None,
                    (e.a_name.into(), read(e.a)?),
                    (e.b_name.into(), read(e.b)?),
                    Some((e.a_label.into(), e.b_label.into())),
                    Some(e.id),
                )?;
                let dir = std::path::Path::new(out);
                std::fs::create_dir_all(dir).map_err(|e| format!("{out}: {e}"))?;
                let write = |name: &str, text: String| {
                    std::fs::write(dir.join(name), text).map_err(|e| format!("{name}: {e}"))
                };
                for (narration, stem) in [(false, "demo"), (true, "demo-narration")] {
                    let plan = crate::demo::plan(&c, narration);
                    write(&format!("{stem}.srt"), crate::demo::srt(&plan.cues))?;
                    write(&format!("{stem}.vtt"), crate::demo::vtt(&plan.cues))?;
                    println!("{stem}: {:.2} s, {} cues", plan.length, plan.cues.len());
                    if narration {
                        write("cue-sheet.md", crate::demo::cue_sheet(&plan))?;
                    }
                }
                // The use-case videos' captions, each from its example (the
                // large layout's come from the recording: its numbers are
                // measured as it plays).
                for f in crate::demo::Feature::ALL {
                    let c = if f == crate::demo::Feature::Big {
                        // The large layout, made here as for the recorder.
                        crate::compare::Compare::new(
                            None,
                            (
                                "depot-rev-a.dxf".into(),
                                read(crate::depot::depot(false).as_bytes())?,
                            ),
                            (
                                "depot-rev-b.dxf".into(),
                                read(crate::depot::depot(true).as_bytes())?,
                            ),
                            None,
                            None,
                        )?
                    } else {
                        let e = crate::examples::find(f.example());
                        crate::compare::Compare::new(
                            None,
                            (e.a_name.into(), read(e.a)?),
                            (e.b_name.into(), read(e.b)?),
                            Some((e.a_label.into(), e.b_label.into())),
                            Some(e.id),
                        )?
                    };
                    let plan = crate::demo::feature_plan(Some(&c), f);
                    let stem = format!("feature-{}", f.id());
                    // (The large layout's captions come from the recording:
                    // its numbers are measured as it plays.)
                    if f != crate::demo::Feature::Big {
                        write(&format!("{stem}.srt"), crate::demo::srt(&plan.cues))?;
                        write(&format!("{stem}.vtt"), crate::demo::vtt(&plan.cues))?;
                    }
                    // The voice-over: cue sheet and a guide to record to.
                    // The large layout's counts are known here; its request
                    // count only as it plays.
                    let n = c.design_count();
                    let sheet = crate::demo::voice_sheet(&plan)
                        .replace(
                            "{changes}",
                            &if n == 1 {
                                "1 change".into()
                            } else {
                                format!("{n} changes")
                            },
                        )
                        .replace("{objects}", &crate::demo::count(c.object_count()))
                        .replace("{requests}", "[as shown on screen]");
                    write(&format!("{stem}-voice.md"), sheet)?;
                    write(&format!("{stem}-voice.srt"), crate::demo::srt(&plan.voice))?;
                    println!("{stem}: {:.2} s, {} cues", plan.length, plan.cues.len());
                }
                // The large layout the recorder drops (not committed).
                let big = dir.join("usecases");
                std::fs::create_dir_all(&big).map_err(|e| format!("{}: {e}", big.display()))?;
                for (rev_b, name) in [(false, "depot-rev-a.dxf"), (true, "depot-rev-b.dxf")] {
                    std::fs::write(big.join(name), crate::depot::depot(rev_b))
                        .map_err(|e| format!("{name}: {e}"))?;
                }
                Ok(())
            })(),
            [cmd, path] if cmd == "info" => (|| {
                let drawing = load(path)?;
                println!(
                    "{path}: version {}, {} layers, {} entities",
                    drawing.header.acadver().unwrap_or("unknown"),
                    drawing.tables.layers.len(),
                    drawing.entities.len()
                );
                Ok(())
            })(),
            _ => return None,
        })
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(result) = cli::run(&args) {
        return match result {
            Ok(()) => std::process::ExitCode::SUCCESS,
            Err(message) => {
                eprintln!("{message}");
                std::process::ExitCode::FAILURE
            }
        };
    }
    let mut side_by_side = false;
    let mut report_and_quit = false;
    let mut demo = false;
    let mut narration = false;
    let mut feature = None;
    let mut example = None;
    let (bench, paths) = match args.as_slice() {
        [flag, path, rest @ ..] if flag == "--bench" => {
            let frames = rest.first().and_then(|n| n.parse().ok()).unwrap_or(600);
            (Some(app::Bench::new(frames)), vec![path.clone()])
        }
        [flag, path, out] if flag == "--shot" => (
            Some(app::Bench::screenshot(out.clone())),
            vec![path.clone()],
        ),
        [flag, rest @ ..]
            if flag == "--example" || flag == "--demo" || (flag == "--diff" && rest.len() >= 2) =>
        {
            let (paths, rest) = if flag == "--example" || flag == "--demo" {
                demo = flag == "--demo";
                // `--example [ID]`: an optional example id.
                match rest.first() {
                    Some(id) if !id.starts_with("--") => {
                        example = Some(id.clone());
                        (Vec::new(), &rest[1..])
                    }
                    _ => {
                        example = Some(String::new());
                        (Vec::new(), rest)
                    }
                }
            } else {
                (vec![rest[0].clone(), rest[1].clone()], &rest[2..])
            };
            let mut bench = None;
            let mut rest = rest.iter();
            while let Some(opt) = rest.next() {
                match (
                    opt.as_str(),
                    (opt == "--shot" || opt == "--feature")
                        .then(|| rest.next())
                        .flatten(),
                ) {
                    ("--feature", Some(id)) => feature = Some(id.clone()),
                    ("--side", _) => side_by_side = true,
                    ("--narration", _) => narration = true,
                    ("--report", _) => report_and_quit = true,
                    ("--shot", Some(out)) => bench = Some(app::Bench::screenshot(out.clone())),
                    _ => {
                        eprintln!(
                            "usage: lightable --diff A.dxf B.dxf | --example [bracket|plan] | --demo [--narration] [--feature find|explain|compare|report|private]  [--side] [--report] [--shot OUT.ppm]"
                        );
                        return std::process::ExitCode::FAILURE;
                    }
                }
            }
            (bench, paths)
        }
        [path] => (None, vec![path.clone()]),
        [] => (None, Vec::new()),
        _ => {
            eprintln!(
                "usage: lightable [FILE.dxf] | --diff A.dxf B.dxf | --bench FILE.dxf [FRAMES] | info|convert|generate|save|diff ..."
            );
            return std::process::ExitCode::FAILURE;
        }
    };
    let mut files = Vec::new();
    for path in paths {
        match std::fs::read(&path) {
            Ok(bytes) => files.push((path, bytes)),
            Err(e) => {
                eprintln!("{path}: {e}");
                return std::process::ExitCode::FAILURE;
            }
        }
    }
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_title("Lightable"),
        // The benchmark measures frames as fast as they can be drawn.
        glow_options: eframe::egui_glow::GlowConfiguration {
            vsync: bench.is_none(),
            ..Default::default()
        },
        ..Default::default()
    };
    let result = eframe::run_native(
        "lightable",
        options,
        Box::new(move |cc| {
            Ok(Box::new(app::CadApp::new(
                cc,
                app::Start {
                    files,
                    example,
                    bench,
                    side_by_side,
                    report_and_quit,
                    demo,
                    narration,
                    record: false,
                    cut: false,
                    feature,
                    own: false,
                    machine: None,
                    stage: None,
                },
            )))
        }),
    );
    match result {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            std::process::ExitCode::FAILURE
        }
    }
}

#[cfg(target_arch = "wasm32")]
fn main() {
    use eframe::wasm_bindgen::JsCast as _;
    wasm_bindgen_futures::spawn_local(async {
        let Some(canvas) = eframe::web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.get_element_by_id("cad_canvas"))
            .and_then(|e| e.dyn_into::<eframe::web_sys::HtmlCanvasElement>().ok())
        else {
            return;
        };
        // The page opens with the default example; `?example=plan` picks
        // another, and `?demo` plays the demo sequence.
        let search = eframe::web_sys::window()
            .and_then(|w| w.location().search().ok())
            .unwrap_or_default();
        let params: Vec<&str> = search.trim_start_matches('?').split('&').collect();
        let example = params
            .iter()
            .find_map(|p| p.strip_prefix("example="))
            .map(str::to_owned);
        let demo = params
            .iter()
            .any(|p| *p == "demo" || p.starts_with("demo="));
        let flag = |name: &str| params.contains(&name);
        let start = app::Start {
            example,
            demo,
            narration: flag("narration"),
            record: flag("record"),
            cut: flag("cut"),
            feature: params
                .iter()
                .find_map(|p| p.strip_prefix("feature="))
                .map(str::to_owned),
            own: flag("own"),
            machine: params
                .iter()
                .find_map(|p| p.strip_prefix("machine="))
                .and_then(|m| js_sys::decode_uri_component(&m.replace('+', " ")).ok())
                .map(String::from),
            // The brand stage (tools/stage/): `stage&zoom=..&frameh=..&inset=..`.
            stage: flag("stage").then(|| {
                let num = |name: &str, fallback: f32| {
                    params
                        .iter()
                        .find_map(|p| p.strip_prefix(name)?.strip_prefix('='))
                        .and_then(|v| v.parse::<f32>().ok())
                        .filter(|v| v.is_finite() && *v > 0.0)
                        .unwrap_or(fallback)
                };
                app::Stage {
                    zoom: num("zoom", 1.75),
                    frame_height: num("frameh", 540.0),
                    inset: num("inset", 0.0),
                }
            }),
            ..Default::default()
        };
        let _ = eframe::WebRunner::new()
            .start(
                canvas,
                eframe::WebOptions::default(),
                Box::new(|cc| Ok(Box::new(app::CadApp::new(cc, start)))),
            )
            .await;
    });
}
