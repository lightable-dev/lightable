//! What the web page shows while the app loads (run by
//! `tools/build_web.sh`): the overlay of two drawings as SVG, and their
//! change headlines as static HTML, both made by the comparison itself.
//!
//!     cargo run --release -p lightable-diff --example web_preview -- \
//!         A.dxf B.dxf "Rev B" "Rev C" "Steel bracket" OUT_DIR [NOTE]
//!
//! Writes OUT_DIR/preview.svg and OUT_DIR/changes.html. The list is read
//! back from the JSON diff report (`Diff::to_json`), the same report the
//! app exports, so it says exactly what the report says.

use std::fmt::Write as _;
use std::path::Path;

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (args, note) = match args.as_slice() {
        [rest @ .., note] if rest.len() == 6 => (rest, Some(note.as_str())),
        all => (all, None),
    };
    let [a_path, b_path, a_label, b_label, title, out] = args else {
        return Err("usage: web_preview A.dxf B.dxf A_LABEL B_LABEL TITLE OUT_DIR [NOTE]".into());
    };
    let read = |p: &str| {
        lightable_dxf::read_file(p)
            .map(|(d, _)| d)
            .map_err(|e| format!("{p}: {e}"))
    };
    let (a, b) = (read(a_path)?, read(b_path)?);
    let diff = lightable_diff::diff(&a, &b, &lightable_diff::DiffOptions::default());
    let out = Path::new(out);
    let write = |name: &str, text: &str| {
        std::fs::write(out.join(name), text).map_err(|e| format!("{name}: {e}"))
    };

    let (sa, sb) = (lightable_render::build(&a), lightable_render::build(&b));
    let overlay = lightable_diff::view::overlay(&sa, &sb, &diff);
    write(
        "preview.svg",
        &lightable_render::svg::to_svg(&overlay, None),
    )?;

    let name = |p: &str| {
        Path::new(p)
            .file_name()
            .map_or(p.to_owned(), |n| n.to_string_lossy().into())
    };
    let report: serde_json::Value =
        serde_json::from_str(&diff.to_json(&name(a_path), &name(b_path)))
            .map_err(|e| e.to_string())?;
    let changes = report["changes"]
        .as_array()
        .ok_or("report without changes")?;
    let list = |markup: bool| {
        changes
            .iter()
            .filter(|c| c["markup"].as_bool() == Some(markup))
            .filter_map(|c| c["headline"].as_str())
            .map(|h| format!("    <li>{}</li>\n", escape(h)))
            .collect::<String>()
    };
    let design = report["summary"]["design_changes"].as_u64().unwrap_or(0);
    let markup = report["summary"]["markup"].as_u64().unwrap_or(0);
    // The same words as the app's summary line.
    let found = match design {
        0 => "No changes found".to_owned(),
        1 => "1 change found".to_owned(),
        n => format!("{n} changes found"),
    };
    let mut html = String::new();
    let _ = writeln!(
        html,
        r#"<section id="changes" aria-label="Changes in the example">"#
    );
    let _ = writeln!(html, "  <h1>{}</h1>", escape(title));
    let _ = writeln!(
        html,
        "  <p class=\"summary\">{found} between {} and {}</p>",
        escape(a_label),
        escape(b_label)
    );
    if let Some(markup) = diff.markup_summary() {
        let _ = writeln!(
            html,
            "  <p class=\"markup-summary\">{}</p>",
            escape(&markup)
        );
    }
    if let Some(note) = note {
        let _ = writeln!(html, "  <p class=\"example-note\">{}</p>", escape(note));
    }
    let _ = writeln!(html, "  <ol>\n{}  </ol>", list(false));
    if markup > 0 {
        let _ = writeln!(html, "  <h2>Revision markup</h2>");
        let _ = writeln!(html, "  <ol class=\"markup\">\n{}  </ol>", list(true));
    }
    let _ = writeln!(
        html,
        "  <p class=\"note\">Listed from the comparison of the example's two DXF files when this page was built.</p>"
    );
    let _ = writeln!(html, "</section>");
    write("changes.html", &html)
}
