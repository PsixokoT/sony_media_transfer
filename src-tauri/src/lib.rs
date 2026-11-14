mod cards;
mod copy;
mod exif;
mod library;

use std::io;
use std::path::PathBuf;
use tauri::http::{Response, StatusCode, header};
use tauri::ipc::Channel;

/// Disk work goes to a blocking thread so it doesn't hold up the async runtime.
async fn blocking<T: Send + 'static>(work: impl FnOnce() -> io::Result<T> + Send + 'static) -> Result<T, String> {
    match tauri::async_runtime::spawn_blocking(work).await {
        Ok(result) => result.map_err(|e| e.to_string()),
        Err(e) => Err(e.to_string()),
    }
}

#[tauri::command]
async fn scan() -> Result<cards::Scan, String> {
    blocking(|| Ok(cards::scan(&cards::removable_drives()))).await
}

/// The days come back from the UI as `scan` gave them, so nothing is kept between calls.
#[tauri::command]
async fn check_library(root: PathBuf, days: Vec<cards::Day>) -> Result<library::Library, String> {
    blocking(move || library::check(&root, &days)).await
}

#[tauri::command]
async fn copy_files(root: PathBuf, jobs: Vec<copy::Job>, on_progress: Channel<copy::Progress>) -> Result<(), String> {
    // A failed send only means the window is gone; the copy goes on.
    blocking(move || copy::copy(&root, &jobs, |p| drop(on_progress.send(p.clone())))).await
}

/// `thumb://localhost/{offset}-{len}-{url-encoded path}` -> that JPEG. The whole location is in the URL,
/// so there is no state to go stale between scans. Only photo files (ARW/JPG/HIF) are served.
fn thumb(path: &str) -> Option<Vec<u8>> {
    let decoded = percent_encoding::percent_decode_str(path.trim_start_matches('/')).decode_utf8().ok()?;
    let mut parts = decoded.splitn(3, '-');
    let (offset, len) = (parts.next()?.parse().ok()?, parts.next()?.parse().ok()?);
    let path = PathBuf::from(parts.next()?);
    let ext = path.extension()?.to_string_lossy().to_uppercase();
    if !cards::PHOTO_EXTS.contains(&ext.as_str()) {
        return None;
    }
    cards::Thumb { path, offset, len }.read().ok()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .register_asynchronous_uri_scheme_protocol("thumb", |_ctx, request, responder| {
            let path = request.uri().path().to_string();
            tauri::async_runtime::spawn_blocking(move || {
                let response = match thumb(&path) {
                    Some(jpeg) => Response::builder().header(header::CONTENT_TYPE, "image/jpeg").body(jpeg),
                    None => Response::builder().status(StatusCode::NOT_FOUND).body(Vec::new()),
                };
                responder.respond(response.unwrap());
            });
        })
        .invoke_handler(tauri::generate_handler![scan, check_library, copy_files])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
