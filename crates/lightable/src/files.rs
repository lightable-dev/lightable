//! Saving exported files (change reports). Native: next to the drawing
//! they describe, or in Downloads when that folder is unknown. Web: a
//! download made in the page from the bytes in memory (a `blob:` URL), so
//! nothing is uploaded anywhere.

/// Saves `bytes` as `name`, next to `beside` when it is a path to an
/// existing file; returns a message for the status bar.
pub fn save(beside: &str, name: &str, bytes: &[u8], mime: &str) -> String {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = mime;
        let dir = std::path::Path::new(beside)
            .parent()
            .filter(|d| {
                std::path::Path::new(beside).is_file() && (d.as_os_str().is_empty() || d.is_dir())
            })
            .map(|d| d.to_path_buf())
            .or_else(|| {
                std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join("Downloads"))
            })
            .unwrap_or_default();
        let out = dir.join(name);
        match std::fs::write(&out, bytes) {
            Ok(()) => format!("Saved {}", out.display()),
            Err(e) => format!("{}: {e}", out.display()),
        }
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = beside;
        match download(name, bytes, mime) {
            Ok(()) => format!("Downloaded {name}"),
            Err(e) => format!("{name}: could not download ({e})"),
        }
    }
}

#[cfg(target_arch = "wasm32")]
fn download(name: &str, bytes: &[u8], mime: &str) -> Result<(), String> {
    use eframe::wasm_bindgen::JsCast as _;
    let err = |e: eframe::wasm_bindgen::JsValue| format!("{e:?}");
    let window = web_sys::window().ok_or("no window")?;
    let document = window.document().ok_or("no document")?;
    let parts = js_sys::Array::new();
    parts.push(&js_sys::Uint8Array::from(bytes));
    let options = web_sys::BlobPropertyBag::new();
    options.set_type(mime);
    let blob =
        web_sys::Blob::new_with_u8_array_sequence_and_options(&parts, &options).map_err(err)?;
    let url = web_sys::Url::create_object_url_with_blob(&blob).map_err(err)?;
    let a: web_sys::HtmlAnchorElement = document
        .create_element("a")
        .map_err(err)?
        .dyn_into()
        .map_err(|_| "not an anchor".to_owned())?;
    a.set_href(&url);
    a.set_download(name);
    a.click();
    web_sys::Url::revoke_object_url(&url).map_err(err)?;
    Ok(())
}

/// The name of the exported file: `<stem of b>.changes.<extension>`.
pub fn export_name(b: &str, extension: &str) -> String {
    let file = b.rsplit(['/', '\\']).next().unwrap_or(b);
    let stem = file.rsplit_once('.').map_or(file, |(s, _)| s);
    format!("{stem}.changes.{extension}")
}

/// An RGBA image as PNG.
pub fn png(width: usize, height: usize, rgba: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, width as u32, height as u32);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
    writer.write_image_data(rgba).map_err(|e| e.to_string())?;
    writer.finish().map_err(|e| e.to_string())?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_names() {
        assert_eq!(
            export_name("/x/plan-rev-b.dxf", "json"),
            "plan-rev-b.changes.json"
        );
        assert_eq!(export_name("b", "png"), "b.changes.png");
    }

    #[test]
    fn png_has_a_signature() {
        let p = png(2, 1, &[255, 0, 0, 255, 0, 255, 0, 255]).unwrap();
        assert_eq!(&p[..8], b"\x89PNG\r\n\x1a\n");
    }
}
