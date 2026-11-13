//! An attachment's bytes, opened where the app runs: a new tab for images and PDFs or a
//! download on the web; the system's save dialog, then the file's default app, on desktops;
//! the app's own files and the system's viewer where there is no save dialog (Android).

use base64::{Engine as _, engine::general_purpose::STANDARD};
use mail_core::types::*;

/// Images the reader shows as thumbnails: bitmaps every platform decodes (no SVG, which
/// can carry scripts).
const PREVIEWABLE: &[&str] = &["image/png", "image/jpeg", "image/gif", "image/webp"];
/// Bigger images get no thumbnail: a data URI holds the whole file.
const PREVIEW_MAX: u64 = 8 * 1024 * 1024;

pub fn previewable(a: &Attachment) -> bool {
    PREVIEWABLE.contains(&a.mime_type.as_str()) && a.size <= PREVIEW_MAX
}

pub fn data_uri(a: &Attachment, bytes: &[u8]) -> String {
    format!("data:{};base64,{}", a.mime_type, STANDARD.encode(bytes))
}

/// A name that is safe as a file name on every platform.
fn file_name(a: &Attachment) -> String {
    let name: String = a
        .filename
        .as_deref()
        .unwrap_or_default()
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    let name = name.trim().trim_matches('.');
    if name.is_empty() {
        format!("attachment-{}", a.index + 1)
    } else {
        name.chars().take(200).collect()
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub use native::save;
#[cfg(target_arch = "wasm32")]
pub use web::{Tab, deliver};

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use std::path::PathBuf;

    use mail_core::types::*;
    use nsh_core::{file_dialog::SaveDialog, prelude::*};

    use super::file_name;

    /// Saves `bytes` where the user picks and opens the file. Resolves with the path when no
    /// app opened it, `None` when it opened or the user cancelled.
    pub async fn save(id: &str, a: &Attachment, bytes: Vec<u8>) -> Result<Option<String>> {
        let name = file_name(a);
        let path = match pick_save_path(SaveDialog::new().file_name(name.clone())).await {
            Ok(Some(path)) => path,
            Ok(None) => return Ok(None),
            // No dialog here (Android, a Linux session without a file chooser portal).
            Err(_) => own_copy(id, &name)?,
        };
        let target = path.clone();
        blocking(move || {
            if let Some(dir) = target.parent() {
                std::fs::create_dir_all(dir)?;
            }
            std::fs::write(&target, bytes)
        })
        .await
        .map_err(|e| Error::new(ErrorCode::Internal, e.to_string()))?
        .map_err(|e| {
            Error::new(
                ErrorCode::Storage,
                format!("cannot save {}: {e}", path.display()),
            )
        })?;
        match open_path(path.clone()).await {
            Ok(()) => Ok(None),
            Err(e) => {
                log::info!("no app opened {}: {e}", path.display());
                Ok(Some(path.display().to_string()))
            }
        }
    }

    /// The app's own copy, which Android's viewer reads through NSH's FilesProvider.
    fn own_copy(id: &str, name: &str) -> Result<PathBuf> {
        let dir = data_dir().ok_or_else(|| {
            Error::new(
                ErrorCode::Unsupported,
                "this device keeps no files for the app",
            )
        })?;
        let folder: String = id
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect();
        Ok(dir.join("attachments").join(folder).join(name))
    }
}

#[cfg(target_arch = "wasm32")]
mod web {
    use std::time::Duration;

    use js_sys::{Array, Uint8Array};
    use mail_core::types::*;
    use nsh_core::prelude::*;
    use wasm_bindgen::JsCast;
    use web_sys::{Blob, BlobPropertyBag, HtmlAnchorElement, Url, Window};

    use super::file_name;

    /// Types a browser shows in a tab without running anything of theirs.
    const VIEWABLE: &[&str] = &[
        "image/png",
        "image/jpeg",
        "image/gif",
        "image/webp",
        "application/pdf",
    ];

    /// The tab an image or a PDF opens in. Browsers open windows only while a press is being
    /// handled, so it opens (empty) before the bytes are fetched.
    pub struct Tab(Option<Window>);

    impl Tab {
        pub fn open(a: &Attachment) -> Self {
            let window = VIEWABLE
                .contains(&a.mime_type.as_str())
                .then(web_sys::window)
                .flatten()
                .and_then(|w| w.open_with_url_and_target("", "_blank").ok().flatten());
            Self(window)
        }
    }

    impl Drop for Tab {
        fn drop(&mut self) {
            if let Some(window) = self.0.take() {
                let _ = window.close();
            }
        }
    }

    fn fail(e: wasm_bindgen::JsValue) -> Error {
        Error::new(
            ErrorCode::Unsupported,
            format!("cannot open the attachment: {e:?}"),
        )
    }

    /// Shows `bytes` in `tab`, or downloads them when there is none (another type, or the
    /// browser blocked the tab).
    pub fn deliver(mut tab: Tab, a: &Attachment, bytes: &[u8]) -> Result<()> {
        let window = tab.0.take();
        let options = BlobPropertyBag::new();
        // Anything not shown in a tab is only ever a download.
        options.set_type(if window.is_some() {
            &a.mime_type
        } else {
            "application/octet-stream"
        });
        let parts = Array::of1(&Uint8Array::from(bytes));
        let blob = Blob::new_with_u8_array_sequence_and_options(&parts, &options).map_err(fail)?;
        let url = Url::create_object_url_with_blob(&blob).map_err(fail)?;
        match window {
            Some(window) => window.location().set_href(&url).map_err(fail)?,
            None => {
                let document = web_sys::window()
                    .and_then(|w| w.document())
                    .ok_or_else(|| Error::new(ErrorCode::Unsupported, "no browser document"))?;
                let link: HtmlAnchorElement =
                    document.create_element("a").map_err(fail)?.unchecked_into();
                link.set_href(&url);
                link.set_download(&file_name(a));
                link.click();
            }
        }
        Owner::detached().run(|| {
            spawn(async move {
                timer(Duration::from_secs(60)).await;
                let _ = Url::revoke_object_url(&url);
            })
        });
        Ok(())
    }
}
