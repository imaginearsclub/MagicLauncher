"""Tests for channel_feeds.py. Run: python -I -m unittest discover -s scripts -p "test_*.py" """

import os
import sys
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from channel_feeds import compare, channel_of, targets  # noqa: E402


class Compare(unittest.TestCase):
    def test_numeric_prerelease_identifiers(self):
        self.assertEqual(compare("0.3.0-alpha.10", "0.3.0-alpha.9"), 1)

    def test_release_beats_its_prereleases(self):
        self.assertEqual(compare("1.0.0", "1.0.0-beta.1"), 1)
        self.assertEqual(compare("1.0.0-beta.1", "1.0.0"), -1)

    def test_alpha_before_beta(self):
        self.assertEqual(compare("0.3.0-alpha.5", "0.3.0-beta.1"), -1)

    def test_semver_spec_example_chain(self):
        chain = ["1.0.0-alpha", "1.0.0-alpha.1", "1.0.0-alpha.beta", "1.0.0-beta",
                 "1.0.0-beta.2", "1.0.0-beta.11", "1.0.0-rc.1", "1.0.0"]
        for lo, hi in zip(chain, chain[1:]):
            self.assertEqual(compare(lo, hi), -1, (lo, hi))

    def test_equal_and_v_prefix(self):
        self.assertEqual(compare("v0.2.0", "0.2.0"), 0)


class Routing(unittest.TestCase):
    def test_channels(self):
        self.assertEqual(channel_of("0.2.0"), "release")
        self.assertEqual(channel_of("0.2.1-beta.1"), "beta")
        self.assertEqual(channel_of("0.3.0-alpha.1"), "alpha")
        self.assertEqual(channel_of("0.3.0-rc.1"), "beta")

    def test_release_goes_everywhere_when_newer(self):
        cur = {"release": "0.1.1", "beta": None, "alpha": None}
        self.assertEqual(targets("0.2.0", cur), ["release", "beta", "alpha"])

    def test_beta_never_touches_release(self):
        cur = {"release": "0.2.0", "beta": "0.2.0", "alpha": "0.2.0"}
        self.assertEqual(targets("0.2.1-beta.1", cur), ["beta", "alpha"])
        self.assertEqual(targets("0.2.1-beta.1", cur, force=True), ["beta", "alpha"])

    def test_alpha_only_alpha(self):
        cur = {"release": "0.2.0", "beta": "0.2.0", "alpha": "0.2.0"}
        self.assertEqual(targets("0.3.0-alpha.1", cur), ["alpha"])

    def test_hotfix_does_not_lower_test_feeds(self):
        cur = {"release": "0.2.0", "beta": "0.3.0-beta.1", "alpha": "0.3.0-alpha.4"}
        self.assertEqual(targets("0.2.1", cur), ["release"])

    def test_missing_feed_is_written(self):
        cur = {"release": "0.2.0", "beta": None, "alpha": "0.3.0-alpha.1"}
        self.assertEqual(targets("0.2.1-beta.1", cur), ["beta"])

    def test_force_allows_rollback(self):
        cur = {"release": "0.2.1", "beta": "0.2.1", "alpha": "0.2.1"}
        self.assertEqual(targets("0.2.0", cur), [])
        self.assertEqual(targets("0.2.0", cur, force=True), ["release", "beta", "alpha"])

    def test_outdated_release_goes_nowhere(self):
        cur = {"release": "0.1.1", "beta": None, "alpha": None}
        self.assertEqual(targets("0.1.0", cur), [])

    def test_rerun_same_version(self):
        cur = {"release": "0.2.0", "beta": "0.2.0", "alpha": "0.2.0"}
        self.assertEqual(targets("0.2.0", cur), ["release", "beta", "alpha"])


if __name__ == "__main__":
    unittest.main()
