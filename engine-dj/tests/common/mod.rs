//! Fixtures shared by the export and read tests: temp dirs, synthetic tracks and a snapshot with cues, a tempo change and lists.
#![allow(dead_code)]

use openfader_plugin::library::{
    Analysis, Beat, Cue, ExportOptions, LibraryPlugin, Playlist, Snapshot, Track,
};
use serde_json::Value;
use std::path::PathBuf;

pub const RATE: u32 = 44100;

/// A temp dir that is gone again when the test ends.
pub struct Dirs(PathBuf);

impl Dirs {
    pub fn new(name: &str) -> Self {
        let d = std::env::temp_dir().join(format!("engine-dj-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("src")).unwrap();
        std::fs::create_dir_all(d.join("stick")).unwrap();
        Self(d)
    }
    pub fn src(&self) -> PathBuf {
        self.0.join("src")
    }
    pub fn stick(&self) -> PathBuf {
        self.0.join("stick")
    }
}

impl Drop for Dirs {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// 8-bit mono silence of `secs` seconds: a valid WAV, small enough to copy often.
pub fn wav(secs: u32) -> Vec<u8> {
    let data = vec![128u8; (RATE * secs) as usize];
    let mut b = [
        b"RIFF".as_slice(),
        &(36 + data.len() as u32).to_le_bytes(),
        b"WAVEfmt ",
        &16u32.to_le_bytes(),
    ]
    .concat();
    b.extend([1, 0, 1, 0]);
    b.extend(RATE.to_le_bytes());
    b.extend(RATE.to_le_bytes());
    b.extend([1, 0, 8, 0]);
    b.extend(b"data");
    b.extend((data.len() as u32).to_le_bytes());
    b.extend(data);
    b
}

pub fn track(d: &Dirs, file: &str, title: &str, artist: &str, secs: i64) -> Track {
    let f = d.src().join(file);
    std::fs::write(&f, wav(secs as u32)).unwrap();
    Track {
        title: title.into(),
        artist: artist.into(),
        album: "Album".into(),
        genre: "House".into(),
        label: "Label".into(),
        key: "8A".into(),
        bpm_x100: 12000,
        length_s: secs,
        file: Some(f),
        rating: 4,
        comment: "nice".into(),
        date_added: "2026-01-02".into(),
        bitrate: 1411,
        sample_rate: RATE,
        ..Default::default()
    }
}

pub fn cue(
    hot: u16,
    ms: u32,
    end: Option<u32>,
    color: Option<[u8; 3]>,
    comment: Option<&str>,
) -> Cue {
    Cue {
        hot_cue: hot,
        time_ms: ms,
        loop_end_ms: end,
        color,
        comment: comment.map(Into::into),
    }
}

/// 120 BPM for 16 beats (0 to 8 s), then 150 BPM for 8 beats (to 11.2 s).
pub fn beats() -> Vec<Beat> {
    (0..=24u32)
        .map(|k| {
            let (time_ms, bpm_x100) = if k <= 16 {
                (500 * k, 12000)
            } else {
                (8000 + 400 * (k - 16), 15000)
            };
            Beat {
                bar_pos: (k % 4) as u16 + 1,
                bpm_x100,
                time_ms,
            }
        })
        .collect()
}

pub fn analysis() -> Analysis {
    Analysis {
        beats: beats(),
        cues: vec![
            cue(1, 750, None, Some([0xff, 0x00, 0x17]), Some("Drop é")),
            cue(3, 1500, None, None, None),
            cue(0, 1250, None, None, None),
            cue(0, 3250, Some(5250), None, Some("Loop")),
            cue(2, 6000, Some(7000), None, None),
            cue(0, 9000, None, None, None),
            cue(9, 9500, None, None, None),
        ],
        three_band_preview: (0..1200)
            .map(|i| [(i % 256) as u8, (i * 2 % 256) as u8, 7])
            .collect(),
        key: Some("8A".into()),
        ..Default::default()
    }
}

pub fn list(name: &str, depth: usize, folder: bool) -> Playlist {
    Playlist {
        id: name.into(),
        name: name.into(),
        depth,
        is_folder: folder,
    }
}

/// A (analysed), B (no analysis, key from the track only), C (audio file missing); Sets/Warm = [A, B], Top = [B, A, B].
pub fn setup(d: &Dirs) -> Snapshot {
    let a = track(d, "a.wav", "Alpha é", "Art/ist: X", 12);
    let mut b = track(d, "b.wav", "Beta", "Other", 8);
    b.key = "11B".into();
    b.sample_rate = 0;
    let mut c = track(d, "c.wav", "Gamma", "Other", 1);
    c.file = Some(d.src().join("missing.wav"));
    Snapshot {
        tracks: vec![(a, Some(analysis())), (b, None), (c, None)],
        playlists: vec![
            list("Sets", 0, true),
            list("Warm", 1, false),
            list("Top", 0, false),
        ],
        members: vec![vec![], vec![0, 1], vec![1, 0, 1]],
        ..Default::default()
    }
}

pub fn export(
    d: &Dirs,
    snap: &Snapshot,
) -> Result<openfader_plugin::library::ExportReport, String> {
    engine_dj::Plugin::export(&d.stick(), snap, &ExportOptions::default(), &Value::Null)
}

pub fn export_schema(
    d: &Dirs,
    snap: &Snapshot,
    schema: &str,
) -> Result<openfader_plugin::library::ExportReport, String> {
    let mut options = ExportOptions::default();
    options
        .config
        .insert(engine_dj::SCHEMA_OPTION.into(), schema.into());
    engine_dj::Plugin::export(&d.stick(), snap, &options, &Value::Null)
}
