# OpenFadeR-Plugins

Plugins for [OpenFadeR](https://github.com/OpenRsG/OpenFadeR): one folder per plugin, each with its `plugin.json`.

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
| `now-playing` | events, actions, settings (Python) |
| `yt-dlp` | track sources: YouTube / SoundCloud search, playlists and downloads (Python, needs `yt-dlp`) |

## Build and install

The native plugins are Rust crates of one Cargo workspace built against the SDK of the core repository, found at `../../sdk`:
check this repository out as `extension/plugin/` inside a core checkout (the core's `start.sh` does it), then

```sh
./build.sh        # cargo build --release, then each library is copied next to its plugin.json
```

The app finds `extension/plugin/` next to its binary. Elsewhere, copy a plugin folder into
`~/.config/openfader/extension/plugin/`, drop it on the app window, or list folders in `OPENFADER_PLUGINS`. Keys of encrypted
libraries are plugin settings (PREFERENCES > EXTENSIONS), never environment variables. The plugin format and the library / device
ABI are documented in the core repository's `docs/plugins.md` and `sdk/README.md`.

## License

AGPL-3.0, see `LICENSE`.
