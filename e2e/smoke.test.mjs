// WebDriver smoke tests for the launcher (Windows and Linux; tauri-driver has no macOS support).
//
// Needs a debug build with the frontend embedded (`pnpm tauri build --debug --no-bundle`),
// `tauri-driver` (cargo install tauri-driver --locked) and the platform's native driver on PATH:
// msedgedriver matching the WebView2 runtime on Windows, WebKitWebDriver on Linux.
// Run with `pnpm e2e` (under `xvfb-run` on a Linux machine without a display).
//
// The app runs against a throwaway data folder (MAGIC_DATA_DIR) and a fixture pack
// (MAGIC_PACK_URL), so nothing here touches a real install or depends on the live pack.

import { spawn } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { after, before, describe, it } from "node:test";
import assert from "node:assert/strict";
import { remote } from "webdriverio";

const ROOT = resolve(import.meta.dirname, "..");
const EXE = process.platform === "win32" ? ".exe" : "";
const APP = process.env.E2E_APP ?? join(ROOT, "target", "debug", `magiclauncher${EXE}`);
const DRIVER_PORT = 4444;
// Dots only, like the real one: browsers may wrap at a hyphen but not at a dot.
const LONG_ADDRESS = "play.areallylongserveraddress.imaginears.club";

// Failure screenshots and page state go here (CI uploads the folder).
const ARTIFACTS = join(import.meta.dirname, "artifacts");

let browser;
let driver;
let fixtures;
let dataDir;

async function waitForDriver() {
  for (let i = 0; i < 100; i++) {
    try {
      const r = await fetch(`http://127.0.0.1:${DRIVER_PORT}/status`);
      if (r.ok) return;
    } catch {
      // not listening yet
    }
    await new Promise((r) => setTimeout(r, 200));
  }
  throw new Error("tauri-driver did not start");
}

/** Overflow measurements, in CSS pixels (positive = something sticks out). */
function measureLayout() {
  return browser.execute(() => {
    const doc = document.scrollingElement;
    const side = document.querySelector(".side");
    const sideRight = side ? side.getBoundingClientRect().right : innerWidth;
    let cards = 0;
    for (const p of document.querySelectorAll(".side .panel")) {
      const r = p.getBoundingClientRect();
      cards = Math.max(cards, r.right - sideRight, r.right - innerWidth, p.scrollWidth - p.clientWidth);
    }
    // Vertical: nothing in the home area may start above it (unreachable, cut off at the
    // top), and in the one-column layout the side cards scroll with the page, not inside
    // a box of their own.
    const home = document.querySelector(".home");
    home.scrollTop = 0;
    const homeTop = home.getBoundingClientRect().top;
    let above = 0;
    for (const el of home.querySelectorAll(".hero > *")) {
      above = Math.max(above, homeTop - el.getBoundingClientRect().top);
    }
    const sideScrolls = side ? side.scrollHeight - side.clientHeight : 0;
    return { page: doc.scrollWidth - doc.clientWidth, cards, above, sideScrolls, width: innerWidth };
  });
}

/** `it`, plus a screenshot and a page-state dump when the test fails. */
function check(name, fn) {
  it(name, async () => {
    try {
      await fn();
    } catch (e) {
      const file = name.replace(/[^a-z0-9]+/gi, "-").toLowerCase();
      try {
        mkdirSync(ARTIFACTS, { recursive: true });
        await browser.saveScreenshot(join(ARTIFACTS, `${file}.png`));
        const state = await browser.execute(() => ({
          focused: document.activeElement?.outerHTML.slice(0, 200),
          dialogs: [...document.querySelectorAll('[role="dialog"]')].map((d) => d.outerHTML.slice(0, 200)),
          size: [innerWidth, innerHeight, devicePixelRatio],
          body: document.body.innerText.slice(0, 1500),
        }));
        writeFileSync(join(ARTIFACTS, `${file}.json`), JSON.stringify(state, null, 2));
        console.error(`page state for "${name}":`, JSON.stringify(state));
      } catch (shotError) {
        console.error("could not capture failure state:", shotError);
      }
      throw e;
    }
  });
}

/**
 * Open Settings with a click, or with the keyboard (focus the button, Enter) while the page
 * is zoomed: WebKit's WebDriver ignores page zoom when it places a click, so clicks miss.
 */
async function openSettings({ keyboard = false } = {}) {
  const button = await browser.$("footer").$("button=Settings");
  if (keyboard) {
    await browser.execute((b) => b.focus(), button);
    await browser.keys("Enter");
  } else {
    await button.click();
  }
  const sheet = await browser.$('[aria-labelledby="settings-title"]');
  await sheet.waitForDisplayed();
  return sheet;
}

async function pressEscape() {
  await browser.keys("Escape");
}

before(async () => {
  dataDir = mkdtempSync(join(tmpdir(), "magic-e2e-"));
  // Answer the telemetry question up front so its dialog doesn't cover the screen.
  writeFileSync(join(dataDir, "settings.json"), JSON.stringify({ telemetry: false }));

  const pack = readFileSync(join(import.meta.dirname, "fixtures", "pack.json"));
  fixtures = createServer((req, res) => {
    if (req.url?.endsWith("/pack.json")) {
      res.writeHead(200, { "content-type": "application/json" }).end(pack);
    } else {
      res.writeHead(404).end();
    }
  });
  await new Promise((r) => fixtures.listen(0, "127.0.0.1", r));
  const packUrl = `http://127.0.0.1:${fixtures.address().port}/pack/pack.json`;

  driver = spawn("tauri-driver", ["--port", String(DRIVER_PORT)], {
    stdio: ["ignore", "inherit", "inherit"],
    env: { ...process.env, MAGIC_DATA_DIR: dataDir, MAGIC_PACK_URL: packUrl },
  });
  await waitForDriver();

  browser = await remote({
    hostname: "127.0.0.1",
    port: DRIVER_PORT,
    logLevel: "warn",
    capabilities: {
      "wdio:enforceWebDriverClassic": true,
      "tauri:options": { application: APP },
    },
  });
});

after(async () => {
  await browser?.deleteSession().catch(() => {});
  driver?.kill();
  fixtures?.close();
  if (dataDir) rmSync(dataDir, { recursive: true, force: true });
});

describe("launcher", () => {
  check("starts on the sign-in screen", async () => {
    const signIn = await browser.$("button=Sign in with Microsoft");
    await signIn.waitForDisplayed({ timeout: 30_000 });
    assert.equal(await (await browser.$("h1")).getText(), "Imaginears Club");
    const footer = await (await browser.$("footer")).getText();
    assert.match(footer, /Launcher v\d+\.\d+\.\d+/);
  });

  check("shows the pack from the fixture", async () => {
    const latest = await browser.$("strong=9.9.9");
    await latest.waitForDisplayed({ timeout: 15_000 });
  });

  check("Settings keeps focus inside, closes with Esc and gives focus back", async () => {
    const sheet = await openSettings();
    for (let i = 0; i < 25; i++) {
      await browser.keys("Tab");
      const inside = await browser.execute(
        (el) => el.contains(document.activeElement),
        sheet,
      );
      assert.ok(inside, `focus left Settings after ${i + 1} Tab presses`);
    }
    await pressEscape();
    await sheet.waitForExist({ reverse: true });
    const focused = await browser.execute(() => document.activeElement?.textContent?.trim());
    assert.equal(focused, "Settings");
  });

  check("side cards fit, even with a long server address", async () => {
    // The real address fits in Windows' font but not in Linux's wider one; use a longer one.
    await browser.execute((name) => {
      const h = document.querySelector(".side .panel-head h2");
      if (h) h.textContent = name;
    }, LONG_ADDRESS);
    const m = await measureLayout();
    assert.ok(m.page <= 0, `page scrolls sideways by ${m.page}px at ${m.width}px wide`);
    assert.ok(m.cards <= 1, `a side card sticks out by ${m.cards}px at ${m.width}px wide`);
  });

  check("everything fits at 150% text size", async () => {
    await openSettings();
    const textSize = await browser.$("label*=Text size").$("select");
    await textSize.selectByAttribute("value", "150");
    await pressEscape();
    await browser.pause(500); // zoom relayout
    const m = await measureLayout();
    assert.ok(m.width < 760, `expected the one-column layout at 150%, got ${m.width}px`);
    assert.ok(m.page <= 0, `page scrolls sideways by ${m.page}px at 150%`);
    assert.ok(m.cards <= 1, `a side card sticks out by ${m.cards}px at 150%`);
    assert.ok(m.above <= 0, `the top of the home area is cut off by ${m.above}px at 150%`);
    assert.ok(m.sideScrolls <= 1, `the side cards scroll in their own box at 150% (${m.sideScrolls}px)`);

    // Back to 100% without clicking (clicks miss while zoomed on WebKit, see openSettings).
    await openSettings({ keyboard: true });
    await browser.execute((s) => {
      s.value = "100";
      s.dispatchEvent(new Event("change", { bubbles: true }));
    }, await browser.$("label*=Text size").$("select"));
    await pressEscape();
    await browser.waitUntil(
      async () => (await browser.execute(() => innerWidth)) >= 760,
      { timeoutMsg: "text size did not go back to 100%" },
    );
  });

  // Opt-in: E2E_SCREENSHOTS=1 saves the main screens to e2e/artifacts/ for a visual check.
  if (process.env.E2E_SCREENSHOTS) {
    check("screenshots of the main screens", async () => {
      mkdirSync(ARTIFACTS, { recursive: true });
      await browser.saveScreenshot(join(ARTIFACTS, "screen-play.png"));
      await (await browser.$("button=Mods & Shaders")).click();
      await (await browser.$("h2=Mods & Shaders")).waitForDisplayed();
      await browser.saveScreenshot(join(ARTIFACTS, "screen-mods.png"));
      await (await browser.$('button[title="Play"]')).click();
      await openSettings();
      await browser.saveScreenshot(join(ARTIFACTS, "screen-settings.png"));
      await pressEscape();
    });
  }

  check("the Custom mod picker lists and filters the optional mods", async () => {
    await openSettings();
    await (await browser.$("#preset-select")).selectByAttribute("value", "custom");
    await (await browser.$("button=Choose mods…")).click();
    const picker = await browser.$('[aria-labelledby="picker-title"]');
    await picker.waitForDisplayed();

    const names = () => browser.$$(".picker-list .toggle-name").map((e) => e.getText());
    assert.deepEqual(await names(), ["Shaders", "Dynamic lights", "Sound physics", "Bobby"]);

    const search = await browser.$('input[aria-label="Search optional mods"]');
    await search.setValue("light");
    assert.deepEqual(await names(), ["Dynamic lights"]);

    await search.setValue("zzz");
    const empty = await browser.$(".picker-empty");
    assert.match(await empty.getText(), /No mods match "zzz"/);

    await pressEscape();
    await picker.waitForExist({ reverse: true });
  });
});
