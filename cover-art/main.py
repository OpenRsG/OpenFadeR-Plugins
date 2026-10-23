#!/usr/bin/env python3
"""OpenFadeR plugin: cover art from MusicBrainz + the Cover Art Archive, the iTunes Search API and (with a token) Discogs.

Protocol: docs/plugins.md, "Artwork". The request is {"op": "artwork", "artist", "album", "title", "duration", "settings"};
the reply {"candidates": [{"url", "thumb", "size", "source"}]}, best first. Only documented public APIs are used.
"""
import json
import re
import sys
import time
import urllib.parse
import urllib.request

AGENT = "OpenFadeR-cover-art/1.0 (+https://github.com/OpenRsG/OpenFadeR)"
TIMEOUT = 20
SETTINGS = {"musicbrainz": True, "itunes": True, "discogs_token": ""}
MB_RELEASES = 3  # releases whose pictures are looked up: each one costs a request, MusicBrainz allows one a second
_last_mb = [0.0]


class Failure(Exception):
    """Sent back as {"error": …}."""


def get(url, mb=False):
    """The JSON at `url`, or None for a 404 (no picture / no such release). MusicBrainz gets one request a second."""
    if mb:
        wait = 1.1 - (time.monotonic() - _last_mb[0])
        if wait > 0:
            time.sleep(wait)
        _last_mb[0] = time.monotonic()
    req = urllib.request.Request(url, headers={"User-Agent": AGENT, "Accept": "application/json"})
    try:
        with urllib.request.urlopen(req, timeout=TIMEOUT) as r:
            return json.loads(r.read().decode("utf-8"))
    except urllib.error.HTTPError as e:
        if e.code in (404, 400):
            return None
        raise Failure(f"{url}: HTTP {e.code}")
    except (urllib.error.URLError, OSError, ValueError) as e:
        raise Failure(f"{url}: {e}")


def lucene(s):
    """`s` as a quoted Lucene phrase (MusicBrainz query syntax)."""
    return '"' + re.sub(r'(["\\])', r"\\\1", s) + '"'


def musicbrainz(q):
    """Front pictures of the releases matching the album (or, without one, the title) of the artist."""
    if q["album"]:
        query = f"release:{lucene(q['album'])}"
        what = "release"
    elif q["title"]:
        query = f"recording:{lucene(q['title'])}"
        what = "recording"
    else:
        return []
    if q["artist"]:
        query += f" AND artist:{lucene(q['artist'])}"
    found = get("https://musicbrainz.org/ws/2/%s?%s" % (what, urllib.parse.urlencode({"query": query, "fmt": "json", "limit": 8})), mb=True)
    rows = (found or {}).get("releases", [])
    if what == "recording":
        rows = [r for rec in (found or {}).get("recordings", []) for r in rec.get("releases", [])]
    out, seen = [], set()
    for rel in rows:
        mbid = rel.get("id")
        if not mbid or mbid in seen:
            continue
        seen.add(mbid)
        if len(seen) > MB_RELEASES:
            break
        art = get(f"https://coverartarchive.org/release/{mbid}")  # the archive is not rate limited like the search
        for img in (art or {}).get("images", []):
            if img.get("front") and img.get("image"):
                thumbs = img.get("thumbnails", {})
                out.append({"url": img["image"], "thumb": thumbs.get("250") or thumbs.get("small"), "source": "MusicBrainz"})
                break
    return out


def itunes(q):
    album = bool(q["album"])
    term = " ".join(x for x in (q["artist"], q["album"] if album else q["title"]) if x)
    if not term:
        return []
    found = get("https://itunes.apple.com/search?" + urllib.parse.urlencode(
        {"term": term, "media": "music", "entity": "album" if album else "song", "limit": 6}))
    out, seen = [], set()
    for r in (found or {}).get("results", []):
        small = r.get("artworkUrl100")
        if not small or small in seen:
            continue
        seen.add(small)
        # inferred: the size in the address is a request (any size up to the original), 100x100bb → 1000x1000bb
        out.append({"url": re.sub(r"\d+x\d+bb", "1000x1000bb", small), "thumb": small, "size": 1000, "source": "iTunes"})
    return out


def discogs(q, token):
    term = " ".join(x for x in (q["artist"], q["album"] or q["title"]) if x)
    if not term or not token:
        return []
    found = get("https://api.discogs.com/database/search?" + urllib.parse.urlencode(
        {"q": term, "type": "release", "per_page": 5, "token": token}))
    return [{"url": r["cover_image"], "thumb": r.get("thumb") or None, "source": "Discogs"}
            for r in (found or {}).get("results", [])
            if (r.get("cover_image") or "").startswith("http") and "spacer.gif" not in r["cover_image"]]


def candidates(q, settings):
    """Every provider's pictures, one after the other; a provider that fails is skipped unless none of them answered."""
    SETTINGS.update(settings or {})
    steps = []
    if SETTINGS.get("musicbrainz", True):
        steps.append(musicbrainz)
    if SETTINGS.get("itunes", True):
        steps.append(itunes)
    if (SETTINGS.get("discogs_token") or "").strip():
        steps.append(lambda x: discogs(x, SETTINGS["discogs_token"].strip()))
    out, errors, answered = [], [], 0
    for step in steps:
        try:
            out += step(q)
            answered += 1
        except Failure as e:
            errors.append(str(e))
    if errors and not answered:
        raise Failure(errors[0])
    return out


def handle(req):
    if req.get("op") != "artwork":
        raise Failure(f"unknown op {req.get('op')!r}")
    q = {k: str(req.get(k) or "").strip() for k in ("artist", "album", "title")}
    return {"candidates": candidates(q, req.get("settings"))}


if __name__ == "__main__":
    try:
        reply = handle(json.loads(sys.stdin.buffer.read().decode("utf-8")))
    except Failure as e:
        reply = {"error": str(e)}
    json.dump(reply, sys.stdout)
