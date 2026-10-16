//! Serato DJ library on a USB stick or SD card (export only).
//!
//! | Where (below the drive root) | What |
//! |---|---|
//! | `_Serato_/database V2` | one `otrk` record per track, merged into the file ([`database`]) |
//! | `_Serato_/Subcrates/<parent>%%<child>.crate` | one crate per playlist ([`crate_file`]) |
//! | the shared audio copy (`Contents/<artist>/<album>/<file>`) | `Serato Markers2` and `Serato BeatGrid` tags, written in place when they fit ([`tag`]) |
//! | `_Serato_/Audio/<artist>/<album>/<file>` | a private copy of a track whose tags do not fit the shared copy |
//!
//! Serato keeps cues, loops, track colour and beat grid only in the audio file's tags. Other software on the same stick finds a
//! track by file name and size, so a shared copy never changes size: the tags are rewritten into the padding the file has, and
//! a track that has no room gets a private copy that is tagged freely (and is the one the database and crates point at from then on).
//!
//! Format sources: <https://github.com/Holzhaus/serato-tags> (tags, colours), Mixxx's `src/library/serato/` and
//! `src/track/serato/` (database and crates); the crate headers and text shapes are observed in files Serato wrote.

pub mod crate_file;
pub mod database;
mod export;
pub mod tag;
mod tlv;

use openfader_plugin::library::{
    DeviceInfo, ExportOptions, ExportReport, Feature, LibraryPlugin, Probe, Snapshot, Source,
};
use serde_json::Value;
use std::path::Path;
use std::time::UNIX_EPOCH;

/// Name of the library in [`DeviceInfo::libraries`].
const LIBRARY: &str = "Serato library";

pub struct Plugin;

impl LibraryPlugin for Plugin {
    fn probe(_path: &Path) -> Option<Probe> {
        None
    }

    fn open(_path: &Path, _settings: &Value) -> Result<Box<dyn Source>, String> {
        Err("the Serato plugin reads no library".into())
    }

    fn device_probe(root: &Path, _settings: &Value) -> Result<DeviceInfo, String> {
        let db = std::fs::metadata(root.join(database::FILE))
            .ok()
            .filter(|m| m.is_file());
        Ok(DeviceInfo {
            libraries: if db.is_some() {
                vec![LIBRARY.into()]
            } else {
                vec![]
            },
            title: "Serato DJ".into(),
            about: "Serato DJ Pro and Serato DJ Lite on a computer; the cues travel inside the audio files".into(),
            can_write: root.is_dir(),
            features: vec![Feature::Export],
            modified: db
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64),
            ..DeviceInfo::default()
        })
    }

    fn export(
        root: &Path,
        snapshot: &Snapshot,
        options: &ExportOptions,
        _settings: &Value,
    ) -> Result<ExportReport, String> {
        export::run(root, snapshot, options)
    }
}

openfader_plugin::export_library!(Plugin);
