// Sentry for the launcher UI, started only after the player agreed to telemetry.
// Privacy: no default PII, no Session Replay (no screen recording), user names in file
// paths are scrubbed before sending. The Rust side has its own client (src-tauri/monitoring.rs).

import * as Sentry from "@sentry/svelte";

let started = false;

const scrub = (s: string) =>
  s
    .replace(/([A-Za-z]:[\\/]+Users[\\/]+)[^\\/\s"']+/g, "$1<user>")
    .replace(/(\/(?:Users|home)\/)[^/\s"']+/g, "$1<user>");

export function startSentry(dsn: string, version: string, dev: boolean) {
  if (started || !dsn) return;
  started = true;
  Sentry.init({
    dsn,
    release: `magiclauncher@${version}`,
    environment: dev ? "development" : "production",
    // No `sendDefaultPii`: the browser SDK sends no default PII unless asked to.
    beforeSend(event) {
      if (event.message) event.message = scrub(event.message);
      for (const ex of event.exception?.values ?? []) {
        if (ex.value) ex.value = scrub(ex.value);
      }
      delete event.user;
      return event;
    },
  });
}

export async function stopSentry() {
  if (!started) return;
  started = false;
  await Sentry.close(2000);
}
