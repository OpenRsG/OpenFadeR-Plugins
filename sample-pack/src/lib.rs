//! Sampler packs as a library plugin.
//!
//! A pack is a zip (observed in the factory pack: PKWARE APPNOTE entries, stored or deflated, names UTF-8, no encryption)
//! holding audio files and a `sampler_pack.xml` ([`pack`]). `open` unpacks the audio below
//! `<cache>/openfader/sample-pack/<pack name>/` (`$XDG_CACHE_HOME`, else `~/.cache`; a file already there with the right
//! size is kept) and serves the pack as a library: `tracks(None)` is every sample (title from the file stem, the pack's BPM,
//! `file` = the extracted file), the pack's `<LISTS>` are the folders and playlists in list order. The pack's name is its top
//! list's name, else the file's stem.

mod pack;
mod zip;

use openfader_plugin::library::{LibraryPlugin, Playlist, Probe, Source, Track};
use pack::{List, Pack};
use compat_rekordbox_common::at;
use serde_json::Value;
use std::fs::File;
use std::path::{Component, Path, PathBuf};
use zip::Zip;

const XML: &str = "sampler_pack.xml";

pub struct SamplePack;

/// A zip error in the pack's wording.
fn pack_error(e: zip::Error) -> String {
    match e {
        zip::Error::NotZip => "not a sampler pack (zip)".into(),
        zip::Error::Crc(name) => format!("{name}: damaged (checksum)"),
        zip::Error::Other(m) => m,
    }
}

/// `location` below `dir`; names that climb out of it or are absolute are refused.
fn inside(dir: &Path, location: &str) -> Result<PathBuf, String> {
    let rel = Path::new(location);
    if rel
        .components()
        .any(|c| !matches!(c, Component::Normal(_) | Component::CurDir))
    {
        return Err(format!("{location}: path leaves the pack folder"));
    }
    Ok(dir.join(rel))
}

/// The zip at `path` and the pack its `sampler_pack.xml` describes.
fn read(path: &Path) -> Result<(Zip, Pack), String> {
    let mut zip = Zip::open(path).map_err(|e| match e {
        zip::Error::NotZip => format!("{} is not a sampler pack (zip)", path.display()),
        e => pack_error(e),
    })?;
    let xml = zip
        .read(XML)
        .map_err(|_| format!("{} has no {XML}: not a sampler pack", path.display()))?;
    Ok((zip, pack::parse(&String::from_utf8_lossy(&xml))?))
}

/// Where unpacked packs live.
fn cache_dir() -> PathBuf {
    let var = |k| {
        std::env::var_os(k)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    };
    var("XDG_CACHE_HOME")
        .or_else(|| var("HOME").map(|h| h.join(".cache")))
        .unwrap_or_else(std::env::temp_dir)
        .join("openfader/sample-pack")
}

/// The opened pack: its samples (pack id, track) and lists.
struct Samples {
    tracks: Vec<(String, Track)>,
    lists: Vec<List>,
}

impl Samples {
    /// Unpacks the pack at `path` below `cache` and lists it. Samples the zip lacks are left out.
    fn open(path: &Path, cache: &Path) -> Result<Self, String> {
        let (mut zip, pack) = read(path)?;
        let name = pack
            .lists
            .first()
            .map(|l| l.name.replace(['/', '\\'], "_"))
            .filter(|n| !matches!(n.as_str(), "" | "." | ".."))
            .unwrap_or_else(|| {
                path.file_stem()
                    .map_or("Sample Pack".into(), |s| s.to_string_lossy().into_owned())
            });
        let dir = cache.join(name);
        let mut tracks = Vec::new();
        for t in &pack.tracks {
            let file = inside(&dir, &t.location)?;
            let Some(size) = zip.size(&t.location) else {
                continue;
            };
            if file.metadata().map(|m| m.len()).ok() != Some(size) {
                at(std::fs::create_dir_all(file.parent().unwrap_or(&dir)), &dir)?;
                let tmp = file.with_extension("part");
                let mut w = at(File::create(&tmp), &tmp)?;
                let r = zip.extract(&t.location, &mut w).map_err(pack_error);
                drop(w);
                if let Err(e) =
                    r.and_then(|_| std::fs::rename(&tmp, &file).map_err(|e| e.to_string()))
                {
                    let _ = std::fs::remove_file(&tmp);
                    return Err(e);
                }
            }
            tracks.push((
                t.id.clone(),
                Track {
                    title: file
                        .file_stem()
                        .map_or_else(String::new, |s| s.to_string_lossy().into_owned()),
                    bpm_x100: t.bpm_x100,
                    file: Some(file),
                    ..Track::default()
                },
            ));
        }
        Ok(Samples {
            tracks,
            lists: pack.lists,
        })
    }
}

impl Source for Samples {
    fn kind(&self) -> String {
        "sample pack".into()
    }

    /// A list is a playlist when it holds tracks, else a folder; the id is its index.
    fn playlists(&self) -> Result<Vec<Playlist>, String> {
        Ok(self
            .lists
            .iter()
            .enumerate()
            .map(|(i, l)| Playlist {
                id: i.to_string(),
                name: l.name.clone(),
                depth: l.depth,
                is_folder: l.tracks.is_empty(),
            })
            .collect())
    }

    fn tracks(&self, playlist: Option<&str>) -> Result<Vec<Track>, String> {
        let Some(id) = playlist else {
            return Ok(self.tracks.iter().map(|(_, t)| t.clone()).collect());
        };
        let list = id
            .parse::<usize>()
            .ok()
            .and_then(|i| self.lists.get(i))
            .ok_or_else(|| format!("{id}: no such list in the pack"))?;
        Ok(list
            .tracks
            .iter()
            .filter_map(|k| self.tracks.iter().find(|(id, _)| id == k))
            .map(|(_, t)| t.clone())
            .collect())
    }
}

impl LibraryPlugin for SamplePack {
    fn probe(path: &Path) -> Option<Probe> {
        Zip::open(path).ok()?.has(XML).then(|| Probe {
            score: 90,
            kind: "sample pack".into(),
        })
    }

    fn open(path: &Path, _settings: &Value) -> Result<Box<dyn Source>, String> {
        Ok(Box::new(Samples::open(path, &cache_dir())?))
    }
}

openfader_plugin::export_library!(SamplePack);

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::{Compression, Crc, write::DeflateEncoder};
    use std::io::Write;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("openfader-pack-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// 100 ms of a 8 kHz mono 8-bit WAV.
    fn wav() -> Vec<u8> {
        let (n, mut b) = (800u32, b"RIFF".to_vec());
        b.extend((36 + n).to_le_bytes());
        b.extend(b"WAVEfmt ");
        b.extend(16u32.to_le_bytes());
        b.extend([1, 0, 1, 0]);
        b.extend(8000u32.to_le_bytes());
        b.extend(8000u32.to_le_bytes());
        b.extend([1, 0, 8, 0]);
        b.extend(b"data");
        b.extend(n.to_le_bytes());
        b.extend((0..n).map(|i| (128.0 + 100.0 * (i as f32 * 0.3).sin()) as u8));
        b
    }

    /// A zip of (name, data, deflate?) like the factory pack: central directory, per-entry CRC.
    fn zip(entries: &[(&str, Vec<u8>, bool)]) -> Vec<u8> {
        let (mut out, mut central) = (Vec::new(), Vec::new());
        for (name, data, deflate) in entries {
            let mut c = Crc::new();
            c.update(data);
            let body = if *deflate {
                let mut e = DeflateEncoder::new(Vec::new(), Compression::default());
                e.write_all(data).unwrap();
                e.finish().unwrap()
            } else {
                data.clone()
            };
            let method = if *deflate { 8u16 } else { 0 };
            let at = out.len() as u32;
            let h = |v: &mut Vec<u8>, local: bool| {
                v.extend(if local { 0x0403_4b50u32 } else { 0x0201_4b50 }.to_le_bytes());
                if !local {
                    v.extend(20u16.to_le_bytes());
                }
                v.extend([20, 0, 0, 0]);
                v.extend(method.to_le_bytes());
                v.extend([0; 4]);
                v.extend(c.sum().to_le_bytes());
                v.extend((body.len() as u32).to_le_bytes());
                v.extend((data.len() as u32).to_le_bytes());
                v.extend((name.len() as u16).to_le_bytes());
                v.extend([0; 2]);
                if !local {
                    v.extend([0; 10]);
                    v.extend(at.to_le_bytes());
                }
                v.extend(name.as_bytes());
            };
            h(&mut out, true);
            out.extend(&body);
            h(&mut central, false);
        }
        let at = out.len() as u32;
        out.extend(&central);
        out.extend([0x50, 0x4b, 5, 6, 0, 0, 0, 0]);
        out.extend((entries.len() as u16).to_le_bytes().repeat(2));
        out.extend((central.len() as u32).to_le_bytes());
        out.extend(at.to_le_bytes());
        out.extend([0, 0]);
        out
    }

    const XML_TEXT: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<PACK Version="1.0.0"><TRACKS num="2">
<TRACK id="1" location="Kit/a.wav" mode="1" bpm="123.50" />
<TRACK id="2" location="Kit/b.wav" mode="1" bpm="90.00" />
</TRACKS><LISTS><LIST id="1" name="My Pack" attr="3" seq="0">
<LIST id="2" name="Kit &amp; Co" attr="2" seq="0"><TRACK key="2" /><TRACK key="1" /></LIST>
</LIST></LISTS></PACK>"#;

    fn names(tracks: &[Track]) -> Vec<(String, i64)> {
        tracks
            .iter()
            .map(|t| {
                let f = t.file.as_ref().unwrap();
                (
                    f.file_name().unwrap().to_string_lossy().into_owned(),
                    t.bpm_x100,
                )
            })
            .collect()
    }

    #[test]
    fn a_pack_unpacks_into_tracks_and_lists_and_reopens_cleanly() {
        let d = tmp("open");
        let spp = d.join("pack.zip");
        std::fs::write(
            &spp,
            zip(&[
                ("Kit/", vec![], false),
                (XML, XML_TEXT.into(), false),
                ("Kit/a.wav", wav(), true),
                ("Kit/b.wav", wav(), false),
            ]),
        )
        .unwrap();
        assert_eq!(SamplePack::probe(&spp).unwrap().kind, "sample pack");
        let cache = d.join("out");
        let lib = Samples::open(&spp, &cache).unwrap();
        assert_eq!(
            std::fs::read(cache.join("My Pack/Kit/a.wav")).unwrap(),
            wav(),
            "the deflated entry comes out intact"
        );
        let all = lib.tracks(None).unwrap();
        assert_eq!(
            names(&all),
            [("a.wav".to_string(), 12350), ("b.wav".to_string(), 9000)]
        );
        assert_eq!(all[0].title, "a");
        assert_eq!(
            all[0].file.as_deref(),
            Some(cache.join("My Pack/Kit/a.wav").as_path())
        );

        let pls = lib.playlists().unwrap();
        let shape: Vec<_> = pls
            .iter()
            .map(|p| (p.name.as_str(), p.depth, p.is_folder))
            .collect();
        assert_eq!(shape, [("My Pack", 0, true), ("Kit & Co", 1, false)]);
        assert_eq!(
            names(&lib.tracks(Some(&pls[1].id)).unwrap()),
            [("b.wav".to_string(), 9000), ("a.wav".to_string(), 12350)],
            "the list's order"
        );
        assert!(lib.tracks(Some(&pls[0].id)).unwrap().is_empty());
        assert!(lib.tracks(Some("9")).is_err());

        // Again: files of the right size are kept, not rewritten.
        let marker = cache.join("My Pack/Kit/b.wav");
        let before = marker.metadata().unwrap().modified().unwrap();
        let again = Samples::open(&spp, &cache).unwrap();
        assert_eq!(again.tracks(None).unwrap().len(), 2);
        assert_eq!(marker.metadata().unwrap().modified().unwrap(), before);
        let _ = std::fs::remove_dir_all(d); // cleanup: Windows may still hold a handle
    }

    #[test]
    fn damage_foreign_zips_and_escaping_names_are_refused() {
        let d = tmp("bad");
        let run = |bytes: Vec<u8>| {
            let p = d.join("x.zip");
            std::fs::write(&p, bytes).unwrap();
            Samples::open(&p, &d.join("out")).err().unwrap()
        };
        assert!(SamplePack::probe(&d.join("missing.zip")).is_none());
        assert!(run(b"not a zip at all".to_vec()).contains("not a sampler pack"));
        let other = zip(&[("other.txt", b"x".to_vec(), false)]);
        std::fs::write(d.join("o.zip"), &other).unwrap();
        assert!(SamplePack::probe(&d.join("o.zip")).is_none());
        assert!(run(other).contains("no sampler_pack.xml"));
        let escape = XML_TEXT.replace("Kit/a.wav", "../a.wav");
        assert!(
            run(zip(&[
                (XML, escape.into_bytes(), false),
                ("../a.wav", wav(), false)
            ]))
            .contains("leaves the pack folder")
        );
        let mut z = zip(&[
            (XML, XML_TEXT.into(), false),
            ("Kit/a.wav", wav(), false),
            ("Kit/b.wav", wav(), false),
        ]);
        let at = z.windows(4).position(|w| w == b"RIFF").unwrap() + 50;
        z[at] ^= 0xff;
        assert!(run(z).contains("damaged"));
        let _ = std::fs::remove_dir_all(d); // cleanup: Windows may still hold a handle
    }

    /// The factory pack of the installer (git-ignored, read in place) unpacks with every CRC right; skipped when it is not there.
    #[test]
    fn the_factory_pack_opens_when_present() {
        let spp = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../.local/extract/installer/GROOVE_CIRCUIT_FACTORY_SAMPLES.spp");
        if !spp.is_file() {
            return;
        }
        let d = tmp("factory");
        let lib = Samples::open(&spp, &d).unwrap();
        let (all, lists) = (lib.tracks(None).unwrap(), lib.playlists().unwrap());
        let first = lists.iter().find(|l| !l.is_folder).unwrap();
        let bank = lib.tracks(Some(&first.id)).unwrap();
        assert_eq!((all.len(), lists.len(), bank.len()), (8, 2, 8));
        assert_eq!(
            bank[0].bpm_x100, 12300,
            "House1 is the first track of the first list"
        );
        let _ = std::fs::remove_dir_all(d); // cleanup: Windows may still hold a handle
    }
}
