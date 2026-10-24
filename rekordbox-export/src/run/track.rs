//! Writing one planned track: the audio copy, its ANLZ files, its cover and its library row.

use super::Job;
use super::plan::Plan;
use crate::pdb::db::Item;
use crate::{anlz, onelibrary::image_variant, put};
use openfader_plugin::library::{Analysis, DETAIL_ENTRIES_PER_SEC, Track, host};
use rekordbox_common::at;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Covers placed so far (source file → device path without a leading `/`), so tracks of one album share one `a<n>.jpg`.
pub(super) struct Covers {
    placed: HashMap<PathBuf, String>,
    /// Number of the next new `a<n>.jpg`.
    next: usize,
}

impl Covers {
    pub(super) fn new(next: usize) -> Self {
        Self {
            placed: HashMap::new(),
            next,
        }
    }

    /// The device path of `src`: the one it got already, else the path the device track `old` has, else the next free one. The
    /// image is written on first sight, as is for both sizes (there is no image resizer in the build); `b` is OneLibrary's copy.
    fn place(
        &mut self,
        dest: &Path,
        created: &mut Vec<PathBuf>,
        src: &Path,
        old: Option<&str>,
    ) -> Result<String, String> {
        let rel = match (self.placed.get(src), old) {
            (Some(r), _) => r.clone(),
            (None, Some(r)) => r.trim_start_matches('/').to_owned(),
            (None, None) => {
                let n = self.next;
                self.next += 1;
                format!("PIONEER/Artwork/{:05}/a{n}.jpg", (n - 1) / 100 + 1)
            }
        };
        if self.placed.insert(src.to_owned(), rel.clone()).is_none() {
            let bytes = at(std::fs::read(src), src)?;
            for rel in [rel.clone(), image_variant(&rel)] {
                let path = dest.join(&rel);
                if !path.exists() {
                    created.push(path.clone());
                }
                put(&path, &bytes)?;
                put(&dest.join(rel.replace(".jpg", "_m.jpg")), &bytes)?;
            }
        }
        Ok(rel)
    }
}

/// 1 mp3, 4 m4a, 5 flac, 0x0b wav, 0x0c aiff (observed: 1 and 4; the rest rekordcrate's `FileType`); 0 unknown.
fn file_type(file: &Path) -> u16 {
    match file
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .as_deref()
    {
        Some("mp3") => 1,
        Some("m4a" | "mp4" | "aac") => 4,
        Some("flac") => 5,
        Some("wav") => 0x0b,
        Some("aif" | "aiff") => 0x0c,
        _ => 0,
    }
}

/// The analysis the ANLZ files are written from: what the snapshot carries (cues, grid, key, phrases; waveforms stay behind),
/// else the track's own library analysis, and for a missing grid, waveform or key the host's analysis of the audio.
fn complete(track: &Track, given: Option<&Analysis>, warnings: &mut Vec<String>) -> Analysis {
    let own = || host::analysis(track, false);
    let mut a = given.cloned().or_else(own).unwrap_or_default();
    let has_wave = |a: &Analysis| {
        !(a.three_band_detail.is_empty() && a.detail.is_empty() && a.color_detail.is_empty())
    };
    if given.is_some() && !has_wave(&a) {
        if let Some(w) = own() {
            (a.preview, a.detail, a.color_preview) = (w.preview, w.detail, w.color_preview);
            (a.color_detail, a.three_band_preview, a.three_band_detail) =
                (w.color_detail, w.three_band_preview, w.three_band_detail);
        }
    }
    let has_wave = has_wave(&a);
    if let Some(file) = track
        .file
        .as_deref()
        .filter(|_| a.beats.is_empty() || !has_wave || a.key.is_none() && track.key.is_empty())
    {
        match host::analysis(track, true) {
            Some(fresh) => {
                if a.beats.is_empty() {
                    a.beats = fresh.beats;
                }
                if !has_wave {
                    (a.three_band_detail, a.three_band_preview) =
                        (fresh.three_band_detail, fresh.three_band_preview);
                }
                a.key = a.key.or(fresh.key);
            }
            None => warnings.push(format!("{}: analysis failed", file.display())),
        }
    }
    a
}

impl Job<'_> {
    /// Copies the audio of a new track, writes its ANLZ files and cover, and returns its library row (device id `id`). What it
    /// creates is recorded in `created`; an existing track keeps its files' places.
    pub(super) fn write_track(
        &mut self,
        plan: &Plan,
        id: u32,
        today: &str,
        covers: &mut Covers,
    ) -> Result<Item, String> {
        let (snap, dest) = (self.snap, self.dest);
        let (t, given) = &snap.tracks[plan.i];
        let file = t.file.as_deref().ok_or("track without a file")?;
        let a = complete(t, given.as_ref(), &mut self.report.warnings);
        let target = dest.join(&plan.rel);
        let old = plan.at.map(|at| &self.db.tracks[at]);
        if old.is_none() {
            if let Some(dir) = target.parent() {
                at(std::fs::create_dir_all(dir), dir)?;
            }
            self.created.push(target.clone());
            at(std::fs::copy(file, &target), &target)?;
        }
        let anlz_dir = match old
            .map(|t| t.analyze_path.trim_start_matches('/'))
            .and_then(|p| p.rsplit_once('/'))
        {
            Some((dir, _)) => dir.to_owned(),
            None => format!("PIONEER/USBANLZ/P{:03X}/{id:08X}", id % 0x1000),
        };
        if old.is_none() {
            self.created.push(dest.join(&anlz_dir));
        }
        let files = anlz::encode(&a, &format!("/{}", plan.rel));
        for (ext, bytes) in [
            ("DAT", &files.dat),
            ("EXT", &files.ext),
            ("2EX", &files.ex2),
        ] {
            put(&dest.join(format!("{anlz_dir}/ANLZ0000.{ext}")), bytes)?;
        }
        let jpeg = t.artwork.as_deref().filter(|p| {
            p.extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("jpg") || e.eq_ignore_ascii_case("jpeg"))
        });
        let artwork = match jpeg {
            Some(p) => {
                let old_rel = old.and_then(|t| t.artwork.as_deref());
                Some(format!(
                    "/{}",
                    covers.place(dest, &mut self.created, p, old_rel)?
                ))
            }
            None => old.and_then(|t| t.artwork.clone()),
        };
        Ok(Item {
            id,
            title: t.title.clone(),
            artist: t.artist.clone(),
            album: t.album.clone(),
            genre: t.genre.clone(),
            key: if t.key.is_empty() {
                a.key.clone().unwrap_or_default()
            } else {
                t.key.clone()
            },
            artwork,
            // A track the device has keeps its colour when the snapshot gives none (a stick's own track written back).
            color: self
                .colors
                .get(&plan.i)
                .copied()
                .or(old.map(|o| o.color))
                .unwrap_or(0),
            rating: t.rating,
            bpm_x100: if t.bpm_x100 > 0 {
                t.bpm_x100 as u32
            } else {
                a.beats.first().map_or(0, |b| u32::from(b.bpm_x100))
            },
            duration_s: (if t.length_s > 0 {
                t.length_s
            } else {
                a.three_band_detail.len() as i64 / i64::from(DETAIL_ENTRIES_PER_SEC)
            })
            .clamp(0, i64::from(u16::MAX)) as u16,
            sample_rate: if t.sample_rate > 0 {
                t.sample_rate
            } else {
                host::sample_rate(file)
            },
            // The file's average when the track has none.
            bitrate: if t.bitrate > 0 || t.length_s <= 0 {
                t.bitrate
            } else {
                (plan.size * 8 / t.length_s as u64 / 1000) as u32
            },
            file_size: u32::try_from(plan.size).unwrap_or(u32::MAX),
            file_type: file_type(file),
            comment: t.comment.clone(),
            date_added: if !t.date_added.is_empty() {
                t.date_added.clone()
            } else {
                old.map_or_else(|| today.to_owned(), |x| x.date_added.clone())
            },
            analyze_date: today.to_owned(),
            analyze_path: format!("/{anlz_dir}/ANLZ0000.DAT"),
            filename: target
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            file_path: format!("/{}", plan.rel),
        })
    }
}
