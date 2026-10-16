//! VirtualDJ's library of a removable drive as a device plugin: `export` writes the drive's own database and its lists, so a
//! stick that carries a collection opens in VirtualDJ with its tracks, cues, loops and playlists.
//!
//! Written on the drive (below its root):
//!
//! | file | content |
//! |---|---|
//! | `VirtualDJ/database.xml` | one `<Song>` per track: tags, tempo, key, grid anchor, hot and memory cues, saved loops ([`song`]) |
//! | `VirtualDJ/MyLists/<folders>/<name>.vdjfolder` | one list per snapshot playlist; `VirtualDJ/Folders/…` on a stick that only has that folder ([`lists`]) |
//! | `Contents/<artist>/<album>/<file>` | the audio, shared with the other formats written to the stick (`place_audio`) |
//!
//! Where VirtualDJ looks: every drive that holds media has `VirtualDJ/database.xml` at its root, VirtualDJ merges the databases
//! of all connected drives while it runs, and the one in the home folder only serves the system drive (Phantom Deejay, staff of
//! virtualdj.com: forum threads 236400, 244461 and 252597; `VirtualDJ Local Database v6.xml` at the root is the format of
//! VirtualDJ 7 and older). `FilePath` is the absolute path with the drive letter (`E:\Contents\…`, `/Volumes/NAME/…` on macOS);
//! VirtualDJ replaces the drive letter or volume name by the real one when the drive is plugged in, so the database moves
//! between computers and between Windows and macOS as it is (forum threads 91468, 193485 and 242242), but a list file is not as
//! portable (the same threads). The format details are in [`song`] and [`lists`].
//!
//! Merge: songs of the drive that the snapshot does not hold are kept byte for byte, as is every other top-level element; a
//! song the snapshot holds (same file, found by the path without the drive name) keeps its play counts, colours and automix
//! points and takes the snapshot's tags, tempo, key, grid, cues and loops; a list replaces the file of the same name. The
//! database is written through a temporary file, so a failed or cancelled export leaves it as it was.
//!
//! Limits: the grid is VirtualDJ's constant one (anchor and tempo); `Size` of a loop counts beats (inferred, see [`song`]); lists
//! hold the path with the drive letter the database uses, VirtualDJ 2024 and later link them to the drive, older versions need
//! the same letter on the next computer.
//!
//! Reading (`read.rs`): `probe` and `open` take a `database.xml`, or a folder that holds `VirtualDJ/database.xml` (a drive root)
//! or `database.xml` (the `VirtualDJ` folder); the lists come from `MyLists`, `Folders` and `Playlists` next to the database.
//! A track's audio is looked for below the drive root when its `FilePath` names another drive.

pub mod song;
pub mod xml;

mod lists;
mod read;

use openfader_plugin::library::{
    audio_slot, host, place_audio, DeviceInfo, ExportOptions, ExportReport, Feature, LibraryPlugin,
    Probe, Snapshot, Source,
};
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// `database.xml` of a drive, relative to its root.
pub const DATABASE: &str = "VirtualDJ/database.xml";
/// The library name of [`DeviceInfo`].
pub const LIBRARY: &str = "VirtualDJ database";
const ROOT: &str = "VirtualDJ_Database";

pub struct Plugin;

impl LibraryPlugin for Plugin {
    /// A `database.xml` of VirtualDJ, or a folder holding one (a drive root or the `VirtualDJ` folder).
    fn probe(path: &Path) -> Option<Probe> {
        read::probe(path)
    }

    fn open(path: &Path, _settings: &Value) -> Result<Box<dyn Source>, String> {
        Ok(Box::new(read::Reader::open(path)?))
    }

    fn analysis_ref(
        reference: &str,
        _settings: &Value,
    ) -> Option<openfader_plugin::library::Analysis> {
        read::analysis_ref(reference)
    }

    fn device_probe(root: &Path, _settings: &Value) -> Result<DeviceInfo, String> {
        let modified = std::fs::metadata(root.join(DATABASE)).ok().map(|m| {
            m.modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_secs() as i64)
        });
        Ok(DeviceInfo {
            libraries: if modified.is_some() {
                vec![LIBRARY.into()]
            } else {
                vec![]
            },
            title: "VirtualDJ".into(),
            about: "VirtualDJ 8 and newer on Windows and macOS: opens the drive's own database when it is plugged in".into(),
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

    /// Evidence (module doc, Limits): the grid is VirtualDJ's constant one, anchor and tempo; the eight hot cue slots are
    /// the ones [`song::fill`] writes.
    fn capabilities(_settings: &Value) -> openfader_plugin::library::Capabilities {
        openfader_plugin::library::Capabilities {
            hot_cues: Some(8),
            tempo_changes: false,
            ..Default::default()
        }
    }
}

openfader_plugin::export_library!(Plugin);

/// A track of the snapshot that is on the stick now: its path from the root and its size.
struct Placed {
    rel: String,
    size: u64,
}

fn cancelled(phase: &str, done: usize, total: usize) -> Result<(), String> {
    if host::progress(phase, done, total) {
        Err("cancelled".into())
    } else {
        Ok(())
    }
}

/// Finds or copies the audio of every track below `root`; `None` for a track that is skipped (warning).
fn place(
    root: &Path,
    snapshot: &Snapshot,
    options: &ExportOptions,
    report: &mut ExportReport,
) -> Result<Vec<Option<Placed>>, String> {
    let mut slots = Vec::new();
    let mut need = 0u64;
    for (t, _) in &snapshot.tracks {
        let slot = audio_slot(root, t).and_then(|(rel, there)| {
            if !there {
                let file = t.file.as_deref().unwrap_or(Path::new(""));
                need += std::fs::metadata(file)
                    .map_err(|e| format!("{}: {e}", file.display()))?
                    .len();
            }
            Ok((rel, there))
        });
        slots.push(
            slot.map_err(|e| report.warnings.push(format!("skipped: {e}")))
                .ok(),
        );
    }
    if let Some(free) = options.free.filter(|f| need > *f) {
        return Err(format!(
            "not enough space on the device: {} MB of tracks to copy, {} MB free",
            need.div_ceil(1 << 20),
            free >> 20
        ));
    }
    let total = slots.iter().flatten().count();
    let mut done = 0;
    let mut placed = Vec::new();
    for ((t, _), slot) in snapshot.tracks.iter().zip(slots) {
        let Some((rel, there)) = slot else {
            placed.push(None);
            continue;
        };
        cancelled("Copying tracks", done, total)?;
        done += 1;
        let rel = if there { rel } else { place_audio(root, t)?.0 };
        let size = std::fs::metadata(root.join(&rel))
            .map_err(|e| format!("{rel}: {e}"))?
            .len();
        placed.push(Some(Placed { rel, size }));
    }
    Ok(placed)
}

/// The database of the drive: the one it holds, else an empty one.
fn load(root: &Path) -> Result<xml::Doc, String> {
    let path = root.join(DATABASE);
    match std::fs::read(&path) {
        Ok(bytes) => {
            let text =
                String::from_utf8(bytes).map_err(|_| format!("{}: not UTF-8", path.display()))?;
            let doc = xml::split(&text).map_err(|e| format!("{}: {e}", path.display()))?;
            if doc.root != ROOT {
                return Err(format!("{}: not a VirtualDJ database", path.display()));
            }
            Ok(doc)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(xml::Doc {
            head: format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<{ROOT} Version=\"8.5\">"),
            items: Vec::new(),
            root: ROOT.into(),
        }),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

/// Puts the tracks into `doc`: a song of the same file is refreshed in place, any other is appended. Returns the `FilePath`
/// and size of each track.
fn merge(
    doc: &mut xml::Doc,
    root: &Path,
    snapshot: &Snapshot,
    placed: &[Option<Placed>],
    report: &mut ExportReport,
) -> Result<Vec<Option<(String, u64)>>, String> {
    let style = doc
        .items
        .iter()
        .find_map(|i| song::Style::of(i.file_path.as_deref()?))
        .unwrap_or_else(|| song::Style::of_root(root));
    let mut at: HashMap<String, usize> = HashMap::new();
    for (n, item) in doc.items.iter().enumerate() {
        if let Some(p) = &item.file_path {
            at.entry(song::identity(p)).or_insert(n);
        }
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    let mut files = Vec::new();
    for ((t, a), p) in snapshot.tracks.iter().zip(placed) {
        let Some(p) = p else {
            files.push(None);
            continue;
        };
        let path = style.path(&p.rel);
        let entry = song::Entry {
            track: t,
            analysis: a.as_ref(),
            size: p.size,
            now,
        };
        let known = at.get(&song::identity(&path)).copied();
        let mut el = match known {
            Some(n) => xml::parse(&doc.items[n].raw).map_err(|e| format!("{path}: {e}"))?,
            None => xml::El::new("Song"),
        };
        song::fill(&mut el, &path, &entry, &mut report.warnings);
        let mut raw = String::from("\n");
        el.write(1, &mut raw);
        match known {
            Some(n) => {
                doc.items[n].raw = raw;
                report.updated += 1;
            }
            None => {
                at.insert(song::identity(&path), doc.items.len());
                doc.items.push(xml::Item {
                    raw,
                    file_path: Some(path.clone()),
                });
                report.added += 1;
            }
        }
        files.push(Some((path, p.size)));
    }
    Ok(files)
}

/// Writes `files` (and the folders they need): each through a temporary file, renamed once all are written.
fn commit(dirs: &[PathBuf], files: Vec<(PathBuf, String)>) -> Result<(), String> {
    for d in dirs {
        std::fs::create_dir_all(d).map_err(|e| format!("{}: {e}", d.display()))?;
    }
    let tmp = |p: &Path| {
        let mut t = p.as_os_str().to_owned();
        t.push(".part");
        PathBuf::from(t)
    };
    let result = files.iter().try_for_each(|(p, text)| {
        std::fs::write(tmp(p), text).map_err(|e| format!("{}: {e}", p.display()))
    });
    let result = result.and_then(|()| {
        files.iter().try_for_each(|(p, _)| {
            std::fs::rename(tmp(p), p).map_err(|e| format!("{}: {e}", p.display()))
        })
    });
    if result.is_err() {
        for (p, _) in &files {
            let _ = std::fs::remove_file(tmp(p));
        }
    }
    result
}

fn export(
    root: &Path,
    snapshot: &Snapshot,
    options: &ExportOptions,
) -> Result<ExportReport, String> {
    if !root.is_dir() {
        return Err(format!("{}: not a folder", root.display()));
    }
    let mut report = ExportReport::default();
    let mut doc = load(root)?;
    let placed = place(root, snapshot, options, &mut report)?;
    let files = merge(&mut doc, root, snapshot, &placed, &mut report)?;
    report.tracks = report.added + report.updated;
    report.playlists = snapshot.playlists.len();

    let list_dir = lists::dir(root);
    let (dirs, mut out) = lists::build(snapshot, &list_dir, &files, &mut report.warnings);
    if report.tracks > 0 {
        let mut text = doc.head.clone();
        text.extend(doc.items.iter().map(|i| i.raw.as_str()));
        text.push_str(&format!("\n</{ROOT}>\n"));
        out.push((root.join(DATABASE), text));
    }
    let mut dirs = dirs;
    dirs.push(root.join("VirtualDJ"));
    cancelled("Writing library", 0, 1)?;
    commit(&dirs, out)?;
    if !snapshot.playlists.is_empty() {
        report.notes.push(format!(
            "lists were written to VirtualDJ/{} on the device",
            list_dir.file_name().unwrap_or_default().to_string_lossy()
        ));
    }
    Ok(report)
}
