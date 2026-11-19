# Imaginears MagicLauncher

The official launcher for **Imaginears Club**, the 1:1 Disney parks Minecraft server, plus
**Imaginears PackPublisher**, the tool staff use to publish the Imaginears Magic Pack.

One click: Microsoft sign-in, install Minecraft + Fabric + the approved mods and shaders,
keep them up to date, and join the server. Windows, macOS and Linux.

Built with Tauri v2, Vite and Svelte 5 on a Rust core.

> Status: **0.1.0 in development.** See [docs/roadmap.md](docs/roadmap.md).

## Layout

| Path | What |
|---|---|
| `crates/launcher-core` | The launcher engine (no UI): Microsoft sign-in, Java runtime, Minecraft + loader install, pack sync, launch, server ping, channels, telemetry. |
| `crates/pack-builder` | Turns a launcher instance into `pack.json` + `files/`. |
| `crates/pack-publisher` | `pack-publisher.exe` (command line) and the shared publish library (S3 + CloudFront). |
| `src-tauri` | Launcher app shell (Tauri commands, self-update, Discord presence, Sentry). |
| `src` | Launcher UI (Svelte). |
| `apps/publisher-gui` | Imaginears PackPublisher, the window version of the publisher. |
| `scripts` | Release helpers (frontend build, channel feeds, allowlist). |
| `e2e` | WebDriver smoke tests of the real app. |
| `docs` | [Azure setup](docs/azure-setup.md), [pack manifest](docs/pack-manifest.md), [releasing](docs/releasing.md), [roadmap](docs/roadmap.md). |

## Development

Requirements: Rust (see `rust-version` in `Cargo.toml`), Node (see `.node-version`), pnpm,
and the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for your OS.

```sh
pnpm install
pnpm tauri dev            # the launcher
pnpm publisher dev        # Imaginears PackPublisher (window)
cargo run -p pack-publisher -- --dry-run   # publisher, command line
```

Checks (CI runs the same on Linux, Windows and macOS):

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
pnpm check && pnpm build
python -I -m unittest discover -s scripts -p "test_*.py"
```

WebDriver smoke tests: `pnpm tauri build --debug --no-bundle`, then `pnpm e2e`.

Debug builds have a "Play offline (dev)" button so installs can be tested before Microsoft
sign-in is approved (docs/azure-setup.md). `MAGIC_PACK_URL` points a debug build at another
`pack.json`, `MAGIC_DATA_DIR` at a throwaway data folder.

## Versioning

SemVer. The version lives only in `[workspace.package]` in `Cargo.toml`; every crate and
both apps inherit it. Each release has a `CHANGELOG.md` section and a `vX.Y.Z` tag
(pre-releases `vX.Y.Z-beta.N` / `-alpha.N`). See [docs/releasing.md](docs/releasing.md).

## Not affiliated

Imaginears Club is a fan community. Not affiliated with The Walt Disney Company, Mojang or
Microsoft.
