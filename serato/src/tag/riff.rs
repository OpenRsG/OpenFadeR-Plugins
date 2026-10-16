//! WAV and AIFF: an ID3v2 tag inside an `id3 ` (WAV, little-endian sizes) or `ID3 ` (AIFF, big-endian sizes) chunk
//! (<https://github.com/Holzhaus/serato-tags/blob/main/docs/fileformats.md>). An existing chunk is rewritten inside its own
//! size; one that has to grow, or that is missing, is written at the end of the container (the container size is updated).

use super::{id3, read_n, Edit, Error, Tags};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};

pub fn plan(f: &mut File, tags: &Tags, fixed: bool) -> Result<Vec<Edit>, Error> {
    let file_len = f.metadata()?.len();
    let mut h = [0; 12];
    f.read_exact(&mut h)?;
    let le = match &h[..4] {
        b"RIFF" => true,
        b"FORM" => false,
        _ => return Err(Error::Unsupported),
    };
    let int = |b: &[u8]| {
        let b = [b[0], b[1], b[2], b[3]];
        if le {
            u32::from_le_bytes(b)
        } else {
            u32::from_be_bytes(b)
        }
    };
    let put = |n: u32| if le { n.to_le_bytes() } else { n.to_be_bytes() };
    let size = int(&h[4..8]);
    let end = (8 + u64::from(size)).min(file_len);
    let chunk_id: &[u8; 4] = if le { b"id3 " } else { b"ID3 " };

    let (mut pos, mut found) = (12, None);
    while pos + 8 <= end {
        f.seek(SeekFrom::Start(pos))?;
        let mut c = [0; 8];
        f.read_exact(&mut c)?;
        let len = u64::from(int(&c[4..8]));
        if c[..4].eq_ignore_ascii_case(chunk_id) {
            found = Some((pos, len));
            break;
        }
        pos += 8 + len + (len & 1);
    }

    let (mut room, mut old) = (0, None);
    if let Some((pos, len)) = found {
        f.seek(SeekFrom::Start(pos + 8))?;
        let body = read_n(f, len)?;
        room = len as usize;
        if body.len() != room {
            return Err(Error::Unsupported);
        }
        old = id3::total(&body)
            .filter(|&t| t <= room)
            .and_then(|t| id3::parse(&body[..t]));
        if old.is_none() && tags.blank() {
            return Ok(vec![]);
        }
        if old.is_none() && fixed {
            return Err(Error::NoRoom);
        }
    }
    let Some(tag) = id3::rebuild(old.as_ref(), room, tags, fixed)? else {
        return Ok(vec![]);
    };
    if found.is_some() && tag.len() == room {
        let start = found.map_or(0, |(p, _)| p) + 8;
        return Ok(vec![Edit {
            start,
            end: start + room as u64,
            data: tag,
        }]);
    }
    // A chunk of its own size, replacing the old one or appended at the declared end of the container.
    let mut chunk = [&chunk_id[..], &put(tag.len() as u32)].concat();
    chunk.extend(&tag);
    chunk.resize(chunk.len() + (tag.len() & 1), 0);
    let (start, stop) = found.map_or((end, end), |(p, len)| (p, p + 8 + len + (len & 1)));
    let new_size = u64::from(size) + chunk.len() as u64 - (stop - start);
    Ok(vec![
        Edit {
            start: 4,
            end: 8,
            data: put(new_size as u32).to_vec(),
        },
        Edit {
            start,
            end: stop,
            data: chunk,
        },
    ])
}
