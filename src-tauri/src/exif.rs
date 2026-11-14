//! Minimal EXIF reader for ARW, JPG and HIF. It reads only the few bytes it needs,
//! not the whole 30–60 MB ARW the way general-purpose crates do.

use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{self, BufReader, Read, Seek, SeekFrom};
use std::path::Path;

#[derive(Debug, PartialEq)]
pub struct Exif {
    /// DateTimeOriginal as "YYYY-MM-DD HH:MM:SS" (camera local time).
    pub date: Option<String>,
    /// EXIF Orientation: 1 normal, 3 = 180°, 6 = 90° CW, 8 = 90° CCW. In a HEIF, its irot wins.
    pub orientation: u16,
    /// Largest embedded JPEG: (absolute offset in the file, length).
    pub preview: Option<(u64, u64)>,
    /// HEVC thumbnails of a HEIF, largest first. Sony HIF: 1616×1080 in the shot's chroma, 320×212 always 4:2:0.
    pub hevc: Vec<Hevc>,
}

/// An HEVC image in a HEIF. The UI decodes it with WebCodecs: the WebView has no HEIF decoder, and Windows has
/// none without the HEIF Image Extensions.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Hevc {
    pub offset: u64,
    pub len: u64,
    /// The hvcC box: decoder configuration with VPS/SPS/PPS.
    pub config: Vec<u8>,
    /// From ispe. The coded picture is padded (1616×1080 is coded as 1664×1088); the decoder crops it to this.
    pub width: u32,
    pub height: u32,
}

pub fn read(path: &Path) -> io::Result<Exif> {
    read_from(&mut BufReader::new(File::open(path)?))
}

pub fn read_from<R: Read + Seek>(r: &mut R) -> io::Result<Exif> {
    let mut magic = [0u8; 8];
    r.read_exact(&mut magic)?;
    // Where the TIFF header starts: 0 for ARW, inside APP1 for JPEG, inside the Exif item for HEIF.
    let (base, heif) = match magic {
        [0xFF, 0xD8, ..] => (jpeg_tiff_start(r)?, None),
        [_, _, _, _, b'f', b't', b'y', b'p'] => heif(r).map(|h| (h.exif, Some(h)))?,
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

    let mut exif = Exif { date: None, orientation: 1, preview: None, hevc: Vec::new() };
    let mut irot = 0;
    if let Some(h) = heif {
        (exif.preview, exif.hevc, irot) = (h.jpeg, h.hevc, h.irot);
    }
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
    // irot turns the image anticlockwise in 90° steps; HEIF readers take it over EXIF Orientation.
    if irot != 0 {
        exif.orientation = [1, 8, 3, 6][irot as usize];
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

/// What a HEIF's meta box tells: where the Exif TIFF starts, the largest JPEG item, HEVC thumbnails, irot angle.
struct Heif {
    exif: u64,
    jpeg: Option<(u64, u64)>,
    hevc: Vec<Hevc>,
    irot: u8,
}

/// HEIF (ISO BMFF). The main image (a grid of HEVC tiles) is never read: previews come from the thumbnails.
fn heif<R: Read + Seek>(r: &mut R) -> io::Result<Heif> {
    let meta = top_box(r, b"meta")?;
    let mut heif = heif_items(&meta).ok_or_else(|| bad("no EXIF in HEIF"))?;
    // The Exif item starts with a 4-byte offset to the TIFF header (past "Exif\0\0").
    r.seek(SeekFrom::Start(heif.exif))?;
    let mut skip = [0u8; 4];
    r.read_exact(&mut skip)?;
    heif.exif += 4 + u32::from_be_bytes(skip) as u64;
    Ok(heif)
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

/// From the meta box: iinf gives item types, iloc where items lie in the file, iref which items are thumbnails,
/// iprp the items' properties (ipco lists them, ipma ties them to items).
fn heif_items(meta: &[u8]) -> Option<Heif> {
    let mut types = Vec::new(); // (item id, type)
    let mut places = Vec::new(); // (item id, file offset, length)
    let mut thumbs = Vec::new(); // item ids
    let mut props = Vec::new(); // (type, contents); ipma counts them from 1
    let mut assoc = Vec::new(); // (item id, property numbers)
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
            b"iref" => {
                let id_size = if c.uint(1)? == 0 { 2 } else { 4 };
                c.take(3)?;
                // each reference: from item id, count, to item ids; a 'thmb' from item is a thumbnail
                thumbs.extend(boxes(c.0).filter(|(kind, _)| *kind == b"thmb").filter_map(|(_, r)| Cur(r).uint(id_size)));
            }
            b"iprp" => {
                for (kind, body) in boxes(body) {
                    let mut c = Cur(body);
                    match kind {
                        b"ipco" => props.extend(boxes(body)),
                        b"ipma" => {
                            let (version, flags) = (c.uint(1)?, c.uint(3)?);
                            let id_size = if version == 0 { 2 } else { 4 };
                            // property numbers: 1 or 2 bytes, the top bit marks an essential property
                            let (prop_size, mask) = if flags & 1 == 1 { (2, 0x7FFF) } else { (1, 0x7F) };
                            for _ in 0..c.uint(4)? {
                                let id = c.uint(id_size)?;
                                let numbers: Option<Vec<u64>> = (0..c.uint(1)?).map(|_| Some(c.uint(prop_size)? & mask)).collect();
                                assoc.push((id, numbers?));
                            }
                        }
                        _ => {}
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
    let prop = |id: u64, kind: &[u8]| -> Option<&[u8]> {
        let numbers = &assoc.iter().find(|(i, _)| *i == id)?.1;
        numbers.iter().filter_map(|&n| props.get(n.checked_sub(1)? as usize)).find(|(k, _)| *k == kind).map(|(_, b)| *b)
    };
    let mut hevc: Vec<Hevc> = thumbs
        .iter()
        .filter(|&&id| types.contains(&(id, &b"hvc1"[..])))
        .filter_map(|&id| {
            let &(_, offset, len) = places.iter().find(|p| p.0 == id)?;
            let mut size = Cur(prop(id, b"ispe")?.get(4..)?); // after version and flags
            let (width, height) = (size.uint(4)? as u32, size.uint(4)? as u32);
            Some(Hevc { offset, len, config: prop(id, b"hvcC")?.to_vec(), width, height })
        })
        .collect();
    hevc.sort_by_key(|h| std::cmp::Reverse(h.width));
    let irot = thumbs.iter().find_map(|&id| prop(id, b"irot")?.first()).map_or(0, |angle| angle & 3);
    Some(Heif { exif: place(b"Exif").first()?.0, jpeg: place(b"jpeg").into_iter().max_by_key(|p| p.1), hevc, irot })
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

    /// ftyp + meta + mdat (Exif item, a 1000-byte "JPEG", a 500-byte "HEVC" thumbnail 320×212 turned by `irot`).
    /// meta: iinf (1 = Exif, 2 = jpeg, 3 = hvc1), iref (3 is a thumbnail), iprp (hvcC, ispe, irot of 3), iloc v0.
    pub fn heif(date: &str, irot: u8) -> Vec<u8> {
        let exif = [&[0, 0, 0, 6][..], b"Exif\0\0", &tiff(date)].concat();
        let (jpeg, hevc) = (vec![0xFF; 1000], vec![0xEE; 500]);
        let infe = |id: u8, kind: &[u8]| bmff(b"infe", &[&[2, 0, 0, 0, 0, id, 0, 0][..], kind, b"\0"].concat());
        let iinf = [&[0, 0, 0, 0, 0, 3][..], &infe(1, b"Exif"), &infe(2, b"jpeg"), &infe(3, b"hvc1")].concat();
        let iinf = bmff(b"iinf", &iinf);
        let iref = bmff(b"iref", &[&[0, 0, 0, 0][..], &bmff(b"thmb", &[0, 3, 0, 1, 0, 9])].concat());
        let ispe = [0, 0, 0, 0, 0, 0, 1, 64, 0, 0, 0, 212]; // 320×212
        let ipco = bmff(b"ipco", &[bmff(b"hvcC", &[1, 2, 3]), bmff(b"ispe", &ispe), bmff(b"irot", &[irot])].concat());
        let ipma = bmff(b"ipma", &[0, 0, 0, 0, 0, 0, 0, 1, 0, 3, 3, 0x81, 2, 3]); // item 3: properties 1 (essential), 2, 3
        let iprp = bmff(b"iprp", &[ipco, ipma].concat());
        let ftyp = bmff(b"ftyp", b"heicmif1");
        let iloc_len = 8 + 4 + 4 + 3 * 14; // header, version+flags, sizes+count, 3 items
        let mdat = (ftyp.len() + 8 + 4 + iinf.len() + iref.len() + iprp.len() + iloc_len + 8) as u32;
        // item: id, data reference index, one extent: offset, length
        let item = |id: u16, offset: usize, len: usize| {
            let offset = mdat + offset as u32;
            [&id.to_be_bytes()[..], &[0, 0, 0, 1], &offset.to_be_bytes(), &(len as u32).to_be_bytes()].concat()
        };
        let items = [item(1, 0, exif.len()), item(2, exif.len(), jpeg.len()), item(3, exif.len() + jpeg.len(), hevc.len())];
        let iloc = bmff(b"iloc", &[&[0, 0, 0, 0, 0x44, 0x00, 0, 3][..], &items.concat()].concat());
        assert_eq!(iloc.len(), iloc_len);
        let meta = bmff(b"meta", &[&[0, 0, 0, 0][..], &iinf, &iref, &iprp, &iloc].concat());
        [ftyp, meta, bmff(b"mdat", &[exif, jpeg, hevc].concat())].concat()
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
            hevc: Vec::new(),
        };
        let date = "2023:09:21 15:30:12\0";
        assert_eq!(read_from(&mut Cursor::new(sample::tiff(date))).unwrap(), want(0));
        assert_eq!(read_from(&mut Cursor::new(sample::jpeg(date))).unwrap(), want(30));
    }

    #[test]
    fn reads_heif() {
        let date = "2023:09:21 15:30:12\0";
        let file = sample::heif(date, 1);
        let end = file.len() as u64;
        let exif = read_from(&mut Cursor::new(&file)).unwrap();
        // irot 1 (90° anticlockwise) beats EXIF Orientation 6
        assert_eq!((exif.date.as_deref(), exif.orientation), (Some("2023-09-21 15:30:12"), 8));
        // the 1000-byte JPEG item beats the 50-byte EXIF preview
        assert_eq!(exif.preview, Some((end - 1500, 1000)));
        let hevc = Hevc { offset: end - 500, len: 500, config: vec![1, 2, 3], width: 320, height: 212 };
        assert_eq!(exif.hevc, [hevc]);
        // irot 0 leaves EXIF Orientation alone
        assert_eq!(read_from(&mut Cursor::new(sample::heif(date, 0))).unwrap().orientation, 6);
    }

    #[test]
    fn unset_clock_gives_no_date() {
        let exif = read_from(&mut Cursor::new(sample::tiff("    :  :     :  :  \0"))).unwrap();
        assert_eq!(exif.date, None);
    }
}
