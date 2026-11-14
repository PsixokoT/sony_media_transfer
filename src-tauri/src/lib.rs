mod cards;
mod exif;

use std::path::PathBuf;
use tauri::http::{Response, StatusCode, header};

#[tauri::command]
async fn scan() -> Result<cards::Scan, String> {
    // Disk work goes to a blocking thread so it doesn't hold up the async runtime.
    tauri::async_runtime::spawn_blocking(|| cards::scan(&cards::removable_drives()))
        .await
        .map_err(|e| e.to_string())
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
        .invoke_handler(tauri::generate_handler![scan])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
