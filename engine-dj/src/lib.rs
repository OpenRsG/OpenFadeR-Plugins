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
//! | `m.db`, `PerformanceData` | per track: `trackData`, `overviewWaveFormData`, `beatData`, `quickCues`, `loops` ([`blob`]); a table in 3.x, a view over `Track` in 2.x |
//! | `m.db`, `Playlist`, `PlaylistEntity` | the folder and list tree ([`playlists`]) |
//!
//! A track is identified by its `path`: a library track found there is refreshed (metadata, cues, loops, grid, waveform) and
//! never deleted, any other is added. A list that exists by name path gets the snapshot's tracks, other lists stay. The new
//! database is built beside `m.db` and renamed over it once it passes `PRAGMA integrity_check`, so a failed or cancelled export
//! leaves the stick as it was.
//!
//! # Format version: schema 3.0.0, or 2.18.0 for older Engine
//!
//! libdjinterop (`to_application_version_string`) names the Engine DJ Desktop/OS releases that write each schema version of
//! `Database2/m.db`; the last column is what this plugin does with it:
//!
//! | Schema | Written by Engine DJ Desktop and OS | Here |
//! |---|---|---|
//! | 2.18.0 | 2.0.x to 2.1.x | new library with the option `engine_dj.schema` = `2.18.0`; extended |
//! | 2.20.1 to 2.20.3 | 2.2.x to 3.0.1 | extended |
//! | 2.21.0 to 2.21.2 | 3.1.0 to 4.0.1 | extended |
//! | 3.0.0 | 4.1.0 to 4.2.1 | new library by default (`engine_dj.schema` = `3.0.0`); extended |
//! | 3.0.1 | 4.3.x | extended |
//! | 3.0.2 | 4.5.0, 5.0.x | extended |
//!
//! An existing `m.db` of one of these versions is extended in place and keeps its version: the rows written are valid for each
//! (3.0.1 only changes triggers, 3.0.2 adds the nullable `Track.albumArtSourceHash` which stays NULL, 2.20.3 and later have
//! `Track.lastEditTime`, 2.18.0 to 2.20.2 do not). The option only decides the schema of a new library and converts nothing: on
//! a stick that holds one, a report note says that the version found was kept. Any other version (1.x, a release newer than 3.0.2)
//! is refused with the version found and nothing is changed, as is a library in the old two-file layout (`Engine Library/m.db`).
//!
//! A new library is 3.0.0, the oldest schema of the 3.x line: the later releases (4.3, 4.5, 5.x) still carry it, read it and
//! upgrade it [inferred]. 3.0.0 has the separate `PerformanceData` table, so one `UPDATE` of one row carries every analysis blob.
//! For Engine DJ and players older than 4.1 (they refuse 3.x) the option gives 2.18.0, the oldest `Database2` schema (Engine
//! 2.0.x); it is the schema that reaches back furthest, and the releases up to 4.0.1 are expected to read an older schema and
//! upgrade it [inferred: libdjinterop reads and writes each version but says nothing about upgrades, and no library of
//! this version was tried on a player]. 2.x keeps the analysis blobs in `Track`; `PerformanceData` is a view over `Track` with an
//! `INSTEAD OF UPDATE` trigger per column, so the `UPDATE PerformanceData` used for 3.x reaches the same columns
//! ([`schema_2`] has the DDL). The blob layouts are the same in both lines (libdjinterop shares `engine/v2/*_blob.cpp`). Only `m.db`
//! is made: libdjinterop writes no other file for these schemas (the history and streaming databases are the players' own)
//! [inferred from libdjinterop, not tried on hardware]. Schema 1.x (Engine Prime and Engine OS 1.x, the two-file layout) is not
//! written.
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
//! libdjinterop (<https://github.com/xsco/libdjinterop>, LGPL-3.0): `include/djinterop/engine/engine_schema.hpp` (the schema lists
//! and the Engine releases of each), `src/djinterop/engine/schema/schema.cpp` (version detection), `schema_2_18_0.cpp` and
//! `schema_3_0_0.cpp` (DDL, the `Information` row), `schema_3_0_1.cpp`, `schema_3_0_2.cpp` and the 2.x files (differences),
//! `engine/v2/*_blob.cpp` (blob layouts), `engine/v2/track_table.cpp` (`lastEditTime` from 2.20.3), `engine/v3/track_impl.cpp` and
//! `engine/v2/track_impl.cpp` (`Track` row defaults, the same in both), `engine/v2/playlist_*` and `playlist_entity_table.cpp`
//! (list and entity chains), `engine/engine.cpp` (`normalize_beatgrid`), `engine/track_utils.hpp` (overview extents). This plugin
//! shares no code with it.

pub mod blob;
pub mod perf;
pub mod playlists;
pub mod schema;
pub mod schema_2;
pub mod tracks;

use openfader_plugin::library::{
    audio_slot, host, place_audio, Choice, DeviceInfo, ExportOptions, ExportReport, Feature, Field,
    FieldKind, LibraryPlugin, Probe, Scope, Snapshot, Source,
};
use serde_json::Value;
use std::path::{Path, PathBuf};

/// Name of the library in [`DeviceInfo::libraries`] and the folder it lives in.
const LIBRARY: &str = "Engine Library";

/// Key of the device option that picks the schema of a new library ([`schema::NEW`], as [`schema::text`]).
pub const SCHEMA_OPTION: &str = "engine_dj.schema";

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
            title: "Engine DJ".into(),
            about: "Engine DJ 4.1 and newer and Denon DJ Prime/SC, Numark and RANE players on firmware 4.1 and newer; the schema option adds 2.0 to 4.0.1"
                .into(),
            options: vec![Field {
                key: SCHEMA_OPTION.into(),
                label: "Schema of a new library".into(),
                hint: "Only for a library that is made now: one that exists keeps its schema and is extended. Older Engine \
                       versions need 2.18.0 and are expected to be read by the newer ones too."
                    .into(),
                default: schema::text(schema::CURRENT),
                scope: Scope::Device,
                kind: FieldKind::Choice {
                    choices: vec![
                        Choice {
                            value: schema::text(schema::CURRENT),
                            label: "3.0.0: Engine DJ and players 4.1 and newer".into(),
                        },
                        Choice {
                            value: schema::text(schema::OLDEST),
                            label: "2.18.0: Engine DJ and players 2.0 to 4.0.1".into(),
                        },
                    ],
                    from_list: None,
                    none: None,
                },
            }],
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
    let want = match options.config.get(SCHEMA_OPTION) {
        None => schema::CURRENT,
        Some(v) => schema::parse_new(v)
            .ok_or_else(|| format!("{SCHEMA_OPTION}: unknown schema {v:?}; use 3.0.0 or 2.18.0"))?,
    };
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

    let mut staged = schema::stage(&db, want)?;
    if staged.version != want && want != schema::CURRENT {
        report.notes.push(format!(
            "the library on the device has schema {}, so it was extended as it is; {} applies to a new library only",
            schema::text(staged.version),
            schema::text(want)
        ));
    }
    let edit_time = schema::has_edit_time(staged.version);
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
            edit_time,
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
