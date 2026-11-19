# Releasing the launcher

Installed launchers check `<MAGIC_CDN>/launcher/latest.json` when they
start. If it names a newer version they show **"Launcher X is available → Update now"**, which
downloads the update, checks its signature, installs it and restarts. New players download from
`<MAGIC_CDN>/launcher/download/index.html`.

## One-time setup

### Update signing key

Updates are signed with a minisign key (free, separate from Windows/macOS code signing). The
launcher only installs updates signed by it; the public half is `plugins.updater.pubkey` in
`src-tauri/tauri.conf.json`.

- Private key: `%USERPROFILE%\.tauri\magiclauncher.key` (no password).
- **Back it up** (password manager / offline). If it's lost, installed launchers can never be
  updated again: every player would have to reinstall by hand.
- Never commit it. If it leaks, generate a new pair, ship the new public key in a release
  signed with the *old* key, then switch CI to the new one.

### GitHub repository secrets

| Secret | Value |
|---|---|
| `TAURI_SIGNING_PRIVATE_KEY` | contents of `magiclauncher.key` |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | empty (the key has no password); create it anyway |
| `ALLOWLIST_SIGNING_KEY` | contents of `magiclauncher-alpha-allowlist.key` (Alpha allowlist, see below) |
| `AWS_ACCESS_KEY_ID` / `AWS_SECRET_ACCESS_KEY` | keys of the CI IAM user `magiclauncher-ci` (policy below). Not the pack owner's `magiclauncherpublisher`: separate keys, separate revocation |
| `SENTRY_AUTH_TOKEN` | optional: uploads source maps / debug files to the Imaginears Sentry projects |

CI IAM user `magiclauncher-ci` policy (stages releases, promotes feeds, download page, tools,
Alpha allowlist; the `ListBucket` is for the recursive copy of a staged release):

```json
{
  "Version": "2012-10-17",
  "Statement": [
    { "Effect": "Allow", "Action": ["s3:PutObject", "s3:GetObject"],
      "Resource": "arn:aws:s3:::magiclauncher/pack/launcher/*" },
    { "Effect": "Allow", "Action": "s3:ListBucket",
      "Resource": "arn:aws:s3:::magiclauncher",
      "Condition": { "StringLike": { "s3:prefix": ["pack/launcher/*"] } } },
    { "Effect": "Allow", "Action": "cloudfront:CreateInvalidation",
      "Resource": "arn:aws:cloudfront::*:distribution/E2AZL86MSJLTJV" }
  ]
}
```

### GitHub repository variables

Settings → Secrets and variables → Actions → **Variables**. The release, promote and
allowlist workflows stop with a message if a hosting variable is missing.

| Variable | Value |
|---|---|
| `MAGIC_BUCKET` | the Imaginears S3 bucket name |
| `MAGIC_CDN` | its public CDN base URL, no trailing slash (`https://launcher.imaginears.club`) |
| `MAGIC_DISTRIBUTION_ID` | the CloudFront distribution in front of the bucket |
| `MAGIC_AWS_REGION` | optional, default `us-east-1` |
| `MAGIC_MS_CLIENT_ID` | optional: Azure client ID (docs/azure-setup.md) if not built in |
| `MAGIC_PACK_URL` | optional: pack.json URL if not the built-in default |
| `SENTRY_ORG` / `SENTRY_PROJECT` | optional, defaults `imaginears-club` / `magiclauncher` |

### Build machines

All builds run on GitHub-hosted runners. To use your own always-on machines later, register
them as self-hosted runners for this repo and set `RUNNER_WINDOWS` / `RUNNER_MAC` to their
label lists (e.g. `["self-hosted","magic-mac"]`). Self-hosted runners are only safe on a
private repo.

## Each release

0. On a Windows PC, run the WebDriver smoke tests (CI only runs them on Linux):
   `pnpm tauri build --debug --no-bundle` then `pnpm e2e` (see README, Development).
1. Update `CHANGELOG.md` (Added / Changed / Fixed / Breaking) and bump `version` in
   `[workspace.package]` in the root `Cargo.toml` (SemVer).
2. Commit, tag `vX.Y.Z`, push the tag. `release.yml`:
   - checks the tag matches Cargo,
   - builds Windows (NSIS), macOS arm64 + x64 (dmg + updater bundle), Linux (AppImage, deb, rpm),
   - builds the pack owner's tools (same version): `pack-publisher.exe` (command line) and the
     Imaginears PackPublisher installer (`pnpm publisher build --bundles nsis`, signed with the
     launcher's updater key) plus its own feed, staged in `launcher/X.Y.Z/tools/`,
   - opens a **draft** GitHub release with the installers and both tools,
   - stages everything plus a validated `latest.json` in `s3://<MAGIC_BUCKET>/pack/launcher/X.Y.Z/`.

   Nothing is live yet.
3. Optional: install a staged build to test it, e.g.
   `<MAGIC_CDN>/launcher/X.Y.Z/magiclauncher-setup.exe`.
4. **Publish the draft release.** `promote-launcher.yml` copies the installers to
   `launcher/download/`, writes the download page, then `launcher/latest.json`, and clears the
   CDN cache. Every launcher offers the update on its next start.
   Publishing a **Release** (not a pre-release) also copies the tools to
   `<MAGIC_CDN>/launcher/tools/` (`imaginears-pack-publisher-setup.exe`,
   `pack-publisher.exe`) and updates `tools/latest.json`, so installed Imaginears
   PackPublishers offer the update. That page isn't linked from the public download page.


## Release channels

| Channel | Who | Tag | Feed |
|---|---|---|---|
| Release | everyone (default) | `v0.3.0` | `launcher/latest.json` |
| Beta | anyone who picks it in Settings | `v0.3.0-beta.1` | `launcher/beta/latest.json` |
| Alpha | allowlisted staff/testers only | `v0.3.0-alpha.1` | `launcher/alpha/latest.json` |

Publishing a release updates its channel **and every less stable one**: a Release goes to all
three feeds (and the download page), a Beta to beta + alpha, an Alpha to alpha only.
`scripts/channel_feeds.py` makes sure a feed never moves to a lower version (a 0.2.1 hotfix
won't pull Beta back from 0.3.0-beta.1) and that a pre-release can never reach the Release
feed or the download page. Pre-release tags need their own CHANGELOG section too
(`## [0.3.0-beta.1]`); the draft release is marked as a pre-release. Pre-releases have no
Linux `.rpm` (RPM versions can't contain `-`); they ship the AppImage and `.deb`.

Players stay on their channel until Release catches up. *Settings → Go back to Release…*
(with a warning) installs the newest Release even if it's older. Each channel has its own game
folder (`instance`, `instance-beta`, `instance-alpha`), so test builds never touch the Release
install.

**Rollback:** *Actions → Promote launcher release → Run workflow* with the version and
**force** ticked. That lowers the feeds; players who already installed the bad version stay on
it until a higher version ships (installed launchers never downgrade on their own).

## Alpha access (allowlist)

`channels/alpha-testers.txt` lists Minecraft names (or UUIDs), one per line, optionally
`Name until 2027-01-31`. Pushing a change to main runs **Publish Alpha allowlist**, which
resolves names to UUIDs, signs `launcher/channels/alpha.json` with `ALLOWLIST_SIGNING_KEY`
and uploads it. On their next launcher start, listed players see Alpha in Settings; removed
players lose it (and follow Beta; their Alpha game folder stays on disk).

- Key: `%USERPROFILE%\.tauri\magiclauncher-alpha-allowlist.key` (no password); public half is
  `ALLOWLIST_PUBKEY` in `crates/launcher-core/src/allowlist.rs`. Back it up. If lost, generate
  a new pair, ship the new public key in a launcher release, then update the secret.
- The file format is fixed, so a Discord bot or the staff web panel can later produce the list
  (by committing to the testers file, or by signing and uploading it themselves).
- Alpha is hidden in the launcher, but alpha builds are still public URLs on the CDN; the
  allowlist controls who is offered them, not who could download one by hand.

## Required updates

Add `"mandatory": true` to a version's `latest.json` (edit the staged file before publishing,
or re-upload `launcher/latest.json`) to disable Play until players update, e.g. when a new
`pack.json` format needs a newer launcher.

## Platform notes

- Windows updates run the NSIS installer in passive mode (progress bar, no questions).
- macOS: unsigned, so macOS may ask again after an update.
- Linux: the AppImage updates itself. `.deb`/`.rpm` installs get the "download the new
  version" message instead and update by reinstalling.
