# Changelog

All notable changes to MagicLauncher and Imaginears PackPublisher. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/). Each section lists Added, Changed, Fixed and
Breaking.

## [Unreleased]

### Added
- Launcher core, app shell, PackPublisher (CLI + window), release workflows and docs, ported
  from an existing Tauri launcher codebase and rebranded for Imaginears Club.
- New update-signing and Alpha-allowlist keys, separate from any other project.
- CI check that no names, IDs or hosts of the reference project come back.
- Fabric support: the launcher installs Fabric Loader from Fabric meta (no installer run).
- PackPublisher reads Prism Launcher, Modrinth App and CurseForge instances (or a plain folder
  with `--minecraft` / `--fabric`) and blocks publishing when a mod needs another Fabric
  Loader or Minecraft version.

### Fixed
- Server status picks the SRV record by priority and weight instead of the first one.
