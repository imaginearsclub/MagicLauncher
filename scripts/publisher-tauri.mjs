// Run the Tauri CLI for the Imaginears PackPublisher app instead of the launcher:
//   pnpm publisher dev                 (window with hot reload, port 1430)
//   pnpm publisher build --no-bundle   (target/release/imaginears-pack-publisher.exe)
// The Tauri CLI finds its app through TAURI_APP_PATH / TAURI_FRONTEND_PATH.
import { spawnSync } from "node:child_process";
import { resolve } from "node:path";

const app = resolve("apps/publisher-gui");
const r = spawnSync("pnpm", ["exec", "tauri", ...process.argv.slice(2)], {
  stdio: "inherit",
  shell: process.platform === "win32",
  env: {
    ...process.env,
    TAURI_APP_PATH: resolve(app, "src-tauri"),
    TAURI_FRONTEND_PATH: app,
  },
});
process.exit(r.status ?? 1);
