# Microsoft / Minecraft sign-in setup

The launcher signs players in with Microsoft. That needs an Azure app owned by Imaginears
Club, and Mojang must approve the app before it may call the Minecraft services API.
**Start this early, approval can take weeks.** Until then, debug builds have a
"Play offline (dev)" button so installs and launches can be tested locally (the Imaginears
server runs in online mode and rejects offline profiles; that's expected).

Status (2026-11-24): **approved by Mojang**, real sign-in works. Client ID
`693dce49-2443-4b96-8912-d1b353b81fcd` (built-in default of `MS_CLIENT_ID` in
`crates/launcher-core/src/config.rs`); registered and review requested 2026-11-19. Before
approval, sign-in got as far as Xbox Live and then failed at the Minecraft login with
`403` / "Invalid app registration"; a new app ID would go through the same review.

## 1. Register the Azure app

1. Go to <https://portal.azure.com> → **Microsoft Entra ID** → **App registrations** → **New registration**.
2. Name: `Imaginears MagicLauncher`.
3. Supported account types: **Personal Microsoft accounts only**.
4. Redirect URI: leave empty (the device-code flow doesn't use one).
5. After creating it, open **Authentication** → **Advanced settings** → set
   **Allow public client flows** to **Yes**, then save.
6. Copy the **Application (client) ID**. It's a public identifier, not a secret.

No client secret is needed and none must ever be shipped in the launcher.

## 2. Ask Mojang for Minecraft API access

New apps get `403`/"Invalid app registration" from `api.minecraftservices.com` until
approved. Submit the app ID with Mojang's form for new launcher apps:
<https://aka.ms/mce-reviewappid> (check on Mojang's help site that the link is still
current). Describe it as the official launcher of the Imaginears Club community server
that signs players in with their own Microsoft accounts.

## 3. The client ID in the launcher

The client ID is the built-in default of `MS_CLIENT_ID` in
`crates/launcher-core/src/config.rs` (it's public, like the pack URL). The repo variable
`MAGIC_MS_CLIENT_ID` overrides it in `release.yml` builds if it is ever set.

To test a different Azure app, override it at build time:

```sh
# PowerShell
$env:MAGIC_MS_CLIENT_ID = "client-id"; pnpm tauri dev
# bash
MAGIC_MS_CLIENT_ID=client-id pnpm tauri dev
```

## What the launcher stores

- Microsoft refresh token: OS keychain (Windows Credential Manager, macOS Keychain,
  Linux Secret Service), under `club.imaginears.magiclauncher`.
- Player name and UUID: `account.json` in the app data folder.
- Minecraft access token: memory only.
