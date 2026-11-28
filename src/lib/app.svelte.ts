// Shared launcher state and actions. Every view/component reads `app` and calls its methods,
// so the screens stay small and nothing has to be threaded through props.

import { applyTextScale } from "./a11y";
import { startSentry, stopSentry } from "./sentry";
import {
  api,
  DOWNLOAD_PAGE,
  onGameCrashed,
  onGameExited,
  onDeepLink,
  onProgress,
  onUpdateProgress,
  type Accounts,
  type AppInfo,
  type Channel,
  type ChannelStatus,
  type CrashInfo,
  type DeviceCode,
  type LinkPrompt,
  type MaintenanceStatus,
  type PackCheck,
  type PackOverview,
  type Profile,
  type Progress,
  type ServerStatus,
  type Settings,
  type UpdateInfo,
} from "./api";

/** Mirrors launcher-core LARGE_WINDOW_PIXELS: above 2560×1440 needs more heap. */
const LARGE_WINDOW_PIXELS = 2560 * 1440;
export const BIG_WINDOW_MEMORY_MB = 10240;
export const WINDOW_SIZES = [
  [1280, 720],
  [1600, 900],
  [1920, 1080],
  [2560, 1080],
  [2560, 1440],
  [3440, 1440],
  [3840, 2160],
] as const;
export const SIZE_LABEL: Record<string, string> = {
  "1920x1080": "Full HD",
  "2560x1080": "ultrawide",
  "2560x1440": "QHD",
  "3440x1440": "ultrawide",
  "3840x2160": "4K",
};
export const CHANNEL_LABEL: Record<Channel, string> = { release: "Release", beta: "Beta", alpha: "Alpha" };
const STATUS_REFRESH_MS = 60_000;

export type View = "play" | "mods";

/** The "link your Minecraft account to the website" prompt, opened by `imaginears://link/…`. */
export type LinkDialog =
  | { step: "loading" }
  | { step: "confirm" | "linking"; token: string; prompt: LinkPrompt }
  | { step: "done"; minecraftName: string; purpose: LinkPrompt["purpose"] }
  | { step: "error"; message: string; signIn: boolean; purpose?: LinkPrompt["purpose"] };

class AppState {
  // ---- data from the launcher ----
  info = $state<AppInfo | null>(null);
  profile = $state<Profile | null>(null);
  skin = $state<string | null>(null);
  settings = $state<Settings | null>(null);
  status = $state<ChannelStatus | null>(null);
  server = $state<ServerStatus | null>(null);
  serverChecked = $state(false);
  pack = $state<PackOverview | null>(null);
  /** Why `pack` is null: not checked yet, nothing published for the channel, or unreachable. */
  packState = $state<"unchecked" | "not_published" | "unreachable" | "ready">("unchecked");
  maintenance = $state<MaintenanceStatus | null>(null);
  accountList = $state<Accounts | null>(null);
  update = $state<UpdateInfo | null>(null);

  // ---- what's going on ----
  code = $state<DeviceCode | null>(null);
  progress = $state<Progress | null>(null);
  busy = $state(false);
  predownloading = $state(false);
  error = $state<string | null>(null);
  notice = $state<string | null>(null);
  updating = $state(false);
  updatePercent = $state<number | null>(null);
  updateError = $state<string | null>(null);
  crashInfo = $state<CrashInfo | null>(null);
  crashMsg = $state<string | null>(null);

  // ---- UI ----
  view = $state<View>("play");
  showSettings = $state(false);
  showModPicker = $state(false);
  showAccounts = $state(false);
  linkDialog = $state<LinkDialog | null>(null);
  /** Bumped when a link starts or the dialog closes; results of older requests are dropped. */
  private linkRequest = 0;
  offlineName = $state("Dev");

  percent = $derived(
    this.progress && this.progress.total > 0
      ? Math.round((this.progress.done / this.progress.total) * 100)
      : 0,
  );
  blockedByUpdate = $derived(!!this.update?.mandatory && !this.updateError);
  blockedByMaintenance = $derived(!!this.maintenance?.maintenance && !this.maintenance.can_bypass);
  largeWindow = $derived(
    !!this.settings?.window_width &&
      !!this.settings?.window_height &&
      this.settings.window_width * this.settings.window_height > LARGE_WINDOW_PIXELS,
  );

  /** Start listening and load everything. Returns the cleanup for `onMount`. */
  start(): () => void {
    const unlisten = [
      onProgress((p) => (this.progress = p)),
      onUpdateProgress((p) => {
        this.updatePercent = p.total ? Math.min(100, Math.round((p.downloaded / p.total) * 100)) : null;
      }),
      onGameCrashed((c) => {
        this.crashInfo = c;
        this.crashMsg = null;
      }),
      onGameExited(() => {
        this.busy = false;
        this.progress = null;
        this.notice = null;
        this.refreshHome();
      }),
    ];
    // Take the waiting action only once the listener is registered: a link that arrives in
    // between would otherwise emit to nobody and wait until the next link.
    const deepLink = onDeepLink(() => this.openDeepLink());
    unlisten.push(deepLink);
    deepLink.then(() => this.openDeepLink());
    (async () => {
      this.info = await api.appInfo();
      this.settings = await api.getSettings();
      applyTextScale(this.settings.text_scale);
      if (this.settings.telemetry === true || this.info.sentry_test) {
        startSentry(this.info.sentry_dsn_frontend, this.info.version, this.info.dev_build);
      }
      if (this.info.sentry_test) {
        // Dev only: a real uncaught UI error, to verify the frontend Sentry setup.
        setTimeout(() => {
          throw new Error(`Sentry frontend test error ${Date.now()}`);
        }, 1500);
      }
      this.refreshServer();
      try {
        this.profile = await api.account();
      } catch (e) {
        this.error = String(e);
      }
      this.accountList = await api.accounts().catch(() => null);
      await this.refreshChannel();
      this.refreshHome();
    })();
    const timer = setInterval(() => {
      if (!document.hidden) this.refreshServer();
    }, STATUS_REFRESH_MS);
    return () => {
      clearInterval(timer);
      unlisten.forEach((u) => u.then((f) => f()));
    };
  }

  async refreshServer() {
    try {
      this.server = await api.serverStatus();
    } catch {
      this.server = null;
    }
    this.serverChecked = true;
  }

  /** Pack overview, maintenance and the player's skin: things that depend on the account/channel. */
  refreshHome() {
    api
      .packOverview()
      .then((p) => {
        this.setPack(p);
        this.maybePredownload();
      })
      .catch(() => this.setPack({ status: "unreachable" }));
    api
      .maintenanceStatus()
      .then((m) => (this.maintenance = m))
      .catch(() => (this.maintenance = null));
    // The skin comes from a local cache (Mojang is asked at most every few hours), so keep the
    // current head on a failed lookup instead of flashing back to the letter.
    if (this.profile) api.skinUrl().then((u) => (this.skin = u)).catch(() => {});
    else this.skin = null;
  }

  setPack(p: PackCheck) {
    this.packState = p.status;
    this.pack = p.status === "ready" ? p : null;
  }

  /** Pack update available and the launcher idle: download it now so Play starts at once. */
  async maybePredownload() {
    if (!this.pack?.update_available || !this.profile || this.busy || !this.settings?.background_updates)
      return;
    if (this.blockedByMaintenance) return;
    this.busy = true;
    this.predownloading = true;
    try {
      const v = await api.prepareUpdate();
      if (v) {
        this.notice = `Pack ${v} is downloaded and ready.`;
        this.setPack(await api.packOverview());
      }
    } catch (e) {
      // Not fatal: Play will try again and show the error then.
      console.warn("background download failed", e);
    } finally {
      this.busy = false;
      this.predownloading = false;
      this.progress = null;
    }
  }

  /** Channel access depends on the signed-in account; re-check after sign-in/out and saves. */
  async refreshChannel() {
    try {
      this.status = await api.channelStatus();
    } catch (e) {
      console.warn("channel status failed", e);
    }
    // Dev builds never self-update (a release would replace your dev binary).
    if (this.info && !this.info.dev_build) {
      try {
        this.update = await api.checkUpdate(false);
      } catch (e) {
        // Offline or no feed yet: behave as "no update"; Play must keep working.
        console.warn("update check failed", e);
        this.update = null;
      }
    }
  }

  // ---- accounts ----

  async signIn() {
    this.error = null;
    try {
      this.code = await api.loginStart();
      this.profile = await api.loginFinish(this.code);
      await this.afterAccountChange();
    } catch (e) {
      this.error = String(e);
    } finally {
      this.code = null;
    }
  }

  /** Sign out of the active account; another saved account (if any) takes over. */
  async signOut() {
    this.showAccounts = false;
    await api.logout();
    this.profile = null;
    try {
      this.profile = await api.account();
    } catch (e) {
      this.error = String(e);
    }
    await this.afterAccountChange();
  }

  async switchTo(uuid: string) {
    this.showAccounts = false;
    this.error = null;
    try {
      this.profile = await api.switchAccount(uuid);
    } catch (e) {
      this.error = String(e);
    }
    await this.afterAccountChange();
  }

  /** Add another account: the normal sign-in flow, which makes the new one active. */
  async addAccount() {
    this.showAccounts = false;
    this.view = "play";
    await this.signIn();
  }

  async afterAccountChange() {
    this.accountList = await api.accounts().catch(() => null);
    this.skin = null;
    await this.refreshChannel();
    this.refreshHome();
  }

  // ---- website links ----

  /** Apply a waiting `imaginears://` action. Never starts the game: the player clicks Play. */
  async openDeepLink() {
    const action = await api.takeDeepLink().catch(() => null);
    if (action?.action === "link") {
      await this.startLink(action.token);
    } else if (action?.action === "play") {
      this.showSettings = false;
      this.showModPicker = false;
      this.showAccounts = false;
      this.view = "play";
      this.refreshServer();
    }
  }

  // ---- linking the account to the website ----

  /** Shows who would be linked. Nothing is linked until the player confirms. */
  async startLink(token: string) {
    const request = ++this.linkRequest;
    this.linkDialog = { step: "loading" };
    try {
      const prompt = await api.linkPrompt(token);
      if (request !== this.linkRequest) return;
      this.linkDialog = { step: "confirm", token, prompt };
    } catch (e) {
      if (request !== this.linkRequest) return;
      const message = String(e);
      const signIn = /not signed in/i.test(message);
      this.linkDialog = {
        step: "error",
        signIn,
        message: signIn
          ? "Sign in to the launcher first, then click Verify on the website again."
          : message,
      };
    }
  }

  async confirmLink() {
    const d = this.linkDialog;
    if (d?.step !== "confirm") return;
    const request = this.linkRequest;
    this.linkDialog = { ...d, step: "linking" };
    try {
      const { purpose } = d.prompt;
      const minecraftName = await api.linkConfirm(d.token, d.prompt.minecraft_uuid, purpose);
      if (request === this.linkRequest) this.linkDialog = { step: "done", minecraftName, purpose };
    } catch (e) {
      if (request === this.linkRequest) {
        this.linkDialog = { step: "error", message: String(e), signIn: false, purpose: d.prompt.purpose };
      }
    }
  }

  /** Not while linking: the website call is already under way and its result should show. */
  closeLink() {
    if (this.linkDialog?.step === "linking") return;
    this.linkRequest++;
    this.linkDialog = null;
  }

  /** "Sign in" in the link dialog: start the Microsoft sign-in from the Play screen. */
  signInForLink() {
    this.closeLink();
    this.showSettings = false;
    this.showModPicker = false;
    this.showAccounts = false;
    this.view = "play";
    void this.signIn();
  }

  // ---- playing ----

  async launch(offline = false) {
    this.error = null;
    this.notice = null;
    this.busy = true;
    try {
      const packVersion = offline ? await api.playOffline(this.offlineName) : await api.play();
      this.notice = `Pack ${packVersion} is running. Have a magical day!`;
      this.progress = null;
      this.refreshHome();
    } catch (e) {
      this.error = String(e);
      this.busy = false;
      this.progress = null;
    }
  }

  async runRepair() {
    this.showSettings = false;
    this.view = "play";
    this.error = null;
    this.notice = null;
    this.busy = true;
    try {
      const v = await api.repair();
      this.notice = `Repair finished: pack ${v} and all game files are verified.`;
      this.refreshHome();
    } catch (e) {
      this.error = `Repair failed: ${e}`;
    } finally {
      this.busy = false;
      this.progress = null;
    }
  }

  async runUpdate() {
    if (!this.update || this.busy) return;
    this.updating = true;
    this.updateError = null;
    this.updatePercent = null;
    try {
      await api.installUpdate(); // restarts the launcher on success
    } catch (e) {
      this.updating = false;
      this.updateError = `The update couldn't be installed automatically (${e}). Download it from ${DOWNLOAD_PAGE}`;
    }
  }

  // ---- crash helper ----

  async copyCrashLog() {
    try {
      await navigator.clipboard.writeText(await api.crashLog());
      this.crashMsg = "Log copied. Paste it in the Imaginears Club Discord support channel.";
    } catch (e) {
      this.crashMsg = `Couldn't copy the log: ${e}`;
    }
  }

  async shareCrashLog() {
    this.crashMsg = "Uploading…";
    try {
      const url = await api.shareCrashLog();
      await navigator.clipboard.writeText(url).catch(() => {});
      this.crashMsg = `Link copied: ${url}. Send it to staff on Discord.`;
    } catch (e) {
      this.crashMsg = `Couldn't upload the log: ${e}`;
    }
  }

  // ---- settings ----

  /** First-start prompt and the Settings toggle. */
  async setTelemetry(enabled: boolean) {
    if (!this.settings || !this.info) return;
    await api.setTelemetry(enabled);
    this.settings.telemetry = enabled;
    if (enabled) startSentry(this.info.sentry_dsn_frontend, this.info.version, this.info.dev_build);
    else await stopSentry();
  }

  /** Pick a preset; "custom" starts from what the current choice switches off. */
  choosePreset(id: string) {
    const s = this.settings;
    if (!s || !this.pack) return;
    if (id === "custom" && s.preset !== "custom") {
      const current = this.pack.presets.find((p) => p.id === s.preset);
      s.custom_features_off = current
        ? [...current.disable]
        : this.pack.features.filter((f) => !f.default).map((f) => f.id);
    }
    s.preset = id || null;
  }

  /** Is an optional feature on with the current preset / custom choice? */
  featureOn(id: string): boolean {
    const s = this.settings;
    if (!s || !this.pack) return true;
    if (s.preset === "custom") return !s.custom_features_off.includes(id);
    const preset = this.pack.presets.find((p) => p.id === s.preset);
    if (preset) return !preset.disable.includes(id);
    return this.pack.features.find((f) => f.id === id)?.default ?? true;
  }

  toggleFeature(id: string, on: boolean) {
    if (!this.settings) return;
    const off = this.settings.custom_features_off.filter((x) => x !== id);
    this.settings.custom_features_off = on ? off : [...off, id];
  }

  /** All on/off for the given feature ids. */
  setAll(ids: string[], on: boolean) {
    if (!this.settings) return;
    const set = new Set(ids);
    const rest = this.settings.custom_features_off.filter((x) => !set.has(x));
    this.settings.custom_features_off = on ? rest : [...rest, ...set];
  }

  /** Save without closing anything (the Mods & Shaders screen saves each change). */
  async saveQuietly() {
    if (!this.settings) return;
    try {
      await api.saveSettings($state.snapshot(this.settings));
    } catch (e) {
      this.error = `Couldn't save: ${e}`;
    }
  }

  async saveSettings() {
    if (!this.settings) return;
    await api.saveSettings($state.snapshot(this.settings));
    applyTextScale(this.settings.text_scale);
    this.showSettings = false;
    await this.refreshChannel();
    this.refreshHome();
  }

  /** Confirmed "go back to Release": switch channel, then install the newest Release build. */
  async goBackToRelease() {
    if (!this.settings) return;
    this.settings.channel = "release";
    await api.saveSettings($state.snapshot(this.settings));
    this.showSettings = false;
    this.updateError = null;
    try {
      this.update = await api.checkUpdate(true);
      if (this.update) await this.runUpdate();
      else this.notice = "No Release build is available right now.";
    } catch (e) {
      this.updateError = `Couldn't switch back to Release (${e}). Download it from ${DOWNLOAD_PAGE}`;
    }
    this.status = await api.channelStatus();
  }
}

export const app = new AppState();
