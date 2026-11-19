// Release build of the launcher UI (tauri.release.conf.json → beforeBuildCommand):
//   1. vite build with hidden source maps,
//   2. sentry-cli injects debug IDs into the JS + maps (so Sentry can match them whatever the
//      app's internal URL is),
//   3. uploads the maps to Sentry (magiclauncher-frontend) when SENTRY_AUTH_TOKEN is set,
//   4. deletes the .map files so they are never bundled into the app players download.
// Without a token (local builds) steps 2–3 are skipped and the maps are still removed.

import { execSync } from "node:child_process";
import { readdirSync, rmSync, statSync } from "node:fs";
import { join } from "node:path";
import { readFileSync } from "node:fs";

const run = (cmd, env = {}) =>
  execSync(cmd, { stdio: "inherit", env: { ...process.env, ...env } });

const cargo = readFileSync("Cargo.toml", "utf8");
const version = /\[workspace\.package\][^[]*?version\s*=\s*"([^"]+)"/s.exec(cargo)?.[1];
if (!version) throw new Error("could not read the workspace version from Cargo.toml");
const release = `magiclauncher@${version}`;

run("pnpm exec vite build --sourcemap hidden");

if (process.env.SENTRY_AUTH_TOKEN) {
  const sentry = (args) =>
    run(`pnpm exec sentry-cli ${args}`, { SENTRY_ORG: process.env.SENTRY_ORG ?? "imaginears-club", SENTRY_PROJECT: process.env.SENTRY_PROJECT ?? "magiclauncher-frontend" });
  try {
    sentry("sourcemaps inject dist");
    sentry(`sourcemaps upload --release ${release} dist`);
    console.log(`Uploaded source maps for ${release}`);
  } catch (e) {
    // Never block a release over source maps.
    console.warn(`::warning::Source map upload to Sentry failed: ${e.message}`);
  }
} else {
  console.log("SENTRY_AUTH_TOKEN not set: skipping source map upload");
}

const removeMaps = (dir) => {
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    if (statSync(p).isDirectory()) removeMaps(p);
    else if (name.endsWith(".map")) rmSync(p);
  }
};
removeMaps("dist");
