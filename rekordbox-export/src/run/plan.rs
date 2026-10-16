//! Planning: the device path of each snapshot track, and whether the playlist count and the free space allow the job.

use crate::clean;
use crate::merge::{self, MAX_LISTS};
use crate::pdb::db::Db;
use openfader_plugin::library::{ExportOptions, ExportReport, Snapshot};
use rekordbox_common::at;
use std::collections::{HashMap, HashSet};

/// Bytes assumed for the three ANLZ files of one track when checking the free space (inferred: ~400 kB measured on 5-minute
/// tracks, rounded up).
const ANLZ_ESTIMATE: u64 = 512 * 1024;
/// Slack for the library files themselves.
const MARGIN: u64 = 2 * 1024 * 1024;

/// One snapshot track that will be written.
pub(super) struct Plan {
    /// Index into [`Snapshot::tracks`].
    pub i: usize,
    /// Device path without a leading `/`.
    pub rel: String,
    /// Index into [`Db::tracks`] of the device track it is, `None` for a new one.
    pub at: Option<usize>,
    pub size: u64,
}

fn mib(b: u64) -> String {
    format!("{:.1} MiB", b as f64 / (1024.0 * 1024.0))
}

/// Hands out device paths: the first of `name`, `stem (2).ext`, … in a directory that is free, or that holds the same track
/// already (an unclaimed device track of the same size); failing that, an unclaimed device track of the same file name and
/// size anywhere (a library written by another program files its audio under other folder names).
struct Resolver<'a> {
    db: &'a Db,
    /// Lowercased device path → index into [`Db::tracks`].
    by_path: HashMap<String, usize>,
    /// (Lowercased file name, size) → indexes into [`Db::tracks`].
    by_name: HashMap<(String, u64), Vec<usize>>,
    /// Lowercased paths given to new tracks.
    used: HashSet<String>,
    /// Device tracks already matched to a snapshot track.
    claimed: HashSet<usize>,
}

impl<'a> Resolver<'a> {
    fn new(db: &'a Db) -> Self {
        let mut by_name: HashMap<(String, u64), Vec<usize>> = HashMap::new();
        for (i, t) in db.tracks.iter().enumerate() {
            let name = t.file_path.rsplit('/').next().unwrap_or_default();
            by_name
                .entry((name.to_lowercase(), u64::from(t.file_size)))
                .or_default()
                .push(i);
        }
        let by_path = db
            .tracks
            .iter()
            .enumerate()
            .map(|(i, t)| (t.file_path.trim_start_matches('/').to_lowercase(), i))
            .collect();
        Self {
            db,
            by_path,
            by_name,
            used: HashSet::new(),
            claimed: HashSet::new(),
        }
    }

    /// The path of a track of `size` bytes whose source file is `original` (cleaned: `name`) in directory `dir`, and the device
    /// track it is (`None` = new).
    fn resolve(
        &mut self,
        dir: &str,
        name: &str,
        original: &str,
        size: u64,
    ) -> (String, Option<usize>) {
        let size = size.min(u64::from(u32::MAX));
        let (stem, ext) = name
            .rsplit_once('.')
            .map_or((name, String::new()), |(s, e)| (s, format!(".{e}")));
        for n in 1.. {
            let p = if n == 1 {
                format!("{dir}/{name}")
            } else {
                format!("{dir}/{stem} ({n}){ext}")
            };
            let key = p.to_lowercase();
            match self.by_path.get(&key) {
                Some(&known)
                    if !self.claimed.contains(&known)
                        && u64::from(self.db.tracks[known].file_size) == size =>
                {
                    self.claimed.insert(known);
                    return (p, Some(known));
                }
                Some(_) => {}
                None if !self.used.contains(&key) => break,
                None => {}
            }
        }
        let elsewhere = [original, name].iter().find_map(|n| {
            self.by_name
                .get(&(n.to_lowercase(), size))?
                .iter()
                .copied()
                .find(|i| !self.claimed.contains(i))
        });
        if let Some(known) = elsewhere {
            self.claimed.insert(known);
            let p = self.db.tracks[known].file_path.trim_start_matches('/');
            return (p.to_owned(), Some(known));
        }
        for n in 1.. {
            let p = if n == 1 {
                format!("{dir}/{name}")
            } else {
                format!("{dir}/{stem} ({n}){ext}")
            };
            let key = p.to_lowercase();
            if !self.by_path.contains_key(&key) && self.used.insert(key) {
                return (p, None);
            }
        }
        unreachable!()
    }
}

/// A [`Plan`] per snapshot track that has an audio file (a warning in `report` names the others); fails when the lists would
/// exceed [`MAX_LISTS`] or the new audio and analysis files do not fit `o.free`.
pub(super) fn make(
    snap: &Snapshot,
    db: &Db,
    o: &ExportOptions,
    report: &mut ExportReport,
) -> Result<Vec<Plan>, String> {
    let mut paths = Resolver::new(db);
    let mut plans = Vec::new();
    for (i, (t, _)) in snap.tracks.iter().enumerate() {
        let Some(file) = t.file.as_deref().filter(|f| f.is_file()) else {
            report
                .warnings
                .push(format!("{}: audio file missing, track left out", t.title));
            continue;
        };
        let size = at(std::fs::metadata(file), file)?.len();
        let original = file
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let name = clean(&original, "track");
        let dir = format!(
            "Contents/{}/{}",
            clean(&t.artist, "UnknownArtist"),
            clean(&t.album, "UnknownAlbum")
        );
        let (rel, known) = paths.resolve(&dir, &name, &original, size);
        plans.push(Plan {
            i,
            rel,
            at: known,
            size,
        });
    }
    let new_lists = merge::playlists(&mut db.nodes.clone(), snap, &vec![None; snap.tracks.len()]);
    if db.nodes.len() + new_lists > MAX_LISTS {
        return Err(format!(
            "The number of lists and folders cannot exceed {MAX_LISTS} on one device ({} there, {new_lists} more to add)",
            db.nodes.len()
        ));
    }
    let need = plans
        .iter()
        .filter(|p| p.at.is_none())
        .map(|p| p.size)
        .sum::<u64>()
        + plans.len() as u64 * ANLZ_ESTIMATE
        + MARGIN;
    if let Some(free) = o.free.filter(|&f| need > f) {
        return Err(format!(
            "not enough space on the device: about {} needed, {} free",
            mib(need),
            mib(free)
        ));
    }
    Ok(plans)
}
