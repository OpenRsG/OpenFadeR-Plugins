#!/usr/bin/env python3
"""OpenFadeR plugin: YouTube and SoundCloud through the yt-dlp program (protocol: docs/plugins.md).

One request as JSON on stdin, one reply as JSON on stdout. Needs python3 and yt-dlp; ffmpeg turns streams the decks
cannot play (opus, webm) into AAC, without it only streams that already are m4a or mp3 can be fetched.

Track ids: the video id on YouTube, the path of the track page (`artist/track`) on SoundCloud.
Playlists: the file playlists.json next to this script lists addresses to show under each source; edit it freely.
"""
import json
import os
import shutil
import subprocess
import sys
import tempfile
from urllib.parse import urlparse

HERE = os.path.dirname(os.path.abspath(__file__))
# The plugin's settings (plugin manager), set from each request: "results" (tracks a search or a playlist shows) and
# "format" (m4a, mp3, or "best": the best stream the decks play, never converted).
SETTINGS = {"results": 25, "format": "m4a"}


def results():
    return max(1, min(500, int(SETTINGS.get("results") or 25)))


def convert_args(download=False):
    """yt-dlp arguments that turn the stream into the chosen format (ffmpeg), or pick a playable one without it."""
    fmt = SETTINGS.get("format", "m4a")
    if fmt == "best" or not shutil.which("ffmpeg"):
        return ["-f", PLAYABLE]
    quality = [] if download else ["--audio-quality", "192K"]
    return ["-f", "bestaudio/best", "-x", "--audio-format", fmt, *quality, *(["--embed-metadata"] if download else [])]

SOURCES = {
    "youtube": {"search": "ytsearch{n}:{q}", "page": "https://www.youtube.com/watch?v={id}"},
    "soundcloud": {"search": "scsearch{n}:{q}", "page": "https://soundcloud.com/{id}"},
}

# Streams the decks decode, best first, for when ffmpeg cannot convert the others.
PLAYABLE = "bestaudio[ext=m4a]/bestaudio[ext=mp3]"


class Failure(Exception):
    """Sent back as {"error": …}."""


def ytdlp(args, timeout=120):
    if not shutil.which("yt-dlp"):
        raise Failure("yt-dlp is not installed (https://github.com/yt-dlp/yt-dlp)")
    try:
        done = subprocess.run(["yt-dlp", "--no-warnings", *args], capture_output=True, text=True, timeout=timeout)
    except subprocess.TimeoutExpired:
        raise Failure(f"yt-dlp did not answer within {timeout} s")
    return done


def last_line(text):
    lines = [l for l in text.strip().splitlines() if l.strip()]
    return lines[-1] if lines else "no message"


def entries(target):
    """The entries yt-dlp lists for a search or a playlist address, without resolving each one."""
    done = ytdlp(["--flat-playlist", "--playlist-end", str(results()), "--dump-json", "--ignore-errors", target])
    rows = []
    for line in done.stdout.splitlines():
        try:
            rows.append(json.loads(line))
        except ValueError:
            pass
    if not rows and done.returncode != 0:
        raise Failure(last_line(done.stderr))
    return rows


def track_id(source, e):
    if source == "youtube":
        return e.get("id")
    page = e.get("webpage_url") or e.get("url") or ""
    return urlparse(page).path.strip("/") if "soundcloud.com" in page else None


def track(source, e):
    ident = track_id(source, e)
    if not ident or not e.get("title"):
        return None
    title = e["title"]
    artist = (e.get("uploader") or e.get("channel") or "").removesuffix(" - Topic")
    if source == "soundcloud":
        artist = artist or ident.split("/")[0]  # flat playlist entries have no uploader: the user's name in the address
    elif " - " in title:  # "Artist - Title" is how music is named on YouTube
        artist, title = title.split(" - ", 1)
    out = {"id": ident, "title": title, "artist": artist}
    if e.get("duration"):
        out["length"] = e["duration"]
    if e.get("genres"):
        out["genre"] = e["genres"][0]
    if e.get("webpage_url") or e.get("url"):
        out["url"] = e.get("webpage_url") or e["url"]
    return out


def tracks(source, target):
    return {"tracks": [t for t in (track(source, e) for e in entries(target)) if t]}


def playlists(source):
    try:
        with open(os.path.join(HERE, "playlists.json"), encoding="utf-8") as f:
            return {"playlists": json.load(f).get(source, [])}
    except FileNotFoundError:
        return {"playlists": []}


def fetch(source, ident, dest, name):
    """Downloads the track into `dest` as `<name>.<ext>`; the file appears there only when it is complete."""
    work = tempfile.mkdtemp(prefix=f".part-{name}-", dir=dest)
    try:
        convert = convert_args()
        done = ytdlp(
            ["--no-playlist", "--no-progress", *convert,
             "-o", os.path.join(work, name + ".%(ext)s"), SOURCES[source]["page"].format(id=ident)],
            timeout=600,
        )
        made = [f for f in os.listdir(work) if f.startswith(name + ".") and not f.endswith((".part", ".ytdl"))]
        if done.returncode != 0 or not made:
            hint = " (without ffmpeg only m4a and mp3 streams can be fetched)" if "-x" not in convert else ""
            raise Failure(last_line(done.stderr) + hint)
        os.replace(os.path.join(work, made[0]), os.path.join(dest, made[0]))
        return {"file": made[0]}
    finally:
        shutil.rmtree(work, ignore_errors=True)


def download(address, folder):
    """The action: downloads a track or playlist into `folder` and has the app import it."""
    if not folder:
        raise Failure("the app gave no folder to download into")
    os.makedirs(folder, exist_ok=True)
    convert = convert_args(download=True)
    done = ytdlp(
        ["--no-progress", "--yes-playlist", *convert, "-o", os.path.join(folder, "%(uploader)s - %(title)s.%(ext)s"),
         "--print", "after_move:filepath", address],
        timeout=1800,
    )
    files = [l for l in done.stdout.splitlines() if os.path.isfile(l)]
    if not files:
        raise Failure(last_line(done.stderr))
    return {"commands": [
        {"cmd": "import", "files": files},
        {"cmd": "toast", "text": f"yt-dlp: {len(files)} track{'s' if len(files) != 1 else ''} added to the collection"},
    ]}


def handle(req):
    op, source = req.get("op"), req.get("source")
    SETTINGS.update(req.get("settings") or {})
    if op == "action":
        if req.get("action") != "download":
            raise Failure(f"unknown action {req.get('action')!r}")
        if not (req.get("input") or "").strip():
            raise Failure("no address given")
        where = (req.get("context") or {}).get("dir")
        return download(req["input"].strip(), where and os.path.join(where, "downloads"))
    if source not in SOURCES:
        raise Failure(f"unknown source {source!r}")
    if op == "playlists":
        return playlists(source)
    if op == "tracks":
        return tracks(source, req["playlist"])
    if op == "search":
        return tracks(source, SOURCES[source]["search"].format(n=results(), q=req["query"]))
    if op == "fetch":
        return fetch(source, req["id"], req["dir"], req["name"])
    raise Failure(f"unknown op {op!r}")


if __name__ == "__main__":
    try:
        reply = handle(json.load(sys.stdin))
    except (Failure, KeyError) as e:
        reply = {"error": f"missing {e}" if isinstance(e, KeyError) else str(e)}
    json.dump(reply, sys.stdout)
