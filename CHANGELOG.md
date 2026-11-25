# Changelog

All notable changes to MagicLauncher and Imaginears PackPublisher. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/). Each section lists Added, Changed, Fixed and
Breaking.

## [Unreleased]

## [0.2.0] - 2026-11-25

Website links: the launcher now opens `imaginears://` links, so the Imaginears Club website
can open it and verify your Minecraft account. The website's buttons go live after this
release. Only one launcher runs at a time.

### Added
- Website links: `imaginears://play` opens the launcher on the Play screen (starting it if
  it's closed), `imaginears://open` just opens it. Links never start the game by themselves.
  See [docs/deep-links.md](docs/deep-links.md) for the website button.
- Verify your Minecraft account on the Imaginears Club website: the website's **Verify with
  MagicLauncher** button opens `imaginears://link/<token>`, the launcher shows which Minecraft
  account and which website account would be linked, and links them only after you confirm.
  Ownership is proven through Mojang's session server; your Minecraft login is never sent to
  the website. Staff need a verified account to sign in to Utilidor.
- Only one launcher runs at a time: opening it again (or a link) brings the open window to
  the front.

### Fixed
- The account menu no longer opens behind the server status card on the Play screen, which
  hid some of its options.
- PackPublisher no longer ships `options.txt`, `servers.dat` or Sodium's
  `config/sodium-fingerprint.json` from the source instance. The game writes them for whoever
  played that instance, so they would have become every new player's settings, server list
  and install ID.

### Breaking
- None. Windows, macOS and AppImage installs of 0.1.0 update in place; Linux `.deb`/`.rpm`
  installs show the "download the new version" message and update by reinstalling. The
  update registers the `imaginears://` link type (see [docs/deep-links.md](docs/deep-links.md)).

## [0.1.0] - 2026-11-24

First release of MagicLauncher for every Imaginears Club player: Microsoft sign-in, the
Imaginears Magic Pack on Minecraft 26.3, and the Release, Beta and Alpha channels. It includes
everything from 0.1.0-alpha.1 below.

### Added
- Real Microsoft sign-in: Mojang approved the launcher's Azure app (2026-11-24). Signed-in
  players see their account and skin in the top bar and join the server with their own
  account.

### Changed
- The launcher joins and pings the server at `iears.us`, the server's own address, instead of
  `imaginears.club`, which only redirected to it.
- The Imaginears Magic Pack is on Minecraft 26.3 (pack 2.0.0 on the Alpha channel, Java 25).
  No launcher change was needed; docs and the live launch test now use 26.3.
- Updated Tauri to 2.12.7 (tauri-build 2.7.6), clap 4.6.9 and zip 9.8.0.

### Fixed
- When no modpack is published for the selected channel, the Play and Mods & Shaders screens
  now say so instead of "Couldn't check for pack updates", which is kept for real failures
  (offline, server errors).

### Breaking
- None. Installs of 0.1.0-alpha.1 update in place.

## [0.1.0-alpha.1] - 2026-11-19

First test build (Alpha channel only). Release rehearsal for 0.1.0.

### Added
- Launcher core, app shell, PackPublisher (CLI + window), release workflows and docs, ported
  from an existing Tauri launcher codebase and rebranded for Imaginears Club.
- New update-signing and Alpha-allowlist keys, separate from any other project.
- CI check that no names, IDs or hosts of the reference project come back.
- Fabric support: the launcher installs Fabric Loader from Fabric meta (no installer run).
- PackPublisher reads Prism Launcher, Modrinth App and CurseForge instances (or a plain folder
  with `--minecraft` / `--fabric`) and blocks publishing when a mod needs another Fabric
  Loader or Minecraft version.

- New look from the MagicLauncher design system: top bar with live server status, side
  navigation, a launch cockpit on the Play screen, and a Mods & Shaders screen to pick a
  graphics preset or switch optional mods on and off.
- Title bar in the launcher's colours: midnight navy on Windows 11, dark on Windows 10 and
  Linux, and on macOS the top bar runs under the window buttons (drag the top bar to move).

### Fixed
- Server status picks the SRV record by priority and weight instead of the first one.
- A temporary Microsoft sign-in outage (rate limit, 5xx) no longer signs the player out;
  only an expired or revoked login does.
- Files that differ only in extension (`a.json`, `a.toml`) no longer share a temporary file
  while downloading in parallel.
- Publishing a changed self-hosted file no longer overwrites the copy the live pack uses:
  files are stored under `files/<sha1>/<path>`. The publisher also publishes when only the
  storage layout changed.
- Dev-only dependency: minimatch pinned to a fixed version (GHSA-g3hv-75wg-m249).
