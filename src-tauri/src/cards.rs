//! Finds camera cards and groups their files into tiles (one frame or one clip) by day.

use crate::exif;
use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, File, Metadata};
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct FileEntry {
    pub path: PathBuf,
    pub size: u64,
}

/// A JPEG lying in a file: embedded in ARW/JPG/HIF, or the whole M4ROOT\THMBNL\*.JPG.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Thumb {
    pub path: PathBuf,
    pub offset: u64,
    pub len: u64,
}

impl Thumb {
    pub fn read(&self) -> io::Result<Vec<u8>> {
        let mut f = File::open(&self.path)?;
        f.seek(SeekFrom::Start(self.offset))?;
        let mut buf = Vec::new();
        // take() instead of vec![0; len]: a broken EXIF length can't make us allocate gigabytes
        f.take(self.len).read_to_end(&mut buf)?;
        Ok(buf)
    }
}

/// One tile: a frame (ARW + JPG) or a clip (MP4 + XML + proxy).
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Item {
    pub name: String,
    pub video: bool,
    /// "YYYY-MM-DD HH:MM:SS", camera local time.
    pub time: String,
    pub files: Vec<FileEntry>,
    pub thumb: Option<Thumb>,
    pub rotate: u16,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Day {
    pub day: String,
    pub items: Vec<Item>,
}

#[derive(Serialize, Debug)]
pub struct Scan {
    pub cards: Vec<PathBuf>,
    pub days: Vec<Day>,
}

/// Root folders of removable drives (`F:\`, `G:\`…), card readers' empty slots included.
#[cfg(windows)]
pub fn removable_drives() -> Vec<PathBuf> {
    use windows_sys::Win32::Storage::FileSystem::{GetDriveTypeW, GetLogicalDrives};
    use windows_sys::Win32::System::Diagnostics::Debug::{SEM_FAILCRITICALERRORS, SetErrorMode};
    const DRIVE_REMOVABLE: u32 = 2;
    // unsafe: plain WinAPI calls; Rust can't check them itself, they have no memory-safety preconditions here
    unsafe { SetErrorMode(SEM_FAILCRITICALERRORS) }; // no "insert a disk" dialog for empty slots
    let mask = unsafe { GetLogicalDrives() };
    (0..26u8)
        .filter(|i| mask & (1 << i) != 0)
        .map(|i| format!("{}:\\", (b'A' + i) as char))
        .filter(|root| {
            let wide: Vec<u16> = root.encode_utf16().chain([0]).collect();
            unsafe { GetDriveTypeW(wide.as_ptr()) == DRIVE_REMOVABLE }
        })
        .map(PathBuf::from)
        .collect()
}

pub fn scan(roots: &[PathBuf]) -> Scan {
    let cards: Vec<PathBuf> = roots.iter().filter(|r| is_card(r)).cloned().collect();
    let mut items: HashMap<(String, String), Item> = HashMap::new();
    for item in cards.iter().flat_map(|c| photos(c).into_iter().chain(videos(c))) {
        // A frame is matched by name + capture time; a clip by number + day, since a proxy
        // on the other card has no XML and only its file date (end of recording).
        let when = if item.video { &item.time[..10] } else { &item.time };
        let key = (item.name.to_uppercase(), when.to_string());
        match items.get_mut(&key) {
            Some(existing) => merge(existing, item),
            None => {
                items.insert(key, item);
            }
        }
    }

    let mut days: Vec<Day> = Vec::new();
    let mut items: Vec<Item> = items.into_values().collect();
    items.sort_by(|a, b| (&a.time, &a.name).cmp(&(&b.time, &b.name)));
    for item in items {
        match days.last_mut() {
            Some(d) if d.day == item.time[..10] => d.items.push(item),
            _ => days.push(Day { day: item.time[..10].to_string(), items: vec![item] }),
        }
    }
    Scan { cards, days }
}

/// A card has DCIM\NNNxxxxx, M4ROOT (CFexpress) or PRIVATE\M4ROOT (SD).
/// PMHOME (camera's internal memory) and empty slots have none of these.
pub fn is_card(root: &Path) -> bool {
    m4root(root).is_some() || !dcim_dirs(root).is_empty()
}

fn m4root(card: &Path) -> Option<PathBuf> {
    [card.join("M4ROOT"), card.join("PRIVATE").join("M4ROOT")].into_iter().find(|p| p.is_dir())
}

/// DCF folders: 3 digits + 5 characters, e.g. 100MSDCF.
fn dcim_dirs(card: &Path) -> Vec<PathBuf> {
    list(&card.join("DCIM"))
        .into_iter()
        .filter(|(p, m)| {
            let name = p.file_name().unwrap_or_default().to_string_lossy();
            m.is_dir() && name.len() == 8 && name.bytes().take(3).all(|c| c.is_ascii_digit())
        })
        .map(|(p, _)| p)
        .collect()
}

pub const PHOTO_EXTS: [&str; 3] = ["ARW", "JPG", "HIF"];

fn photos(card: &Path) -> Vec<Item> {
    let mut out = Vec::new();
    for dir in dcim_dirs(card) {
        for (path, meta) in list(&dir) {
            let (name, ext) = name_ext(&path);
            if !meta.is_file() || !PHOTO_EXTS.contains(&ext.as_str()) {
                continue;
            }
            let exif = exif::read(&path).ok();
            let time = exif.as_ref().and_then(|e| e.date.clone()).unwrap_or_else(|| file_time(&meta));
            let thumb = exif.as_ref().and_then(|e| e.preview).map(|(offset, len)| Thumb { path: path.clone(), offset, len });
            let rotate = exif.map_or(1, |e| e.orientation);
            let files = vec![FileEntry { path, size: meta.len() }];
            out.push(Item { name, video: false, time, files, thumb, rotate });
        }
    }
    out
}

/// Clip C0001: CLIP\C0001.MP4 + CLIP\C0001M01.XML + SUB\C0001S03.MP4 (proxy), preview THMBNL\C0001T01.JPG.
/// MEDIAPRO.XML lies in M4ROOT itself and is never looked at.
fn videos(card: &Path) -> Vec<Item> {
    const PARTS: [(&str, &str, &str); 4] =
        [("CLIP", "MP4", ""), ("CLIP", "XML", "M01"), ("SUB", "MP4", "S03"), ("THMBNL", "JPG", "T01")];
    let Some(m4) = m4root(card) else { return Vec::new() };
    let mut clips: HashMap<String, (Item, Option<Metadata>)> = HashMap::new();
    for dir in ["CLIP", "SUB", "THMBNL"] {
        for (path, meta) in list(&m4.join(dir)) {
            let (stem, ext) = name_ext(&path);
            let clip = PARTS.iter().find_map(|&(d, e, suffix)| {
                (d == dir && e == ext).then(|| stem.strip_suffix(suffix)).flatten()
            });
            let Some(clip) = clip.filter(|_| meta.is_file()) else { continue };
            let (item, first) = clips.entry(clip.to_uppercase()).or_insert_with(|| {
                let item = Item { name: clip.to_string(), video: true, time: String::new(), files: vec![], thumb: None, rotate: 1 };
                (item, None)
            });
            if dir == "THMBNL" {
                item.thumb = Some(Thumb { path, offset: 0, len: meta.len() });
                continue;
            }
            if ext == "XML" {
                item.time = xml_date(&path).unwrap_or_default();
            }
            item.files.push(FileEntry { path, size: meta.len() });
            first.get_or_insert(meta);
        }
    }
    clips
        .into_values()
        .filter_map(|(mut item, first)| {
            if item.time.is_empty() {
                item.time = file_time(first.as_ref()?); // None: only a thumbnail, no files
            }
            Some(item)
        })
        .collect()
}

/// Same file on two cards (same name and size) is kept once.
fn merge(into: &mut Item, from: Item) {
    for f in from.files {
        if !into.files.iter().any(|g| g.size == f.size && g.path.file_name() == f.path.file_name()) {
            into.files.push(f);
        }
    }
    into.time = into.time.clone().min(from.time); // XML start time beats the proxy's file date
    // The ARW preview (1616×1080) beats the JPG's 160×120 thumbnail.
    if from.thumb.as_ref().map_or(0, |t| t.len) > into.thumb.as_ref().map_or(0, |t| t.len) {
        into.thumb = from.thumb;
        into.rotate = from.rotate;
    }
}

/// `<CreationDate value="2023-09-21T15:30:12+03:00"/>` -> "2023-09-21 15:30:12".
pub fn xml_date(path: &Path) -> Option<String> {
    let xml = fs::read_to_string(path).ok()?;
    let start = xml.find("<CreationDate value=\"")? + "<CreationDate value=\"".len();
    let date = xml.get(start..start + 19)?.replacen('T', " ", 1);
    date.bytes().take(4).all(|c| c.is_ascii_digit()).then_some(date)
}

fn file_time(meta: &Metadata) -> String {
    let t: DateTime<Local> = meta.modified().map_or_else(|_| Local::now(), DateTime::from);
    t.format("%Y-%m-%d %H:%M:%S").to_string()
}

/// Folder contents. An unreadable folder (or one that doesn't exist) is just empty.
pub fn list(dir: &Path) -> Vec<(PathBuf, Metadata)> {
    let Ok(rd) = fs::read_dir(dir) else { return Vec::new() };
    rd.flatten().filter_map(|e| Some((e.path(), e.metadata().ok()?))).collect()
}

/// "DSC00001.arw" -> ("DSC00001", "ARW")
pub fn name_ext(path: &Path) -> (String, String) {
    let part = |s: Option<&std::ffi::OsStr>| s.unwrap_or_default().to_string_lossy().into_owned();
    (part(path.file_stem()), part(path.extension()).to_uppercase())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exif::sample;
    use chrono::TimeZone;

    fn put(path: PathBuf, bytes: &[u8]) -> PathBuf {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, bytes).unwrap();
        path
    }

    #[test]
    fn finds_cards_and_groups_by_day() {
        let root = std::env::temp_dir().join("sony_transfer_test");
        let _ = fs::remove_dir_all(&root);
        let (a, b, pmhome, empty) = (root.join("a"), root.join("b"), root.join("pmhome"), root.join("empty"));
        let date = "2023:09:21 15:30:12\0";

        // Card A, CFexpress: RAW + video with an XML.
        put(a.join(r"DCIM\100MSDCF\DSC00001.ARW"), &sample::tiff(date));
        put(a.join(r"DCIM\100MSDCF\DSC00002.JPG"), &sample::jpeg(date));
        put(a.join(r"M4ROOT\CLIP\C0001.MP4"), b"video");
        put(a.join(r"M4ROOT\CLIP\C0001M01.XML"), br#"<CreationDate value="2023-09-22T10:00:00+03:00"/>"#);
        put(a.join(r"M4ROOT\THMBNL\C0001T01.JPG"), b"jpg");
        put(a.join(r"M4ROOT\MEDIAPRO.XML"), b"xml");
        // Card B, SD: JPG for DSC00001, a duplicate of DSC00002, a proxy dated by its file time.
        put(b.join(r"DCIM\100MSDCF\DSC00001.JPG"), &sample::jpeg(date));
        put(b.join(r"DCIM\100MSDCF\DSC00002.JPG"), &sample::jpeg(date));
        put(b.join(r"DCIM\100MSDCF\DSC00001.HIF"), &sample::heif(date));
        let proxy = put(b.join(r"PRIVATE\M4ROOT\SUB\C0001S03.MP4"), b"proxy");
        let noon = Local.with_ymd_and_hms(2023, 9, 22, 12, 0, 0).unwrap();
        File::options().write(true).open(proxy).unwrap().set_modified(noon.into()).unwrap();
        // Not cards.
        put(pmhome.join(r"LICENSE\LICENSE.TXT"), b"");
        fs::create_dir_all(&empty).unwrap();

        let scan = scan(&[a.clone(), b.clone(), pmhome, empty]);
        assert_eq!(scan.cards, [a, b]);

        let summary: Vec<_> = scan
            .days
            .iter()
            .flat_map(|d| d.items.iter().map(move |i| (d.day.as_str(), i.name.as_str(), i.files.len(), i.thumb.is_some())))
            .collect();
        assert_eq!(
            summary,
            [
                ("2023-09-21", "DSC00001", 3, true), // ARW + JPG + HIF from two cards
                ("2023-09-21", "DSC00002", 1, true), // duplicate kept once
                ("2023-09-22", "C0001", 3, true),    // MP4 + XML + proxy from the other card
            ]
        );
        let first = &scan.days[0].items[0];
        assert_eq!((first.time.as_str(), first.rotate), ("2023-09-21 15:30:12", 6));
        assert_eq!(scan.days[1].items[0].time, "2023-09-22 10:00:00");
    }
}
