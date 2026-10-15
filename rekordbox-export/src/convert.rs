//! "Convert from Device Library" (rekordbox: [Devices] > device > OneLibrary > Convert from Device Library): builds
//! `exportLibrary.db` from the `export.pdb` / `exportExt.pdb` the device already holds, same tracks, playlists, MyTag and
//! histories, same ids. An existing OneLibrary is replaced, so playlists or histories that only it held are lost (observed
//! strings: "OneLibrary already exists in the device. Do you want to overwrite it?"; the web FAQ warns about the loss).

use super::pdb::read;
use super::settings::DeviceConfig;
use super::{ONE, carry, onelibrary, rebuild_onelibrary};
use openfader_plugin::library::{ActionAnswer, Confirm};
use rekordbox_common::{at, on_device};
use std::path::Path;

/// Id of the device action.
pub const ID: &str = "convert";

/// The question asked before an existing OneLibrary is replaced.
const EXISTS: &str = "OneLibrary already exists in the device. Do you want to overwrite it?";

/// `device_action("convert")`: writes `<dest>/PIONEER/rekordbox/exportLibrary.db` (encrypted with `key`) from the Device Library.
/// An existing one is replaced only when `confirmed` (else the answer is a [`Confirm`]). Artwork gets its `b` copy where missing.
pub fn to_onelibrary(
    dest: &Path,
    key: Option<&str>,
    confirmed: bool,
    config: &DeviceConfig,
) -> Result<ActionAnswer, String> {
    let key = key.ok_or("converting needs the OneLibrary key: set it in the plugin's settings")?;
    let one = dest.join(ONE);
    if one.is_file() && !confirmed {
        return Ok(ActionAnswer {
            confirm: Some(Confirm {
                text: EXISTS.into(),
                yes: "Overwrite".into(),
            }),
            ..ActionAnswer::default()
        });
    }
    let (db, tags) = read::open(dest, config)?;
    for t in &db.tracks {
        copy_cover(dest, t.artwork.as_deref())?;
    }
    rebuild_onelibrary(&one, key, &db, &tags, |tmp| carry::histories(tmp, key, &db))?;
    Ok(ActionAnswer {
        message: format!(
            "Converted {} track(s) and {} playlist(s) to the OneLibrary",
            db.tracks.len(),
            db.nodes.len()
        ),
        confirm: None,
    })
}

/// `a<n>.jpg` → its OneLibrary twin `b<n>.jpg` (byte-identical, observed), when the cover exists and the twin does not.
fn copy_cover(dest: &Path, artwork: Option<&str>) -> Result<(), String> {
    let Some(artwork) = artwork else {
        return Ok(());
    };
    let a = on_device(dest, artwork);
    let b = on_device(dest, &onelibrary::image_variant(artwork));
    if a.is_file() && !b.exists() {
        at(std::fs::copy(&a, &b), &b)?;
    }
    Ok(())
}
