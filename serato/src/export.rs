//! The export: audio placement and tags, then `database V2`, then the crates.

use crate::tag::{self, Edit, Tags};
use crate::{crate_file, database};
use openfader_plugin::library::{
    audio_slot, host, place_audio, ExportOptions, ExportReport, Snapshot,
};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::time::{Duration, Instant, UNIX_EPOCH};

/// Where a track's private copy lives (below the drive root), see the crate doc.
const PRIVATE: &str = "_Serato_/Audio";

fn private_rel(rel: &str) -> String {
    format!("{PRIVATE}/{}", rel.strip_prefix("Contents/").unwrap_or(rel))
}

/// How a track's tags reach Serato.
enum Route {
    /// Edits of the shared copy, which keeps its size.
    Shared(Vec<Edit>),
    /// Into the track's private copy (made first when `copy`).
    Private { copy: bool },
    /// The shared copy stays as it is: why.
    Plain(String),
}

fn route(root: &Path, rel: &str, src: &Path, tags: &Tags) -> Route {
    if rel.starts_with("_Serato_/") || root.join(private_rel(rel)).is_file() {
        return Route::Private { copy: false };
    }
    match tag::plan(src, tags, true) {
        Ok(edits) => Route::Shared(edits),
        Err(tag::Error::NoRoom) => Route::Private { copy: true },
        Err(tag::Error::Unsupported) => Route::Plain("its file type holds no Serato tags".into()),
        Err(tag::Error::Io(e)) => Route::Plain(e),
    }
}

fn tag_err(path: &Path, e: tag::Error) -> String {
    format!("{}: cannot write tags ({e:?})", path.display())
}

/// Unix seconds of the file's last change.
fn mtime(p: &Path) -> u32 {
    fs::metadata(p)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs() as u32)
}

/// Writes `bytes` to `rel` below `root` through a temporary file, so a failure leaves the old file.
fn write_atomic(root: &Path, rel: &str, bytes: &[u8]) -> Result<(), String> {
    let path = root.join(rel);
    let tmp = path.with_extension("part");
    let err = |e: std::io::Error| format!("{}: {e}", path.display());
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(err)?;
    }
    fs::write(&tmp, bytes)
        .and_then(|_| fs::rename(&tmp, &path))
        .map_err(|e| {
            let _ = fs::remove_file(&tmp);
            err(e)
        })
}

/// A copy of `from` at `to`, through a temporary file.
fn copy_private(from: &Path, to: &Path) -> Result<(), String> {
    let tmp = to.with_extension("part");
    let err = |e: std::io::Error| format!("{}: {e}", to.display());
    if let Some(dir) = to.parent() {
        fs::create_dir_all(dir).map_err(err)?;
    }
    fs::copy(from, &tmp)
        .and_then(|_| fs::rename(&tmp, to))
        .map_err(|e| {
            let _ = fs::remove_file(&tmp);
            err(e)
        })
}

pub fn run(root: &Path, snap: &Snapshot, opts: &ExportOptions) -> Result<ExportReport, String> {
    let mut rep = ExportReport::default();
    let colors: HashMap<usize, u8> = snap.colors.iter().copied().collect();
    let tags: Vec<(Tags, usize)> = snap
        .tracks
        .iter()
        .enumerate()
        .map(|(i, (_, a))| Tags::new(a.as_ref(), colors.get(&i).copied()))
        .collect();

    // Where each track's audio goes, and how much room that takes.
    let mut slots = Vec::new();
    let mut need = 0;
    for (i, (t, _)) in snap.tracks.iter().enumerate() {
        let slot = audio_slot(root, t).and_then(|(rel, there)| {
            let src = t.file.clone().unwrap_or_default();
            let size = fs::metadata(&src).map_or(0, |m| m.len());
            let src = if there { root.join(&rel) } else { src };
            Ok((rel, there, src, size))
        });
        match slot {
            Ok((rel, there, src, size)) => {
                need += if there { 0 } else { size };
                if matches!(
                    route(root, &rel, &src, &tags[i].0),
                    Route::Private { copy: true }
                ) {
                    need += size;
                }
                slots.push(Some(rel));
            }
            Err(e) => {
                rep.warnings.push(e);
                slots.push(None);
            }
        }
    }
    if let Some(free) = opts.free.filter(|&f| need > f) {
        return Err(format!(
            "not enough space on the device: {:.1} MB to copy, {:.1} MB free",
            need as f64 / 1e6,
            free as f64 / 1e6
        ));
    }

    let (mut recs, mut target, mut dropped_paths) =
        (Vec::new(), vec![None; snap.tracks.len()], Vec::new());
    let (mut privates, mut memory_cues) = (0, 0);
    let (mut last, total) = (None::<Instant>, snap.tracks.len());
    for (i, (t, _)) in snap.tracks.iter().enumerate() {
        let step = Duration::from_millis(50);
        if last.is_none_or(|l| l.elapsed() >= step) {
            last = Some(Instant::now());
            if host::progress("Writing Serato library", i, total) {
                return Err("cancelled".into());
            }
        }
        if slots[i].is_none() {
            continue;
        }
        let (rel, _) = place_audio(root, t)?;
        let (tag_set, memory) = &tags[i];
        memory_cues += memory;
        let path = match route(root, &rel, &root.join(&rel), tag_set) {
            Route::Shared(mut edits) => {
                tag::apply(&root.join(&rel), &mut edits)
                    .map_err(|e| tag_err(&root.join(&rel), e))?;
                rel
            }
            Route::Plain(why) => {
                rep.warnings
                    .push(format!("{}: cues and grid not written, {why}", t.title));
                rel
            }
            Route::Private { copy } => {
                let p = if rel.starts_with("_Serato_/") {
                    rel.clone()
                } else {
                    private_rel(&rel)
                };
                let dest = root.join(&p);
                if copy {
                    copy_private(&root.join(&rel), &dest)?;
                    privates += 1;
                }
                let mut edits = tag::plan(&dest, tag_set, false).map_err(|e| tag_err(&dest, e))?;
                tag::apply(&dest, &mut edits).map_err(|e| tag_err(&dest, e))?;
                if p != rel {
                    dropped_paths.push(rel);
                }
                p
            }
        };
        let file = root.join(&path);
        let size = fs::metadata(&file).map_or(0, |m| m.len());
        recs.push((path.clone(), database::track(&path, t, size, mtime(&file))));
        target[i] = Some(path);
    }
    // One record per path: the last track of a path wins.
    let mut seen = HashSet::new();
    recs.reverse();
    recs.retain(|(p, _)| seen.insert(p.clone()));
    recs.reverse();

    let old = match fs::read(root.join(database::FILE)) {
        Ok(b) => Some(b),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(format!("{}: {e}", database::FILE)),
    };
    let (db, replaced) = database::merge(
        old.as_deref().filter(|b| !b.is_empty()),
        &recs,
        &dropped_paths,
    )
    .map_err(|e| format!("{}: {e}", database::FILE))?;
    (rep.added, rep.updated) = (recs.len() - replaced, replaced);
    rep.tracks = recs.len();

    for (i, name) in crate_file::names(&snap.playlists).iter().enumerate() {
        let rel = format!("{}/{name}.crate", crate_file::DIR);
        if snap.playlists[i].is_folder {
            if !root.join(&rel).is_file() {
                write_atomic(root, &rel, &crate_file::build(&[]))?;
            }
            continue;
        }
        let members = snap.members.get(i).map_or(&[][..], |m| m);
        let paths: Vec<&str> = members
            .iter()
            .filter_map(|&m| target.get(m)?.as_deref())
            .collect();
        write_atomic(root, &rel, &crate_file::build(&paths))?;
    }
    rep.playlists = snap.playlists.len();
    write_atomic(root, database::FILE, &db)?;

    if privates > 0 {
        rep.notes.push(format!(
            "{privates} tracks got a private copy under {PRIVATE}: their cues, loops and grid did not fit the shared copy without changing its size"
        ));
    }
    if memory_cues > 0 {
        rep.notes.push(format!(
            "{memory_cues} memory cues were not written: Serato has no memory cues"
        ));
    }
    Ok(rep)
}
