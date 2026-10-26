//! Zip reader (PKWARE APPNOTE 4.3: local headers, central directory, end record; names UTF-8; no zip64, no encryption):
//! the entries of a file, stored or deflated, streamed, CRC-32 and size checked.
//! ponytail: sizes and offsets are `u32`, so an archive over 4 GiB is not supported.

use flate2::{Crc, read::DeflateDecoder};
use compat_rekordbox_common::at;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

/// Why an archive could not be read; the caller words what the user sees.
#[derive(Debug)]
pub enum Error {
    /// The file is no zip, or its central directory is damaged.
    NotZip,
    /// The entry's CRC-32 or size does not match its header.
    Crc(String),
    /// Anything else, as a message.
    Other(String),
}

fn io(e: std::io::Error) -> Error {
    Error::Other(e.to_string())
}

struct Entry {
    name: String,
    method: u16,
    crc: u32,
    csize: u64,
    size: u64,
    local: u64,
}

/// The central directory of a zip, with the file to read entries from.
pub struct Zip {
    f: File,
    entries: Vec<Entry>,
}

fn le16(b: &[u8], at: usize) -> usize {
    u16::from_le_bytes([b[at], b[at + 1]]) as usize
}

fn le32(b: &[u8], at: usize) -> u64 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]]) as u64
}

impl Zip {
    pub fn open(path: &Path) -> Result<Zip, Error> {
        let mut f = at(File::open(path), path).map_err(Error::Other)?;
        let len = f.metadata().map_err(io)?.len();
        let tail_len = len.min(66_000);
        let mut tail = vec![0; tail_len as usize];
        f.seek(SeekFrom::Start(len - tail_len))
            .and_then(|_| f.read_exact(&mut tail))
            .map_err(io)?;
        let end = (0..tail.len().saturating_sub(21))
            .rev()
            .find(|&i| tail[i..].starts_with(&[0x50, 0x4b, 5, 6]))
            .ok_or(Error::NotZip)?;
        let (n, size, at) = (
            le16(&tail, end + 10),
            le32(&tail, end + 12),
            le32(&tail, end + 16),
        );
        let mut dir = vec![0; size as usize];
        f.seek(SeekFrom::Start(at))
            .and_then(|_| f.read_exact(&mut dir))
            .map_err(|_| Error::NotZip)?;
        let (mut entries, mut p) = (Vec::new(), 0);
        for _ in 0..n {
            if dir.get(p..p + 46).is_none() || le32(&dir, p) != 0x0201_4b50 {
                return Err(Error::NotZip);
            }
            let (nl, xl, cl) = (le16(&dir, p + 28), le16(&dir, p + 30), le16(&dir, p + 32));
            let name = dir.get(p + 46..p + 46 + nl).ok_or(Error::NotZip)?;
            entries.push(Entry {
                name: String::from_utf8_lossy(name).into_owned(),
                method: le16(&dir, p + 10) as u16,
                crc: le32(&dir, p + 16) as u32,
                csize: le32(&dir, p + 20),
                size: le32(&dir, p + 24),
                local: le32(&dir, p + 42),
            });
            p += 46 + nl + xl + cl;
        }
        Ok(Zip { f, entries })
    }

    fn find(&self, name: &str) -> Option<&Entry> {
        self.entries.iter().find(|e| e.name == name)
    }

    /// Whether the archive has an entry `name`.
    pub fn has(&self, name: &str) -> bool {
        self.find(name).is_some()
    }

    /// The uncompressed size of entry `name`.
    pub fn size(&self, name: &str) -> Option<u64> {
        self.find(name).map(|e| e.size)
    }

    /// Streams entry `name` into `out`, checking its CRC-32 and size.
    pub fn extract(&mut self, name: &str, out: &mut dyn Write) -> Result<(), Error> {
        let e = self
            .find(name)
            .ok_or_else(|| Error::Other(format!("{name} is not in the zip")))?;
        let (method, crc, csize, size, local) = (e.method, e.crc, e.csize, e.size, e.local);
        let mut h = [0; 30];
        self.f
            .seek(SeekFrom::Start(local))
            .and_then(|_| self.f.read_exact(&mut h))
            .map_err(io)?;
        if le32(&h, 0) != 0x0403_4b50 {
            return Err(Error::Other(format!("{name}: damaged zip")));
        }
        let data = local + 30 + le16(&h, 26) as u64 + le16(&h, 28) as u64;
        self.f.seek(SeekFrom::Start(data)).map_err(io)?;
        let raw = (&self.f).take(csize);
        let mut src: Box<dyn Read> = match method {
            0 => Box::new(raw),
            8 => Box::new(DeflateDecoder::new(raw)),
            m => {
                return Err(Error::Other(format!(
                    "{name}: zip method {m} is not supported (stored and deflate are)"
                )));
            }
        };
        let (mut sum, mut buf, mut n) = (Crc::new(), vec![0; 1 << 16], 0u64);
        loop {
            let k = src
                .read(&mut buf)
                .map_err(|e| Error::Other(format!("{name}: {e}")))?;
            if k == 0 {
                break;
            }
            sum.update(&buf[..k]);
            out.write_all(&buf[..k])
                .map_err(|e| Error::Other(format!("{name}: {e}")))?;
            n += k as u64;
        }
        if sum.sum() != crc || n != size {
            return Err(Error::Crc(name.to_owned()));
        }
        Ok(())
    }

    /// The whole entry `name`, CRC-checked.
    pub fn read(&mut self, name: &str) -> Result<Vec<u8>, Error> {
        let mut v = Vec::new();
        self.extract(name, &mut v)?;
        Ok(v)
    }
}
