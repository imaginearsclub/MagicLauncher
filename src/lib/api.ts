// Typed wrappers around the Rust commands in src-tauri/src/lib.rs.
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export interface AppInfo {
  version: string;
  server: string;
  dev_build: boolean;
  sentry_dsn_frontend: string;
  sentry_test: boolean;
}

export interface CrashInfo {
  cause: string;
  title: string;
  hint: string;
  culprit: string | null;
  exit_code: number | null;
  crash_report: string | null;
}

export interface Profile {
  name: string;
  uuid: string;
}

export interface Accounts {
  active: string | null;
  list: Profile[];
}

export interface DeviceCode {
  user_code: string;
  device_code: string;
  verification_uri: string;
  expires_in: number;
  interval: number;
  message: string;
}

export type Channel = "release" | "beta" | "alpha";

export interface Settings {
  memory_mb: number | null;
  auto_join: boolean;
  close_on_launch: boolean;
  extra_jvm_args: string[];
  channel: Channel;
  /** null = not asked yet (first-start prompt shows). */
  telemetry: boolean | null;
  window_width: number | null;
  window_height: number | null;
  fullscreen: boolean;
  background_updates: boolean;
  discord_presence: boolean;
  /** Graphics preset id, or "custom"; null = pack default (everything on). */
  preset: string | null;
  /** With preset "custom": optional features switched off. */
  custom_features_off: string[];
  /** Launcher text size in percent (100 = default). */
  text_scale: number;
}

export interface ChannelStatus {
  selected: Channel;
  /** Alpha falls back to Beta when the account isn't allowlisted. */
  effective: Channel;
  alpha_allowed: boolean;
  /** Channel of the running launcher version. */
  running: Channel;
}

export interface UpdateInfo {
  version: string;
  notes: string | null;
  mandatory: boolean;
  /** "Go back to Release": installs an older version. */
  downgrade: boolean;
}

export interface ServerStatus {
  online: number;
  max: number;
  motd: string;
  version: string;
  latency_ms: number;
  sample: string[];
}

export interface PackChanges {
  from?: string;
  notes?: string;
  added: string[];
  removed: string[];
  updated: string[];
  other_files: number;
}

export interface PackPreset {
  id: string;
  name: string;
  description?: string;
  disable: string[];
}

export interface PackFeature {
  id: string;
  name: string;
  description?: string;
  default: boolean;
}

export interface PackOverview {
  installed: string | null;
  latest: string;
  update_available: boolean;
  changes: PackChanges | null;
  presets: PackPreset[];
  features: PackFeature[];
}

export type PackCheck =
  | ({ status: "ready" } & PackOverview)
  | { status: "not_published" }
  | { status: "unreachable" };

export interface MaintenanceStatus {
  maintenance: boolean;
  message: string | null;
  can_bypass: boolean;
}

export interface UpdateProgress {
  downloaded: number;
  total: number | null;
}

export interface Progress {
  stage: string;
  done: number;
  total: number;
}

/** What an `imaginears://` link asks for (see src-tauri/src/deeplink.rs). */
export type DeepLinkAction = { action: "open" } | { action: "play" } | { action: "link"; token: string };

/** Shown before linking the signed-in Minecraft account to a website account. */
export interface LinkPrompt {
  website_account: string;
  minecraft_name: string;
  expires_in_seconds: number;
}

export const api = {
  appInfo: () => invoke<AppInfo>("app_info"),
  account: () => invoke<Profile | null>("account"),
  loginStart: () => invoke<DeviceCode>("login_start"),
  loginFinish: (code: DeviceCode) => invoke<Profile>("login_finish", { code }),
  logout: () => invoke<void>("logout"),
  accounts: () => invoke<Accounts>("accounts"),
  switchAccount: (uuid: string) => invoke<Profile>("switch_account", { uuid }),
  getSettings: () => invoke<Settings>("get_settings"),
  saveSettings: (settings: Settings) => invoke<void>("save_settings", { settings }),
  openGameFolder: () => invoke<void>("open_game_folder"),
  play: () => invoke<string>("play"),
  /** Debug builds only. */
  playOffline: (username: string) => invoke<string>("play_offline", { username }),
  channelStatus: () => invoke<ChannelStatus>("channel_status"),
  /** `null` = server offline/unreachable. */
  serverStatus: () => invoke<ServerStatus | null>("server_status"),
  /** `null` = pack couldn't be fetched (offline). */
  packOverview: () => invoke<PackCheck>("pack_overview"),
  maintenanceStatus: () => invoke<MaintenanceStatus>("maintenance_status"),
  /** Re-verify and fix all game files for the current channel; resolves to the pack version. */
  repair: () => invoke<string>("repair"),
  skinUrl: () => invoke<string | null>("skin_url"),
  setTelemetry: (enabled: boolean) => invoke<void>("set_telemetry", { enabled }),
  /** Scrubbed log of the last crash (for Copy log). */
  crashLog: () => invoke<string>("crash_log"),
  /** Uploads the scrubbed crash log to mclo.gs; resolves to the public link. */
  shareCrashLog: () => invoke<string>("share_crash_log"),
  openLogsFolder: () => invoke<void>("open_logs_folder"),
  /** Background pre-download; resolves to the pack version, or null if the launcher was busy. */
  prepareUpdate: () => invoke<string | null>("prepare_update"),
  /** `goBack`: check the Release feed and allow installing an older version. */
  checkUpdate: (goBack = false) => invoke<UpdateInfo | null>("check_update", { goBack }),
  /** Installs the update from the last check, then the launcher restarts. */
  installUpdate: () => invoke<void>("install_update"),
  /** The `imaginears://` action waiting for the UI (taken once), or null. */
  takeDeepLink: () => invoke<DeepLinkAction | null>("take_deep_link"),
  /** Who an `imaginears://link/<token>` link would link. Changes nothing. */
  linkPrompt: (token: string) => invoke<LinkPrompt>("link_prompt", { token }),
  /** After the player confirmed: verify the account with Mojang and link it. Resolves to the name. */
  linkConfirm: (token: string) => invoke<string>("link_confirm", { token }),
};

export const DOWNLOAD_PAGE = "https://launcher.imaginears.club/launcher/download/index.html";

export const onUpdateProgress = (cb: (p: UpdateProgress) => void): Promise<UnlistenFn> =>
  listen<UpdateProgress>("update-progress", (e) => cb(e.payload));

export const onProgress = (cb: (p: Progress) => void): Promise<UnlistenFn> =>
  listen<Progress>("progress", (e) => cb(e.payload));

export const onGameCrashed = (cb: (c: CrashInfo) => void): Promise<UnlistenFn> =>
  listen<CrashInfo>("game-crashed", (e) => cb(e.payload));

/** An `imaginears://` link was opened; call `api.takeDeepLink()` for its action. */
export const onDeepLink = (cb: () => void): Promise<UnlistenFn> => listen("deep-link", () => cb());

export const onGameExited = (cb: (code: number | null) => void): Promise<UnlistenFn> =>
  listen<number | null>("game-exited", (e) => cb(e.payload));
