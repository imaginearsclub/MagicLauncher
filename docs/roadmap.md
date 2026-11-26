# Roadmap and work in progress

Living TODO. Keep it updated as steps finish so work can resume in a new session.
Decisions are recorded here so they don't have to be re-asked. Full plan (2026-11-19):
`~/.claude/plans/cheeky-baking-melody.md`.

## Decisions (2026-11-19)
- MagicLauncher is its **own project**. The launcher it was ported from is a code reference
  only: no shared repo, bucket, CDN, keys, Azure app, Sentry/Aptabase/Discord apps, runners or
  branding. `scripts/separation-patterns.txt` + the CI `separation` job enforce it.
- Game: **Fabric on Minecraft 26.3**. The server moved to 26.3 on 2026-11-24 and the pack
  followed (pack 2.0.0, Java 25). Before that it was 1.21.11 (2026-11-19; first planned as
  1.21.10 from an outdated README).
- Pack mods are pinned to versions that accept each other: Sodium **0.9.2**, not 0.9.3,
  because Sodium 0.9.3 refuses Iris up to 1.12.0 and the newest Iris for 26.3 is 1.11.7
  (2026-11-24). Check `breaks` / `depends` in `fabric.mod.json` before bumping either.
- Hosting: the **existing Imaginears S3 bucket** + CloudFront.
- PackPublisher reads **Prism / Modrinth App and CurseForge** instances. Shader packs and
  resource packs ship in the pack; datapacks don't (they live on the server).
- UI follows the Stitch design system (project 16716545876698088757). Core screens first,
  the rest in later minors.

## Waiting on (outside the code)
- [x] Azure app registered (client ID `693dce49-…`, built in, 2026-11-19)
- [x] Public client flows enabled; device-code request accepted by Microsoft (2026-11-19)
- [x] **Mojang approval**: requested 2026-11-19, approved 2026-11-24. Real sign-in checked the
      same day (account and skin in the top bar, joined the server on the Alpha channel).
- [x] Bucket: `magiclauncher` (same AWS account as the other launcher, 2026-11-19; publisher
      default set). Pack owner's IAM user: `magiclauncherpublisher`.
- [x] CDN: CloudFront `E2AZL86MSJLTJV` → `https://launcher.imaginears.club` (ACM cert, Cloudflare
      DNS-only CNAME), us-east-1; code defaults set (2026-11-19)
- [x] Repo variables MAGIC_BUCKET / MAGIC_CDN / MAGIC_DISTRIBUTION_ID set (2026-11-19)
- [x] Origin path `/pack` confirmed; `magiclauncherpublisher` policy works (write to pack paths,
      denied elsewhere, invalidation OK; test file `pack/files/permission-check.txt`, 2026-11-19)
- [x] AWS_* secrets are the CI user `magiclauncher-ci` (policy in docs/releasing.md), 2026-11-19
- [x] Connect address: `iears.us` (SRV → `play.iears.us`; the server's own address, switched
      from `imaginears.club` on 2026-11-24, whose SRV only points at it).
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
- [x] **UI** (2026-11-19): Stitch tokens in `src/app.css`; fonts (@fontsource) and Material
      Symbols (inline SVG) bundled, CSP unchanged; state in `src/lib/app.svelte.ts`, shell =
      TopBar (server pill, account switcher) + NavRail + StatusBar; views Play (launch
      cockpit, progress, server + modpack cards) and Mods & Shaders (preset cards, optional
      mod switches, saved instantly); Settings sheet and mod picker kept. e2e 6/6 + opt-in
      screenshots (`E2E_SCREENSHOTS=1 pnpm e2e`)
- [x] Signed-in Play screen and account menu (2026-11-24, after Mojang approval)
- [x] **Minecraft 26.3** (2026-11-24): alpha pack 2.0.0 (Fabric Loader 0.20.1, Fabric API
      0.166.2, Sodium 0.9.2, Iris 1.11.7, LambDynamicLights 4.13.0, Mod Menu 21.0.1,
      SmoothCoasters 26.3-v1). Boots through the launcher (Java 25 downloaded) and joins the
      server. No launcher code change was needed.
- [x] Pack round trip (2026-11-19): Prism test instance → `--channel alpha` publish (only
      iris.properties self-hosted, rest Modrinth CDN) → launcher installs + boots it (6 mods,
      shader). Still to do: CurseForge instance, release rehearsal
- [x] Verify: offline launch to title screen (live_launch, 2026-11-24 on 26.3), release
      rehearsal `v0.1.0-alpha.1` on all three OSes (2026-11-19)
- [x] Release `v0.1.0` (2026-11-25): tagged and published; Release, Beta and Alpha packs at
      2.1.0 from the CurseForge instance

## 0.2.0 (MINOR, 2026-11-25)
- [x] Website links (`imaginears://play`, `imaginears://open`) and single instance (#13)
- [x] Verify the Minecraft account on the website (`imaginears://link/<token>`, #14). Pulled
      forward from 0.5 MagicPass: the website starts the flow, so no device/OAuth flow needed
- [x] Account menu above the server card (#15); PackPublisher skips player files (#12)
- [x] Release `v0.2.0` (2026-11-26): tagged and published; all launcher feeds on 0.2.0
- [ ] Website buttons ("Open in launcher", "Verify with MagicLauncher") in imaginears-web;
      snippet in [deep-links.md](deep-links.md)

## Later minors
- 0.3 Showtimes & Parks (StatusPage public JSON; no website API key in the app)
- 0.4 Screenshot gallery (local `screenshots/`)
- 0.5 Audio & radio (OpenAudioMC web client / radio stream)
- 0.6 MagicPass: more website account features on top of the 0.2 account link
- 0.7 Park guide / teleports, Cast Member tools (staff-gated, after MagicPass)
