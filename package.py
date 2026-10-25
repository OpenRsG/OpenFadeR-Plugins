#!/usr/bin/env python3
"""Packs each native plugin's built library (after build.sh) as dist/<plugin>-<os>-<arch>.zip, the release asset the app's
one-click install downloads (<os>, <arch> as Rust's std::env::consts::OS / ARCH: linux|windows|macos, x86_64|aarch64).
PACKAGE_ARCH overrides the arch of the running machine (a cross-compiled build)."""
import json, os, pathlib, platform, sys, zipfile

OS = {"linux": "linux", "win32": "windows", "darwin": "macos"}[sys.platform]
ARCH = {"amd64": "x86_64", "x86_64": "x86_64", "arm64": "aarch64", "aarch64": "aarch64"}[(os.environ.get("PACKAGE_ARCH") or platform.machine()).lower()]
root = pathlib.Path(__file__).resolve().parent
dist = root / "dist"
dist.mkdir(exist_ok=True)
for manifest in sorted(root.glob("*/plugin.json")):
    m = json.loads(manifest.read_text())
    lib = (m.get("library") or {}).get("lib")
    lib = lib.get(OS) if isinstance(lib, dict) else lib
    if not lib:
        continue
    built = manifest.parent / lib
    if not built.is_file():
        sys.exit(f"{m['name']}: {lib} is not built (run build.sh first)")
    out = dist / f"{m['name']}-{OS}-{ARCH}.zip"
    with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as z:
        z.write(built, lib)
        # What the stick checker reads next to plugin.json (a device plugin's table of players).
        if (manifest.parent / "players.json").is_file():
            z.write(manifest.parent / "players.json", "players.json")
    print(out.name)
