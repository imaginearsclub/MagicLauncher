#!/usr/bin/env python3
"""Build the Alpha allowlist (launcher/channels/alpha.json) from channels/alpha-testers.txt.

    build_allowlist.py channels/alpha-testers.txt > alpha.json

Input: one Minecraft name (or UUID) per line, optionally `until YYYY-MM-DD`; `#` comments.
Names are resolved to UUIDs with Mojang's public API (UUIDs never change; names can).
Output format is fixed (crates/launcher-core/src/allowlist.rs) so a Discord bot or the staff
web panel can produce the same file later. The workflow signs the exact bytes printed here.
"""

import calendar
import datetime
import json
import os
import re
import sys
import time
import urllib.error
import urllib.request

UUID_RE = re.compile(r"^[0-9a-fA-F]{8}-?[0-9a-fA-F]{4}-?[0-9a-fA-F]{4}-?[0-9a-fA-F]{4}-?[0-9a-fA-F]{12}$")
NAME_RE = re.compile(r"^[A-Za-z0-9_]{1,16}$")


def parse_lines(text):
    """[(name_or_uuid, expires_at_or_None)]; raises ValueError on a bad line."""
    out = []
    for n, raw in enumerate(text.splitlines(), 1):
        line = raw.split("#", 1)[0].strip()
        if not line:
            continue
        parts = line.split()
        who, expires = parts[0], None
        if len(parts) == 3 and parts[1].lower() == "until":
            day = datetime.date.fromisoformat(parts[2])
            # Valid through the whole given day (UTC).
            expires = calendar.timegm((day + datetime.timedelta(days=1)).timetuple())
        elif len(parts) != 1:
            raise ValueError(f"line {n}: expected 'Name' or 'Name until YYYY-MM-DD', got {raw!r}")
        if not (UUID_RE.match(who) or NAME_RE.match(who)):
            raise ValueError(f"line {n}: {who!r} is not a Minecraft name or UUID")
        out.append((who, expires))
    return out


def _get_json(url):
    req = urllib.request.Request(url, headers={"User-Agent": "MagicLauncher-allowlist"})
    for attempt in range(4):
        try:
            with urllib.request.urlopen(req, timeout=20) as r:
                body = r.read()
                return json.loads(body) if body else None
        except urllib.error.HTTPError as e:
            if e.code in (204, 404):
                return None
            if e.code == 429 and attempt < 3:
                time.sleep(5 * (attempt + 1))
                continue
            raise
    return None


def resolve(who):
    """(uuid without dashes, current name)."""
    if UUID_RE.match(who):
        uuid = who.replace("-", "").lower()
        prof = _get_json(f"https://sessionserver.mojang.com/session/minecraft/profile/{uuid}")
        if not prof:
            raise SystemExit(f"No Minecraft profile with UUID {who}")
        return uuid, prof["name"]
    prof = _get_json(f"https://api.mojang.com/users/profiles/minecraft/{who}")
    if not prof:
        raise SystemExit(f"No Minecraft account named {who!r}")
    return prof["id"].lower(), prof["name"]


def build(entries, resolver=resolve, now=None, issued_by=None):
    members, seen = [], set()
    for who, expires in entries:
        uuid, name = resolver(who)
        if uuid in seen:
            continue
        seen.add(uuid)
        members.append({"uuid": uuid, "name": name, "added_by": None, "added_at": None,
                        "expires_at": expires})
    return {"format": 1, "channel": "alpha", "issued_at": int(now if now is not None else time.time()),
            "issued_by": issued_by, "members": members}


def main(argv):
    if len(argv) != 1:
        sys.exit(__doc__)
    with open(argv[0], encoding="utf-8") as f:
        entries = parse_lines(f.read())
    doc = build(entries, issued_by=os.environ.get("GITHUB_ACTOR"))
    sys.stdout.write(json.dumps(doc, separators=(",", ":")))
    print(f"{len(doc['members'])} alpha tester(s): {', '.join(m['name'] for m in doc['members'])}",
          file=sys.stderr)


if __name__ == "__main__":
    main(sys.argv[1:])
