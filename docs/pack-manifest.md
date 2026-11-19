# Pack manifest (`pack.json`, format 1)

The launcher downloads one JSON file on every launch and makes the game folder match it.
For Imaginears Club it is the **Imaginears Magic Pack**: Fabric on Minecraft 1.21.11 (the
server asks for 1.21.11 or newer), with performance, park and show mods (Sodium, Iris + shaders,
SmoothCoasters, LambDynamicLights, ...). The server is Paper, so no mod is required to join;
the pack is about the best park experience.

```json
{
  "format_version": 1,
  "id": "imaginears-magic-pack",
  "name": "Imaginears Magic Pack",
  "version": "1.0.0",
  "minecraft": "1.21.11",
  "loader": { "type": "fabric", "version": "0.20.1" },
  "recommended_memory_mb": 4096,
  "files": [
    {
      "path": "mods/sodium-fabric-0.7.2+mc1.21.11.jar",
      "sha1": "…",
      "size": 1300000,
      "url": "https://cdn.modrinth.com/data/…/sodium-fabric-0.7.2+mc1.21.11.jar",
      "policy": "managed"
    },
    {
      "path": "config/iris.properties",
      "sha1": "…",
      "size": 200,
      "url": "https://launcher.imaginears.club/files/config/iris.properties",
      "policy": "default"
    }
  ]
}
```

| Field | Meaning |
|---|---|
| `format_version` | Schema version. The launcher refuses formats it doesn't know and tells the player to update the launcher. Bump only for breaking schema changes. |
| `version` | **The pack's SemVer.** MAJOR = players must start fresh / configs incompatible; MINOR = mods added; PATCH = config or mod bug-fix updates. |
| `files[].path` | Relative to the game folder, forward slashes. `..`, absolute paths and backslashes are rejected. |
| `files[].policy` | `managed`: always kept identical, deleted when removed from the manifest. `default`: copied only if missing, then belongs to the player. |

## Sync rules

- A file is downloaded only when missing or when its SHA-1 differs.
- The launcher deletes **only** files it installed as `managed` in the previous
  version (tracked in `instance/.magiclauncher/pack-lock.json`). Saves, screenshots, and
  shader/resource packs the player added are never touched.

## What goes in the pack

| Content | How it ships |
|---|---|
| `mods/*.jar` | `managed`. Optional ones (shaders, dynamic lights) are `features` (below). |
| `shaderpacks/*.zip` | `managed`, part of the `shaders` feature. |
| `config/iris.properties` | `default`: preselects the club shader once; the player's later choice is kept. |
| `resourcepacks/*.zip` | `managed`. `options.txt` (which packs are on) is `default`. |
| `config/` | `default` (the player's afterwards). |
| `servers.dat` | `default`, with the Imaginears server listed. |
| Datapacks | **Not shipped.** Datapacks belong to a world; in multiplayer they run on the Paper server, so players don't need them. |

## Building it

The pack is made in a normal launcher instance and read by `pack-builder` /
`pack-publisher`. Supported instance folders:

- **Prism Launcher** (`mmc-pack.json` + `.minecraft/`) and **Modrinth App** profiles
  (recommended: most Fabric mods are on Modrinth).
- **CurseForge** (`minecraftinstance.json`).

```sh
cargo run -p pack-builder --release -- \
  --input "<instance folder>" \
  --out dist-pack \
  --pack-version 1.0.0 \
  --base-url https://launcher.imaginears.club
```

- Every jar/zip is looked up on Modrinth by hash. Matches point at Modrinth's CDN; the rest
  are copied to `dist-pack/files/` to be uploaded. The tool lists every self-hosted file:
  **check each license allows redistribution** before uploading (some CurseForge-only mods
  forbid it).
- Upload `dist-pack/pack.json` and `dist-pack/files/` to the base URL (or use the publisher).

## Publishing (production)

The live pack is served from **`https://launcher.imaginears.club`**: CloudFront distribution
`E2AZL86MSJLTJV` (also reachable as `d16088bhvzdl6m.cloudfront.net`) in front of the private
S3 bucket `magiclauncher` (us-east-1), with **origin path `/pack`**: S3 key `pack/pack.json`
is `https://launcher.imaginears.club/pack.json`, `pack/launcher/latest.json` is
`…/launcher/latest.json`. Only the distribution may read the bucket (origin access control).
TLS: ACM certificate for `launcher.imaginears.club` (us-east-1, RSA 2048, auto-renews).
DNS (Cloudflare, **DNS only**): `launcher` CNAME → `d16088bhvzdl6m.cloudfront.net`, plus the
ACM validation CNAME, which must stay so the certificate renews.

### For the pack owner: Imaginears PackPublisher

A window app (`Imaginears PackPublisher`, Windows installer) and a command-line
`pack-publisher.exe`; no Rust, AWS CLI or launcher source needed.

1. Update the pack in Prism / Modrinth App / CurseForge as usual.
2. Open the publisher. The first time it asks which instance is the pack (remembered
   afterwards) and for the owner's AWS access key (kept in the OS credential store).
3. It shows what changed compared with the live pack (mods added / removed / updated, other
   files) and suggests the version: **MAJOR** if mods were removed or Minecraft changed,
   **MINOR** if mods were added, **PATCH** otherwise.
4. Confirm. It uploads only new or changed files, then `pack.json`, then clears the
   CloudFront cache.

Useful flags (CLI): `--dry-run` (show changes, upload nothing, no keys needed), `--setup`
(re-enter keys), `--instance <folder>`, `--reupload-all` (repair the bucket),
`--bucket`, `--distribution-id`, `--base-url`, `--region`.

### What's new and maintenance

- Every publish asks for an optional **note for players** (or `--notes "…"`). Together with
  the mods added/removed/updated, it's written into `pack.json` as `changes` and shown in the
  launcher.
- `--maintenance "Park closed for a refurbishment, back at 18:00"` disables Play in every
  launcher with that message (allowlisted Alpha testers can still play to test).
  `--maintenance off` re-enables it. It writes only `launcher/status.json`.

### Optional features and graphics presets

`crates/pack-builder/magic-pack.toml` (built into the publisher) defines:

- **features**: groups of files by path pattern: `shaders` (Iris + shader packs) and
  `dynamic_lights` (LambDynamicLights);
- **presets**: High (everything), Balanced (no shaders), Low-end PC (no shaders or extras).

Players pick a preset in Settings, or single mods under **Custom**. Only **client-side** mods
may be optional, and a mod another mod requires must stay in its group. The publisher stops
with an error if that rule is broken. `--suggest-optional` lists more candidates; put mods you
decided to keep in `keep = [...]`.

To change the groups without a new publisher, put an edited `magic-pack.toml` in the root
of the instance; that copy wins over the built-in one.

### Test-channel packs (Beta / Alpha)

Launchers on the Beta or Alpha channel use `beta/pack.json` / `alpha/pack.json` if one is
published, otherwise the Release pack. Each channel has its own game folder on the player's
PC, so a test pack never touches their Release install.

```powershell
pack-publisher.exe --channel beta      # remembers a separate instance per channel
```

To retire a channel pack, delete its folder in the bucket; launchers on that channel fall back
to the Release pack.

The owner's IAM user needs only this policy (fill in `<bucket>`, `<account>`,
`<distribution>`):

```json
{
  "Version": "2012-10-17",
  "Statement": [
    { "Effect": "Allow", "Action": ["s3:PutObject", "s3:GetObject"],
      "Resource": ["arn:aws:s3:::<bucket>/pack/files/*",
                   "arn:aws:s3:::<bucket>/pack/pack.json",
                   "arn:aws:s3:::<bucket>/pack/beta/*",
                   "arn:aws:s3:::<bucket>/pack/alpha/*",
                   "arn:aws:s3:::<bucket>/pack/launcher/status.json"] },
    { "Effect": "Allow", "Action": "cloudfront:CreateInvalidation",
      "Resource": "arn:aws:cloudfront::<account>:distribution/<distribution>" }
  ]
}
```

## Launcher side

The launcher defaults to `https://launcher.imaginears.club/pack.json` (placeholder);
`MAGIC_PACK_URL` overrides it at build time. Debug builds also read it at runtime, so you can
test against a local server: `MAGIC_PACK_URL=http://localhost:8000/pack.json pnpm tauri dev`.

## Compatibility rules (keep these after 1.0)

Launchers in the wild are older than the pack, and players can go back a launcher version.

**`pack.json`** (`format_version`, currently 1):
- **Allowed without a bump:** new *optional* fields (`#[serde(default)]`); older launchers ignore
  unknown fields. Example: `changes`, `features`, `presets`.
- **Needs `format_version` + 1:** a new required field, a new value of an existing enum
  (`loader.type`, file `policy`), or a field whose meaning changes. The launcher reads
  `format_version` *before* anything else, so an old launcher says "please update the
  launcher" instead of failing on the new content.
- A format bump is a **MAJOR** launcher change only if this launcher drops the old format.

**`settings.json`, `accounts.json`** (on the player's PC): every new field needs a default.
A value a launcher can't read falls back to its default on its own; the other settings are kept.

**Update feeds** (`launcher/*.json`, `launcher/tools/latest.json`): Tauri's updater format.
Our only extra key is `mandatory` (optional, launcher only).

**Alpha allowlist**: has its own `format` (1) and a signature; same rules as `pack.json`.
