//! `PIONEER/rekordbox/exportLibrary.db`: the OneLibrary database next to `export.pdb`, a SQLCipher 4 file (cipher
//! defaults, page size 4096) whose key comes from the plugin's `onelibrary_key` setting (never stored here).
//!
//! The table and index definitions are those of a real rekordbox 7 export (observed, read-only with sqlcipher; recorded in
//! `.local/re/export/notes.md`, "OneLibrary"); `rekordbox-device`'s reader reads the same tables back. Row conventions
//! (observed unless marked):
//! - `content`: ids are the device-library track ids; `*_id` columns are NULL for "none" (artist, album, genre, key, image),
//!   `artist_id_lyricist` and `color_id` are 0; `titleForSearch` NULL; `path` is the `/Contents/...` path, `analysisDataFilePath`
//!   the `/PIONEER/USBANLZ/.../ANLZ0000.DAT` path, `bitDepth` 16, `isHotCueAutoLoadOn`/`isKuvoDeliverStatusOn` 1, `masterDbId` one
//!   number per library, `masterContentId` = `content_id`, `analysedBits` 105, `contentLink` 1902336, `analysisDataUpdateCount` 3,
//!   `informationUpdateCount` 2 (the values of an analysed track). `rating` is 0-5 stars (inferred: the sample has only 0), the
//!   dates are the `YYYY-MM-DD` strings of `export.pdb`.
//! - `album(album_id, name, NULL, NULL, 0, NULL)`: an album is its name, with no artist (every album of the sample is like that);
//!   `artist(artist_id, name, NULL)`; `key(key_id, name)` holds Camelot names; `color` is the fixed 1 Pink .. 8 Purple table.
//! - `playlist`: `sequenceNo` counts siblings from 0, folders have `attribute` 1, top level has `playlist_id_parent` 0;
//!   `playlist_content.sequenceNo` counts from 1. `history`, `hotCueBankList`, `recommendedLike` and `cue` are empty here; `carry`
//!   copies them over from an older library and [`super::bank`] adds Hot Cue Bank Lists.
//! - `image(image_id, path)`: `/PIONEER/Artwork/<00001>/b<n>.jpg`, a copy of the `a<n>.jpg` that `export.pdb` names (`image_variant`).
//! - `menuItem` / `category` / `sort` / `color` come from the device's [`crate::settings::DeviceConfig`]: `menuItem` is the
//!   `columns` table of `export.pdb`; `category` / `sort` are its tables 17/18 recoded: `category(id = t17[1], menuItem =
//!   t17[0], sequenceNo = t17[3], isVisible = t17[3] != 0)`, `sort(id = t18[1], menuItem = t18[0], sequenceNo = t18[2] >> 8,
//!   isVisible = t18[2] != 1, isSelectedAsSubColumn = the "Column" choice: inferred)` (a sequence word of 1 is the hidden
//!   marker). The default configuration reproduces the real file's rows (the USB test compares them).
//! - `myTag`: categories are `attribute` 1 with parent 0, tags `attribute` 0 with the category as parent, same ids as
//!   `exportExt.pdb` (rekordbox uses random ids); `myTag_content(myTag_id, content_id)`.
//! - `property`: `deviceName` and `backGroundColorType` of the config (defaults `''` and 0, observed), `dbVersion '1000'`, the
//!   track count, the creation date. `content.isHotCueAutoLoadOn` is `config.hotcue_autoload` (observed 1).
//!
//! Cue rows exist only for Hot Cue Bank Lists ([`super::bank`]). The sample export has none, so the column semantics (`kind`,
//! `colorTableIndex`, the `*Frame*` and `*Mpeg*` positions) are unobserved; cues are in the ANLZ files, which players read.
//!
//! Journal mode: the real file is in WAL mode (header version 2). We write the default rollback journal: a single file that
//! opens read-only without `-shm`/`-wal` sidecars (inferred, not tested on a player).

use super::{
    Names, column_label,
    pdb::db::{Db, Tags},
    settings::browse::COLUMNS,
};
use rekordbox_common::at;
use rusqlite::{Connection, Transaction, params};
use std::collections::HashMap;
use std::path::Path;

/// The real export's `sqlite_master` entries, in order.
pub(super) const SCHEMA: &str = "
CREATE TABLE content(content_id integer primary key, title varchar, titleForSearch varchar, subtitle varchar, bpmx100 integer, length integer, trackNo integer, discNo integer, artist_id_artist integer, artist_id_remixer integer, artist_id_originalArtist integer, artist_id_composer integer, artist_id_lyricist integer, album_id integer, genre_id integer, label_id integer, key_id integer, color_id integer, image_id integer, djComment varchar, rating integer, releaseYear integer, releaseDate varchar, dateCreated varchar, dateAdded varchar, path varchar, fileName varchar, fileSize integer, fileType integer, bitrate integer, bitDepth integer, samplingRate integer, isrc varchar, djPlayCount integer, isHotCueAutoLoadOn integer, isKuvoDeliverStatusOn integer, kuvoDeliveryComment varchar, masterDbId integer, masterContentId integer, analysisDataFilePath varchar, analysedBits integer, contentLink integer, hasModified integer, cueUpdateCount integer, analysisDataUpdateCount integer, informationUpdateCount integer);
CREATE TABLE genre(genre_id integer primary key, name varchar);
CREATE TABLE artist(artist_id integer primary key, name varchar, nameForSearch varchar);
CREATE TABLE album(album_id integer primary key, name varchar, artist_id integer, image_id integer, isComplation integer, nameForSearch varchar);
CREATE TABLE label(label_id integer primary key, name varchar);
CREATE TABLE key(key_id integer primary key, name varchar);
CREATE TABLE color(color_id integer primary key, name varchar);
CREATE TABLE playlist(playlist_id integer primary key, sequenceNo integer, name varchar, image_id integer, attribute integer, playlist_id_parent integer);
CREATE TABLE playlist_content(playlist_id integer, content_id integer, sequenceNo integer);
CREATE TABLE hotCueBankList(hotCueBankList_id integer primary key, sequenceNo integer, name varchar, image_id integer, attribute integer, hotCueBankList_id_parent integer);
CREATE TABLE hotCueBankList_cue(hotCueBankList_id integer, cue_id integer, sequenceNo integer);
CREATE TABLE history(history_id integer primary key, sequenceNo integer, name varchar, attribute integer, history_id_parent integer);
CREATE TABLE history_content(history_id integer, content_id integer, sequenceNo integer);
CREATE TABLE image(image_id integer primary key, path varchar);
CREATE TABLE cue(cue_id integer primary key, content_id integer, kind integer, colorTableIndex integer, cueComment varchar, isActiveLoop integer, beatLoopNumerator integer, beatLoopDenominator integer, inUsec integer, outUsec integer, in150FramePerSec integer, out150FramePerSec integer, inMpegFrameNumber integer, outMpegFrameNumber integer, inMpegAbs integer, outMpegAbs integer, inDecodingStartFramePosition integer, outDecodingStartFramePosition integer, inFileOffsetInBlock integer, OutFileOffsetInBlock integer, inNumberOfSampleInBlock integer, outNumberOfSampleInBlock integer);
CREATE TABLE menuItem(menuItem_id integer primary key, kind integer, name varchar);
CREATE TABLE category(category_id integer primary key, menuItem_id integer, sequenceNo integer, isVisible integer);
CREATE TABLE sort(sort_id integer primary key, menuItem_id integer, sequenceNo integer, isVisible integer, isSelectedAsSubColumn integer);
CREATE TABLE property(deviceName varchar, dbVersion varchar, numberOfContents integer, createdDate varchar, backGroundColorType integer, myTagMasterDBID integer);
CREATE TABLE recommendedLike(content_id_1 integer, content_id_2 integer, rating integer, createdDate integer);
CREATE TABLE myTag(myTag_id integer primary key, sequenceNo integer, name varchar, attribute integer, myTag_id_parent integer);
CREATE TABLE myTag_content(myTag_id integer, content_id integer);
CREATE INDEX index_playlist_content_playlist_id on playlist_content(playlist_id);
CREATE INDEX index_myTag_content_myTag_id on myTag_content(myTag_id);
CREATE INDEX index_myTag_content_content_id on myTag_content(content_id);
CREATE INDEX index_hotCueBankList_cue_hotCueBankList_id on hotCueBankList_cue(hotCueBankList_id);
";

/// `/PIONEER/Artwork/00001/a1.jpg` → `/PIONEER/Artwork/00001/b1.jpg`: the file the `image` table names (observed: byte-identical to `a1.jpg`).
pub(super) fn image_variant(a: &str) -> String {
    let (dir, file) = a.rsplit_once('/').unwrap_or(("", a));
    format!("{dir}/b{}", file.strip_prefix('a').unwrap_or(file))
}

/// `Some(id)` for a named entity, `None` (SQL NULL) for "none" ([`Names`] gives the empty name id 0).
fn some(id: u32) -> Option<u32> {
    (id != 0).then_some(id)
}

/// Writes the database to `path` (which must not exist). `tags` may be empty; `library_id` is the one number that
/// `masterDbId` / `myTagMasterDBID` carry.
pub fn write(path: &Path, key: &str, db: &Db, tags: &Tags, library_id: u32) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        at(std::fs::create_dir_all(dir), dir)?;
    }
    if path.exists() {
        return Err(format!("{} already exists", path.display()));
    }
    let mut conn = super::open_keyed(path, key)?;
    at(fill(&mut conn, db, tags, library_id), path)
}

fn fill(conn: &mut Connection, db: &Db, tags: &Tags, library_id: u32) -> rusqlite::Result<()> {
    conn.execute_batch(SCHEMA)?;
    let tx = conn.transaction()?;
    insert_content(&tx, db, library_id)?;
    insert_menus(&tx, db)?;
    insert_playlists(&tx, db)?;
    insert_mytag(&tx, tags)?;
    tx.execute(
        "insert into property values (?, '1000', ?, ?, ?, ?)",
        params![
            db.config.device_name,
            db.tracks.len() as u32,
            db.analyze_date,
            db.config.background,
            library_id
        ],
    )?;
    tx.commit()
}

/// `content` rows, then the artist, genre, key, image and album tables they reference.
fn insert_content(tx: &Transaction, db: &Db, library_id: u32) -> rusqlite::Result<()> {
    let (mut artists, mut albums, mut genres, mut keys, mut images) =
        <(Names, Names, Names, Names, Names)>::default();
    {
        let mut ins = tx.prepare(&format!(
            "insert into content values ({})",
            vec!["?"; 46].join(",")
        ))?;
        for t in &db.tracks {
            let image = t.artwork.as_deref().map(|a| images.id(&image_variant(a)));
            ins.execute(params![
                t.id,
                t.title,
                None::<String>,
                "",
                t.bpm_x100,
                t.duration_s,
                0,
                0,
                some(artists.id(&t.artist)),
                None::<u32>,
                None::<u32>,
                None::<u32>,
                0,
                some(albums.id(&t.album)),
                some(genres.id(&t.genre)),
                None::<u32>,
                some(keys.id(&t.key)),
                t.color,
                image,
                t.comment,
                t.rating,
                0,
                "",
                t.date_added,
                t.date_added,
                t.file_path,
                t.filename,
                t.file_size,
                t.file_type,
                t.bitrate,
                16,
                t.sample_rate,
                "",
                0,
                u8::from(db.config.hotcue_autoload),
                1,
                "",
                library_id,
                t.id,
                t.analyze_path,
                105,
                1_902_336,
                0,
                0,
                3,
                2,
            ])?;
        }
    }
    let named = |sql: &str, n: &Names| -> rusqlite::Result<()> {
        let mut st = tx.prepare(sql)?;
        n.rows
            .iter()
            .try_for_each(|(id, s)| st.execute(params![id, s]).map(drop))
    };
    named(
        "insert into artist(artist_id, name) values (?, ?)",
        &artists,
    )?;
    named("insert into genre values (?, ?)", &genres)?;
    named("insert into key values (?, ?)", &keys)?;
    named("insert into image values (?, ?)", &images)?;
    // Albums have no artist or cover of their own (observed): artist_id, image_id, nameForSearch stay NULL, isComplation 0.
    named(
        "insert into album(album_id, name, isComplation) values (?, ?, 0)",
        &albums,
    )
}

/// The device configuration's tables: colour names, `menuItem`, `category` and `sort`.
fn insert_menus(tx: &Transaction, db: &Db) -> rusqlite::Result<()> {
    for (i, c) in db.config.colors.iter().enumerate() {
        tx.execute("insert into color values (?, ?)", params![i as u32 + 1, c])?;
    }
    for &(id, field, label) in &COLUMNS {
        tx.execute(
            "insert into menuItem values (?, ?, ?)",
            params![id, field, column_label(label)],
        )?;
    }
    for [menu, id, _, seq] in db.config.table17() {
        tx.execute(
            "insert into category values (?, ?, ?, ?)",
            params![id, menu, seq, seq != 0],
        )?;
    }
    for [menu, id, seq, _] in db.config.table18() {
        tx.execute(
            "insert into sort values (?, ?, ?, ?, ?)",
            params![id, menu, seq >> 8, seq != 1, db.config.column == Some(id)],
        )?;
    }
    Ok(())
}

fn insert_playlists(tx: &Transaction, db: &Db) -> rusqlite::Result<()> {
    let mut node = tx.prepare("insert into playlist values (?, ?, ?, NULL, ?, ?)")?;
    let mut entry = tx.prepare("insert into playlist_content values (?, ?, ?)")?;
    let mut order: HashMap<u32, u32> = HashMap::new();
    for n in &db.nodes {
        let seq = order.entry(n.parent).or_insert(0);
        node.execute(params![n.id, *seq, n.name, n.is_folder, n.parent])?;
        *seq += 1;
        for (i, t) in n.tracks.iter().enumerate() {
            entry.execute(params![n.id, t, i as u32 + 1])?;
        }
    }
    Ok(())
}

/// Same ids as exportExt.pdb: categories 1.., then the tags.
fn insert_mytag(tx: &Transaction, tags: &Tags) -> rusqlite::Result<()> {
    let mut next = tags.categories.len() as u32 + 1;
    let mut tag_ids: Vec<Vec<u32>> = Vec::new();
    for (c, (name, list)) in tags.categories.iter().enumerate() {
        tx.execute(
            "insert into myTag values (?, ?, ?, 1, 0)",
            params![c as u32 + 1, c as u32, name],
        )?;
        tag_ids.push((next..next + list.len() as u32).collect());
        for (i, tag) in list.iter().enumerate() {
            tx.execute(
                "insert into myTag values (?, ?, ?, 0, ?)",
                params![next + i as u32, i as u32, tag, c as u32 + 1],
            )?;
        }
        next += list.len() as u32;
    }
    let mut assign = tx.prepare("insert into myTag_content values (?, ?)")?;
    for &(track, c, i) in &tags.assigned {
        assign.execute(params![tag_ids[c][i], track])?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_variant_swaps_the_prefix_letter() {
        assert_eq!(
            image_variant("/PIONEER/Artwork/00001/a12.jpg"),
            "/PIONEER/Artwork/00001/b12.jpg"
        );
    }

    /// The derived browse tables have the real export's row counts and spot values (the USB test checks every row).
    #[test]
    fn browse_tables_are_derived_from_the_pdb_constants() {
        let dir = std::env::temp_dir().join(format!("rk-onelib-w-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("x.db");
        write(
            &path,
            "k",
            &Db {
                analyze_date: "2026-01-01".into(),
                ..Db::default()
            },
            &Tags::default(),
            7,
        )
        .unwrap();
        let c = crate::open_keyed(&path, "k").unwrap();
        let n = |t: &str| {
            c.query_row(&format!("select count(*) from {t}"), [], |r| {
                r.get::<_, i64>(0)
            })
            .unwrap()
        };
        assert_eq!(
            (n("menuItem"), n("category"), n("sort"), n("color")),
            (27, 22, 17, 8)
        );
        let row = |table: &str, id: i64| {
            c.query_row(
                &format!(
                    "select menuItem_id, sequenceNo, isVisible from {table} where {table}_id = ?"
                ),
                [id],
                |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        r.get::<_, i64>(1)?,
                        r.get::<_, i64>(2)?,
                    ))
                },
            )
            .unwrap()
        };
        assert_eq!(
            (
                row("category", 1),
                row("category", 4),
                row("category", 12),
                row("category", 26)
            ),
            ((1, 0, 0), (4, 3, 1), (11, 4, 1), (27, 8, 1))
        );
        assert_eq!(
            (row("sort", 0), row("sort", 6), row("sort", 12)),
            ((25, 1, 1), (1, 0, 0), (11, 7, 1))
        );
        drop(c);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// The device configuration reaches `category`, `sort` (with the Column), `color`, `property` and `isHotCueAutoLoadOn`.
    #[test]
    fn the_device_config_reaches_the_tables() {
        let dir = std::env::temp_dir().join(format!("rk-onelib-c-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("x.db");
        let mut db = Db {
            analyze_date: "2026-01-01".into(),
            ..Db::default()
        };
        for (k, v) in [
            ("device_name", "BOOTH"),
            ("background", "3"),
            ("categories", "5,1"),
            ("sorts", "0,4"),
            ("column", "4"),
            ("color.1", "Warm-up"),
            ("hotcue_autoload", "0"),
        ] {
            db.config.set(k, v).unwrap();
        }
        db.tracks.push(Default::default());
        write(&path, "k", &db, &Tags::default(), 7).unwrap();
        let c = crate::open_keyed(&path, "k").unwrap();
        let one = |sql: &str| c.query_row(sql, [], |r| r.get::<_, String>(0)).unwrap();
        assert_eq!(
            one("select deviceName || '/' || backGroundColorType from property"),
            "BOOTH/3"
        );
        assert_eq!(one("select name from color where color_id = 1"), "Warm-up");
        assert_eq!(
            one(
                "select group_concat(category_id) from (select category_id from category where isVisible = 1 order by sequenceNo)"
            ),
            "5,1"
        );
        assert_eq!(
            one(
                "select group_concat(sort_id) from (select sort_id from sort where isVisible = 1 order by sequenceNo)"
            ),
            "0,4"
        );
        assert_eq!(
            one("select group_concat(sort_id) from sort where isSelectedAsSubColumn = 1"),
            "4"
        );
        assert_eq!(
            one("select cast(isHotCueAutoLoadOn as text) from content"),
            "0"
        );
        drop(c);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
