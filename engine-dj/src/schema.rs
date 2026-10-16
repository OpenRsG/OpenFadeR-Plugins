//! The database file: schema 3.0.0 (DDL of libdjinterop `schema_3_0_0.cpp::create`), creating it, opening an existing one for a
//! merge, and swapping the result in.

use rusqlite::{params, Connection, OpenFlags};
use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// `Information.schemaVersionMajor/Minor/Patch` this plugin writes and the only version it extends.
pub const VERSION: (i64, i64, i64) = (3, 0, 0);

/// Everything `Engine Library/Database2/m.db` is made of, with the row of the stock album art (id 1: "no artwork") the tracks
/// point at. `Information` is filled in by [`create`].
const DDL: &str = "
CREATE TABLE Information (id INTEGER PRIMARY KEY AUTOINCREMENT, uuid TEXT, schemaVersionMajor INTEGER,
  schemaVersionMinor INTEGER, schemaVersionPatch INTEGER, currentPlayedIndiciator INTEGER,
  lastRekordBoxLibraryImportReadCounter INTEGER);
CREATE TABLE AlbumArt (id INTEGER PRIMARY KEY AUTOINCREMENT, hash TEXT, albumArt BLOB);
CREATE TABLE Pack (id INTEGER PRIMARY KEY AUTOINCREMENT, packId TEXT, changeLogDatabaseUuid TEXT, changeLogId INTEGER,
  lastPackTime DATETIME);
CREATE TABLE Playlist (id INTEGER PRIMARY KEY AUTOINCREMENT, title TEXT, parentListId INTEGER, isPersisted BOOLEAN,
  nextListId INTEGER, lastEditTime DATETIME, isExplicitlyExported BOOLEAN,
  CONSTRAINT C_NAME_UNIQUE_FOR_PARENT UNIQUE (title, parentListId),
  CONSTRAINT C_NEXT_LIST_ID_UNIQUE_FOR_PARENT UNIQUE (parentListId, nextListId));
CREATE TABLE PlaylistEntity (id INTEGER PRIMARY KEY AUTOINCREMENT, listId INTEGER, trackId INTEGER, databaseUuid TEXT,
  nextEntityId INTEGER, membershipReference INTEGER,
  CONSTRAINT C_NAME_UNIQUE_FOR_LIST UNIQUE (listId, databaseUuid, trackId),
  FOREIGN KEY (listId) REFERENCES Playlist (id) ON DELETE CASCADE);
CREATE TABLE Smartlist (listUuid TEXT NOT NULL PRIMARY KEY, title TEXT, parentPlaylistPath TEXT, nextPlaylistPath TEXT,
  nextListUuid TEXT, rules TEXT, lastEditTime DATETIME,
  CONSTRAINT C_NAME_UNIQUE_FOR_PARENT UNIQUE (title, parentPlaylistPath),
  CONSTRAINT C_NEXT_LIST_UNIQUE_FOR_PARENT UNIQUE (parentPlaylistPath, nextPlaylistPath, nextListUuid));
CREATE TABLE Track (id INTEGER PRIMARY KEY AUTOINCREMENT, playOrder INTEGER, length INTEGER, bpm INTEGER, year INTEGER,
  path TEXT, filename TEXT, bitrate INTEGER, bpmAnalyzed REAL, albumArtId INTEGER, fileBytes INTEGER, title TEXT,
  artist TEXT, album TEXT, genre TEXT, comment TEXT, label TEXT, composer TEXT, remixer TEXT, key INTEGER,
  rating INTEGER, albumArt TEXT, timeLastPlayed DATETIME, isPlayed BOOLEAN, fileType TEXT, isAnalyzed BOOLEAN,
  dateCreated DATETIME, dateAdded DATETIME, isAvailable BOOLEAN, isMetadataOfPackedTrackChanged BOOLEAN,
  isPerfomanceDataOfPackedTrackChanged BOOLEAN, playedIndicator INTEGER, isMetadataImported BOOLEAN,
  pdbImportKey INTEGER, streamingSource TEXT, uri TEXT, isBeatGridLocked BOOLEAN, originDatabaseUuid TEXT,
  originTrackId INTEGER, streamingFlags INTEGER, explicitLyrics BOOLEAN, lastEditTime DATETIME,
  CONSTRAINT C_originDatabaseUuid_originTrackId UNIQUE (originDatabaseUuid, originTrackId),
  CONSTRAINT C_path UNIQUE (path),
  FOREIGN KEY (albumArtId) REFERENCES AlbumArt (id) ON DELETE RESTRICT);
CREATE TABLE PerformanceData (trackId INTEGER PRIMARY KEY, trackData BLOB, overviewWaveFormData BLOB, beatData BLOB,
  quickCues BLOB, loops BLOB, thirdPartySourceId INTEGER, activeOnLoadLoops INTEGER,
  FOREIGN KEY(trackId) REFERENCES Track(id) ON DELETE CASCADE ON UPDATE CASCADE);
CREATE TABLE PreparelistEntity (id INTEGER PRIMARY KEY AUTOINCREMENT, trackId INTEGER, trackNumber INTEGER,
  FOREIGN KEY (trackId) REFERENCES Track (id) ON DELETE CASCADE);
DELETE FROM sqlite_sequence;
CREATE INDEX index_AlbumArt_hash ON AlbumArt (hash);
CREATE INDEX index_PlaylistEntity_nextEntityId_listId ON PlaylistEntity(nextEntityId, listId);
CREATE TRIGGER trigger_after_insert_Pack_timestamp AFTER INSERT ON Pack FOR EACH ROW WHEN NEW.lastPackTime IS NULL
  BEGIN UPDATE Pack SET lastPackTime = strftime('%s') WHERE ROWID = NEW.ROWID; END;
CREATE TRIGGER trigger_after_insert_Pack_changeLogId AFTER INSERT ON Pack FOR EACH ROW WHEN NEW.changeLogId = 0
  BEGIN UPDATE Pack SET changeLogId = 1 WHERE ROWID = NEW.ROWID; END;
CREATE VIEW ChangeLog (id, trackId) AS SELECT 0, 0 WHERE FALSE;
CREATE TRIGGER trigger_before_insert_List BEFORE INSERT ON Playlist FOR EACH ROW
  BEGIN UPDATE Playlist SET nextListId = -(1 + nextListId)
    WHERE nextListId = NEW.nextListId AND parentListId = NEW.parentListId; END;
CREATE TRIGGER trigger_after_insert_List AFTER INSERT ON Playlist FOR EACH ROW
  BEGIN UPDATE Playlist SET nextListId = NEW.id
    WHERE nextListId = -(1 + NEW.nextListId) AND parentListId = NEW.parentListId; END;
CREATE TRIGGER trigger_after_delete_List AFTER DELETE ON Playlist FOR EACH ROW
  BEGIN UPDATE Playlist SET nextListId = OLD.nextListId WHERE nextListId = OLD.id;
    DELETE FROM Playlist WHERE parentListId = OLD.id; END;
CREATE TRIGGER trigger_after_update_isPersistParent AFTER UPDATE ON Playlist
  WHEN (old.isPersisted = 0 AND new.isPersisted = 1) OR (old.parentListId != new.parentListId AND new.isPersisted = 1)
  BEGIN UPDATE Playlist SET isPersisted = 1 WHERE id IN (SELECT parentListId FROM PlaylistAllParent WHERE id=new.id); END;
CREATE TRIGGER trigger_after_update_isPersistChild AFTER UPDATE ON Playlist WHEN old.isPersisted = 1 AND new.isPersisted = 0
  BEGIN UPDATE Playlist SET isPersisted = 0 WHERE id IN (SELECT childListId FROM PlaylistAllChildren WHERE id=new.id); END;
CREATE TRIGGER trigger_after_insert_isPersist AFTER INSERT ON Playlist WHEN new.isPersisted = 1
  BEGIN UPDATE Playlist SET isPersisted = 1 WHERE id IN (SELECT parentListId FROM PlaylistAllParent WHERE id=new.id); END;
CREATE VIEW PlaylistAllParent AS WITH FindAllParent AS (
    SELECT id, parentListId FROM Playlist
    UNION ALL
    SELECT recursiveCTE.id, Plist.parentListId FROM Playlist Plist
      INNER JOIN FindAllParent recursiveCTE ON recursiveCTE.parentListId = Plist.id)
  SELECT * FROM FindAllParent;
CREATE VIEW PlaylistAllChildren AS WITH FindAllChild AS (
    SELECT id, id as childListId FROM Playlist
    UNION ALL
    SELECT recursiveCTE.id, Plist.id FROM Playlist Plist
      INNER JOIN FindAllChild recursiveCTE ON recursiveCTE.childListId = Plist.parentListId)
  SELECT * FROM FindAllChild WHERE id <> childListId;
CREATE VIEW PlaylistPath AS WITH RECURSIVE Heirarchy AS (
    SELECT id AS child, parentListId AS parent, title AS name, 1 AS depth FROM Playlist
    UNION ALL
    SELECT child, parentListId AS parent, title AS name, h.depth + 1 AS depth FROM Playlist c
      JOIN Heirarchy h ON h.parent = c.id ORDER BY depth DESC),
  OrderedList AS (
    SELECT id, nextListId, 1 AS position FROM Playlist WHERE nextListId = 0
    UNION ALL
    SELECT c.id, c.nextListId, l.position + 1 FROM Playlist c INNER JOIN OrderedList l ON c.nextListId = l.id),
  NameConcat AS (
    SELECT child AS id, GROUP_CONCAT(name, ';') || ';' AS path
      FROM (SELECT child, name FROM Heirarchy ORDER BY depth DESC) GROUP BY child)
  SELECT id, path, ROW_NUMBER() OVER (
      ORDER BY (SELECT COUNT(*) FROM (SELECT * FROM Heirarchy WHERE child = id)) DESC,
               (SELECT position FROM OrderedList ol WHERE ol.id = c.id) ASC) AS position
    FROM Playlist c LEFT JOIN NameConcat g USING (id);
CREATE TRIGGER trigger_before_delete_PlaylistEntity BEFORE DELETE ON PlaylistEntity WHEN OLD.trackId > 0
  BEGIN UPDATE PlaylistEntity SET nextEntityId = OLD.nextEntityId WHERE nextEntityId = OLD.id AND listId = OLD.listId; END;
CREATE INDEX index_Track_filename ON Track (filename);
CREATE INDEX index_Track_albumArtId ON Track (albumArtId);
CREATE INDEX index_Track_uri ON Track (uri);
CREATE INDEX index_Track_title ON Track(title);
CREATE INDEX index_Track_length ON Track(length);
CREATE INDEX index_Track_rating ON Track(rating);
CREATE INDEX index_Track_year ON Track(year);
CREATE INDEX index_Track_dateAdded ON Track(dateAdded);
CREATE INDEX index_Track_genre ON Track(genre);
CREATE INDEX index_Track_artist ON Track(artist);
CREATE INDEX index_Track_album ON Track(album);
CREATE INDEX index_Track_key ON Track(key);
CREATE INDEX index_Track_bpmAnalyzed ON Track(CAST(bpmAnalyzed + 0.5 AS int));
CREATE TRIGGER trigger_after_insert_Track_check_id AFTER INSERT ON Track
  WHEN NEW.id <= (SELECT seq FROM sqlite_sequence WHERE name = 'Track')
  BEGIN SELECT RAISE(ABORT, 'Recycling deleted track id''s are not allowed'); END;
CREATE TRIGGER trigger_after_update_Track_check_Id BEFORE UPDATE ON Track WHEN NEW.id <> OLD.id
  BEGIN SELECT RAISE(ABORT, 'Changing track id''s are not allowed'); END;
CREATE TRIGGER trigger_after_insert_Track_fix_origin AFTER INSERT ON Track
  WHEN IFNULL(NEW.originTrackId, 0) = 0 OR IFNULL(NEW.originDatabaseUuid, '') = ''
  BEGIN UPDATE Track SET originTrackId = NEW.id, originDatabaseUuid = (SELECT uuid FROM Information) WHERE track.id = NEW.id; END;
CREATE TRIGGER trigger_after_update_Track_fix_origin AFTER UPDATE ON Track
  WHEN IFNULL(NEW.originTrackId, 0) = 0 OR IFNULL(NEW.originDatabaseUuid, '') = ''
  BEGIN UPDATE Track SET originTrackId = NEW.id, originDatabaseUuid = (SELECT uuid FROM Information) WHERE track.id = NEW.id; END;
CREATE TRIGGER trigger_after_update_Track_timestamp AFTER UPDATE OF length, bpm, year, filename, bitrate, bpmAnalyzed,
    albumArtId, title, artist, album, genre, comment, label, composer, remixer, key, rating, albumArt, fileType, isAnalyzed,
    isBeatgridLocked, trackData, overviewWaveformData, beatData, quickCues, loops, explicitLyrics, activeOnLoadLoops
  ON Track FOR EACH ROW BEGIN UPDATE Track SET lastEditTime = strftime('%s') WHERE ROWID=NEW.ROWID; END;
CREATE TRIGGER trigger_after_insert_Track_insert_performance_data AFTER INSERT ON Track
  BEGIN INSERT INTO PerformanceData(trackId) VALUES(NEW.id); END;
CREATE INDEX index_PreparelistEntity_trackId ON PreparelistEntity (trackId);
INSERT INTO AlbumArt VALUES (1, '', NULL);
";

/// The current time in unix seconds, for `lastEditTime` columns.
pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

/// 64 random bits: the std hasher is seeded from the OS per instance.
fn random() -> u64 {
    let mut h = RandomState::new().build_hasher();
    h.write_u128(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos()),
    );
    h.finish()
}

/// A random version 4 UUID, lower case.
fn uuid() -> String {
    let (a, b) = (random(), random());
    format!(
        "{:08x}-{:04x}-4{:03x}-{:04x}-{:012x}",
        a >> 32,
        (a >> 16) & 0xffff,
        a & 0xfff,
        (b >> 48) & 0x3fff | 0x8000,
        b & 0xffff_ffff_ffff
    )
}

/// The library file being written: a copy of `m.db` (or a new one) next to it, swapped in by [`Staged::commit`]; dropped
/// without `commit` (an error, a cancel) it is deleted and the stick keeps what it had.
pub struct Staged {
    pub conn: Connection,
    /// `Information.uuid`: the id of this library, the `databaseUuid` of its playlist entries.
    pub uuid: String,
    target: PathBuf,
    temp: Temp,
}

/// The staged file, deleted when this is dropped (a renamed file is gone already).
struct Temp(PathBuf);

impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Opens the library at `db` for writing: a consistent copy of it when it exists (`VACUUM INTO` also folds in a journal a
/// player left behind), refusing a file that is not an Engine library or has another schema version than [`VERSION`]; else a
/// new library.
pub fn stage(db: &Path) -> Result<Staged, String> {
    let err = |e: rusqlite::Error| format!("{}: {e}", db.display());
    let temp = Temp(db.with_extension("db.new"));
    let _ = std::fs::remove_file(&temp.0);
    if let Some(dir) = db.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let uuid = if db.is_file() {
        let old = Connection::open_with_flags(db, OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(err)?;
        let (uuid, major, minor, patch): (String, i64, i64, i64) = old
            .query_row(
                "SELECT uuid, schemaVersionMajor, schemaVersionMinor, schemaVersionPatch FROM Information",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .map_err(|e| format!("{} is not an Engine library: {e}", db.display()))?;
        if (major, minor, patch) != VERSION {
            return Err(format!(
                "{} has schema version {major}.{minor}.{patch}; only {}.{}.{} (Engine DJ 4.1 to 4.2) can be extended, so it was left untouched",
                db.display(),
                VERSION.0,
                VERSION.1,
                VERSION.2
            ));
        }
        old.execute("VACUUM INTO ?1", [temp.0.to_string_lossy()])
            .map_err(err)?;
        uuid
    } else {
        let uuid = uuid();
        let conn = Connection::open(&temp.0).map_err(err)?;
        conn.execute_batch(DDL).map_err(err)?;
        conn.execute(
            "INSERT INTO Information (uuid, schemaVersionMajor, schemaVersionMinor, schemaVersionPatch, \
             currentPlayedIndiciator, lastRekordBoxLibraryImportReadCounter) VALUES (?1, ?2, ?3, ?4, ?5, 0)",
            params![uuid, VERSION.0, VERSION.1, VERSION.2, (random() >> 1) as i64],
        )
        .map_err(err)?;
        uuid
    };
    let conn = Connection::open(&temp.0).map_err(err)?;
    Ok(Staged {
        conn,
        uuid,
        target: db.to_path_buf(),
        temp,
    })
}

impl Staged {
    /// Checks the file and renames it over the library; a `-wal`/`-shm` pair of the old file (already folded in) is removed.
    pub fn commit(self) -> Result<(), String> {
        let ok: String = self
            .conn
            .query_row("PRAGMA integrity_check", [], |r| r.get(0))
            .map_err(|e| e.to_string())?;
        if ok != "ok" {
            return Err(format!("the library failed its integrity check: {ok}"));
        }
        let Staged {
            conn, target, temp, ..
        } = self;
        conn.close().map_err(|(_, e)| e.to_string())?;
        std::fs::rename(&temp.0, &target).map_err(|e| format!("{}: {e}", target.display()))?;
        for suffix in ["-wal", "-shm"] {
            let mut side = target.clone().into_os_string();
            side.push(suffix);
            let _ = std::fs::remove_file(side);
        }
        Ok(())
    }
}
