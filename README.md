<p align="center">
  <img src="https://raw.githubusercontent.com/OpenRsG/OpenFadeR/main/assets/openfader.png" alt="OpenFadeR logo" width="160">
</p>

<h1 align="center">OpenFadeR-Plugins</h1>

<p align="center">
  <a href="LICENSE"><img alt="License: AGPL-3.0" src="https://img.shields.io/github/license/OpenRsG/OpenFadeR-Plugins"></a>
</p>

Plugins for [OpenFadeR](https://github.com/OpenRsG/OpenFadeR), the open-source DJ app: libraries, devices, effects, pad and knob modes, events and track sources. One folder per plugin, each with its `plugin.json`. Native plugins are Rust crates, the rest are JSON, Lua or Python.

## Install in one click

In OpenFadeR open PREFERENCES > EXTENSIONS > GET PLUGINS. This repository is in the list on first run. INSTALL installs one plugin, INSTALL ALL all of them. Native plugins come prebuilt from this repository's latest release.

## What is here

| Plugin | Kind |
|---|---|
| `rekordbox-xml` | library: reads and writes a rekordbox XML collection |
| `rekordbox-device` | library + device: rekordbox Device Library (`export.pdb`, ANLZ files; rekordbox 5 to 7, every CDJ / XDJ) and OneLibrary (`exportLibrary.db`, built-in key; rekordbox 7 and the newest players, a device option), on USB sticks and SD cards: read, export, update, remove, convert, player settings, backups |
| `engine-dj` | device: Engine DJ library (`Engine Library/Database2/m.db`: tracks, grid, hot cues, loops, waveform overview, playlists) for Denon DJ, Numark and RANE players and Engine DJ; a new library is schema 3.0.0 (Engine 4.1 and newer) or 2.18.0 (Engine 2.0 to 4.0.1, device option), an existing one of schema 2.18.0 to 3.0.2 is extended as it is |
| `serato` | device: Serato DJ library (`_Serato_/database V2`, crates; cues, loops, colour and grid in the audio tags; a private copy only when the tags do not fit the shared file) |
| `virtualdj` | device: VirtualDJ drive database (`VirtualDJ/database.xml`: tags, grid, hot cues, memory cues, loops; lists in `VirtualDJ/MyLists`) |
| `traktor` | device: Traktor collection (`Traktor/collection.nml`, NML 19: tags, grid, hot and memory cues, loops, playlists), merged into an existing one, for Traktor's Import another Collection |
| `rekordbox-db` | library: the rekordbox desktop collection (`master.db`, smart playlists; key and path map as settings) |
| `itunes` | library: an iTunes / Music `Library.xml` |
| `sample-pack` | library: sampler packs (`.spp`) |
| `rekordbox-common`, `rekordbox-export` | Rust crates shared by the plugins above |
| `beat-fx`, `mic-fx`, `send-fx`, `lever-fx`, `cross-pass`, `soft-mix`, `tutorial-fx` | effects (JSON) |
| `center-lock`, `gate-cue` | knob and pad modes (JSON) |
| `now-playing`, `mix-log` | events, actions, settings (Python, Lua) |
| `yt-dlp` | track sources: YouTube / SoundCloud search, playlists and downloads (Python, needs `yt-dlp`) |

### One stick for every player and app

The five device plugins write side by side on the same stick and share one copy of each audio file (`Contents/<artist>/<album>/<file>`). In the app's device window (also opened by the USB icon of a connected stick in the browser), the DEVICES tab asks what the stick is for: UNIVERSAL writes every format, DEDICATED only the ticked ones. Each format says which software and players read it and shows its options (rekordbox: OneLibrary on or off). By default the formats are those already on the stick, or all of them on an empty stick. A stick written by rekordbox is extended, not duplicated, because its audio files are found by name and size. None of the four new formats has been tried on real hardware or in the vendor apps yet: please report what does not load.

## Build from source

The native plugins form one Cargo workspace built against the SDK of the core repository, found at `../../sdk`. Check this repository out as `extension/plugin/` inside a core checkout (the core's `start.sh` does it), then:

```sh
./build.sh        # cargo build --release, then each library is copied next to its plugin.json
```

The app finds `extension/plugin/` next to its binary. Elsewhere, copy a plugin folder into `~/.config/openfader/extension/plugin/`, drop it on the app window, or list folders in `OPENFADER_PLUGINS`. Keys of encrypted libraries are plugin settings (PREFERENCES > EXTENSIONS), never environment variables.

The plugin format and the library / device ABI are documented in the core repository's [docs/plugins.md](https://github.com/OpenRsG/OpenFadeR/blob/main/docs/plugins.md) and `sdk/README.md`.

## Releases

Pushing a tag `v*` makes CI build the native plugins on Linux, Windows and macOS and attach one `<plugin>-<os>-<arch>.zip` per plugin to the release (`<os>`: `linux`, `windows`, `macos`; `<arch>`: `x86_64`, `aarch64`), each holding the library file the plugin's `plugin.json` names. `package.py` makes them locally after `build.sh`.

## Contributing and license

Contributions of any size are welcome. For anything more than a small fix, open an issue first. [CONTRIBUTING.md](CONTRIBUTING.md) shows how to build inside a core checkout and which tests to run. The [code of conduct](CODE_OF_CONDUCT.md) applies, and security reports go through [SECURITY.md](SECURITY.md). Related repositories: [OpenFadeR](https://github.com/OpenRsG/OpenFadeR), [OpenFadeR-Themes](https://github.com/OpenRsG/OpenFadeR-Themes) and [OpenFadeR-Map](https://github.com/OpenRsG/OpenFadeR-Map).

AGPL-3.0, see [LICENSE](LICENSE). Made by [OpenRsG](https://github.com/OpenRsG).
