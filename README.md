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
| `rekordbox-device` | library + device: rekordbox Device Library (`export.pdb`, ANLZ files) and OneLibrary (`exportLibrary.db`, key as a setting) on USB sticks and SD cards: read, export, update, remove, convert, player settings, backups |
| `rekordbox-db` | library: the rekordbox desktop collection (`master.db`, smart playlists; key and path map as settings) |
| `itunes` | library: an iTunes / Music `Library.xml` |
| `sample-pack` | library: sampler packs (`.spp`) |
| `rekordbox-common`, `rekordbox-export` | Rust crates shared by the plugins above |
| `beat-fx`, `mic-fx`, `send-fx`, `lever-fx`, `cross-pass`, `soft-mix`, `tutorial-fx` | effects (JSON) |
| `center-lock`, `gate-cue` | knob and pad modes (JSON) |
| `now-playing`, `mix-log` | events, actions, settings (Python, Lua) |
| `yt-dlp` | track sources: YouTube / SoundCloud search, playlists and downloads (Python, needs `yt-dlp`) |

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
