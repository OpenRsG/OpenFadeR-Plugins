//! Engine DJ library on a device: `Engine Library/Database2/m.db`, the database Denon DJ (Prime, SC6000, SC Live), Numark and
//! RANE standalone players and Engine DJ desktop read. Write only (`probe` and `open` answer nothing); `export` merges the
//! snapshot into the library of the device and makes one when there is none.
//!
//! # What is written where
//!
//! | Where | What |
//! |---|---|
//! | `Contents/<artist>/<album>/<file>` | the audio, one copy shared with every other format on the stick (`place_audio`) |
//! | `Engine Library/Database2/m.db`, `Track` | one row per track: metadata, key, rating, `path` = `../<audio path>` |
//! | `m.db`, `PerformanceData` | per track: `trackData`, `overviewWaveFormData`, `beatData`, `quickCues`, `loops` ([`blob`]) |
//! | `m.db`, `Playlist`, `PlaylistEntity` | the folder and list tree ([`playlists`]) |
//!
//! A track is identified by its `path`: a library track found there is refreshed (metadata, cues, loops, grid, waveform) and
//! never deleted, any other is added. A list that exists by name path gets the snapshot's tracks, other lists stay. The new
//! database is built beside `m.db` and renamed over it once it passes `PRAGMA integrity_check`, so a failed or cancelled export
//! leaves the stick as it was.
//!
//! # Format version: schema 3.0.0
//!
//! Engine DJ 4.1 to 4.2 wrote it (libdjinterop `to_application_version_string`), and it is the oldest schema of the line the
//! later releases (4.3, 4.5, 5.x) still carry: they read it and upgrade it [inferred], while 3.0.1 and 3.0.2 only add album art
//! columns and are refused by firmware older than 4.3. Earlier schemas (2.x, Engine DJ 2.0 to 4.0.1) keep the analysis blobs
//! inside `Track`; 3.0.0 has the separate `PerformanceData` table, so one `UPDATE` of one row carries every analysis blob.
//! An existing `m.db` with another version is not touched: the export fails with the version it found. A library in the old
//! two-file layout (`Engine Library/m.db`) is refused the same way. Only `m.db` is made: libdjinterop writes no other file for
//! this schema (the history and streaming databases are the players' own) [inferred from libdjinterop, not tried on hardware].
//!
//! # Conventions
//!
//! * `Track.path` is relative to the `Engine Library` folder: `../Contents/...` for audio at the root of the stick
//!   (libdjinterop `track_table.hpp`: "relative path from the location of the Engine library to the music file").
//! * Positions are samples (`f64`): `time_ms * rate / 1000`, the rate from the track or the host.
//! * Rating is stars x 20; the key is Engine's id (0 = C major = 8B, 1 = A minor = 8A, 2 = 9B, ...).
//! * The sample count is `length_s * rate`: the host knows the length in whole seconds only. It scales the overview waveform
//!   and places the last grid marker; the error is under half a second of the track.
//! * A track without a sample rate gets its row and no performance data (a warning says so).
//! * `isAnalyzed` is 1 when the track has an overview waveform and a grid (written now or by the player), else 0: a row without
//!   blobs and with `isAnalyzed = 0` is what Engine itself leaves a track it has not analysed yet [inferred: the insert trigger of
//!   the schema creates the empty `PerformanceData` row, `isAnalyzed` is the flag Engine desktop's analyser selects on].
//! * Not written: artwork (`albumArtId` 1 is the stock "none" row), memory cues beyond the main cue, cues beyond the eighth
//!   slot, MyTags and track colours (the report says how many cues were dropped).
//!
//! # Sources
//!
//! libdjinterop (<https://github.com/xsco/libdjinterop>, LGPL-3.0): `src/djinterop/engine/schema/schema_3_0_0.cpp` (DDL, the
//! `Information` row), `engine/v2/*_blob.cpp` (blob layouts), `engine/v3/track_impl.cpp` (`Track` row defaults, `PerformanceData`
//! from a track), `engine/v2/playlist_*` and `playlist_entity_table.cpp` (list and entity chains), `engine/engine.cpp`
//! (`normalize_beatgrid`), `engine/track_utils.hpp` (overview extents). This plugin shares no code with it.

pub mod blob;
pub mod perf;
pub mod playlists;
pub mod schema;
pub mod tracks;

use openfader_plugin::library::{
    audio_slot, host, place_audio, DeviceInfo, ExportOptions, ExportReport, Feature, LibraryPlugin,
    Probe, Snapshot, Source,
};
use serde_json::Value;
use std::path::{Path, PathBuf};

/// Name of the library in [`DeviceInfo::libraries`] and the folder it lives in.
const LIBRARY: &str = "Engine Library";

/// `Engine Library/Database2/m.db` of the device mounted at `root`.
pub fn database(root: &Path) -> PathBuf {
    root.join(LIBRARY).join("Database2").join("m.db")
}

pub struct Plugin;

impl LibraryPlugin for Plugin {
    fn probe(_path: &Path) -> Option<Probe> {
        None
    }

    fn open(_path: &Path, _settings: &Value) -> Result<Box<dyn Source>, String> {
        Err("the Engine DJ plugin reads no library, it only writes one to a device".into())
    }

    fn device_probe(root: &Path, _settings: &Value) -> Result<DeviceInfo, String> {
        let db = database(root);
        let modified = std::fs::metadata(&db)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64);
        Ok(DeviceInfo {
            libraries: if db.is_file() {
                vec![LIBRARY.into()]
            } else {
                vec![]
            },
            can_write: root.is_dir(),
            features: vec![Feature::Export],
            modified,
            ..Default::default()
        })
    }

    fn export(
        root: &Path,
        snapshot: &Snapshot,
        options: &ExportOptions,
        _settings: &Value,
    ) -> Result<ExportReport, String> {
        export(root, snapshot, options)
    }
}

/// Megabytes, rounded up, for a message.
fn mb(bytes: u64) -> u64 {
    bytes.div_ceil(1 << 20)
}

/// `Err("cancelled")` when the user cancelled the job.
fn tick(phase: &str, done: usize, total: usize) -> Result<(), String> {
    if host::progress(phase, done, total) {
        Err("cancelled".into())
    } else {
        Ok(())
    }
}

fn export(
    root: &Path,
    snapshot: &Snapshot,
    options: &ExportOptions,
) -> Result<ExportReport, String> {
    let db = database(root);
    if root.join(LIBRARY).join("m.db").is_file() {
        return Err(format!(
            "{} holds an Engine library in the old two-file format; open it in Engine DJ once to convert it, nothing was changed",
            root.join(LIBRARY).display()
        ));
    }
    let mut report = ExportReport::default();

    // Where each track's audio goes, and whether it all fits before anything is copied.
    let mut slots: Vec<Option<String>> = Vec::with_capacity(snapshot.tracks.len());
    let mut need = 0u64;
    for (t, _) in &snapshot.tracks {
        slots.push(match audio_slot(root, t) {
            Ok((rel, there)) => {
                if !there {
                    need += t
                        .file
                        .as_deref()
                        .and_then(|f| std::fs::metadata(f).ok())
                        .map_or(0, |m| m.len());
                }
                Some(rel)
            }
            Err(e) => {
                report.warnings.push(format!("{}: skipped, {e}", t.title));
                None
            }
        });
    }
    if let Some(free) = options.free.filter(|&f| need > f) {
        return Err(format!(
            "not enough space on the device: the new tracks need {} MB and {} MB are free",
            mb(need),
            mb(free)
        ));
    }

    let total = slots.iter().flatten().count();
    let mut done = 0;
    for (slot, (t, _)) in slots.iter_mut().zip(&snapshot.tracks) {
        if slot.is_none() {
            continue;
        }
        tick("Copying tracks", done, total)?;
        done += 1;
        match place_audio(root, t) {
            Ok((rel, _)) => *slot = Some(rel),
            Err(e) => {
                report.warnings.push(format!("{}: skipped, {e}", t.title));
                *slot = None;
            }
        }
    }

    let mut staged = schema::stage(&db)?;
    let tx = staged.conn.transaction().map_err(|e| e.to_string())?;
    let uuid = staged.uuid.clone();
    let db_err = |e: rusqlite::Error| format!("{}: {e}", db.display());
    let mut ids: Vec<Option<i64>> = vec![None; snapshot.tracks.len()];
    let (mut dropped, mut unanalysed) = (0, 0);
    let total = slots.iter().flatten().count();
    let mut done = 0;
    for (i, ((t, a), slot)) in snapshot.tracks.iter().zip(&slots).enumerate() {
        let Some(rel) = slot else { continue };
        tick("Writing library", done, total)?;
        done += 1;
        let file = root.join(rel);
        let rate = if t.sample_rate > 0 {
            t.sample_rate
        } else {
            host::sample_rate(&file)
        };
        if rate == 0 && a.is_some() {
            // The row below still goes in; only the performance data is skipped.
            report.warnings.push(format!(
                "{}: sample rate unknown, its cues, loops and grid were not written",
                t.title
            ));
        }
        let placed = tracks::Placed {
            track: t,
            analysis: a.as_ref(),
            path: format!("../{rel}"),
            file: &file,
            rate,
        };
        let (id, w) = tracks::upsert(&tx, &uuid, &placed).map_err(db_err)?;
        ids[i] = Some(id);
        if w.added {
            report.added += 1;
        } else {
            report.updated += 1;
        }
        dropped += w.dropped;
        unanalysed += usize::from(w.added && !w.analyzed);
    }
    playlists::write(&tx, &uuid, snapshot, &ids).map_err(db_err)?;
    tx.commit().map_err(db_err)?;
    tick("Writing library", total, total)?;
    staged.commit()?;

    report.tracks = report.added + report.updated;
    report.playlists = snapshot.playlists.len();
    if dropped > 0 {
        report.notes.push(format!("{dropped} cues or loops have no place in an Engine library (one main cue, 8 hot cues, 8 loops) and were left out"));
    }
    if unanalysed > 0 {
        report.notes.push(format!(
            "{unanalysed} new tracks have no waveform and grid: the players analyse them when loaded"
        ));
    }
    Ok(report)
}

openfader_plugin::export_library!(Plugin);
