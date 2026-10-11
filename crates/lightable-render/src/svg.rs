//! Scene → SVG. Deterministic (fixed number formatting, item order), so
//! the output works as a golden snapshot.

use std::fmt::Write as _;

use crate::scene::*;

/// The SVG background, matching the app's canvas.
pub const BACKGROUND: Rgb = Rgb(0x21, 0x21, 0x21);

/// A number with at most 4 decimals, trailing zeros removed.
pub fn num(v: f64) -> String {
    let mut s = format!("{:.4}", if v.is_finite() { v } else { 0.0 });
    if s.contains('.') {
        while s.ends_with('0') {
            s.pop();
        }
        if s.ends_with('.') {
            s.pop();
        }
    }
    if s == "-0" { "0".into() } else { s }
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn path_data(points: &[P2], bulges: &[f64], closed: bool) -> String {
    let mut d = format!("M{} {}", num(points[0][0]), num(points[0][1]));
    let n = points.len();
    let segments = if closed { n } else { n - 1 };
    for i in 0..segments {
        let (a, b) = (points[i], points[(i + 1) % n]);
        let bulge = bulges.get(i).copied().unwrap_or(0.0);
        if bulge == 0.0 {
            if closed && i == segments - 1 {
                d.push('Z');
            } else {
                let _ = write!(d, "L{} {}", num(b[0]), num(b[1]));
            }
        } else {
            // Radius from the chord; large arc if the sweep exceeds 180°;
            // positive bulges turn counter-clockwise (positive angles).
            let chord = (b[0] - a[0]).hypot(b[1] - a[1]);
            let r = chord * (1.0 + bulge * bulge) / (4.0 * bulge.abs());
            let large = u8::from(bulge.abs() > 1.0);
            let sweep = u8::from(bulge > 0.0);
            let _ = write!(
                d,
                "A{} {} 0 {large} {sweep} {} {}",
                num(r),
                num(r),
                num(b[0]),
                num(b[1])
            );
        }
    }
    d
}

/// Draws the items on visible layers (`visible[layer]`, or the scene's
/// default visibility) as SVG, with world Y up.
pub fn to_svg(scene: &Scene, visible: Option<&[bool]>) -> String {
    let default = scene.default_visibility();
    let visible = visible.unwrap_or(&default);
    let shown = |item: &Item| visible.get(item.layer as usize).copied().unwrap_or(true);
    let bounds = scene.bounds(Some(visible)).unwrap_or(Bounds {
        min: [0.0, 0.0],
        max: [1.0, 1.0],
    });
    let size = bounds.size();
    let pad = size[0].max(size[1]).max(1e-9) * 0.02;
    let (x, y) = (bounds.min[0] - pad, bounds.min[1] - pad);
    let (w, h) = (size[0] + 2.0 * pad, size[1] + 2.0 * pad);
    let stroke = w.max(h) / 1500.0;
    let mut out = String::new();
    let _ = writeln!(
        out,
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="{} {} {} {}" width="800" height="{}">"#,
        num(x),
        num(-(y + h)),
        num(w),
        num(h),
        num((800.0 * h / w).clamp(1.0, 4000.0).round())
    );
    let _ = writeln!(
        out,
        r#"<rect x="{}" y="{}" width="{}" height="{}" fill="{}"/>"#,
        num(x),
        num(-(y + h)),
        num(w),
        num(h),
        BACKGROUND.hex()
    );
    // World Y up: flip the group.
    let _ = writeln!(
        out,
        r#"<g transform="scale(1 -1)" fill="none" stroke-width="{}" stroke-linecap="round">"#,
        num(stroke)
    );
    let mut current_layer: Option<u32> = None;
    // Filled regions first, so every line is drawn on top of them.
    let is_region = |i: &&Item| matches!(i.shape, Shape::Region { .. });
    let regions = scene.items.iter().filter(is_region);
    let rest = scene.items.iter().filter(|i| !is_region(i));
    for item in regions.chain(rest).filter(|i| shown(i)) {
        if current_layer != Some(item.layer) {
            if current_layer.is_some() {
                out.push_str("</g>\n");
            }
            let _ = writeln!(
                out,
                r#"<g data-layer="{}">"#,
                escape(&scene.layers[item.layer as usize].name)
            );
            current_layer = Some(item.layer);
        }
        let color = item.color.hex();
        match &item.shape {
            Shape::Path {
                points,
                bulges,
                closed,
            } => {
                if points.len() < 2 {
                    continue;
                }
                let dash = item
                    .dash
                    .map(|d| dasharray(&scene.dashes[d as usize], stroke))
                    .unwrap_or_default();
                let _ = writeln!(
                    out,
                    r#"<path d="{}" stroke="{color}"{dash}/>"#,
                    path_data(points, bulges, *closed)
                );
            }
            Shape::Fill { points } => {
                let pts: Vec<String> = points
                    .iter()
                    .map(|p| format!("{},{}", num(p[0]), num(p[1])))
                    .collect();
                let _ = writeln!(
                    out,
                    r#"<polygon points="{}" fill="{color}"/>"#,
                    pts.join(" ")
                );
            }
            Shape::Region { loops } => {
                let mut d = String::new();
                for l in loops.iter().filter(|l| l.len() >= 3) {
                    for (i, p) in l.iter().enumerate() {
                        let _ = write!(
                            d,
                            "{}{},{} ",
                            if i == 0 { "M" } else { "L" },
                            num(p[0]),
                            num(p[1])
                        );
                    }
                    d.push_str("Z ");
                }
                let _ = writeln!(
                    out,
                    r#"<path d="{}" fill="{color}" fill-rule="evenodd"/>"#,
                    d.trim_end()
                );
            }
            Shape::Point(p) => {
                let _ = writeln!(
                    out,
                    r#"<circle cx="{}" cy="{}" r="{}" fill="{color}"/>"#,
                    num(p[0]),
                    num(p[1]),
                    num(stroke * 1.5)
                );
            }
            Shape::Text(t) => {
                let anchor = match t.halign {
                    HAlign::Left => "start",
                    HAlign::Center => "middle",
                    HAlign::Right => "end",
                };
                let baseline = match t.valign {
                    VAlign::Baseline => "alphabetic",
                    VAlign::Bottom => "text-after-edge",
                    VAlign::Middle => "central",
                    VAlign::Top => "text-before-edge",
                };
                let _ = writeln!(
                    out,
                    r#"<text transform="translate({} {}) rotate({}) scale({} -1) skewX({})" font-size="{}" font-family="sans-serif" text-anchor="{anchor}" dominant-baseline="{baseline}" fill="{color}" stroke="none">{}</text>"#,
                    num(t.at[0]),
                    num(t.at[1]),
                    num(t.rotation.to_degrees()),
                    num(t.width_factor),
                    num(-t.oblique.to_degrees()),
                    num(t.height),
                    escape(&t.text)
                );
            }
        }
    }
    if current_layer.is_some() {
        out.push_str("</g>\n");
    }
    out.push_str("</g>\n</svg>\n");
    out
}

/// An SVG dash array for a pattern (dash > 0, gap < 0, dot = 0). SVG needs
/// non-negative lengths starting with a dash; dots get a tiny length.
fn dasharray(pattern: &[f64], stroke: f64) -> String {
    let mut parts: Vec<f64> = Vec::new();
    for &len in pattern {
        let dash = len >= 0.0;
        let value = if len == 0.0 { stroke * 0.5 } else { len.abs() };
        let wants_dash = parts.len().is_multiple_of(2);
        if dash == wants_dash {
            parts.push(value);
        } else if let Some(last) = parts.last_mut() {
            *last += value;
        } else {
            // Starts with a gap: an empty dash first.
            parts.push(0.0);
            parts.push(value);
        }
    }
    if parts.len() % 2 == 1 {
        parts.push(0.0);
    }
    let list: Vec<String> = parts.iter().map(|v| num(*v)).collect();
    format!(r#" stroke-dasharray="{}""#, list.join(" "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers() {
        assert_eq!(num(1.0), "1");
        assert_eq!(num(-0.00001), "0");
        assert_eq!(num(2.50001), "2.5");
        assert_eq!(num(1234.56789), "1234.5679");
    }

    #[test]
    fn bulges_become_arcs_with_the_right_flags() {
        // Counter-clockwise semicircle: sweep flag 1, not large.
        assert_eq!(
            path_data(&[[0.0, 0.0], [2.0, 0.0]], &[1.0], false),
            "M0 0A1 1 0 0 1 2 0"
        );
        // Clockwise and more than half a turn: large arc, sweep 0.
        assert_eq!(
            path_data(&[[0.0, 0.0], [2.0, 0.0]], &[-2.0], false),
            "M0 0A1.25 1.25 0 1 0 2 0"
        );
        // Closed straight polygon ends with Z.
        assert_eq!(
            path_data(&[[0.0, 0.0], [1.0, 0.0], [1.0, 1.0]], &[], true),
            "M0 0L1 0L1 1Z"
        );
    }

    #[test]
    fn dash_arrays_are_valid_svg() {
        assert_eq!(dasharray(&[2.0, -1.0], 0.1), r#" stroke-dasharray="2 1""#);
        assert_eq!(
            dasharray(&[2.0, -1.0, 0.0, -1.0], 0.1),
            r#" stroke-dasharray="2 1 0.05 1""#
        );
        assert_eq!(
            dasharray(&[-1.0, 2.0], 0.1),
            r#" stroke-dasharray="0 1 2 0""#
        );
    }

    #[test]
    fn text_is_escaped_and_upright() {
        let scene = Scene {
            layers: vec![LayerInfo {
                name: "0".into(),
                color: Rgb(255, 255, 255),
                visible: true,
            }],
            items: vec![Item {
                layer: 0,
                color: Rgb(255, 255, 255),
                dash: None,
                entity: 0,
                shape: Shape::Text(TextShape {
                    at: [1.0, 2.0],
                    height: 2.5,
                    rotation: 0.0,
                    width_factor: 1.0,
                    oblique: 0.0,
                    text: "a<b & c".into(),
                    halign: HAlign::Left,
                    valign: VAlign::Baseline,
                }),
            }],
            ..Scene::default()
        };
        let svg = to_svg(&scene, None);
        assert!(svg.contains(">a&lt;b &amp; c</text>"), "{svg}");
        assert!(svg.contains("translate(1 2) rotate(0) scale(1 -1)"));
    }
}
