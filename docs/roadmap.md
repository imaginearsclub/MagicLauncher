# Roadmap and work in progress

Living TODO. Keep it updated as steps finish so work can resume in a new session.
Decisions are recorded here so they don't have to be re-asked. Full plan (2026-11-19):
`~/.claude/plans/cheeky-baking-melody.md`.

## Decisions (2026-11-19)
- MagicLauncher is its **own project**. The launcher it was ported from is a code reference
  only: no shared repo, bucket, CDN, keys, Azure app, Sentry/Aptabase/Discord apps, runners or
  branding. `scripts/separation-patterns.txt` + the CI `separation` job enforce it.
- Game: **Fabric on Minecraft 1.21.11**. The live server reports "MC 1.21.11+" and asks
  for 1.21.11 or above (2026-11-19; first planned as 1.21.10 from an outdated README).
- Hosting: the **existing Imaginears S3 bucket** + CloudFront.
- PackPublisher reads **Prism / Modrinth App and CurseForge** instances. Shader packs and
  resource packs ship in the pack; datapacks don't (they live on the server).
- UI follows the Stitch design system (project 16716545876698088757). Core screens first,
  the rest in later minors.

## Waiting on (outside the code)
- [x] Azure app registered (client ID `693dce49-…`, built in, 2026-11-19)
- [x] Public client flows enabled; device-code request accepted by Microsoft (2026-11-19)
- [ ] **Mojang approval**: requested 2026-11-19. Blocks real sign-in (Minecraft login returns
      403 until approved); debug builds can play offline meanwhile.
- [x] Bucket: `magiclauncher` (same AWS account as the other launcher, 2026-11-19; publisher
      default set). Pack owner's IAM user: `magiclauncherpublisher`.
- [x] CDN: CloudFront `E2AZL86MSJLTJV` → `https://launcher.imaginears.club` (ACM cert, Cloudflare
      DNS-only CNAME), us-east-1; code defaults set (2026-11-19)
- [x] Repo variables MAGIC_BUCKET / MAGIC_CDN / MAGIC_DISTRIBUTION_ID set (2026-11-19)
- [x] Origin path `/pack` confirmed; `magiclauncherpublisher` policy works (write to pack paths,
      denied elsewhere, invalidation OK; test file `pack/files/permission-check.txt`, 2026-11-19)
- [x] AWS_* secrets are the CI user `magiclauncher-ci` (policy in docs/releasing.md), 2026-11-19
- [x] Connect address: `imaginears.club` (SRV → the real host; checked 2026-11-19).
- [x] Repo secrets set (2026-11-19): TAURI_SIGNING_PRIVATE_KEY (+ empty _PASSWORD),
      ALLOWLIST_SIGNING_KEY, AWS_ACCESS_KEY_ID / AWS_SECRET_ACCESS_KEY; optional SENTRY_AUTH_TOKEN
- [ ] Optional services (each feature stays off until set): Sentry projects, Aptabase app,
      Discord application for Rich Presence.
- [ ] Real app icon art (currently generated from the website logo).

## 0.1.0 (MINOR, first release)
- [x] Repo bootstrap: fresh clone, `Cargo.toml`/`Cargo.lock` case fix
- [x] Port the code; rebrand names, identifiers, URLs, env prefix (`MAGIC_*`), file markers
- [x] New update + allowlist signing keys (`~/.tauri/magiclauncher*.key`)
- [x] Workflows on GitHub-hosted runners, hosting from repo variables, separation CI job
- [x] Docs rewritten (azure-setup, pack-manifest, releasing), README, CHANGELOG
- [x] **Fabric**: `launcher-core/src/fabric.rs` (Fabric meta profile, `inheritsFrom` merge)
      replaces the NeoForge installer; Java runtime from Mojang's version JSON. Live check:
      `cargo test -p launcher-core live_fabric_profile -- --ignored`
- [x] **PackPublisher instances**: Prism / Modrinth App (legacy profile.json) / CurseForge /
      plain folder (`--minecraft`, `--fabric`); `fabric.mod.json` deps checked (Fabric
      Loader + Minecraft mismatches block publishing); reference pack's TaCZ/Xaero defaults
      dropped; shaderpacks/resourcepacks managed, config/ + iris.properties default
- [x] Server ping picks SRV records by priority/weight (imaginears.club has a weight-0
      record behind Cloudflare's proxy that never answers)
- [x] **Real install + launch** (2026-11-19, Windows): local pack.json (1.21.11, Fabric 0.20.1,
      Fabric API, Sodium) → prepare + offline launch loads both mods and the client's
      resources in ~18 s. Rerun: `cargo test -p launcher-core --test live_launch -- --ignored`
- [ ] **UI**: Stitch tokens in `src/app.css`, fonts + icons bundled locally (CSP-safe),
      `App.svelte` split into views/components; Play cockpit, Launch progress, Settings,
      Mods & Shaders, Account switcher
- [x] Pack round trip (2026-11-19): Prism test instance → `--channel alpha` publish (only
      iris.properties self-hosted, rest Modrinth CDN) → launcher installs + boots it (6 mods,
      shader). Still to do: CurseForge instance, release rehearsal
- [ ] Verify: offline launch to title
      screen, release rehearsal `v0.1.0-alpha.1` on all three OSes

## Later minors
- 0.2 Showtimes & Parks (StatusPage public JSON; no website API key in the app)
- 0.3 Screenshot gallery (local `screenshots/`)
- 0.4 Audio & radio (OpenAudioMC web client / radio stream)
- 0.5 MagicPass: website account link (needs a device/OAuth flow in imaginears-web first)
- 0.6 Park guide / teleports, Cast Member tools (staff-gated, after MagicPass)
