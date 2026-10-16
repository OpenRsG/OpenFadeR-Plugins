//! Reading the libraries of a device: the `export.pdb` Device Library ([`pdb`]) and the encrypted OneLibrary
//! ([`onelibrary`], `exportLibrary.db`). Both list the playlists (histories under the SDK's "Histories" folder, ids
//! `history:<id>`), the tracks with the absolute path of their analysis file as `analysis_ref`, and answer for that
//! analysis through [`rekordbox_common::anlz`].

mod onelibrary;
mod pdb;

use openfader_plugin::library::{Analysis, Probe, Source, Track};
use rekordbox_common::{anlz, setting};
use serde_json::Value;
use std::path::Path;

/// The library files below a device root.
const PDB: &str = "PIONEER/rekordbox/export.pdb";
const ONE: &str = "PIONEER/rekordbox/exportLibrary.db";

/// A device root holding either library file (the score 90 leaves room for plugins that know better).
pub fn probe(path: &Path) -> Option<Probe> {
    let kind = if path.join(PDB).is_file() {
        "USB (Device Library)"
    } else if path.join(ONE).is_file() {
        "USB (OneLibrary)"
    } else {
        return None;
    };
    Some(Probe {
        score: 90,
        kind: kind.into(),
    })
}

/// The OneLibrary when its key setting is set and the file is there, else the `export.pdb` library.
pub fn open(path: &Path, settings: &Value) -> Result<Box<dyn Source>, String> {
    let one = path.join(ONE).is_file();
    match setting(settings, "onelibrary_key") {
        Some(key) if one => Ok(Box::new(onelibrary::OneLibrary::open(path, &key)?)),
        _ if path.join(PDB).is_file() => Ok(Box::new(pdb::Export::open(path)?)),
        None if one => Err(format!(
            "{}: only the encrypted OneLibrary is there: set its key in the plugin's settings",
            path.display()
        )),
        _ => Err(format!("{}: no device library", path.display())),
    }
}

/// The analysis behind a track's `analysis_ref` (the absolute path of its `.DAT`).
pub fn analysis_ref(reference: &str) -> Option<Analysis> {
    anlz::load_track(Path::new(reference)).ok()
}

/// [`analysis_ref`] of `track`.
fn analysis(track: &Track) -> Option<Analysis> {
    analysis_ref(track.analysis_ref.as_deref()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn probe_and_open_pick_the_library() {
        let dir = std::env::temp_dir().join(format!("openfader-read-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("PIONEER/rekordbox")).unwrap();
        assert!(probe(&dir).is_none() && open(&dir, &json!({})).is_err());
        std::fs::write(dir.join(ONE), b"x").unwrap();
        assert_eq!(probe(&dir).unwrap().kind, "USB (OneLibrary)");
        let err = open(&dir, &json!({"onelibrary_key": " "})).err().unwrap();
        assert!(err.contains("key"), "{err}"); // no key: the encrypted library alone cannot be read
        std::fs::write(dir.join(PDB), b"x").unwrap();
        let p = probe(&dir).unwrap();
        assert_eq!((p.score, p.kind.as_str()), (90, "USB (Device Library)"));
        assert!(open(&dir, &json!({})).is_err()); // export.pdb is no database either: the error, not a panic
        let _ = std::fs::remove_dir_all(&dir); // cleanup: Windows may still hold a handle
    }
}
