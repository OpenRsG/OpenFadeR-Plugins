//! The device side of the rekordbox plugins: writes, updates and edits the libraries on a device, its settings files and its
//! backups (the device ops of `openfader_plugin::library`). Called by `rekordbox-device`.
//!
//! Device export (rekordbox EXPORT mode): writes a [`Snapshot`] of a library to a folder or removable drive as a rekordbox
//! Device Library that players and rekordbox read, or adds it to one that is already there:
//!
//! ```text
//! PIONEER/rekordbox/export.pdb        DeviceSQL: tracks, artists, albums, genres, keys, colors, playlist tree + entries, histories, columns
//! PIONEER/rekordbox/exportExt.pdb     MyTag (only when there is MyTag)
//! PIONEER/rekordbox/exportLibrary.db  OneLibrary (SQLCipher; the built-in key, or the plugin's `onelibrary_key` setting)
//! PIONEER/USBANLZ/P<xxx>/<8 hex>/ANLZ0000.{DAT,EXT,2EX}   grid, cues, waveforms of each track
//! PIONEER/Artwork/<00001>/{a,b}<n>.jpg   covers (the file itself, no thumbnail resizing; `a` for export.pdb, `b` for OneLibrary)
//! PIONEER/{MYSETTING,MYSETTING2,DJMMYSETTING,DEVSETTING}.DAT   device settings
//! PIONEER/djprofile.nxs                DJ profile (written when the device has none, see [`profile`])
//! Contents/<artist>/<album>/<file>    copies of the audio files
//! ```
//!
//! Children: [`pdb`] (DeviceSQL writer + reader), [`onelibrary`] (`exportLibrary.db`), [`anlz`] (analysis files), [`settings`],
//! `run` (the engine of `export`), `merge` (snapshot playlists and MyTag into an existing library), [`remove`] (delete tracks,
//! playlists and histories), [`convert`] (Device Library → OneLibrary), `carry` (what a rewritten OneLibrary keeps), [`update`]
//! (Update Collection: what the player changed), [`bank`] (Hot Cue Bank Lists), `progress` (host progress and cancel).
//! Format sources: `.local/re/export/notes.md`.
//! The tracks' rating, comment, cues and grid are whatever the snapshot carries; the host sends the *overlaid* library with the
//! store's colours, MyTag and user playlists already in it.
//!
//! `export` reads an existing library into [`pdb::db::Db`], adds / updates the snapshot's tracks and playlists and writes the
//! whole library again (tmp file + rename). The library files are written last, so a cancelled or failed export leaves the device
//! as it was (the audio and analysis files it had already copied for new tracks are removed again).

pub mod anlz;
pub mod bank;
mod carry;
pub mod convert;
mod merge;
pub mod onelibrary;
pub mod pdb;
pub mod profile;
pub mod progress;
pub mod remove;
mod run;
pub mod settings;
pub mod update;

pub use settings::{read as settings_read, schema as settings_schema, write as settings_write};

use openfader_plugin::library::{
    ActionAnswer, BackupTrack, DeviceAction, DeviceInfo, ExportOptions, ExportReport, Feature,
    RemoveReport, RemoveTarget, Snapshot, UpdateReport,
};
use pdb::db::{Db, Tags};
use progress::Progress;
use rekordbox_common::at;
use rusqlite::{Connection, OpenFlags};
use serde_json::Value;
use settings::DeviceConfig;
use std::collections::{BTreeMap, HashMap};
use std::path::Path;

/// `PIONEER/rekordbox/export.pdb`, relative to the device root.
pub const PDB: &str = "PIONEER/rekordbox/export.pdb";
/// `PIONEER/rekordbox/exportExt.pdb` (MyTag).
pub const EXT: &str = "PIONEER/rekordbox/exportExt.pdb";
/// `PIONEER/rekordbox/exportLibrary.db` (OneLibrary).
pub const ONE: &str = "PIONEER/rekordbox/exportLibrary.db";

/// Ids by name in order of first use; the empty name is id 0 (= none). Shared by the DeviceSQL and OneLibrary writers.
#[derive(Default)]
struct Names {
    ids: HashMap<String, u32>,
    rows: Vec<(u32, String)>,
}

impl Names {
    fn id(&mut self, name: &str) -> u32 {
        if name.is_empty() {
            return 0;
        }
        if let Some(&id) = self.ids.get(name) {
            return id;
        }
        let id = self.rows.len() as u32 + 1;
        self.ids.insert(name.to_owned(), id);
        self.rows.push((id, name.to_owned()));
        id
    }
}

/// Opens the SQLCipher database `path` with `key`.
pub(crate) fn open_keyed(path: &Path, key: &str) -> Result<Connection, String> {
    keyed(Connection::open(path), path, key)
}

/// [`open_keyed`] without write access.
pub(crate) fn open_keyed_ro(path: &Path, key: &str) -> Result<Connection, String> {
    keyed(
        Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY),
        path,
        key,
    )
}

fn keyed(
    opened: rusqlite::Result<Connection>,
    path: &Path,
    key: &str,
) -> Result<Connection, String> {
    let conn = at(opened, path)?;
    at(conn.pragma_update(None, "key", key), path)?;
    Ok(conn)
}

/// The name of a `columns` / `menuItem` row: the label between the marker characters rekordbox wraps it in.
fn column_label(label: &str) -> String {
    format!("\u{fffa}{label}\u{fffb}")
}

fn put(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        at(std::fs::create_dir_all(dir), dir)?;
    }
    at(std::fs::write(path, bytes), path)
}

/// Writes `bytes` next to `path` and renames them over it, so `path` is whole or untouched.
fn put_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let tmp = path.with_extension("tmp");
    put(&tmp, bytes)?;
    at(std::fs::rename(&tmp, path), path)
}

/// A file or folder name: characters FAT and rekordbox reject become `_`; empty → `fallback` (the SDK's rule, shared by every
/// device format so they find each other's audio copies).
pub use openfader_plugin::library::clean_name as clean;

/// Today's local date as `YYYY-MM-DD`.
fn today() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

/// Builds a new `exportLibrary.db` next to `one` from `db` and `tags`, lets `then` add to it, and renames it over `one`; the
/// temporary file is removed when any step fails. The library id is kept from an existing `one`.
fn rebuild_onelibrary(
    one: &Path,
    key: &str,
    db: &Db,
    tags: &Tags,
    then: impl FnOnce(&Path) -> Result<(), String>,
) -> Result<(), String> {
    let id = one
        .is_file()
        .then(|| carry::library_id(one, key))
        .flatten()
        .unwrap_or(chrono::Local::now().timestamp() as u32); // any number; rekordbox's is random per library
    let tmp = one.with_extension("new");
    let _ = std::fs::remove_file(&tmp);
    onelibrary::write(&tmp, key, db, tags, id)
        .and_then(|()| then(&tmp))
        .and_then(|()| at(std::fs::rename(&tmp, one), one))
        .inspect_err(|_| drop(std::fs::remove_file(&tmp)))
}

/// Writes the library files of `dest` from `db` and `tags`: `exportLibrary.db` (encrypted with `key`; an existing one hands its
/// histories and cue rows over, see `carry`), `exportExt.pdb` (when there is MyTag or one existed) and, last, `export.pdb`.
fn commit(dest: &Path, db: &Db, tags: &Tags, key: &str) -> Result<(), String> {
    let (one, ext) = (dest.join(ONE), dest.join(EXT));
    rebuild_onelibrary(&one, key, db, tags, |tmp| {
        let carried = if one.is_file() {
            carry::carry(&one, tmp, key)
        } else {
            carry::histories(tmp, key, db)
        };
        carried.and_then(|()| bank::apply(tmp, key, &db.banks))
    })?;
    if !tags.categories.is_empty() || ext.is_file() {
        put_atomic(&ext, &tags.encode()?)?;
    }
    put_atomic(&dest.join(PDB), &db.encode()?)
}

/// The OneLibrary key: the plugin's setting, else the built-in one, so both libraries are always written.
fn key(settings: &Value) -> String {
    rekordbox_common::onelibrary_key(settings)
}

/// What the plugin does with a device root: its libraries, whether it can write, the convert action, the files to read ahead and
/// to back up.
pub fn device_probe(root: &Path, _settings: &Value) -> Result<DeviceInfo, String> {
    let libs = [("Device Library", PDB), ("OneLibrary", ONE)];
    let found = |f: &str| root.join(f).is_file();
    let modified = [PDB, ONE].iter().find(|f| found(f)).and_then(|f| {
        let m = std::fs::metadata(root.join(f)).ok()?.modified().ok()?;
        Some(m.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs() as i64)
    });
    Ok(DeviceInfo {
        libraries: libs
            .iter()
            .filter(|(_, f)| found(f))
            .map(|(n, _)| (*n).to_owned())
            .collect(),
        can_write: root.is_dir(),
        features: vec![
            Feature::Export,
            Feature::Update,
            Feature::Remove,
            Feature::Histories,
            Feature::Backup,
            Feature::Settings,
        ],
        actions: vec![DeviceAction {
            id: convert::ID.into(),
            label: "Convert from Device Library".into(),
            help: "Builds the OneLibrary from the Device Library of this device; an existing OneLibrary is replaced.".into(),
            enabled: found(PDB),
            writes: true,
        }],
        prefetch: [PDB, EXT, ONE]
            .iter()
            .filter(|f| found(f))
            .map(|f| (*f).to_owned())
            .collect(),
        backup: vec![settings::DIR.into()],
        modified,
    })
}

/// Writes `snapshot` to the device (a new library when it has none, else extends it), see the module docs.
pub fn export(
    root: &Path,
    snapshot: &Snapshot,
    options: &ExportOptions,
    settings: &Value,
) -> Result<ExportReport, String> {
    let key = key(settings);
    let config = DeviceConfig::from_values(&options.config);
    let mut progress = Progress::new();
    let (mut report, ids) = run::run(root, snapshot, options, &config, &key, &mut progress)?;
    match update::baselines(root, &ids, snapshot, &mut progress) {
        Ok(b) => report.baselines = b,
        Err(e) => report.warnings.push(format!("baselines not recorded: {e}")),
    }
    report.fingerprint = settings_read(root, settings)
        .ok()
        .map(|r| r.fingerprint)
        .filter(|f| !f.is_empty());
    Ok(report)
}

/// What changed on the player since `baselines`.
pub fn update(
    root: &Path,
    snapshot: &Snapshot,
    baselines: &BTreeMap<String, i64>,
    _settings: &Value,
) -> Result<UpdateReport, String> {
    update::update(root, snapshot, baselines)
}

/// Deletes a playlist, histories or a track from the device.
pub fn remove(
    root: &Path,
    target: &RemoveTarget,
    delete_tracks: bool,
    config: &BTreeMap<String, String>,
    settings: &Value,
) -> Result<RemoveReport, String> {
    remove::run(
        root,
        target,
        delete_tracks,
        &DeviceConfig::from_values(config),
        &key(settings),
    )
}

/// Runs the device action `action` (`convert`).
pub fn device_action(
    root: &Path,
    action: &str,
    confirmed: bool,
    config: &BTreeMap<String, String>,
    settings: &Value,
) -> Result<ActionAnswer, String> {
    match action {
        convert::ID => convert::to_onelibrary(
            root,
            &key(settings),
            confirmed,
            &DeviceConfig::from_values(config),
        ),
        _ => Err(format!("unknown action {action}")),
    }
}

/// The tracks the library inside the backup folder `dir` lists: what a restore copies back when the device lacks them.
pub fn backup_tracks(dir: &Path, _settings: &Value) -> Result<Vec<BackupTrack>, String> {
    if !dir.join(PDB).is_file() {
        return Err(format!("{} is not a device library backup", dir.display()));
    }
    let (db, _) = pdb::read::open(dir, &DeviceConfig::default())?;
    Ok(db
        .tracks
        .into_iter()
        .map(|t| BackupTrack {
            title: t.title,
            name: t.filename,
            size: u64::from(t.file_size),
            file: t.file_path.trim_start_matches('/').to_owned(),
        })
        .collect())
}
