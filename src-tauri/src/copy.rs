//! Copies the chosen card files into library folders. Nothing on the card is touched.

use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

/// Files that go into one library folder.
#[derive(Deserialize, Debug)]
pub struct Job {
    /// Folder name as typed, e.g. "2023.09.21 - 2023.09.22 - Поездка".
    pub folder: String,
    pub files: Vec<PathBuf>,
}

/// Running totals, reported after every chunk.
#[derive(Serialize, Clone, Default, Debug, PartialEq)]
pub struct Progress {
    pub bytes: u64,
    pub files: usize,
}

/// Returns the folders copied into, so the UI can show and open them.
pub fn copy(root: &Path, jobs: &[Job], mut report: impl FnMut(&Progress)) -> io::Result<Vec<PathBuf>> {
    if !root.is_dir() {
        let msg = format!("{}: {}", crate::tr("Папка библиотеки не найдена", "Library folder not found"), root.display());
        return Err(io::Error::new(io::ErrorKind::NotFound, msg));
    }
    // Big chunks: fewer round trips to the card and the HDD. Files go one by one, an HDD hates parallel writes.
    let mut buf = vec![0u8; 8 << 20];
    let mut done = Progress::default();
    let mut folders = Vec::new();
    for job in jobs {
        let dir = root.join(folder_name(&job.folder)?);
        fs::create_dir_all(&dir)?;
        if !folders.contains(&dir) {
            folders.push(dir.clone()); // two days typed into one folder show it once
        }
        for src in &job.files {
            let dst = dir.join(src.file_name().ok_or_else(|| io::Error::other(crate::tr("путь без имени файла", "path without a file name")))?);
            let size = fs::metadata(src)?.len();
            match fs::metadata(&dst) {
                // Same name and size already there: copied earlier, maybe into this folder by hand.
                Ok(m) if m.len() == size => done.bytes += size,
                // Never overwrite the library.
                Ok(_) => {
                    let msg = format!("{}: {}", dst.display(), crate::tr("уже есть, и размер у него другой", "already exists with a different size"));
                    return Err(io::Error::new(io::ErrorKind::AlreadyExists, msg));
                }
                Err(_) => copy_file(src, &dst, &mut buf, |n| {
                    done.bytes += n;
                    report(&done);
                })?,
            }
            done.files += 1;
            report(&done);
        }
    }
    Ok(folders)
}

/// One plain folder name inside the root. The trailing " - " of an empty description is dropped.
fn folder_name(name: &str) -> io::Result<&str> {
    let name = name.trim().trim_end_matches([' ', '-', '.']);
    if name.is_empty() || name.contains(['\\', '/', ':', '*', '?', '"', '<', '>', '|']) {
        let msg = format!("{}: «{name}»", crate::tr("Недопустимое имя папки", "Invalid folder name"));
        return Err(io::Error::new(io::ErrorKind::InvalidInput, msg));
    }
    Ok(name)
}

/// Writes `dst.part`, renames it to `dst`, then checks the size, so a half-written file never has the real name.
fn copy_file(src: &Path, dst: &Path, buf: &mut [u8], on_chunk: impl FnMut(u64)) -> io::Result<()> {
    let mut part = dst.as_os_str().to_owned();
    part.push(".part");
    if let Err(e) = write_part(src, Path::new(&part), buf, on_chunk) {
        let _ = fs::remove_file(&part);
        return Err(e);
    }
    fs::rename(&part, dst)?;
    let (want, got) = (fs::metadata(src)?.len(), fs::metadata(dst)?.len());
    if want != got {
        let _ = fs::remove_file(dst);
        return Err(io::Error::other(format!("{}: {got} / {want} {}", dst.display(), crate::tr("байт записано", "bytes written"))));
    }
    Ok(())
}

// shortcut: no fsync before the rename, so a power cut can leave a broken file; add `to.sync_all()` if that ever bites.
fn write_part(src: &Path, part: &Path, buf: &mut [u8], mut on_chunk: impl FnMut(u64)) -> io::Result<()> {
    let mut from = File::open(src)?;
    let mut to = File::create(part)?;
    loop {
        let n = from.read(buf)?;
        if n == 0 {
            break;
        }
        to.write_all(&buf[..n])?;
        on_chunk(n as u64);
    }
    // Keep the card's file date, as Explorer does.
    to.set_modified(from.metadata()?.modified()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copies_into_folders() {
        let root = std::env::temp_dir().join("sony_transfer_copy_test");
        let _ = fs::remove_dir_all(&root);
        let (card, lib) = (root.join("card"), root.join("lib"));
        fs::create_dir_all(&card).unwrap();
        fs::create_dir_all(&lib).unwrap();
        let file = |name: &str, len: usize| {
            fs::write(card.join(name), vec![7u8; len]).unwrap();
            card.join(name)
        };
        let (a, b, c) = (file("A.ARW", 10 << 20), file("B.JPG", 5), file("C.MP4", 0));
        let job = |folder: &str, files: &[&PathBuf]| Job { folder: folder.into(), files: files.iter().map(|f| f.to_path_buf()).collect() };

        let mut reports = Vec::new();
        let jobs = [job("2023.09.21 - 2023.09.22 - ", &[&a]), job("2023.09.23 - Дача", &[&c]), job("2023.09.21 - 2023.09.22", &[&b])];
        let folders = copy(&lib, &jobs, |p| reports.push(p.clone())).unwrap();
        let range = lib.join("2023.09.21 - 2023.09.22");
        assert_eq!(folders, [range.clone(), lib.join("2023.09.23 - Дача")]);
        assert_eq!(fs::read(range.join("A.ARW")).unwrap(), fs::read(&a).unwrap());
        assert_eq!(fs::metadata(range.join("B.JPG")).unwrap().len(), 5);
        assert!(lib.join(r"2023.09.23 - Дача\C.MP4").is_file());
        assert!(!range.join("A.ARW.part").exists());
        assert_eq!(fs::metadata(range.join("A.ARW")).unwrap().modified().unwrap(), fs::metadata(&a).unwrap().modified().unwrap());
        assert_eq!(reports.last(), Some(&Progress { bytes: (10 << 20) + 5, files: 3 }));
        assert!(reports.len() > 3, "progress inside the 10 MB file too");

        // Again: same files are already there and get skipped; a different file with the same name stops the copy.
        copy(&lib, &[job("2023.09.21 - 2023.09.22", &[&a, &b])], |_| {}).unwrap();
        fs::write(&b, b"other size").unwrap();
        let err = copy(&lib, &[job("2023.09.21 - 2023.09.22", &[&b])], |_| {}).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(fs::read(range.join("B.JPG")).unwrap(), [7u8; 5]);

        for bad in ["..", "a\\b", "c:", " - "] {
            assert_eq!(copy(&lib, &[job(bad, &[&a])], |_| {}).unwrap_err().kind(), io::ErrorKind::InvalidInput, "{bad}");
        }
        assert!(copy(&root.join("missing"), &[], |_| {}).is_err());
    }
}
