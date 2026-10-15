#!/usr/bin/env python3
"""OpenFadeR plugin: the track on air as a text file for stream overlays (protocol: docs/plugins.md).

Events: `deck_play` (or `track_loaded`, setting "on_play" off) writes the track to now-playing.txt; `track_played` (the
track stayed on air long enough for the play history) appends a timestamped line to setlist.txt. Both files live in the
"folder" setting, by default <data dir>/openfader/plugins/now-playing/ (the data dir is $XDG_DATA_HOME or ~/.local/share;
$NOW_PLAYING_DIR overrides both). Actions: "Show now playing" toasts the track of the first playing deck, else the last
one written; "Open the now-playing folder" opens it.
"""
import json
import os
import sys
import time


def folder(settings):
    if os.environ.get("NOW_PLAYING_DIR"):
        return os.environ["NOW_PLAYING_DIR"]
    if settings.get("folder"):
        return os.path.expanduser(settings["folder"])
    data = os.environ.get("XDG_DATA_HOME") or os.path.join(os.path.expanduser("~"), ".local", "share")
    return os.path.join(data, "openfader", "plugins", "now-playing")


def label(track, fmt):
    artist, title = track.get("artist"), track.get("title")
    if fmt == "Title":
        parts = (title,)
    elif fmt == "Title - Artist":
        parts = (title, artist)
    else:
        parts = (artist, title)
    return " - ".join(p for p in parts if p) or "unknown track"


def handle(req):
    settings = req.get("settings") or {}
    fmt = settings.get("format", "Artist - Title")
    out = folder(settings)
    now = os.path.join(out, "now-playing.txt")
    if req["op"] == "event":
        event, data = req["event"], req["data"]
        track = data.get("track")
        on_play = settings.get("on_play", True)
        if track and ((event == "deck_play" and on_play) or (event == "track_loaded" and not on_play)):
            os.makedirs(out, exist_ok=True)
            with open(now, "w", encoding="utf-8") as f:
                f.write(label(track, fmt) + "\n")
        elif track and event == "track_played":
            os.makedirs(out, exist_ok=True)
            with open(os.path.join(out, "setlist.txt"), "a", encoding="utf-8") as f:
                f.write(f"{time.strftime('%H:%M')}  {label(track, fmt)}\n")
        return {}
    if req["op"] == "action":
        if req["action"] == "folder":
            os.makedirs(out, exist_ok=True)
            return {"commands": [{"cmd": "open", "url": out}]}
        for deck in req["context"].get("decks", []):
            if deck.get("playing") and deck.get("track"):
                return {"commands": [{"cmd": "toast", "text": f"Deck {deck['deck']}: {label(deck['track'], fmt)}"}]}
        try:
            with open(now, encoding="utf-8") as f:
                text = f.read().strip()
        except FileNotFoundError:
            text = ""
        return {"commands": [{"cmd": "toast", "text": text or "nothing has been played yet"}]}
    return {"error": f"unknown op {req['op']!r}"}


if __name__ == "__main__":
    json.dump(handle(json.load(sys.stdin)), sys.stdout)
