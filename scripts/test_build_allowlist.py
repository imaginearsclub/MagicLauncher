"""Tests for build_allowlist.py (no network)."""

import os
import sys
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from build_allowlist import build, parse_lines  # noqa: E402


class Parse(unittest.TestCase):
    def test_names_comments_and_expiry(self):
        text = """
        # staff
        Notch            # owner
        jeb_ until 2027-01-31
        069a79f4-44e9-4726-a5be-fca90e38aaf5
        """
        got = parse_lines(text)
        self.assertEqual(got[0], ("Notch", None))
        self.assertEqual(got[1], ("jeb_", 1801440000))  # end of 2027-01-31 UTC
        self.assertEqual(got[2][0], "069a79f4-44e9-4726-a5be-fca90e38aaf5")

    def test_rejects_garbage(self):
        with self.assertRaises(ValueError):
            parse_lines("not a valid name!")
        with self.assertRaises(ValueError):
            parse_lines("Notch forever")


class Build(unittest.TestCase):
    def test_format_and_dedup(self):
        fake = {"Notch": ("069a79f444e94726a5befca90e38aaf5", "Notch"),
                "notch": ("069a79f444e94726a5befca90e38aaf5", "Notch")}
        doc = build([("Notch", None), ("notch", 5)], resolver=fake.__getitem__, now=1, issued_by="ci")
        self.assertEqual(doc["format"], 1)
        self.assertEqual(doc["channel"], "alpha")
        self.assertEqual(doc["issued_at"], 1)
        self.assertEqual(len(doc["members"]), 1)
        self.assertEqual(doc["members"][0]["uuid"], "069a79f444e94726a5befca90e38aaf5")


if __name__ == "__main__":
    unittest.main()
