# Website links (`imaginears://`)

The launcher registers the `imaginears://` URL scheme, so a link on the website opens it.
Code: `src-tauri/src/deeplink.rs`.

| Link | What happens |
|---|---|
| `imaginears://play` | Opens the launcher (starts it if it's closed) on the Play screen, ready to join `iears.us`. The player still clicks **Play**. |
| `imaginears://open` | Opens or focuses the launcher. |

Any web page can send these links, so they only pick from this list and never start the game.
Unknown actions (a link made for a newer launcher) just open the launcher; anything after the
action (path, query) is ignored.

## Where it's registered

- **Windows**: the installer (per user, `HKCU\Software\Classes\imaginears`). Dev builds
  (`pnpm tauri dev`) register themselves on start, pointing the scheme at the dev exe until the
  launcher is installed or updated again.
- **macOS**: the app bundle's `Info.plist`.
- **Linux**: the `.deb`/`.rpm` desktop entry; the AppImage registers itself on start.

Players on 0.1.0 get the scheme with the update that adds it.

## Website button

Browsers can't tell whether an app is installed, so always offer the download next to it.
The first click shows the browser's "Open Imaginears MagicLauncher?" prompt.

```html
<a href="imaginears://play">Launch MagicLauncher</a>
<a href="https://launcher.imaginears.club/launcher/download/">Don't have it? Download</a>
```
