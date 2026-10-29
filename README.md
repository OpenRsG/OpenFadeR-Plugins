<p align="center">
  <img src="https://raw.githubusercontent.com/OpenRsG/OpenFadeR/main/assets/openfader.png" alt="OpenFadeR logo" width="160">
</p>

<h1 align="center">OpenFadeR-Plugins</h1>

<p align="center">
  <a href="LICENSE"><img alt="License: AGPL-3.0" src="https://img.shields.io/github/license/OpenRsG/OpenFadeR-Plugins"></a>
</p>

Plugins for [OpenFadeR](https://github.com/OpenRsG/OpenFadeR), the open-source DJ app: libraries, devices, effects, pad and knob modes, events and track sources. One folder per plugin, each with its `plugin.json`. Native plugins are Rust crates, the rest are JSON or Python.

OpenFadeR's mandatory Rekordbox compatibility adapters (`rekordbox-device`, `rekordbox-xml`, `rekordbox-db` at runtime) live in `compatibility/rekordbox/` of the core repository and are always on. The optional `traktor` adapter lives in the core's `compatibility/traktor/` as `compat-traktor`: it is compiled in, starts on for a fresh profile and preserves existing activation choices. Neither family needs installation. The `itunes` and `sample-pack` plugins share helpers from the core's `compat-rekordbox-common` package; Engine DJ, Serato, VirtualDJ and Mixxx remain here.

## Install in one click

In OpenFadeR open PREFERENCES > EXTENSIONS > GET PLUGINS. This repository is in the list on first run. INSTALL installs one plugin, INSTALL ALL all of them. Native plugins come prebuilt from this repository's latest release.

## What is here

| Plugin | Kind |
|---|---|
| `engine-dj` | library + device: Engine DJ library (`Engine Library/Database2/m.db`: tracks, grid, hot cues, loops, waveform overview, playlists) for Denon DJ, Numark and RANE players and Engine DJ; reads the desktop or a stick's library (schema 2.18.0 to 3.x); a new library is schema 3.0.0 (Engine 4.1 and newer) or 2.18.0 (Engine 2.0 to 4.0.1, device option), an existing one of schema 2.18.0 to 3.0.2 is extended as it is |
| `serato` | library + device: Serato DJ library (`_Serato_/database V2`, crates; cues, loops, colour and grid in the audio tags; a private copy only when the tags do not fit the shared file); reads `~/Music/_Serato_` or a stick's |
| `virtualdj` | library + device: VirtualDJ database (`database.xml`: tags, grid, hot cues, memory cues, loops; lists in `MyLists`); reads the desktop or a drive's |
| `itunes` | library: an iTunes / Music `Library.xml` |
| `sample-pack` | library: sampler packs (`.spp`) |
| `beat-fx`, `mic-fx`, `send-fx`, `lever-fx`, `cross-pass`, `soft-mix`, `tutorial-fx` | effects (JSON) |
| `center-lock`, `gate-cue` | knob and pad modes (JSON) |
| `now-playing` | events, actions, settings (Python) |
| `yt-dlp` | track sources: YouTube / SoundCloud search, playlists and downloads (Python, needs `yt-dlp`; on Windows `winget install -e --id yt-dlp.yt-dlp`, which brings ffmpeg too) |
| `cover-art` | artwork: cover candidates from MusicBrainz + Cover Art Archive, iTunes Search and (with a token) Discogs (Python) |

### A stick you can trust (Engine DJ, Serato, VirtualDJ)

`engine-dj`, `serato` and `virtualdj` give the same guarantees as the app's built-in Rekordbox and Traktor adapters (see the core repository's `docs/plugins.md`) when they write a stick:

- **All or nothing.** One export is one `Commit` of the SDK: the library files (Engine DJ `m.db`; Serato `database V2` and every crate; VirtualDJ `database.xml` and its list files) are staged beside their targets as `*.ofnew`, flushed, read back, then switched in one journalled step (`/.openfader/journal.json`). Pulled out at any point, the stick holds the old library or the new one, never a mix; the next write finishes or rolls back an interrupted switch first. A cancel or an error removes every file and folder the run made and leaves the stick byte-identical (Serato's tag edits of audio that was already there are put back).
- **Read back before it counts.** The staged library is read again with the plugin's own reader (tracks, playlists and their entries, cues and loops where the format holds them). A difference stops the export with a plain sentence, the old library stays, and a clean export reports `verified`.
- VirtualDJ readback uses an exclusively created private scratch directory, so concurrent exports do not overwrite or remove each other's verification files.
- **Dry run.** An export with `dry_run` writes nothing (not a folder, not a temp file) and returns the plan from the code the real export uses: tracks to add or update, bytes to copy and write, files, and what is blocked (no room, a file over the file system's limit, a name the stick cannot hold, a library that cannot be read).
- **Check and fix.** `device_check` reads a stick without writing and lists problems with cause, consequence and action: library unreadable, audio listed but missing (the host copies it again from the collection), leftovers of an interrupted write, and per format (Engine DJ: `PRAGMA quick_check`, stale `-wal`/`-shm` files). `device_fix` cleans leftovers through the same journal. None of these formats keeps a backup copy on a stick, so an unreadable library has no plugin fix.
- **Players.** Serato and VirtualDJ are read by software on a computer, not by players, so they ship no `players.json`. `engine-dj/players.json` lists the Denon DJ models the repository documents as reading an Engine library, all `to_measure`: file systems and firmware are not measured yet.

### One stick for every player and app

The device plugins here and the app's built-in Rekordbox and Traktor adapters write side by side on the same stick and share one copy of each audio file (`Contents/<artist>/<album>/<file>`). In the app's device window (also opened by the USB icon of a connected stick in the browser), the DEVICES tab asks what the stick is for: UNIVERSAL writes every enabled format, DEDICATED only the ticked ones. Each format says which software and players read it and shows its options (rekordbox: OneLibrary on or off). By default the formats are those already on the stick, or all enabled formats on an empty stick. A stick written by rekordbox is extended, not duplicated, because its audio files are found by name and size. The companion device formats and Traktor have not been tried on real hardware or in the vendor apps yet: please report what does not load.

## Build from source

The native plugins form one Cargo workspace built against the core SDK and `compat-rekordbox-common`, found at `../../sdk` and `../../compatibility/rekordbox/common`. This companion's `v0.2.9` tag pairs with core 0.2.8 and uses the SDK's shared `markup` API. Use companion `v0.2.8` with core 0.2.7; older core checkouts use their matching companion revision. Check this repository out as `extension/plugin/` inside a core checkout (`start.sh` clones the matching tag on first use), then:

```sh
./build.sh        # cargo build --release, then each library is copied next to its plugin.json
```

The app finds `extension/plugin/` next to its binary. Elsewhere, copy a plugin folder into `~/.config/openfader/extension/plugin/`, drop it on the app window, or list folders in `OPENFADER_PLUGINS`. Keys of encrypted libraries are plugin settings (PREFERENCES > EXTENSIONS), never environment variables.

The plugin format and the library / device ABI are documented in the core repository's [docs/plugins.md](https://github.com/OpenRsG/OpenFadeR/blob/main/docs/plugins.md) and `sdk/README.md`.

## Releases

Pushing a tag `v*` makes CI build stripped native libraries for Linux x86_64 and aarch64, Windows x86_64, and macOS x86_64 and aarch64. ARM Linux uses the Cortex-A53 baseline for Pi 3/4/5 compatibility; Apple silicon uses the M1 baseline. Each `<plugin>-<os>-<arch>.zip` holds the library named by `plugin.json`. `package.py` makes them locally after `build.sh`. CI attaches them to a draft: publish only after every matrix job passes and the expected native-plugin assets are present.

## Contributing and license

Contributions of any size are welcome. For anything more than a small fix, open an issue first. [CONTRIBUTING.md](CONTRIBUTING.md) shows how to build inside a core checkout and which tests to run. The [code of conduct](CODE_OF_CONDUCT.md) applies, and security reports go through [SECURITY.md](SECURITY.md). Related repositories: [OpenFadeR](https://github.com/OpenRsG/OpenFadeR), [OpenFadeR-Themes](https://github.com/OpenRsG/OpenFadeR-Themes) and [OpenFadeR-Map](https://github.com/OpenRsG/OpenFadeR-Map).

AGPL-3.0, see [LICENSE](LICENSE). Made by [OpenRsG](https://github.com/OpenRsG).
