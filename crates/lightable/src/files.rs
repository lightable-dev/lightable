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

/// A file handed to the app: dropped, chosen in the file dialog or named
/// on the command line.
pub struct Arrived {
    /// Position among the files that came together.
    pub order: usize,
    pub name: String,
    /// The bytes, or why they could not be read.
    pub bytes: Result<Vec<u8>, String>,
    /// Last modified, in milliseconds since 1970, when known: the earlier
    /// file is taken as the earlier revision.
    pub modified: Option<f64>,
}

/// Files being read in the background (browsers read files
/// asynchronously): how many are expected, and those that have arrived.
#[derive(Default)]
pub struct Incoming {
    pub expected: usize,
    pub files: Vec<Arrived>,
}

pub type FileSlot = std::sync::Arc<std::sync::Mutex<Incoming>>;

impl Incoming {
    /// Starts a new batch of `n` files (web: drops and the file dialog).
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    pub fn expect(&mut self, n: usize) {
        self.expected = n;
        self.files.clear();
    }

    /// The batch, once every file has arrived, in the order given.
    pub fn take_complete(&mut self) -> Option<Vec<Arrived>> {
        if self.expected == 0 || self.files.len() < self.expected {
            return None;
        }
        self.expected = 0;
        let mut files = std::mem::take(&mut self.files);
        files.sort_by_key(|f| f.order);
        Some(files)
    }
}

/// The page's file input (`#file_input`, which allows several files).
#[cfg(target_arch = "wasm32")]
fn file_input() -> Result<web_sys::HtmlInputElement, String> {
    use eframe::wasm_bindgen::JsCast as _;
    web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id("file_input"))
        .ok_or("the page has no file input")?
        .dyn_into()
        .map_err(|_| "file_input is not an input".to_owned())
}

/// Listens to the page's file input once, at start: files chosen in the
/// file dialog arrive in `slot`. They are read in the page, never
/// uploaded.
#[cfg(target_arch = "wasm32")]
pub fn listen(slot: &FileSlot, ctx: &eframe::egui::Context) -> Result<(), String> {
    use eframe::wasm_bindgen::{JsCast as _, closure::Closure};
    let input = file_input()?;
    let (slot, ctx, target) = (slot.clone(), ctx.clone(), input.clone());
    let on_change = Closure::<dyn FnMut()>::new(move || {
        let Some(list) = target.files() else { return };
        let files: Vec<web_sys::File> = (0..list.length()).filter_map(|i| list.get(i)).collect();
        // So choosing the same files again is noticed.
        target.set_value("");
        if files.is_empty() {
            return;
        }
        if let Ok(mut s) = slot.lock() {
            s.expect(files.len());
        }
        for (order, file) in files.into_iter().enumerate() {
            let (slot, ctx) = (slot.clone(), ctx.clone());
            wasm_bindgen_futures::spawn_local(async move {
                let bytes = wasm_bindgen_futures::JsFuture::from(file.array_buffer())
                    .await
                    .map(|buf| js_sys::Uint8Array::new(&buf).to_vec())
                    .map_err(|e| format!("{e:?}"));
                if let Ok(mut s) = slot.lock() {
                    s.files.push(Arrived {
                        order,
                        name: file.name(),
                        bytes,
                        modified: Some(file.last_modified()),
                    });
                }
                ctx.request_repaint();
            });
        }
    });
    input.set_onchange(Some(on_change.as_ref().unchecked_ref()));
    // Installed once for the page's lifetime.
    on_change.forget();
    Ok(())
}

/// Opens the browser's file dialog; the files arrive through [`listen`].
#[cfg(target_arch = "wasm32")]
pub fn choose() -> Result<(), String> {
    file_input()?.click();
    Ok(())
}

#[cfg(test)]
mod incoming_tests {
    use super::*;

    #[test]
    fn a_batch_is_complete_when_every_file_arrived_in_any_order() {
        let mut incoming = Incoming::default();
        assert!(incoming.take_complete().is_none());
        incoming.expect(2);
        let file = |order: usize, name: &str| Arrived {
            order,
            name: name.into(),
            bytes: Ok(Vec::new()),
            modified: None,
        };
        incoming.files.push(file(1, "b.dxf"));
        assert!(incoming.take_complete().is_none());
        incoming.files.push(file(0, "a.dxf"));
        let batch = incoming.take_complete().unwrap();
        assert_eq!(
            batch.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(),
            ["a.dxf", "b.dxf"]
        );
        assert!(incoming.take_complete().is_none());
    }
}
