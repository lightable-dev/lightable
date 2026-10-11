//! The "Feedback" link: opens a pre-filled GitHub issue in the browser.
//! Nothing is sent by the app; the issue is only filed if the user submits
//! it on GitHub. The pre-filled text holds the app version and platform,
//! never file names or drawing data.

use eframe::egui;

fn encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn platform() -> &'static str {
    if cfg!(target_arch = "wasm32") {
        "web browser"
    } else if cfg!(target_os = "macos") {
        "macOS"
    } else if cfg!(target_os = "windows") {
        "Windows"
    } else {
        "Linux"
    }
}

/// The new-issue URL with a title and body template.
pub fn url() -> String {
    let body = format!(
        "**What happened?**\n\n\n**What did you expect?**\n\n\n**Steps to reproduce** (attach a DXF only if you are allowed to share it)\n\n\n---\n{} {} on {}\n",
        crate::APP_NAME,
        env!("CARGO_PKG_VERSION"),
        platform()
    );
    format!(
        "{}/issues/new?labels=feedback&title={}&body={}",
        crate::REPO_URL,
        encode("Feedback: "),
        encode(&body)
    )
}

/// Opens the feedback page in the user's browser.
pub fn open(ctx: &egui::Context) -> Option<String> {
    let url = url();
    #[cfg(target_arch = "wasm32")]
    {
        ctx.open_url(egui::OpenUrl::new_tab(url));
        None
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = ctx;
        let result = if cfg!(target_os = "macos") {
            std::process::Command::new("open").arg(&url).spawn()
        } else if cfg!(target_os = "windows") {
            std::process::Command::new("rundll32")
                .args(["url.dll,FileProtocolHandler", &url])
                .spawn()
        } else {
            std::process::Command::new("xdg-open").arg(&url).spawn()
        };
        result.err().map(|e| {
            format!(
                "Could not open the browser ({e}); the feedback page is {}/issues",
                crate::REPO_URL
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_url_is_prefilled_and_encoded() {
        let u = url();
        assert!(u.starts_with("https://github.com/lightable-dev/lightable/issues/new?"));
        assert!(u.contains("title=Feedback%3A%20"));
        assert!(u.contains("body=%2A%2AWhat%20happened"));
        assert!(!u.contains(' ') && !u.contains('\n'));
    }
}
