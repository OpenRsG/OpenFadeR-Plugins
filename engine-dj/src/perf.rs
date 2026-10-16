//! From the host's [`Analysis`] to what [`crate::blob`] stores: positions in samples, the 8 quick cue and 8 loop slots, the beat
//! markers, the 1024-point overview waveform and the key id. Conventions (first marker at beat -4, last one past the end,
//! overview of `1024` points on a multiple of the quantisation number, the stock pad colours) are libdjinterop's
//! (`engine.cpp::normalize_beatgrid`, `track_utils.hpp`, `engine.hpp::standard_pad_colors`).

use crate::blob::{Argb, Loop, Marker, Overview, QuickCue, SLOTS};
use openfader_plugin::library::{Beat, Cue};

/// Points of the overview waveform, always.
const OVERVIEW_POINTS: usize = 1024;

/// A grid marker may be this far (ms, the resolution of the host's beat times) from a beat it spans.
const GRID_TOLERANCE_MS: f64 = 1.0;

/// The pad colours the players light a hot cue with when it has none of its own (libdjinterop `standard_pad_colors`).
const PADS: [[u8; 3]; SLOTS] = [
    [0xEA, 0xC5, 0x32],
    [0xEA, 0x8F, 0x32],
    [0xB8, 0x55, 0xBF],
    [0xBA, 0x2A, 0x41],
    [0x86, 0xC6, 0x4B],
    [0x20, 0xC6, 0x7C],
    [0x00, 0xA8, 0xB1],
    [0x15, 0x8E, 0xE2],
];

/// Sample offset of `ms`.
pub fn samples(ms: u32, rate: f64) -> f64 {
    f64::from(ms) * rate / 1000.0
}

fn argb(rgb: [u8; 3]) -> Argb {
    [0xFF, rgb[0], rgb[1], rgb[2]]
}

/// Engine's key id of a Camelot key ("8A"): 0 is C major (8B), then A minor (8A), G major (9B) … round the circle of fifths.
pub fn key_id(camelot: &str) -> Option<i32> {
    let s = camelot.trim();
    let mode = s.chars().next_back()?;
    let n: i32 = s[..s.len() - mode.len_utf8()]
        .parse()
        .ok()
        .filter(|n| (1..=12).contains(n))?;
    match mode {
        'A' | 'a' => Some((n + 4) % 12 * 2 + 1),
        'B' | 'b' => Some((n + 4) % 12 * 2),
        _ => None,
    }
}

/// The 8 quick cue slots (hot cue `n` is slot `n - 1`; a slot without a cue is empty) and the main cue offset: the first memory
/// cue, else the first hot cue, else 0 (none). Returns how many cues have no place (a second memory cue, a hot cue above H).
pub fn quick_cues(cues: &[Cue], rate: f64) -> (Vec<QuickCue>, f64, usize) {
    let mut slots: Vec<QuickCue> = (0..SLOTS)
        .map(|_| QuickCue {
            label: String::new(),
            offset: -1.0,
            color: [0; 4],
        })
        .collect();
    let mut dropped = 0;
    for c in cues.iter().filter(|c| c.hot_cue > 0) {
        match slots.get_mut(usize::from(c.hot_cue) - 1) {
            Some(s) if s.offset < 0.0 => {
                s.label = c.comment.clone().unwrap_or_default();
                s.offset = samples(c.time_ms, rate);
                s.color = argb(c.color.unwrap_or(PADS[usize::from(c.hot_cue) - 1]));
            }
            _ => dropped += 1,
        }
    }
    let mut memory = cues
        .iter()
        .filter(|c| c.hot_cue == 0 && c.loop_end_ms.is_none());
    let main = memory
        .next()
        .or_else(|| {
            cues.iter()
                .filter(|c| c.hot_cue > 0)
                .min_by_key(|c| c.hot_cue)
        })
        .map_or(0.0, |c| samples(c.time_ms, rate));
    (slots, main, dropped + memory.count())
}

/// The first 8 cues with a loop end, as loops. Returns how many more there are.
pub fn loops(cues: &[Cue], rate: f64) -> (Vec<Option<Loop>>, usize) {
    let mut found = cues.iter().filter_map(|c| Some((c, c.loop_end_ms?)));
    let mut slots: Vec<Option<Loop>> = (&mut found)
        .take(SLOTS)
        .map(|(c, end)| {
            Some(Loop {
                label: c.comment.clone().unwrap_or_default(),
                start: samples(c.time_ms, rate),
                end: samples(end, rate),
                color: argb(c.color.unwrap_or(PADS[4])),
            })
        })
        .collect();
    slots.resize_with(SLOTS, || None);
    (slots, found.count())
}

/// Markers of the grid of `beats` for a track of `total` samples: a marker where the tempo changes (every beat time lies within
/// [`GRID_TOLERANCE_MS`] of the line between its two markers), the first moved back to beat -4 and the last to the first beat
/// past the end, as the players expect. Beat numbers count from the first beat's place in its bar (a downbeat is a multiple of
/// 4). Empty when there is no tempo to extrapolate from.
pub fn markers(beats: &[Beat], rate: f64, total: f64) -> Vec<Marker> {
    let Some(first) = beats.first() else {
        return vec![];
    };
    let n0 = i64::from(first.bar_pos.clamp(1, 4)) - 1;
    let at = |i: usize| Marker {
        offset: samples(beats[i].time_ms, rate),
        number: n0 + i as i64,
    };
    let tol = GRID_TOLERANCE_MS * rate / 1000.0;
    let fits = |a: usize, b: usize| {
        let (pa, pb) = (at(a), at(b));
        (a + 1..b).all(|k| {
            let line = pa.offset + (pb.offset - pa.offset) * (k - a) as f64 / (b - a) as f64;
            (at(k).offset - line).abs() <= tol
        })
    };
    let mut out = vec![at(0)];
    let mut a = 0;
    while a + 1 < beats.len() {
        let mut b = a + 1;
        while b + 1 < beats.len() && fits(a, b + 1) {
            b += 1;
        }
        out.push(at(b));
        a = b;
    }
    if out.len() == 1 {
        let bpm = f64::from(first.bpm_x100) / 100.0;
        if bpm <= 0.0 {
            return vec![];
        }
        out.push(Marker {
            offset: out[0].offset + rate * 60.0 / bpm,
            number: out[0].number + 1,
        });
    }
    let per = |a: Marker, b: Marker| (b.offset - a.offset) / (b.number - a.number) as f64;
    let spb = per(out[0], out[1]);
    if spb.is_nan() || spb <= 0.0 {
        return vec![];
    }
    out[0].offset -= (4 + out[0].number) as f64 * spb;
    out[0].number = -4;
    let n = out.len();
    let spb = per(out[n - 2], out[n - 1]);
    if spb.is_nan() || spb <= 0.0 {
        return vec![];
    }
    // Never past the marker before it, whatever the length says.
    let beyond = (((total - out[n - 1].offset) / spb).ceil() as i64)
        .max(out[n - 2].number - out[n - 1].number + 1);
    out[n - 1].offset += beyond as f64 * spb;
    out[n - 1].number += beyond;
    out
}

/// Samples each overview point stands for: the track rounded down to a multiple of the quantisation number, over 1024.
fn overview_step(total: i64, rate: f64) -> Option<f64> {
    let qn = (rate as i64 / 210) * 2;
    (total > 0 && qn > 0).then(|| ((total / qn) * qn) as f64 / OVERVIEW_POINTS as f64)
}

/// The 3-band preview (any number of columns across the whole track) as the 1024-point overview; `None` without a preview or
/// a length.
pub fn overview(preview: &[[u8; 3]], total: i64, rate: f64) -> Option<Overview> {
    let samples_per_point = overview_step(total, rate).filter(|_| !preview.is_empty())?;
    let points = (0..OVERVIEW_POINTS)
        .map(|i| preview[preview.len() * (2 * i + 1) / (2 * OVERVIEW_POINTS)])
        .collect();
    Some(Overview {
        points,
        samples_per_point,
    })
}
