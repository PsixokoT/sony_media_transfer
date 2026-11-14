mod cards;
mod copy;
mod eject;
mod exif;
mod library;

use std::io;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::http::{Response, StatusCode, header};
use tauri::ipc::Channel;

static ENGLISH: AtomicBool = AtomicBool::new(false);

/// Our own error messages, in the UI's language.
pub fn tr(ru: &'static str, en: &'static str) -> &'static str {
    if ENGLISH.load(Ordering::Relaxed) { en } else { ru }
}

#[tauri::command]
fn set_language(lang: String) {
    ENGLISH.store(lang == "en", Ordering::Relaxed);
}

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

/// Polled by the UI to notice a camera or card coming and going.
#[tauri::command]
async fn cards() -> Result<Vec<PathBuf>, String> {
    blocking(|| Ok(cards::connected())).await
}

/// The drives are found again here, so only removable camera cards are ever ejected.
#[tauri::command]
async fn eject() -> Result<(), String> {
    blocking(|| eject::eject(&cards::connected())).await
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

/// Opens a card file in its default Windows program: photo viewer, video player.
#[tauri::command]
fn open_external(path: PathBuf) -> Result<(), String> {
    let (_, ext) = cards::name_ext(&path);
    // Media only: whatever the UI sends, an .exe is never launched.
    if !cards::PHOTO_EXTS.contains(&ext.as_str()) && ext != "MP4" {
        return Err(format!("{}: {}", tr("Это не фото и не видео", "Not a photo or video"), path.display()));
    }
    // explorer.exe opens a file the way a double click does. Its exit code is meaningless, so it isn't awaited.
    std::process::Command::new("explorer").arg(&path).spawn().map(drop).map_err(|e| e.to_string())
}

/// `thumb://localhost/{offset}-{len}-{url-encoded path}` -> those bytes: a JPEG, or a HIF's HEVC image.
/// The whole location is in the URL, so there is no state to go stale between scans. Only photo files (ARW/JPG/HIF) are served.
fn thumb(path: &str) -> Option<Vec<u8>> {
    let decoded = percent_encoding::percent_decode_str(path.trim_start_matches('/')).decode_utf8().ok()?;
    let mut parts = decoded.splitn(3, '-');
    let (offset, len) = (parts.next()?.parse().ok()?, parts.next()?.parse().ok()?);
    let path = PathBuf::from(parts.next()?);
    let ext = path.extension()?.to_string_lossy().to_uppercase();
    if !cards::PHOTO_EXTS.contains(&ext.as_str()) {
        return None;
    }
    cards::Thumb { path, offset, len, hevc: Vec::new() }.read().ok()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .register_asynchronous_uri_scheme_protocol("thumb", |_ctx, request, responder| {
            let path = request.uri().path().to_string();
            tauri::async_runtime::spawn_blocking(move || {
                let response = match thumb(&path) {
                    Some(jpeg) => Response::builder()
                        .header(header::CONTENT_TYPE, "image/jpeg")
                        // thumb.localhost is another origin: the UI fetch()es HEVC bytes from here
                        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
                        .body(jpeg),
                    None => Response::builder().status(StatusCode::NOT_FOUND).body(Vec::new()),
                };
                responder.respond(response.unwrap());
            });
        })
        .invoke_handler(tauri::generate_handler![set_language, scan, cards, eject, check_library, copy_files, open_external])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
