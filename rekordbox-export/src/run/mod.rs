//! The engine behind [`super::export`]: plan which snapshot track is which device track, check the space, copy / analyse / write
//! the tracks, merge the playlists, then write the library files.
//!
//! Identity of a track (inferred; rekordbox's own rule is not documented): the device path `Contents/<artist>/<album>/<file>` it
//! would get plus the audio's size. A device track at that path (or at its ` (2)`, ` (3)` collision variants) with the same size
//! is the same track: its row, id and analysis folder are reused and its audio is not copied again. Anything else is new.
//!
//! Children: `plan` (device paths and the limits check) and `track` (writing one track).

mod plan;
mod track;

use super::pdb::{
    self,
    db::{Db, Tags},
};
use super::progress::Progress;
use super::settings::DeviceConfig;
use super::{ONE, PDB, commit, merge, profile, put, today};
use openfader_plugin::library::{ExportOptions, ExportReport, Snapshot};
use plan::Plan;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use track::Covers;

/// The library to extend; a new one when `dest` has none.
fn open(dest: &Path, config: &DeviceConfig) -> Result<(Db, Tags), String> {
    let (pdb_path, one) = (dest.join(PDB), dest.join(ONE));
    if pdb_path.exists() {
        return pdb::read::open(dest, config);
    }
    if one.exists() {
        return Err(format!(
            "{} has exportLibrary.db but no export.pdb: only a Device Library can be extended",
            dest.display()
        ));
    }
    Ok((
        Db {
            analyze_date: today(),
            config: config.clone(),
            ..Db::default()
        },
        Tags::default(),
    ))
}

/// Highest `<n>` of the `a<n>.jpg` covers the library names, 0 when none.
fn last_cover(db: &Db) -> usize {
    db.tracks
        .iter()
        .filter_map(|t| {
            t.artwork
                .as_deref()?
                .rsplit_once("/a")?
                .1
                .strip_suffix(".jpg")?
                .parse()
                .ok()
        })
        .max()
        .unwrap_or(0)
}

/// A running job: the library being extended and what the run has created so far.
struct Job<'a> {
    snap: &'a Snapshot,
    dest: &'a Path,
    o: &'a ExportOptions,
    config: &'a DeviceConfig,
    key: &'a str,
    progress: &'a mut Progress,
    /// The snapshot's track colours by track index.
    colors: HashMap<usize, u8>,
    db: Db,
    tags: Tags,
    report: ExportReport,
    /// Files and folders this run made, oldest first; removed again when the run fails or is cancelled.
    created: Vec<PathBuf>,
}

/// Writes `snap` to `dest`; the report and the device ids of the tracks written.
pub(super) fn run(
    dest: &Path,
    snap: &Snapshot,
    o: &ExportOptions,
    config: &DeviceConfig,
    key: &str,
    progress: &mut Progress,
) -> Result<(ExportReport, Vec<u32>), String> {
    let (db, tags) = open(dest, config)?;
    let mut job = Job {
        snap,
        dest,
        o,
        config,
        key,
        progress,
        colors: snap.colors.iter().copied().collect(),
        db,
        tags,
        report: ExportReport::default(),
        created: Vec::new(),
    };
    let plans = plan::make(snap, &job.db, o, &mut job.report)?;
    let result = job.execute(&plans);
    if result.is_err() {
        for p in job.created.iter().rev() {
            let _ = if p.is_dir() {
                std::fs::remove_dir_all(p)
            } else {
                std::fs::remove_file(p)
            };
        }
    }
    result.map(|ids| (job.report, ids))
}

impl Job<'_> {
    /// Writes every planned track and the library files; the device ids of the tracks written.
    fn execute(&mut self, plans: &[Plan]) -> Result<Vec<u32>, String> {
        let snap = self.snap;
        let mut ids: Vec<Option<u32>> = vec![None; snap.tracks.len()];
        let mut next_id = self.db.tracks.iter().map(|t| t.id).max().unwrap_or(0) + 1;
        let mut covers = Covers::new(last_cover(&self.db) + 1);
        let today = today();
        self.progress.phase("Copying tracks", plans.len())?;
        for (k, plan) in plans.iter().enumerate() {
            self.progress.tick(k)?;
            let id = plan.at.map_or(next_id, |at| self.db.tracks[at].id);
            let item = self.write_track(plan, id, &today, &mut covers)?;
            match plan.at {
                Some(at) => {
                    self.db.tracks[at] = item;
                    self.report.updated += 1;
                }
                None => {
                    self.db.tracks.push(item);
                    next_id += 1;
                    self.report.added += 1;
                }
            }
            ids[plan.i] = Some(id);
        }
        self.report.tracks = self.report.added + self.report.updated;
        self.add_banks(plans, &ids);
        self.progress.phase("Writing library", 1)?;
        merge::playlists(&mut self.db.nodes, snap, &ids);
        self.report.playlists = snap.playlists.len();
        merge::mytag(&mut self.tags, snap, &ids);
        self.write_device_files()?;
        commit(self.dest, &self.db, &self.tags, self.key)?;
        Ok(ids.iter().flatten().copied().collect())
    }

    /// Hot Cue Bank Lists: each bank names the audio file of its track; banks of tracks that did not go to the device are dropped.
    fn add_banks(&mut self, plans: &[Plan], ids: &[Option<u32>]) {
        let (snap, o) = (self.snap, self.o);
        let by_file: HashMap<&Path, u32> = plans
            .iter()
            .filter_map(|p| Some((snap.tracks[p.i].0.file.as_deref()?, ids[p.i]?)))
            .collect();
        for b in &o.banks {
            let cues: Vec<_> = b
                .cues
                .iter()
                .filter_map(|c| by_file.get(c.file.as_path()).map(|&id| (id, c.cue.clone())))
                .collect();
            if cues.len() < b.cues.len() {
                self.report.warnings.push(format!("Hot Cue Bank List {}: {} bank(s) of tracks that are not in this export were left out", b.name, b.cues.len() - cues.len()));
            }
            self.db.banks.push((b.name.clone(), cues));
        }
    }

    /// The device settings files and, when the device has none, the DJ profile.
    fn write_device_files(&mut self) -> Result<(), String> {
        let pioneer = self.dest.join("PIONEER");
        for (name, bytes) in self.config.files() {
            let path = pioneer.join(name);
            if !path.exists() {
                self.created.push(path.clone());
            }
            put(&path, &bytes)?;
        }
        if profile::write(&pioneer, &profile::user())? {
            self.created.push(pioneer.join(profile::FILE));
        }
        Ok(())
    }
}
