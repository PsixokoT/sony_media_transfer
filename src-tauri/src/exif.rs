//! Minimal EXIF reader for ARW, JPG and HIF. It reads only the few bytes it needs,
//! not the whole 30–60 MB ARW the way general-purpose crates do.

use std::fs::File;
use std::io::{self, BufReader, Read, Seek, SeekFrom};
use std::path::Path;

#[derive(Debug, PartialEq)]
pub struct Exif {
    /// DateTimeOriginal as "YYYY-MM-DD HH:MM:SS" (camera local time).
    pub date: Option<String>,
    /// EXIF Orientation: 1 normal, 3 = 180°, 6 = 90° CW, 8 = 90° CCW.
    pub orientation: u16,
    /// Largest embedded JPEG: (absolute offset in the file, length).
    pub preview: Option<(u64, u64)>,
}

pub fn read(path: &Path) -> io::Result<Exif> {
    read_from(&mut BufReader::new(File::open(path)?))
}

pub fn read_from<R: Read + Seek>(r: &mut R) -> io::Result<Exif> {
    let mut magic = [0u8; 8];
    r.read_exact(&mut magic)?;
    // Where the TIFF header starts: 0 for ARW, inside APP1 for JPEG, inside the Exif item for HEIF.
    let (base, heif_jpeg) = match magic {
        [0xFF, 0xD8, ..] => (jpeg_tiff_start(r)?, None),
        [_, _, _, _, b'f', b't', b'y', b'p'] => heif(r)?,
        _ => (0, None),
    };
    r.seek(SeekFrom::Start(base))?;
    let mut header = [0u8; 8];
    r.read_exact(&mut header)?;
    let e = match &header[..4] {
        b"II*\0" => Endian(true),
        b"MM\0*" => Endian(false),
        _ => return Err(bad("not a TIFF")),
    };

    let mut exif = Exif { date: None, orientation: 1, preview: heif_jpeg };
    let mut exif_ifd = 0;
    let mut next = e.u32(&header[4..]);
    // IFD0 holds the ARW preview and orientation, IFD1 holds the small JPG thumbnail.
    for ifd in 0..2 {
        if next == 0 {
            break;
        }
        let (entries, n) = read_ifd(r, base + next as u64, e)?;
        next = n;
        let (mut offset, mut len) = (0, 0);
        for entry in &entries {
            let value = e.value(entry);
            match e.u16(&entry[0..2]) {
                0x0112 if ifd == 0 => exif.orientation = value as u16,
                0x8769 => exif_ifd = value,
                0x0201 => offset = value,
                0x0202 => len = value,
                _ => {}
            }
        }
        if len as u64 > exif.preview.map_or(0, |(_, l)| l) {
            exif.preview = Some((base + offset as u64, len as u64));
        }
    }

    if exif_ifd != 0 {
        let (entries, _) = read_ifd(r, base + exif_ifd as u64, e)?;
        if let Some(entry) = entries.iter().find(|en| e.u16(&en[0..2]) == 0x9003) {
            let mut s = [0u8; 19];
            r.seek(SeekFrom::Start(base + e.u32(&entry[8..]) as u64))?;
            r.read_exact(&mut s)?;
            exif.date = parse_date(s);
        }
    }
    Ok(exif)
}

/// JPEG: the TIFF header sits in the APP1 "Exif" segment.
fn jpeg_tiff_start<R: Read + Seek>(r: &mut R) -> io::Result<u64> {
    r.seek(SeekFrom::Start(2))?;
    loop {
        let mut seg = [0u8; 4];
        r.read_exact(&mut seg)?;
        if seg[0] != 0xFF || seg[1] == 0xDA {
            return Err(bad("no EXIF in JPEG"));
        }
        let len = u16::from_be_bytes([seg[2], seg[3]]) as i64 - 2;
        if seg[1] == 0xE1 {
            let mut id = [0u8; 6];
            r.read_exact(&mut id)?;
            if &id == b"Exif\0\0" {
                return r.stream_position();
            }
            r.seek(SeekFrom::Current(len - 6))?;
        } else {
            r.seek(SeekFrom::Current(len))?;
        }
    }
}

/// HEIF (ISO BMFF): returns where the Exif item's TIFF starts and the largest JPEG item, if any.
/// shortcut: a HIF with only HEVC images gets no preview (WebView can't decode HEVC);
/// if Sony HIF turns out to have no JPEG item, decode through Windows WIC.
fn heif<R: Read + Seek>(r: &mut R) -> io::Result<(u64, Option<(u64, u64)>)> {
    let meta = top_box(r, b"meta")?;
    let (exif, jpeg) = heif_items(&meta).ok_or_else(|| bad("no EXIF in HEIF"))?;
    // The Exif item starts with a 4-byte offset to the TIFF header (past "Exif\0\0").
    r.seek(SeekFrom::Start(exif))?;
    let mut skip = [0u8; 4];
    r.read_exact(&mut skip)?;
    Ok((exif + 4 + u32::from_be_bytes(skip) as u64, jpeg))
}

/// Contents of the first top-level box of this type.
fn top_box<R: Read + Seek>(r: &mut R, kind: &[u8; 4]) -> io::Result<Vec<u8>> {
    r.seek(SeekFrom::Start(0))?;
    loop {
        let mut h = [0u8; 8];
        r.read_exact(&mut h)?;
        let (mut size, mut header) = (u32::from_be_bytes([h[0], h[1], h[2], h[3]]) as u64, 8);
        if size == 1 {
            let mut large = [0u8; 8];
            r.read_exact(&mut large)?;
            (size, header) = (u64::from_be_bytes(large), 16);
        }
        let len = size.checked_sub(header).ok_or_else(|| bad("broken HEIF"))?;
        if &h[4..] == kind {
            let mut buf = Vec::new();
            r.by_ref().take(len).read_to_end(&mut buf)?;
            return Ok(buf);
        }
        r.seek(SeekFrom::Current(len as i64))?;
    }
}

/// From the meta box: iinf gives item types, iloc gives where items lie in the file.
/// Returns the Exif item's offset and (offset, length) of the largest JPEG item.
fn heif_items(meta: &[u8]) -> Option<(u64, Option<(u64, u64)>)> {
    let mut types = Vec::new(); // (item id, type)
    let mut places = Vec::new(); // (item id, file offset, length)
    for (kind, body) in boxes(meta.get(4..)?) {
        let mut c = Cur(body);
        match kind {
            b"iinf" => {
                let version = c.uint(1)?;
                c.take(3 + if version == 0 { 2 } else { 4 })?; // flags, entry count
                for (_, infe) in boxes(c.0) {
                    let mut c = Cur(infe);
                    let version = c.uint(1)?;
                    c.take(3)?;
                    if version >= 2 {
                        let id = c.uint(if version == 2 { 2 } else { 4 })?;
                        c.take(2)?; // protection index
                        types.push((id, c.take(4)?));
                    }
                }
            }
            b"iloc" => {
                let version = c.uint(1)?;
                c.take(3)?;
                let (sizes, more) = (c.uint(1)?, c.uint(1)?);
                let (offset_size, length_size) = ((sizes >> 4) as usize, (sizes & 15) as usize);
                let base_size = (more >> 4) as usize;
                let index_size = if version > 0 { (more & 15) as usize } else { 0 };
                let wide = if version < 2 { 2 } else { 4 };
                for _ in 0..c.uint(wide)? {
                    let id = c.uint(wide)?;
                    let method = if version > 0 { c.uint(2)? & 15 } else { 0 };
                    c.take(2)?; // data reference index
                    let base = c.uint(base_size)?;
                    let extents = c.uint(2)?;
                    for _ in 0..extents {
                        c.take(index_size)?;
                        let (offset, length) = (c.uint(offset_size)?, c.uint(length_size)?);
                        // only items stored in the file in one piece (method 0 = file offset)
                        if extents == 1 && method == 0 {
                            places.push((id, base.checked_add(offset)?, length));
                        }
                    }
                }
            }
            _ => {}
        }
    }
    let place = |kind: &[u8]| -> Vec<(u64, u64)> {
        let ids = types.iter().filter(|(_, t)| *t == kind);
        ids.filter_map(|(id, _)| places.iter().find(|p| p.0 == *id)).map(|&(_, o, l)| (o, l)).collect()
    };
    let exif = place(b"Exif").first()?.0;
    Some((exif, place(b"jpeg").into_iter().max_by_key(|p| p.1)))
}

/// ISO BMFF boxes in a byte slice: (type, contents).
fn boxes(mut data: &[u8]) -> impl Iterator<Item = (&[u8], &[u8])> {
    std::iter::from_fn(move || {
        let mut c = Cur(data);
        let (size, kind) = (c.uint(4)?, c.take(4)?);
        let len = match size {
            0 => c.0.len() as u64,
            1 => c.uint(8)?.checked_sub(16)?,
            n => n.checked_sub(8)?,
        };
        let body = c.take(usize::try_from(len).ok()?)?;
        data = c.0;
        Some((kind, body))
    })
}

/// Reads big-endian numbers off the front of a byte slice; None when it runs out.
struct Cur<'a>(&'a [u8]);

impl<'a> Cur<'a> {
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let (head, rest) = self.0.split_at_checked(n)?;
        self.0 = rest;
        Some(head)
    }
    fn uint(&mut self, n: usize) -> Option<u64> {
        Some(self.take(n)?.iter().fold(0, |v, &b| v << 8 | b as u64))
    }
}

/// IFD = a list of 12-byte entries (tag, type, count, value-or-offset) plus the offset of the next IFD.
fn read_ifd<R: Read + Seek>(r: &mut R, at: u64, e: Endian) -> io::Result<(Vec<[u8; 12]>, u32)> {
    r.seek(SeekFrom::Start(at))?;
    let mut count = [0u8; 2];
    r.read_exact(&mut count)?;
    let count = e.u16(&count) as usize;
    let mut buf = vec![0u8; count * 12 + 4];
    r.read_exact(&mut buf)?;
    let entries = buf[..count * 12].chunks_exact(12).map(|c| c.try_into().unwrap()).collect();
    Ok((entries, e.u32(&buf[count * 12..])))
}

/// "YYYY:MM:DD HH:MM:SS" -> "YYYY-MM-DD HH:MM:SS"; None if the camera clock was not set.
fn parse_date(mut s: [u8; 19]) -> Option<String> {
    let digits = s.iter().enumerate().all(|(i, c)| [4, 7, 10, 13, 16].contains(&i) || c.is_ascii_digit());
    if !digits || !s.is_ascii() {
        return None;
    }
    (s[4], s[7], s[10]) = (b'-', b'-', b' ');
    String::from_utf8(s.to_vec()).ok()
}

/// TIFF byte order: true = little-endian ("II"), false = big-endian ("MM").
#[derive(Clone, Copy)]
struct Endian(bool);

impl Endian {
    fn u16(self, b: &[u8]) -> u16 {
        let b = [b[0], b[1]];
        if self.0 { u16::from_le_bytes(b) } else { u16::from_be_bytes(b) }
    }
    fn u32(self, b: &[u8]) -> u32 {
        let b = [b[0], b[1], b[2], b[3]];
        if self.0 { u32::from_le_bytes(b) } else { u32::from_be_bytes(b) }
    }
    /// Value of a single SHORT (type 3) or LONG entry.
    fn value(self, entry: &[u8; 12]) -> u32 {
        if self.u16(&entry[2..4]) == 3 { self.u16(&entry[8..]) as u32 } else { self.u32(&entry[8..]) }
    }
}

fn bad(msg: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg)
}

/// Synthetic files for tests: a little-endian TIFF the way an ARW starts, and the same TIFF inside a JPEG.
#[cfg(test)]
pub mod sample {
    fn entry(tag: u16, typ: u16, count: u32, value: u32) -> Vec<u8> {
        [&tag.to_le_bytes()[..], &typ.to_le_bytes(), &count.to_le_bytes(), &value.to_le_bytes()].concat()
    }

    /// IFD0 @8: orientation 6, preview 200+50, Exif IFD @100. IFD1 @70: thumbnail 300+10.
    /// Exif IFD @100: DateTimeOriginal @130.
    pub fn tiff(date: &str) -> Vec<u8> {
        let mut b = b"II*\0".to_vec();
        b.extend(8u32.to_le_bytes());
        b.extend(4u16.to_le_bytes());
        b.extend(entry(0x0112, 3, 1, 6));
        b.extend(entry(0x0201, 4, 1, 200));
        b.extend(entry(0x0202, 4, 1, 50));
        b.extend(entry(0x8769, 4, 1, 100));
        b.extend(70u32.to_le_bytes());
        b.resize(70, 0);
        b.extend(2u16.to_le_bytes());
        b.extend(entry(0x0201, 4, 1, 300));
        b.extend(entry(0x0202, 4, 1, 10));
        b.extend(0u32.to_le_bytes());
        b.resize(100, 0);
        b.extend(1u16.to_le_bytes());
        b.extend(entry(0x9003, 2, 20, 130));
        b.extend(0u32.to_le_bytes());
        b.resize(130, 0);
        b.extend(date.as_bytes());
        b.resize(310, 0);
        b
    }

    /// SOI + APP0 (JFIF, skipped) + APP1 Exif. The TIFF starts at byte 30.
    pub fn jpeg(date: &str) -> Vec<u8> {
        let tiff = tiff(date);
        let mut b = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10];
        b.resize(20, 0);
        b.extend([0xFF, 0xE1]);
        b.extend((tiff.len() as u16 + 8).to_be_bytes());
        b.extend(b"Exif\0\0");
        b.extend(tiff);
        b
    }

    fn bmff(kind: &[u8], body: &[u8]) -> Vec<u8> {
        [&(body.len() as u32 + 8).to_be_bytes()[..], kind, body].concat()
    }

    /// ftyp + meta (iinf: item 1 = Exif, 2 = jpeg; iloc v0, 4-byte offsets) + mdat (Exif item, then a 1000-byte "JPEG").
    pub fn heif(date: &str) -> Vec<u8> {
        let exif = [&[0, 0, 0, 6][..], b"Exif\0\0", &tiff(date)].concat();
        let jpeg = vec![0xFF; 1000];
        let infe = |id: u8, kind: &[u8]| bmff(b"infe", &[&[2, 0, 0, 0, 0, id, 0, 0][..], kind, b"\0"].concat());
        let iinf = bmff(b"iinf", &[&[0, 0, 0, 0, 0, 2][..], &infe(1, b"Exif"), &infe(2, b"jpeg")].concat());
        let ftyp = bmff(b"ftyp", b"heicmif1");
        let iloc_len = 8 + 4 + 4 + 2 * 14; // header, version+flags, sizes+count, 2 items
        let mdat = (ftyp.len() + 8 + 4 + iinf.len() + iloc_len + 8) as u32;
        // item: id, data reference index, one extent: offset, length
        let item = |id: u16, offset: u32, len: usize| {
            [&id.to_be_bytes()[..], &[0, 0, 0, 1], &offset.to_be_bytes(), &(len as u32).to_be_bytes()].concat()
        };
        let items = [item(1, mdat, exif.len()), item(2, mdat + exif.len() as u32, jpeg.len())].concat();
        let iloc = bmff(b"iloc", &[&[0, 0, 0, 0, 0x44, 0x00, 0, 2][..], &items].concat());
        assert_eq!(iloc.len(), iloc_len);
        let meta = bmff(b"meta", &[&[0, 0, 0, 0][..], &iinf, &iloc].concat());
        [ftyp, meta, bmff(b"mdat", &[exif, jpeg].concat())].concat()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn reads_tiff_and_jpeg() {
        let want = |base: u64| Exif {
            date: Some("2023-09-21 15:30:12".into()),
            orientation: 6,
            preview: Some((base + 200, 50)),
        };
        let date = "2023:09:21 15:30:12\0";
        assert_eq!(read_from(&mut Cursor::new(sample::tiff(date))).unwrap(), want(0));
        assert_eq!(read_from(&mut Cursor::new(sample::jpeg(date))).unwrap(), want(30));
    }

    #[test]
    fn reads_heif() {
        let file = sample::heif("2023:09:21 15:30:12\0");
        let exif = read_from(&mut Cursor::new(&file)).unwrap();
        assert_eq!((exif.date.as_deref(), exif.orientation), (Some("2023-09-21 15:30:12"), 6));
        // the 1000-byte JPEG item at the end of the file beats the 50-byte EXIF preview
        assert_eq!(exif.preview, Some((file.len() as u64 - 1000, 1000)));
    }

    #[test]
    fn unset_clock_gives_no_date() {
        let exif = read_from(&mut Cursor::new(sample::tiff("    :  :     :  :  \0"))).unwrap();
        assert_eq!(exif.date, None);
    }
}
