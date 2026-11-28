# Website links (`imaginears://`)

The launcher registers the `imaginears://` URL scheme, so a link on the website opens it.
Code: `src-tauri/src/deeplink.rs`.

| Link | What happens |
|---|---|
| `imaginears://play` | Opens the launcher (starts it if it's closed) on the Play screen, ready to join `iears.us`. The player still clicks **Play**. |
| `imaginears://open` | Opens or focuses the launcher. |
| `imaginears://link/<token>` | Asks the player whether to verify their signed-in Minecraft account on the website (see below). Nothing happens until they confirm. |

Any web page can send these links, so they only pick from this list, never start the game, and
never act on the player's account without asking.
Unknown actions (a link made for a newer launcher) just open the launcher; anything after the
action (path, query) is ignored.

## Where it's registered

- **Windows**: the installer (per user, `HKCU\Software\Classes\imaginears`). Dev builds
  (`pnpm tauri dev`) register themselves on start, pointing the scheme at the dev exe until the
  launcher is installed or updated again.
- **macOS**: the app bundle's `Info.plist`. macOS can't register a scheme at runtime, so
  `pnpm tauri dev` doesn't get links; test with a built bundle (`pnpm tauri build --debug`).
- **Linux**: the `.deb`/`.rpm` desktop entry; the AppImage registers itself on start.

Players on 0.1.0 get the scheme with the update that adds it.

## Verifying a Minecraft account (`link`)

The website's **Verify with MagicLauncher** button opens `imaginears://link/<token>`. The token
is 40 lowercase hex characters; a link with anything else just opens the launcher.

1. The launcher asks the website which account the token belongs to
   (`GET {WEBSITE_URL}/api/minecraft/link/challenge`) and shows **"Link Minecraft account X to
   website account Y?"**. Anyone can send these links, including with *their own* website
   account's token, so the prompt shows both names and the player must confirm.
2. After **Link account**, the launcher makes a Mojang session `join` with `serverId = token`
   (the call the game makes when joining a server), then calls
   `POST {WEBSITE_URL}/api/minecraft/link/complete`. The website checks Mojang's `hasJoined`.

**Account recovery.** Staff can send the same kind of link to a locked-out player. The website's
challenge reply then says `"purpose": "recovery"`, and the launcher asks **"Prove you own this
account?"** instead: completing it only proves the player owns the Minecraft account already
linked to that website account. Nothing is linked or changed, and a different Minecraft account
gets a clear "this isn't the linked account" message. Without the field (an older website) the
launcher treats it as a normal link.

The Minecraft access token only goes to Mojang. The website origin is built in
(`launcher_core::config::WEBSITE_URL`, default `https://imaginears.club`) and never taken from
the link. To test against a local site, build with `MAGIC_WEBSITE_URL=http://localhost:3000`.
Code: `crates/launcher-core/src/website_link.rs`. Website side: imaginears-web
`docs/minecraft-account-linking.md`.

## Website button

Browsers can't tell whether an app is installed, so always offer the download next to it.
The first click shows the browser's "Open Imaginears MagicLauncher?" prompt.

```html
<a href="imaginears://play">Launch MagicLauncher</a>
<a href="https://launcher.imaginears.club/launcher/download/">Don't have it? Download</a>
```
