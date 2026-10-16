//! `_Serato_/database V2`: `vrsn` (a UTF-16BE version text) followed by one `otrk` record per track, each holding its fields
//! (`pfil` path relative to the drive root without a leading slash, `ttyp` file type, `tsng` title, `tart` artist, `talb`
//! album, `tgen` genre, `tcom` comment, `tlbl` label, `tkey` key, `tbpm`, `tlen`, `tbit`, `tsmp`, `tsiz`, `tadd`/`uadd` date
//! added, `utme` file time, `bbgl` beat grid locked, `bmis` missing). Field names and types: Mixxx's reader (see [`crate::tlv`]);
//! the shapes of the text values (`03:21.00`, `320.0kbps`, `44.1k`, `7.31MB`) are observed in files Serato wrote.
//!
//! Export merges: the records of other paths (and every other top-level record) stay as they are, the record of a path we write
//! is replaced where it stands, new paths are appended.

use crate::tlv::{self, rec, text};
use openfader_plugin::library::Track;
use std::collections::HashMap;

/// Location of the file below the drive root.
pub const FILE: &str = "_Serato_/database V2";
const VERSION: &str = "2.0/Serato Scratch LIVE Database";

/// The `otrk` record of `t`, whose audio is at `path` (relative to the drive root), `size` bytes, last changed at `mtime`.
pub fn track(path: &str, t: &Track, size: u64, mtime: u32) -> Vec<u8> {
    let ext = path.rsplit_once('.').map_or("", |(_, e)| e).to_lowercase();
    let added = added_secs(&t.date_added).unwrap_or(mtime);
    let some = |on: bool, s: String| if on { s } else { String::new() };
    let fields: [(&[u8; 4], String); 15] = [
        (b"ttyp", ext),
        (b"pfil", path.to_string()),
        (b"tsng", t.title.clone()),
        (b"tart", t.artist.clone()),
        (b"talb", t.album.clone()),
        (b"tgen", t.genre.clone()),
        (b"tcom", t.comment.clone()),
        (b"tlbl", t.label.clone()),
        (b"tkey", t.key.clone()),
        (
            b"tlen",
            some(
                t.length_s > 0,
                format!("{:02}:{:02}.00", t.length_s / 60, t.length_s % 60),
            ),
        ),
        (
            b"tbit",
            some(t.bitrate > 0, format!("{:.1}kbps", f64::from(t.bitrate))),
        ),
        (
            b"tsmp",
            some(
                t.sample_rate > 0,
                format!("{:.1}k", f64::from(t.sample_rate) / 1000.0),
            ),
        ),
        (b"tbpm", some(t.bpm_x100 > 0, t.bpm_text())),
        (b"tsiz", format!("{:.2}MB", size as f64 / 1_048_576.0)),
        (b"tadd", added.to_string()),
    ];
    let mut v: Vec<u8> = fields
        .iter()
        .filter(|(_, s)| !s.is_empty())
        .flat_map(|(tag, s)| text(tag, s))
        .collect();
    v.extend(rec(b"uadd", &added.to_be_bytes()));
    v.extend(rec(b"utme", &mtime.to_be_bytes()));
    v.extend(rec(b"bbgl", &[0]));
    v.extend(rec(b"bmis", &[0]));
    rec(b"otrk", &v)
}

/// Unix seconds of a `YYYY-MM-DD…` date.
fn added_secs(s: &str) -> Option<u32> {
    let d = chrono::NaiveDate::parse_from_str(s.get(..10)?, "%Y-%m-%d").ok()?;
    u32::try_from(d.and_hms_opt(0, 0, 0)?.and_utc().timestamp()).ok()
}

/// The `pfil` path of an `otrk` record's data.
fn path_of(data: &[u8]) -> Option<String> {
    let recs = tlv::parse(data).ok()?;
    tlv::find(&recs, b"pfil").map(tlv::from_utf16)
}

/// The paths of every track record in `buf`.
pub fn paths(buf: &[u8]) -> Result<Vec<String>, String> {
    Ok(tlv::parse(buf)?
        .iter()
        .filter(|r| &r.tag == b"otrk")
        .filter_map(|r| path_of(r.data))
        .collect())
}

/// The text fields (tag → value) of the track record at `path`.
pub fn fields(buf: &[u8], path: &str) -> Result<Option<HashMap<String, String>>, String> {
    for r in tlv::parse(buf)?.iter().filter(|r| &r.tag == b"otrk") {
        if path_of(r.data).as_deref() == Some(path) {
            return Ok(Some(
                tlv::parse(r.data)?
                    .iter()
                    .filter(|f| matches!(f.tag[0], b't' | b'p'))
                    .map(|f| (String::from_utf8_lossy(&f.tag).into_owned(), f.text()))
                    .collect(),
            ));
        }
    }
    Ok(None)
}

/// `old` (the file's bytes, `None`: no library yet) with `new` (`(path, otrk record)`) merged in and the records of the paths in
/// `drop` removed. Returns the new file and how many of `new` replaced a record the file had.
pub fn merge(
    old: Option<&[u8]>,
    new: &[(String, Vec<u8>)],
    drop: &[String],
) -> Result<(Vec<u8>, usize), String> {
    let at: HashMap<&str, usize> = new
        .iter()
        .enumerate()
        .map(|(i, (p, _))| (p.as_str(), i))
        .collect();
    let mut done = vec![false; new.len()];
    let (mut out, mut replaced, mut has_version) = (Vec::new(), 0, false);
    for r in tlv::parse(old.unwrap_or_default())? {
        if &r.tag == b"vrsn" {
            has_version = true;
        }
        let path = (&r.tag == b"otrk").then(|| path_of(r.data)).flatten();
        match path {
            Some(p) if drop.contains(&p) => {}
            Some(p) if at.contains_key(p.as_str()) => {
                let i = at[p.as_str()];
                if !std::mem::replace(&mut done[i], true) {
                    out.extend_from_slice(&new[i].1);
                    replaced += 1;
                }
            }
            _ => out.extend(rec(&r.tag, r.data)),
        }
    }
    if !has_version {
        out.splice(0..0, text(b"vrsn", VERSION));
    }
    for (i, (_, r)) in new.iter().enumerate() {
        if !done[i] {
            out.extend_from_slice(r);
        }
    }
    Ok((out, replaced))
}
