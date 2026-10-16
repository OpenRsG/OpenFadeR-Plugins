//! The five `PerformanceData` blobs, from plain values to bytes. Layouts: libdjinterop `src/djinterop/engine/v2/*_blob.cpp`
//! (<https://github.com/xsco/libdjinterop>), which `PerformanceData` of schema 3.0.0 shares with the older `Track` columns.
//!
//! `trackData`, `overviewWaveFormData`, `beatData` and `quickCues` are a zlib stream behind a 4-byte big-endian length of the
//! uncompressed bytes; `loops` is stored raw. Numbers are big-endian except in `loops` and in the beat markers (little-endian).

use flate2::{write::ZlibEncoder, Compression};
use std::io::Write;

/// Hot cue and loop slots of a track.
pub const SLOTS: usize = 8;

/// Pad colour as `[a, r, g, b]`, and `[0; 4]` for an empty slot.
pub type Argb = [u8; 4];

/// One beat marker: the beats between two markers are evenly spaced.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Marker {
    pub offset: f64,
    pub number: i64,
}

pub struct QuickCue {
    pub label: String,
    /// Sample offset, -1 when the slot is empty.
    pub offset: f64,
    pub color: Argb,
}

pub struct Loop {
    pub label: String,
    pub start: f64,
    pub end: f64,
    pub color: Argb,
}

/// The overview waveform: points as `[low, mid, high]`, the samples each stands for, and the per-band maximum.
pub struct Overview {
    pub points: Vec<[u8; 3]>,
    pub samples_per_point: f64,
}

fn zlib(raw: &[u8]) -> Vec<u8> {
    let mut z = ZlibEncoder::new(
        (raw.len() as u32).to_be_bytes().to_vec(),
        Compression::default(),
    );
    // Writing to a Vec cannot fail.
    z.write_all(raw)
        .and_then(|()| z.finish())
        .unwrap_or_default()
}

/// `trackData`: sample rate, sample count, key id and the three average loudnesses (0: the player does not rescale).
pub fn track_data(rate: f64, samples: i64, key: i32) -> Vec<u8> {
    let mut b = rate.to_be_bytes().to_vec();
    b.extend(samples.to_be_bytes());
    b.extend(key.to_be_bytes());
    b.extend([0u8; 24]);
    zlib(&b)
}

/// `overviewWaveFormData`.
pub fn overview(o: &Overview) -> Vec<u8> {
    let n = o.points.len() as i64;
    let mut b = [
        n.to_be_bytes(),
        n.to_be_bytes(),
        o.samples_per_point.to_be_bytes(),
    ]
    .concat();
    b.extend(o.points.iter().flatten());
    b.extend((0..3).map(|c| o.points.iter().map(|p| p[c]).max().unwrap_or(0)));
    zlib(&b)
}

/// `beatData`: the default and the adjusted grid are the same list. libdjinterop notes 9 zero bytes that real data ends with.
pub fn beat_data(rate: f64, samples: f64, grid: &[Marker]) -> Vec<u8> {
    let mut b = [rate.to_be_bytes(), samples.to_be_bytes()].concat();
    b.push(u8::from(!grid.is_empty()));
    for _ in 0..2 {
        b.extend((grid.len() as i64).to_be_bytes());
        for (i, m) in grid.iter().enumerate() {
            let beats = grid
                .get(i + 1)
                .map_or(0, |next| (next.number - m.number) as i32);
            b.extend(m.offset.to_le_bytes());
            b.extend(m.number.to_le_bytes());
            b.extend(beats.to_le_bytes());
            b.extend(0i32.to_le_bytes());
        }
    }
    b.extend([0u8; 9]);
    zlib(&b)
}

/// A label as the blobs store it: a length byte and at most 255 bytes of UTF-8, cut at a character boundary.
fn label(b: &mut Vec<u8>, s: &str) {
    let mut n = s.len().min(255);
    while !s.is_char_boundary(n) {
        n -= 1;
    }
    b.push(n as u8);
    b.extend(&s.as_bytes()[..n]);
}

/// `quickCues`: exactly [`SLOTS`] cues, then the main cue (adjusted, flag, default; 0 when there is none).
pub fn quick_cues(cues: &[QuickCue], main: f64) -> Vec<u8> {
    let mut b = (cues.len() as i64).to_be_bytes().to_vec();
    for c in cues {
        label(&mut b, &c.label);
        b.extend(c.offset.to_be_bytes());
        b.extend(c.color);
    }
    b.extend(main.to_be_bytes());
    b.push(1);
    b.extend(main.to_be_bytes());
    zlib(&b)
}

/// `loops`, uncompressed: exactly [`SLOTS`] entries, `None` for an empty one.
pub fn loops(loops: &[Option<Loop>]) -> Vec<u8> {
    let mut b = (loops.len() as i64).to_le_bytes().to_vec();
    for l in loops {
        let (text, start, end, set, color) = match l {
            Some(l) => (l.label.as_str(), l.start, l.end, 1, l.color),
            None => ("", -1.0, -1.0, 0, [0; 4]),
        };
        label(&mut b, text);
        b.extend(start.to_le_bytes());
        b.extend(end.to_le_bytes());
        b.extend([set, set]);
        b.extend(color);
    }
    b
}
