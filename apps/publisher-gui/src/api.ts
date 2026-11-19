import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export interface Instance {
  name: string;
  path: string;
  /** "Prism Launcher", "CurseForge" or "folder" (Modrinth App profile / plain folder). */
  kind: string;
  minecraft: string;
  loader: string | null;
}

export interface Info {
  version: string;
  instances: Instance[];
  /** Remembered instance per channel ("" = Release). */
  remembered: Record<string, string>;
}

export interface Preview {
  live_version: string | null;
  suggested: string;
  unchanged: boolean;
  first_publish: boolean;
  minecraft: string;
  fabric: string;
  memory_mb: number | null;
  files: number;
  self_hosted_mods: number;
  uploads: number;
  upload_mb: number;
  diff: string[];
  new_self_hosted: string[];
  warnings: string[];
}

export interface Maintenance {
  maintenance: boolean;
  message?: string;
  since?: number;
}

export interface Suggestion {
  name: string;
  id: string;
  file: string;
  keep_reason: string | null;
}

export const api = {
  info: () => invoke<Info>("info"),
  checkInstance: (path: string) => invoke<Instance>("check_instance", { path }),
  preview: (args: {
    instance: string;
    channel: string | null;
    minecraft: string | null;
    fabric: string | null;
    memory_mb: number | null;
    reupload_all: boolean;
  }) => invoke<Preview>("preview", { args }),
  publish: (version: string, notes: string | null, force: boolean) =>
    invoke<string>("publish", { version, notes, force }),
  keysStatus: () => invoke<{ from_env: boolean; saved: boolean }>("keys_status"),
  saveKeys: (accessKeyId: string, secretAccessKey: string) =>
    invoke<void>("save_keys", { accessKeyId, secretAccessKey }),
  maintenanceStatus: () => invoke<Maintenance>("maintenance_status"),
  setMaintenance: (on: boolean, message: string | null) =>
    invoke<void>("set_maintenance", { on, message }),
  suggestOptional: (instance: string) => invoke<Suggestion[]>("suggest_optional", { instance }),
  checkUpdate: () => invoke<{ version: string; notes: string | null } | null>("check_update"),
  installUpdate: () => invoke<void>("install_update"),
};

export const onUpdateProgress = (
  cb: (p: { downloaded: number; total: number | null }) => void,
): Promise<UnlistenFn> =>
  listen<{ downloaded: number; total: number | null }>("update-progress", (e) => cb(e.payload));

export const onLog = (cb: (line: string) => void): Promise<UnlistenFn> =>
  listen<string>("log", (e) => cb(e.payload));

export const onUploadProgress = (cb: (p: { done: number; total: number }) => void): Promise<UnlistenFn> =>
  listen<{ done: number; total: number }>("upload-progress", (e) => cb(e.payload));
