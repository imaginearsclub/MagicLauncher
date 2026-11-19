#!/usr/bin/env python3
"""Decide which launcher update feeds a published version should be written to.

    channel_feeds.py VERSION RELEASE_CUR BETA_CUR ALPHA_CUR [--force]

Each *_CUR is the version currently in that feed, or "-" if the feed doesn't exist.
Prints the feeds to write (release / beta / alpha), one per line.

Rules (docs/roadmap.md):
  x.y.z          -> release, beta, alpha
  x.y.z-beta.N   -> beta, alpha      (any pre-release other than alpha.* counts as beta)
  x.y.z-alpha.N  -> alpha
A feed is only written if the new version is >= its current one (SemVer 2.0.0 precedence),
so a feed never moves backwards; --force skips that check (manual rollback).
A pre-release can never be written to the release feed, even with --force.
"""

import sys

ORDER = ["release", "beta", "alpha"]


def parse(v):
    v = v.strip().lstrip("v").split("+", 1)[0]  # build metadata doesn't affect precedence
    core, _, pre = v.partition("-")
    parts = core.split(".")
    if len(parts) != 3 or not all(p.isdigit() for p in parts):
        raise ValueError(f"not a SemVer version: {v!r}")
    return tuple(int(p) for p in parts), (pre.split(".") if pre else [])


def _cmp_ident(a, b):
    an, bn = a.isdigit(), b.isdigit()
    if an and bn:
        return (int(a) > int(b)) - (int(a) < int(b))
    if an != bn:
        return -1 if an else 1  # numeric identifiers sort before alphanumeric
    return (a > b) - (a < b)


def compare(a, b):
    """-1, 0 or 1 by SemVer 2.0.0 precedence."""
    (ac, ap), (bc, bp) = parse(a), parse(b)
    if ac != bc:
        return (ac > bc) - (ac < bc)
    if not ap or not bp:  # a release outranks its pre-releases
        return (not ap) - (not bp)
    for x, y in zip(ap, bp):
        c = _cmp_ident(x, y)
        if c:
            return c
    return (len(ap) > len(bp)) - (len(ap) < len(bp))


def channel_of(version):
    _, pre = parse(version)
    if not pre:
        return "release"
    return "alpha" if pre[0].startswith("alpha") else "beta"


def targets(version, current, force=False):
    ch = channel_of(version)
    eligible = ORDER[ORDER.index(ch):]
    out = []
    for feed in eligible:
        cur = current.get(feed)
        if force or cur is None or compare(version, cur) >= 0:
            out.append(feed)
    # An outdated release (older than the live Release) goes nowhere, not even into
    # empty test feeds.
    if ch == "release" and "release" not in out:
        return []
    return out


def main(argv):
    args = [a for a in argv if a != "--force"]
    if len(args) != 4:
        sys.exit(__doc__)
    version, *curs = args
    current = {f: (None if c in ("", "-") else c) for f, c in zip(ORDER, curs)}
    for feed in targets(version, current, force="--force" in argv):
        print(feed)


if __name__ == "__main__":
    main(sys.argv[1:])
