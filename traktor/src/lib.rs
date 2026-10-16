//! A Traktor collection on a USB stick, written next to the other libraries the stick holds: one `Traktor/collection.nml`
//! (Traktor's NML, `VERSION` 19 as written by Traktor Pro 3 and read by Pro 4) whose `LOCATION`s point at the one copy of
//! each audio file the stick holds (`Contents/<artist>/<album>/<file>`, placed by the SDK's `place_audio`, shared with every
//! other format written there).
//!
//! * Where: `Traktor/collection.nml` at the stick's root. Traktor has no fixed place for a collection on a drive (its own
//!   sits in the Traktor root folder on the computer and is imported with right-click Collection > Import another
//!   Collection, playlists with Import Playlist); the file name is Traktor's own, and the folder keeps it out of `Contents/`,
//!   which the audio and the other formats own.
//! * What: `ENTRY` per track with tags, tempo, key, grid marker, hot cues, memory cues and loops ([`entry`]), and the
//!   playlist tree ([`playlists`]). The layout is the one of Mixxx (`src/library/traktor/traktorfeature.cpp`), the
//!   traktor-nml-utils fixture (`tests/fixtures/collection.nml`) and polamjag/traktor-nml (`spec/traktor/traktor-nml-demo.nml`);
//!   no official schema exists. The cue records are the ones of Traktor's MP3 tag (hellricer.github.io "Decoding the
//!   TRAKTOR4 field").
//! * Merge: the file is read first (tolerantly, [`xml`]) and everything in it stays: entries of other files with their cues,
//!   other playlists, folders, unknown elements. A track of the snapshot replaces the entry with the same folder and file
//!   name (the volume is not compared, so a stick written by Traktor under another drive letter is not duplicated); with no
//!   analysis in the snapshot the replaced entry keeps its cues, tempo, key and analysis data. A playlist replaces the one
//!   of the same name in the same folder. Nothing is deleted. The file is written through a temporary file and renamed;
//!   a file that is not an NML is not touched and the export fails.
//! * Volume: Traktor locates a file by `VOLUME` plus path. `VOLUME` is the stick's drive letter on Windows (`E:`) and the
//!   name of the mount folder elsewhere, which is the volume label on macOS (`/Volumes/<label>`) and on the usual Linux
//!   automounts (`/run/media/<user>/<label>`); `VOLUMEID` repeats it (all that real macOS collections hold). Observed
//!   on a real Windows collection: `VOLUME="G:"` with `VOLUMEID` the drive's serial number, which a plugin cannot read.
//!   Limit: a stick written on one system and opened on another has its entries under the writer's volume name; Traktor then
//!   shows the tracks missing until the collection is exported again from the system it is used on (or relocated in Traktor).
//! * Limits: one grid marker per track (the first downbeat; the tempo is the track's average), Traktor 4's per-marker tempo
//!   is not written; no cue colours (Traktor has none); hot cues beyond the 8 pads are left out with a warning.

mod entry;
mod playlists;
mod read;
#[cfg(test)]
mod tests;
mod xml;

use chrono::Local;
use openfader_plugin::library::{
    audio_slot, host, place_audio, DeviceInfo, ExportOptions, ExportReport, Feature, LibraryPlugin,
    Probe, Snapshot, Source,
};
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Component, Path};
use std::time::UNIX_EPOCH;
use xml::El;

/// Name [`DeviceInfo::libraries`] lists.
const LIBRARY: &str = "Traktor collection";
/// The collection file, relative to the device root.
pub const FILE: &str = "Traktor/collection.nml";
/// `NML VERSION` written (a file that already has a higher one keeps it).
const VERSION: u32 = 19;

pub struct Plugin;

impl LibraryPlugin for Plugin {
    fn probe(path: &Path) -> Option<Probe> {
        read::probe(path)
    }

    fn open(path: &Path, _settings: &Value) -> Result<Box<dyn Source>, String> {
        read::open(path)
    }

    fn analysis_ref(
        reference: &str,
        _settings: &Value,
    ) -> Option<openfader_plugin::library::Analysis> {
        read::analysis_ref(reference)
    }

    fn device_probe(root: &Path, _settings: &Value) -> Result<DeviceInfo, String> {
        let modified = std::fs::metadata(root.join(FILE)).ok().map(|m| {
            m.modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_secs() as i64)
        });
        Ok(DeviceInfo {
            libraries: if modified.is_some() {
                vec![LIBRARY.into()]
            } else {
                Vec::new()
            },
            title: "Traktor".into(),
            about: "Traktor Pro 3 and 4 on a computer: Collection > Import another Collection"
                .into(),
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

    /// Evidence (module doc, Limits): eight hot cue pads, one grid marker per track (no tempo changes).
    fn capabilities(_settings: &Value) -> openfader_plugin::library::Capabilities {
        openfader_plugin::library::Capabilities {
            hot_cues: Some(8),
            tempo_changes: false,
            ..Default::default()
        }
    }
}

openfader_plugin::export_library!(Plugin);

/// `VOLUME` of the device mounted at `root`: its drive letter (`E:`) when the path starts with one, else the name of the
/// mount folder.
pub fn volume(root: &Path) -> String {
    let s = root.to_string_lossy();
    match s.as_bytes() {
        [d, b':', ..] if d.is_ascii_alphabetic() => {
            format!("{}:", (*d as char).to_ascii_uppercase())
        }
        _ => root
            .components()
            .rfind(|c| matches!(c, Component::Normal(_)))
            .map_or_else(
                || "USB".into(),
                |c| c.as_os_str().to_string_lossy().into_owned(),
            ),
    }
}

/// The collection file of the device, or an empty one when it holds none; `Err` for a file that is no NML (it is never
/// overwritten).
fn load(path: &Path) -> Result<El, String> {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(fresh()),
        Err(e) => return Err(format!("{}: {e}", path.display())),
    };
    let text = String::from_utf8_lossy(&bytes);
    let nml = xml::parse(text.trim_start_matches('\u{feff}'))
        .map_err(|e| format!("{}: {e}; the file was left as it is", path.display()))?;
    if nml.name != "NML" {
        return Err(format!(
            "{}: not a Traktor collection (<{}>); the file was left as it is",
            path.display(),
            nml.name
        ));
    }
    Ok(nml)
}

fn fresh() -> El {
    El::new("NML")
        .attr("VERSION", VERSION)
        .with(
            El::new("HEAD")
                .attr("COMPANY", "www.native-instruments.com")
                .attr("PROGRAM", "Traktor"),
        )
        .with(El::new("MUSICFOLDERS"))
        .with(El::new("COLLECTION").attr("ENTRIES", 0))
        .with(El::new("SETS").attr("ENTRIES", 0))
        .with(El::new("PLAYLISTS"))
        .with(El::new("INDEXING"))
}

/// What a replaced entry keeps when the snapshot has no analysis for its track: Traktor's analysis data, the grid lock, the
/// cues, and tempo and key where the new entry has none.
fn keep_analysis(old: &El, new: &mut El) {
    for k in ["AUDIO_ID", "LOCK", "LOCK_MODIFICATION_TIME"] {
        if let Some(v) = old.get(k) {
            new.set(k, v);
        }
    }
    for kid in &old.kids {
        let kept = match kid.name.as_str() {
            "LOUDNESS" | "CUE_V2" => true,
            "TEMPO" | "MUSICAL_KEY" => new.child(&kid.name).is_none(),
            _ => false,
        };
        if kept {
            new.kids.push(kid.clone());
        }
    }
}

/// `(DIR, FILE)` of an entry.
fn location(e: &El) -> Option<(String, String)> {
    let l = e.child("LOCATION")?;
    Some((l.get("DIR")?.into(), l.get("FILE")?.into()))
}

/// Writes `data` to `path` through a temporary file in the same folder, so a failed write leaves `path` as it was.
fn write_atomic(path: &Path, data: &str) -> Result<(), String> {
    let err = |e: std::io::Error| format!("{}: {e}", path.display());
    let tmp = path.with_extension("nml.part");
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(err)?;
    }
    std::fs::write(&tmp, data)
        .and_then(|_| std::fs::rename(&tmp, path))
        .map_err(|e| {
            let _ = std::fs::remove_file(&tmp);
            err(e)
        })
}

fn mib(bytes: u64) -> u64 {
    bytes.div_ceil(1 << 20)
}

fn export(root: &Path, snap: &Snapshot, options: &ExportOptions) -> Result<ExportReport, String> {
    let path = root.join(FILE);
    let mut nml = load(&path)?;
    let mut rep = ExportReport::default();

    // Which tracks can go on the stick, where, and how much the new copies need.
    let mut plan = Vec::new();
    let mut need = 0u64;
    for (i, (t, _)) in snap.tracks.iter().enumerate() {
        let size = t.file.as_deref().map(std::fs::metadata);
        match (audio_slot(root, t), size) {
            (Ok((rel, there)), Some(Ok(m))) => {
                if !there {
                    need += m.len();
                }
                plan.push((i, rel, m.len()));
            }
            (Err(e), _) => rep.warnings.push(e),
            (_, Some(Err(e))) => rep.warnings.push(format!("{}: {e}", t.title)),
            (_, None) => rep.warnings.push(format!("{}: no audio file", t.title)),
        }
    }
    if let Some(free) = options.free.filter(|f| need > *f) {
        return Err(format!(
            "not enough space on the device: the new audio needs {} MB, {} MB are free",
            mib(need),
            mib(free)
        ));
    }

    let vol = volume(root);
    let now = Local::now().naive_local();
    let colors: HashMap<usize, u8> = snap.colors.iter().copied().collect();
    let mut keys: Vec<Option<String>> = vec![None; snap.tracks.len()];
    let coll = nml.child_mut("COLLECTION");
    let mut at: HashMap<(String, String), usize> = coll
        .kids
        .iter()
        .enumerate()
        .filter_map(|(i, e)| Some((location(e)?, i)))
        .collect();

    for (n, (i, _, size)) in plan.iter().enumerate() {
        if host::progress("Copying tracks", n, plan.len()) {
            return Err("cancelled".into());
        }
        let (t, a) = &snap.tracks[*i];
        let rel = match place_audio(root, t) {
            Ok((rel, _)) => rel,
            Err(e) => {
                rep.warnings.push(e);
                continue;
            }
        };
        let mut e = entry::entry(
            &vol,
            &rel,
            *size,
            t,
            a.as_ref(),
            colors.get(i).copied(),
            now,
            &mut rep.warnings,
        );
        let loc = entry::dir_file(&rel);
        keys[*i] = Some(format!("{vol}{}{}", loc.0, loc.1));
        match at.get(&loc) {
            Some(&k) => {
                if a.is_none() {
                    keep_analysis(&coll.kids[k], &mut e);
                }
                coll.kids[k] = e;
                rep.updated += 1;
            }
            None => {
                at.insert(loc, coll.kids.len());
                coll.kids.push(e);
                rep.added += 1;
            }
        }
    }
    if host::progress("Writing the Traktor collection", plan.len(), plan.len()) {
        return Err("cancelled".into());
    }
    coll.set(
        "ENTRIES",
        coll.kids.iter().filter(|e| e.name == "ENTRY").count(),
    );
    rep.playlists = playlists::merge(&mut nml, snap, &keys);
    if nml
        .get("VERSION")
        .and_then(|v| v.parse::<u32>().ok())
        .is_none_or(|v| v < VERSION)
    {
        nml.set("VERSION", VERSION);
    }
    write_atomic(&path, &nml.document())?;

    rep.tracks = rep.added + rep.updated;
    rep.notes.push(format!(
        "Traktor: right-click Collection > Import another Collection and pick {FILE} on the device (volume \"{vol}\")"
    ));
    Ok(rep)
}
