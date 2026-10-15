//! Shared by the rekordbox plugins (the xml collection, iTunes, the device and database libraries): the mini XML reader,
//! path remapping, `file://` locations, and the readers of the formats the libraries are made of: the DeviceSQL byte
//! readers ([`pdb`]), the analysis files ([`anlz`]), SQLCipher databases ([`sql`]) and the library helpers they share
//! ([`list`]).

pub mod anlz;
pub mod list;
pub mod markup;
pub mod pathmap;
pub mod pdb;
pub mod sql;

pub use markup::{Edge, Tag, tags};
pub use pathmap::{PathMap, parse_path_map, path_map, remap_path};
use serde_json::Value;
use std::path::{Path, PathBuf};

/// A plugin setting (`settings` of a request) as trimmed text; `None` when absent, not a string or empty.
pub fn setting(settings: &Value, id: &str) -> Option<String> {
    let s = settings.get(id)?.as_str()?.trim();
    (!s.is_empty()).then(|| s.to_owned())
}

/// Adds the path to an IO error: `path: message`.
pub fn at<T, E: std::fmt::Display>(r: Result<T, E>, p: &Path) -> Result<T, String> {
    r.map_err(|e| format!("{}: {e}", p.display()))
}

/// `rel` as a device library stores a path (leading `/` allowed) under the device root `root`.
pub fn on_device(root: &Path, rel: &str) -> PathBuf {
    root.join(rel.trim_start_matches('/'))
}

/// `file://localhost/C:/Music/a%20b.mp3` → `C:/Music/a b.mp3`, then through `map`.
pub fn location(url: &str, map: &PathMap) -> Option<PathBuf> {
    let p = url
        .strip_prefix("file://localhost")
        .or_else(|| url.strip_prefix("file://"))
        .unwrap_or(url);
    let b = p.as_bytes();
    let mut raw = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match (
            b[i],
            p.get(i + 1..i + 3)
                .and_then(|h| u8::from_str_radix(h, 16).ok()),
        ) {
            (b'%', Some(byte)) => {
                raw.push(byte);
                i += 3;
            }
            (c, _) => {
                raw.push(c);
                i += 1;
            }
        }
    }
    let p = String::from_utf8_lossy(&raw);
    // A drive path follows the URI's slash: `/C:/Music` → `C:/Music`.
    let drive = matches!(p.as_bytes(), [b'/', d, b':', ..] if d.is_ascii_alphabetic());
    let p = if drive { &p[1..] } else { &p[..] };
    (!p.is_empty()).then(|| PathBuf::from(remap_path(p, map)))
}
