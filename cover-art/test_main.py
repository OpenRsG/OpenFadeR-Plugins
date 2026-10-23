"""Tests: python3 -m unittest cover-art/test_main.py (the network test is skipped offline)."""
import os
import socket
import sys
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import main


def online():
    try:
        socket.create_connection(("itunes.apple.com", 443), timeout=3).close()
        return True
    except OSError:
        return False


class Offline(unittest.TestCase):
    def test_phrase_is_escaped(self):
        self.assertEqual(main.lucene('a "b" \\'), '"a \\"b\\" \\\\"')

    def test_unknown_op_is_an_error(self):
        with self.assertRaises(main.Failure):
            main.handle({"op": "search"})

    def test_providers_are_skipped_not_fatal(self):
        saved = main.itunes, main.musicbrainz
        try:
            def down(q):
                raise main.Failure("no network")
            main.musicbrainz = down
            main.itunes = lambda q: [{"url": "https://x/y.jpg", "source": "iTunes"}]
            got = main.candidates({"artist": "a", "album": "b", "title": ""}, {})
            self.assertEqual([c["source"] for c in got], ["iTunes"])
            main.itunes = down
            with self.assertRaises(main.Failure):
                main.candidates({"artist": "a", "album": "b", "title": ""}, {})
        finally:
            main.itunes, main.musicbrainz = saved

    def test_discogs_needs_a_token(self):
        self.assertEqual(main.discogs({"artist": "a", "album": "b", "title": ""}, ""), [])


@unittest.skipUnless(online(), "offline")
class Online(unittest.TestCase):
    def test_a_famous_album_has_covers(self):
        got = main.handle({"op": "artwork", "artist": "Daft Punk", "album": "Discovery", "title": "", "settings": {}})
        self.assertTrue(got["candidates"])
        self.assertTrue(all(c["url"].startswith("http") for c in got["candidates"]))


if __name__ == "__main__":
    unittest.main()
