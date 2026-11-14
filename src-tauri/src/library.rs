//! The destination library: which card files are already there, and which folder a day belongs to.

use crate::cards::{self, Day, PHOTO_EXTS};
use crate::exif;
use serde::Serialize;
use std::cmp::Reverse;
use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Serialize, Debug)]
pub struct Library {
    /// Card files that are already in the library.
    pub copied: Vec<PathBuf>,
    /// Day ("YYYY-MM-DD") -> the top-level library folder whose date range holds it.
    pub existing: HashMap<String, String>,
}

/// Only what we copy counts; .xmp, .acr and the rest are skipped.
const MEDIA_EXTS: [&str; 5] = ["ARW", "JPG", "HIF", "MP4", "XML"];

pub fn check(root: &Path, days: &[Day]) -> io::Result<Library> {
    if !root.is_dir() {
        let msg = format!("{}: {}", crate::tr("Папка библиотеки не найдена", "Library folder not found"), root.display());
        return Err(io::Error::new(io::ErrorKind::NotFound, msg));
    }
    // Top-level folders with a date range: (from, to, name).
    let folders: Vec<(String, String, String)> = cards::list(root)
        .into_iter()
        .filter(|(_, meta)| meta.is_dir())
        .filter_map(|(path, _)| {
            let name = path.file_name()?.to_string_lossy().into_owned();
            let (from, to) = range(&name)?;
            Some((from, to, name))
        })
        .collect();
    let in_range = |path: &Path, day: &str| {
        let top = path.strip_prefix(root).ok()?.iter().next()?.to_str()?;
        let (from, to, _) = folders.iter().find(|(_, _, name)| name == top)?;
        Some(from.as_str() <= day && day <= to.as_str())
    };

    // (FILE NAME, size) -> where it lies. Range folders hold day subfolders, hence the full walk.
    let mut index: HashMap<(String, u64), Vec<PathBuf>> = HashMap::new();
    walk(root, &mut index);

    let mut copied = Vec::new();
    for item in days.iter().flat_map(|d| &d.items) {
        let day = &item.time[..10];
        for f in &item.files {
            // Name + size is not enough: DSC numbers start over after 9999, so the capture day must match too.
            // The folder's date range settles it for free; the file is opened (~12 ms on HDD) only when it doesn't.
            let same = index.get(&key(&f.path, f.size)).is_some_and(|paths| {
                paths.iter().any(|p| in_range(p, day) == Some(true) || capture_day(p).is_none_or(|d| d == day))
            });
            if same {
                copied.push(f.path.clone());
            }
        }
    }

    let existing = days
        .iter()
        .filter_map(|d| {
            let day = d.day.as_str();
            folders
                .iter()
                .filter(|(from, to, _)| from.as_str() <= day && day <= to.as_str())
                // The narrowest one: a trip folder rather than a half-year dump around it.
                .max_by_key(|(from, to, _)| (from, Reverse(to)))
                .map(|(_, _, name)| (d.day.clone(), name.clone()))
        })
        .collect();

    Ok(Library { copied, existing })
}

fn key(path: &Path, size: u64) -> (String, u64) {
    (path.file_name().unwrap_or_default().to_string_lossy().to_uppercase(), size)
}

fn walk(dir: &Path, index: &mut HashMap<(String, u64), Vec<PathBuf>>) {
    for (path, meta) in cards::list(dir) {
        if meta.is_dir() {
            walk(&path, index);
        } else if MEDIA_EXTS.contains(&cards::name_ext(&path).1.as_str()) {
            index.entry(key(&path, meta.len())).or_default().push(path);
        }
    }
}

/// Capture day of a library file: EXIF for photos, the clip's XML (C0001M01.XML) for MP4 and XML.
/// Read only for files whose name and size already match, so it costs little.
fn capture_day(path: &Path) -> Option<String> {
    let (stem, ext) = cards::name_ext(path);
    let time = if PHOTO_EXTS.contains(&ext.as_str()) {
        exif::read(path).ok()?.date?
    } else {
        let clip = stem.strip_suffix("M01").or(stem.strip_suffix("S03")).unwrap_or(&stem);
        cards::xml_date(&path.with_file_name(format!("{clip}M01.XML")))?
    };
    Some(time[..10].to_string())
}

/// "2023.05.26 - 2023.05.28 - Армения" -> ("2023-05-26", "2023-05-28").
/// Old names work too: "2022.02.03 - 2022.02.04 Видное" and "2022.02.05 Видное" (one day).
fn range(name: &str) -> Option<(String, String)> {
    let date = |s: Option<&str>| {
        let s = s?;
        let ok = s.bytes().enumerate().all(|(i, c)| if i == 4 || i == 7 { c == b'.' } else { c.is_ascii_digit() });
        ok.then(|| s.replace('.', "-"))
    };
    let from = date(name.get(..10))?;
    let to = date(name.get(10..).and_then(|rest| rest.strip_prefix(" - ")).and_then(|rest| rest.get(..10)));
    Some((from.clone(), to.unwrap_or(from)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{FileEntry, Item};
    use crate::exif::sample;
    use std::fs;

    fn put(path: PathBuf, bytes: &[u8]) -> PathBuf {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, bytes).unwrap();
        path
    }

    fn item(time: &str, files: &[&PathBuf]) -> Item {
        let files = files.iter().map(|p| FileEntry { path: p.to_path_buf(), size: fs::metadata(p).unwrap().len() }).collect();
        Item { name: String::new(), video: false, time: time.into(), files, thumb: None, rotate: 1 }
    }

    #[test]
    fn parses_folder_ranges() {
        let r = |a: &str, b: &str| Some((a.to_string(), b.to_string()));
        assert_eq!(range("2023.05.26 - 2023.05.28 - Армения"), r("2023-05-26", "2023-05-28"));
        assert_eq!(range("2022.02.03 - 2022.02.04 Видное, дом"), r("2022-02-03", "2022-02-04"));
        assert_eq!(range("2022.09.03 - Свадьба Чича"), r("2022-09-03", "2022-09-03"));
        assert_eq!(range("2022.02.05 Видное"), r("2022-02-05", "2022-02-05"));
        assert_eq!(range("2026.07.22"), r("2026-07-22", "2026-07-22"));
        assert_eq!(range("Droplet"), None);
        assert_eq!(range("2022-02-05 x"), None);
    }

    #[test]
    fn finds_copied_files_and_existing_folders() {
        let root = std::env::temp_dir().join("sony_transfer_library_test");
        let _ = fs::remove_dir_all(&root);
        let (card, lib) = (root.join("card"), root.join("lib"));
        let date = "2023:09:21 15:30:12\0";

        let arw = put(card.join("DSC00001.ARW"), &sample::tiff(date));
        let jpg = put(card.join("DSC00001.JPG"), &sample::jpeg(date));
        let hif = put(card.join("DSC00001.HIF"), &sample::heif(date, 0));
        let wrapped = put(card.join("DSC00002.ARW"), &sample::tiff(date));
        let mp4 = put(card.join("C0001.MP4"), b"video");
        let xml = put(card.join("C0001M01.XML"), br#"<CreationDate value="2023-09-22T10:00:00+03:00"/>"#);
        let proxy = put(card.join("C0001S03.MP4"), b"proxy");

        let trip = lib.join("2023.09.20 - 2023.09.22 - Поездка");
        put(trip.join(r"2023.09.21\DSC00001.ARW"), &sample::tiff(date)); // nested day folder
        put(trip.join("DSC00001.JPG"), &sample::jpeg("2023:09:22 15:30:12\0")); // in the folder's range: EXIF isn't read
        put(lib.join(r"2023.10.05 - Позже\DSC00001.HIF"), &sample::heif(date, 0)); // outside the folder's range, but EXIF matches
        put(lib.join(r"2020.01.01 - Old\DSC00002.ARW"), &sample::tiff("2020:01:01 10:00:00\0")); // same size, older shot
        put(trip.join("C0001.MP4"), b"video");
        put(trip.join("C0001M01.XML"), br#"<CreationDate value="2023-09-22T10:00:00+03:00"/>"#);
        put(trip.join("C0001S03.MP4.xmp"), b"proxy"); // sidecars don't count
        fs::create_dir_all(lib.join("2023.09.01 - 2023.09.30 - Сентябрь")).unwrap();
        fs::create_dir_all(lib.join("2023.09.22 Старое имя")).unwrap();

        let days = [
            Day { day: "2023-09-21".into(), items: vec![item("2023-09-21 15:30:12", &[&arw, &jpg, &hif, &wrapped])] },
            Day { day: "2023-09-22".into(), items: vec![item("2023-09-22 10:00:00", &[&mp4, &xml, &proxy])] },
            Day { day: "2023-10-01".into(), items: vec![] },
        ];
        let library = check(&lib, &days).unwrap();
        assert_eq!(library.copied, [arw, jpg, hif, mp4, xml]);

        let existing = |day: &str| library.existing.get(day).map(String::as_str);
        assert_eq!(existing("2023-09-21"), Some("2023.09.20 - 2023.09.22 - Поездка"));
        assert_eq!(existing("2023-09-22"), Some("2023.09.22 Старое имя"));
        assert_eq!(existing("2023-10-01"), None);

        assert!(check(&root.join("missing"), &days).is_err());
    }
}

