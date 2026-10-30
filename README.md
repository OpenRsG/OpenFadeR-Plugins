<p align="center">
  <img src="https://raw.githubusercontent.com/OpenRsG/OpenFadeR/main/assets/openfader.png" alt="OpenFadeR logo" width="160">
</p>

<h1 align="center">OpenFadeR-Plugins</h1>

<p align="center">
  <a href="LICENSE"><img alt="License: AGPL-3.0" src="https://img.shields.io/github/license/OpenRsG/OpenFadeR-Plugins"></a>
</p>

Plugins for [OpenFadeR](https://github.com/OpenRsG/OpenFadeR), the open-source DJ app: libraries, devices, effects, pad and knob modes, events and track sources. One folder per plugin, each with its `plugin.json`. Native plugins are Rust crates, the rest are JSON or Python.

OpenFadeR's format adapters (Rekordbox, Traktor, Engine DJ, Serato, VirtualDJ and iTunes / Music) live under `compatibility/` in the core repository and are compiled into the app: nothing from this repository is needed for them. Rekordbox is always on; the others are optional and start on for a fresh profile (iTunes / Music stays off until switched on), keeping any activation, removal or settings choice made earlier. Their old plugin folders are ignored by name. The `sample-pack` plugin shares helpers from the core's `compat-rekordbox-common` package; Mixxx remains here.

## Install in one click

In OpenFadeR open PREFERENCES > EXTENSIONS > GET PLUGINS. This repository is in the list on first run. INSTALL installs one plugin, INSTALL ALL all of them. Native plugins come prebuilt from this repository's latest release.

## What is here

| Plugin | Kind |
|---|---|
| `sample-pack` | library: sampler packs (`.spp`) |
| `mixxx` | controller driver: Mixxx `.midi.xml` mappings and their JavaScript (not a collection reader) |
| `beat-fx`, `mic-fx`, `send-fx`, `lever-fx`, `cross-pass`, `soft-mix`, `tutorial-fx` | effects (JSON) |
| `center-lock`, `gate-cue` | knob and pad modes (JSON) |
| `now-playing` | events, actions, settings (Python) |
| `yt-dlp` | track sources: YouTube / SoundCloud search, playlists and downloads (Python, needs `yt-dlp`; on Windows `winget install -e --id yt-dlp.yt-dlp`, which brings ffmpeg too) |
| `cover-art` | artwork: cover candidates from MusicBrainz + Cover Art Archive, iTunes Search and (with a token) Discogs (Python) |

## Build from source

The native plugins form one Cargo workspace built against the core SDK and `compat-rekordbox-common`, found at `../../sdk` and `../../compatibility/rekordbox/common`. This companion's `v0.2.12` tag pairs with core 0.2.9 (the format adapters moved into the core; `sample-pack` and `mixxx` remain here); `v0.2.11` pairs with core 0.2.8, `v0.2.8` with core 0.2.7; older core checkouts use their matching companion revision. Check this repository out as `extension/plugin/` inside a core checkout (`start.sh` clones the matching tag on first use), then:

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
