# Changelog

All notable changes to MagicLauncher and Imaginears PackPublisher. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/). Each section lists Added, Changed, Fixed and
Breaking.

## [Unreleased]

### Changed
- The launcher joins and pings the server at `iears.us`, the server's own address, instead of
  `imaginears.club`, which only redirected to it.
- The Imaginears Magic Pack is on Minecraft 26.3 (pack 2.0.0 on the Alpha channel, Java 25).
  No launcher change was needed; docs and the live launch test now use 26.3.

### Fixed
- When no modpack is published for the selected channel, the Play and Mods & Shaders screens
  now say so instead of "Couldn't check for pack updates", which is kept for real failures
  (offline, server errors).

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
