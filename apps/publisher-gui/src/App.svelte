<script lang="ts">
  import { onMount } from "svelte";
  import {
    api,
    onLog,
    onUpdateProgress,
    onUploadProgress,
    type Info,
    type Maintenance,
    type Preview,
    type Suggestion,
  } from "./api";
  import { applyTextScale } from "../../../src/lib/a11y";

  const TABS = [
    ["publish", "Publish"],
    ["maintenance", "Maintenance"],
    ["optional", "Optional mods"],
    ["settings", "Settings"],
  ] as const;

  /** Text size is a per-PC preference, kept in this window's local storage. */
  function loadScale(): number {
    try {
      return Number(localStorage.getItem("text_scale")) || 100;
    } catch {
      return 100;
    }
  }
  let textScale = $state(loadScale());
  $effect(() => {
    applyTextScale(textScale);
    try {
      localStorage.setItem("text_scale", String(textScale));
    } catch {
      /* private mode: just don't remember */
    }
  });

  /** Arrow keys move between tabs (the ARIA tabs pattern). */
  function tabKeys(e: KeyboardEvent) {
    const ids = TABS.map(([id]) => id);
    let i = ids.indexOf(tab);
    if (e.key === "ArrowRight") i = (i + 1) % ids.length;
    else if (e.key === "ArrowLeft") i = (i - 1 + ids.length) % ids.length;
    else if (e.key === "Home") i = 0;
    else if (e.key === "End") i = ids.length - 1;
    else return;
    e.preventDefault();
    if (busy) return;
    tab = ids[i];
    document.getElementById(`tab-${ids[i]}`)?.focus();
  }

  type Tab = "publish" | "maintenance" | "optional" | "settings";
  let tab = $state<Tab>("publish");
  let info = $state<Info | null>(null);
  let keys = $state<{ from_env: boolean; saved: boolean } | null>(null);

  // ---- publish ----
  let channel = $state(""); // "" = Release
  let instance = $state("");
  let customPath = $state("");
  let memoryGb = $state(4);
  let minecraft = $state("");
  let fabric = $state("");
  let reuploadAll = $state(false);
  let force = $state(false);

  let checking = $state(false);
  let publishing = $state(false);
  let log = $state<string[]>([]);
  let preview = $state<Preview | null>(null);
  let checkError = $state<string | null>(null);
  let version = $state("");
  let notes = $state("");
  let confirmPublish = $state(false);
  let progress = $state<{ done: number; total: number } | null>(null);
  let result = $state<string | null>(null);
  let publishError = $state<string | null>(null);

  let busy = $derived(checking || publishing);
  let instancePath = $derived(instance === "__custom" ? customPath.trim().replace(/^"|"$/g, "") : instance);
  let channelName = $derived(channel ? channel[0].toUpperCase() + channel.slice(1) : "Release");

  // ---- maintenance ----
  let maint = $state<Maintenance | null>(null);
  let maintMessage = $state("");
  let maintBusy = $state(false);
  let maintConfirm = $state<"on" | "off" | null>(null);
  let maintResult = $state<string | null>(null);

  // ---- optional mods ----
  let suggestions = $state<Suggestion[] | null>(null);
  let suggestError = $state<string | null>(null);

  // ---- self-update ----
  let update = $state<{ version: string; notes: string | null } | null>(null);
  let updating = $state(false);
  let updatePercent = $state<number | null>(null);
  let updateError = $state<string | null>(null);

  async function runUpdate() {
    updating = true;
    updateError = null;
    try {
      await api.installUpdate(); // restarts the app when done
    } catch (e) {
      updateError = String(e);
      updating = false;
    }
  }

  // ---- settings ----
  let keyId = $state("");
  let keySecret = $state("");
  let keyMsg = $state<string | null>(null);

  onMount(() => {
    const un = [
      onLog((line) => (log = [...log, line])),
      onUploadProgress((p) => (progress = p)),
      onUpdateProgress((p) => {
        updatePercent = p.total ? Math.min(100, Math.round((p.downloaded / p.total) * 100)) : null;
      }),
    ];
    (async () => {
      info = await api.info();
      keys = await api.keysStatus();
      pickRemembered();
      refreshMaintenance();
      // No feed yet or offline: just no banner.
      update = await api.checkUpdate().catch(() => null);
    })();
    return () => un.forEach((p) => p.then((f) => f()));
  });

  function pickRemembered() {
    if (!info) return;
    const r = info.remembered[channel];
    if (r) {
      const known = info.instances.some((i) => i.path === r);
      instance = known ? r : "__custom";
      if (!known) customPath = r;
    } else if (info.instances.length) {
      instance = info.instances[0].path;
    } else {
      instance = "__custom";
    }
  }

  function resetPreview() {
    preview = null;
    checkError = null;
    result = null;
    publishError = null;
    confirmPublish = false;
    progress = null;
  }

  async function check() {
    if (!instancePath) return;
    resetPreview();
    log = [];
    checking = true;
    try {
      preview = await api.preview({
        instance: instancePath,
        channel: channel || null,
        minecraft: minecraft.trim() || null,
        fabric: fabric.trim() || null,
        memory_mb: Math.round(memoryGb * 1024),
        reupload_all: reuploadAll,
      });
      version = preview.suggested;
    } catch (e) {
      checkError = String(e);
    } finally {
      checking = false;
    }
  }

  async function doPublish() {
    confirmPublish = false;
    publishError = null;
    publishing = true;
    progress = null;
    try {
      result = await api.publish(version, notes.trim() || null, force);
      preview = null;
      notes = "";
    } catch (e) {
      publishError = String(e);
    } finally {
      publishing = false;
    }
  }

  async function refreshMaintenance() {
    try {
      maint = await api.maintenanceStatus();
      if (maint.message) maintMessage = maint.message;
    } catch {
      maint = null;
    }
  }

  async function doMaintenance(on: boolean) {
    maintConfirm = null;
    maintBusy = true;
    maintResult = null;
    try {
      await api.setMaintenance(on, on ? maintMessage.trim() || null : null);
      maintResult = on
        ? "Maintenance is ON. Launchers disable Play within about a minute."
        : "Maintenance is OFF. Players can play again within about a minute.";
      await refreshMaintenance();
    } catch (e) {
      maintResult = `Couldn't change maintenance: ${e}`;
    } finally {
      maintBusy = false;
    }
  }

  async function scanOptional() {
    suggestions = null;
    suggestError = null;
    try {
      suggestions = await api.suggestOptional(instancePath);
    } catch (e) {
      suggestError = String(e);
    }
  }

  async function saveKeys() {
    keyMsg = null;
    try {
      await api.saveKeys(keyId, keySecret);
      keyId = "";
      keySecret = "";
      keys = await api.keysStatus();
      keyMsg = "Saved in this computer's credential manager.";
    } catch (e) {
      keyMsg = `Couldn't save: ${e}`;
    }
  }

  const noKeys = $derived(keys !== null && !keys.saved && !keys.from_env);
</script>

<main>
  <header>
    <div>
      <h1>Imaginears PackPublisher</h1>
      <span class="muted small">v{info?.version ?? "…"}</span>
    </div>
    <div class="tabs" role="tablist" aria-label="Sections" tabindex="-1" onkeydown={tabKeys}>
      {#each TABS as [id, label]}
        <button
          id={`tab-${id}`}
          class="tab"
          class:active={tab === id}
          role="tab"
          aria-selected={tab === id}
          aria-controls="tab-panel"
          tabindex={tab === id ? 0 : -1}
          onclick={() => (tab = id as Tab)}
          disabled={busy && tab !== id}
        >
          {label}
        </button>
      {/each}
    </div>
  </header>

  {#if update}
    <div class="banner update" role="status">
      {#if updating}
        <span>Updating Imaginears PackPublisher… {updatePercent !== null ? `${updatePercent}%` : ""}</span>
        <div
          class="bar"
          role="progressbar"
          aria-label="Publisher update"
          aria-valuemin="0"
          aria-valuemax="100"
          aria-valuenow={updatePercent ?? undefined}
        >
          <div style={`width:${updatePercent ?? 100}%`}></div>
        </div>
      {:else}
        <span>
          Imaginears PackPublisher <strong>{update.version}</strong> is available.
          {#if update.notes}<span class="muted small">{update.notes}</span>{/if}
        </span>
        <button class="primary" onclick={runUpdate} disabled={busy}>
          {busy ? "Finish the current job first" : "Update now"}
        </button>
      {/if}
      {#if updateError}<p class="bad" role="alert">Update failed: {updateError}</p>{/if}
    </div>
  {/if}

  {#if noKeys && tab !== "settings"}
    <div class="banner warn" role="status">
      No AWS key saved on this PC yet. <button class="link" onclick={() => (tab = "settings")}>Add it in Settings</button> before publishing.
    </div>
  {/if}

  <div id="tab-panel" role="tabpanel" aria-labelledby={`tab-${tab}`}>
  {#if tab === "publish"}
    <section class="grid">
      <div class="card form">
        <label>
          Channel
          <select bind:value={channel} onchange={() => { pickRemembered(); resetPreview(); }} disabled={busy}>
            <option value="">Release (all players)</option>
            <option value="beta">Beta (test channel)</option>
            <option value="alpha">Alpha (test channel)</option>
          </select>
        </label>
        <label>
          Pack instance (Prism Launcher, Modrinth App or CurseForge)
          <select bind:value={instance} onchange={resetPreview} disabled={busy}>
            {#each info?.instances ?? [] as i (i.path)}
              <option value={i.path}>{i.name} ({i.kind}, Minecraft {i.minecraft || "?"}{i.loader ? `, ${i.loader}` : ""})</option>
            {/each}
            <option value="__custom">Another folder…</option>
          </select>
        </label>
        {#if instance === "__custom"}
          <label>
            Folder path
            <input class="text" bind:value={customPath} placeholder="C:\Users\…\AppData\Roaming\PrismLauncher\instances\…" disabled={busy} />
          </label>
        {/if}
        <label>
          Recommended memory: {memoryGb} GB
          <input type="range" min="2" max="16" step="1" bind:value={memoryGb} disabled={busy} />
        </label>
        <details class="advanced">
          <summary>Advanced</summary>
          <label>
            Minecraft version (leave empty to use the instance's; needed for plain folders)
            <input class="text" bind:value={minecraft} placeholder="e.g. 1.21.11" disabled={busy} />
          </label>
          <label>
            Fabric Loader version (leave empty to use the instance's)
            <input class="text" bind:value={fabric} placeholder="e.g. 0.20.1" disabled={busy} />
          </label>
          <label class="check"><input type="checkbox" bind:checked={reuploadAll} disabled={busy} /> Re-upload every file (repairs a broken bucket)</label>
          <label class="check"><input type="checkbox" bind:checked={force} disabled={busy} /> Publish even if nothing changed or the version isn't higher</label>
        </details>
        <button class="primary" onclick={check} disabled={busy || !instancePath}>
          {checking ? "Checking…" : "Check pack"}
        </button>
        <p class="muted small">Checking builds the pack and compares it with the live one. Nothing is uploaded.</p>
      </div>

      <div class="card results" aria-live="polite" aria-busy={checking || publishing}>
        {#if checking || (log.length && !preview && !checkError && !result)}
          <h2>Checking…</h2>
          <pre class="log" role="log" aria-label="Progress">{log.join("\n")}</pre>
        {:else if checkError}
          <h2 class="bad">✗ Can't publish this</h2>
          <pre class="error-box" role="alert">{checkError}</pre>
          <p class="muted small">Fix the problem in the instance, then press Check pack again.</p>
        {:else if result}
          <h2 class="good">✓ Done</h2>
          <p role="status">{result}</p>
        {:else if preview}
          <h2 class="good">✓ Safety checks passed</h2>
          <ul class="facts">
            <li>Live {channelName} pack: <strong>{preview.live_version ?? "none yet"}</strong></li>
            <li>Minecraft {preview.minecraft} · Fabric Loader {preview.fabric} · {((preview.memory_mb ?? 0) / 1024).toFixed(0)} GB recommended</li>
            <li>{preview.files} files ({preview.self_hosted_mods} self-hosted mods) · uploads {preview.uploads} file(s), {preview.upload_mb.toFixed(1)} MB</li>
          </ul>
          {#if preview.warnings.length}
            <div class="note">
              <strong>Worth a look</strong> (doesn't stop publishing):
              <ul>{#each preview.warnings as w}<li>{w}</li>{/each}</ul>
            </div>
          {/if}
          {#if preview.new_self_hosted.length}
            <div class="note">
              <strong>New mods hosted by us</strong> (not on Modrinth). Check their licenses allow it:
              <ul>{#each preview.new_self_hosted as m}<li>{m}</li>{/each}</ul>
            </div>
          {/if}
          {#if preview.unchanged && !force && !reuploadAll}
            <p><strong>Nothing changed:</strong> the live pack already matches this instance.</p>
          {:else}
            {#if preview.diff.length}
              <h3>Changes</h3>
              <pre class="diff">{preview.diff.join("\n")}</pre>
            {:else if preview.first_publish}
              <p>First publish of this channel.</p>
            {/if}
            <label>
              Version
              <input class="text" bind:value={version} disabled={publishing} />
            </label>
            <label>
              Note for players (optional, shown in the launcher's "What's new")
              <textarea class="text" rows="2" bind:value={notes} disabled={publishing}></textarea>
            </label>
            {#if publishing}
              <div class="progress">
                <div
                  class="bar"
                  role="progressbar"
                  aria-label="Uploading files"
                  aria-valuemin="0"
                  aria-valuemax={progress?.total || 100}
                  aria-valuenow={progress?.done ?? 0}
                >
                  <div style={`width:${progress && progress.total ? (progress.done / progress.total) * 100 : 5}%`}></div>
                </div>
                <span class="small muted">
                  {progress ? `Uploading files: ${progress.done}/${progress.total}` : "Uploading…"}
                </span>
              </div>
            {:else if confirmPublish}
              <div class="confirm" role="group" aria-label="Confirm publishing">
                <span>Publish pack <strong>{version}</strong> to <strong>{channelName}</strong> players?</span>
                <button class="secondary" onclick={() => (confirmPublish = false)}>Cancel</button>
                <!-- svelte-ignore a11y_autofocus -->
                <button class="primary" onclick={doPublish} autofocus>Yes, publish</button>
              </div>
            {:else}
              <button class="primary big" onclick={() => (confirmPublish = true)} disabled={noKeys || !version.trim()}>
                Publish {version}
              </button>
            {/if}
            {#if publishError}<pre class="error-box" role="alert">{publishError}</pre>{/if}
          {/if}
        {:else}
          <h2>Ready</h2>
          <p class="muted">Pick the instance and press <strong>Check pack</strong>.</p>
        {/if}
      </div>
    </section>
  {:else if tab === "maintenance"}
    <section class="card narrow">
      <h2>Maintenance mode</h2>
      <p class="muted">Disables Play in every launcher with your message, e.g. while the server updates.</p>
      <p>
        Now: {#if maint === null}<span class="muted">unknown</span>{:else if maint.maintenance}<strong class="bad">ON</strong>{maint.message ? ` · "${maint.message}"` : ""}{:else}<strong class="good">OFF</strong>{/if}
      </p>
      <label>
        Message for players
        <input class="text" bind:value={maintMessage} placeholder="Updating the pack, back at 18:00" disabled={maintBusy} />
      </label>
      {#if maintConfirm}
        <div class="confirm" role="group" aria-label="Confirm maintenance change">
          <span>{maintConfirm === "on" ? "Disable Play for all players?" : "Let players play again?"}</span>
          <button class="secondary" onclick={() => (maintConfirm = null)}>Cancel</button>
          <!-- svelte-ignore a11y_autofocus -->
          <button class="primary" onclick={() => doMaintenance(maintConfirm === "on")} autofocus>Yes</button>
        </div>
      {:else}
        <div class="row">
          <button class="danger" onclick={() => (maintConfirm = "on")} disabled={maintBusy || noKeys}>Turn on</button>
          <button class="secondary" onclick={() => (maintConfirm = "off")} disabled={maintBusy || noKeys}>Turn off</button>
          <button class="link" onclick={refreshMaintenance} disabled={maintBusy}>Refresh</button>
        </div>
      {/if}
      {#if maintResult}<p role="status">{maintResult}</p>{/if}
    </section>
  {:else if tab === "optional"}
    <section class="card narrow wide">
      <h2>Optional mods</h2>
      <p class="muted">
        Client-only mods that nothing requires, which could become switchable in the launcher's Custom preset.
        Test a mod switched off before adding it to <code>magic-pack.toml</code>. UI, library and
        performance mods are listed separately: those should stay on.
      </p>
      <button class="primary" onclick={scanOptional} disabled={!instancePath}>Scan {info?.instances.find((i) => i.path === instancePath)?.name ?? "instance"}</button>
      {#if suggestError}<pre class="error-box" role="alert">{suggestError}</pre>{/if}
      {#if suggestions}
        {@const cands = suggestions.filter((s) => !s.keep_reason)}
        {@const keep = suggestions.filter((s) => s.keep_reason)}
        <h3>Could be optional ({cands.length})</h3>
        {#if cands.length}
          <table><tbody>{#each cands as s}<tr><td>{s.name}</td><td class="muted">{s.id}</td><td class="muted small">{s.file}</td></tr>{/each}</tbody></table>
        {:else}<p class="muted">Nothing new: every candidate is already a toggle or on the keep list.</p>{/if}
        <h3>Probably keep ({keep.length})</h3>
        {#if keep.length}
          <table><tbody>{#each keep as s}<tr><td>{s.name}</td><td class="muted">{s.keep_reason}</td><td class="muted small">{s.file}</td></tr>{/each}</tbody></table>
        {:else}<p class="muted">None.</p>{/if}
      {/if}
    </section>
  {:else}
    <section class="card narrow">
      <h2>AWS key</h2>
      <p>
        {#if keys?.from_env}Using the key from the AWS_ACCESS_KEY_ID environment variable.
        {:else if keys?.saved}<span class="good">✓</span> A key is saved on this PC.
        {:else}<span class="bad">No key saved yet.</span>{/if}
      </p>
      <p class="muted small">
        Stored in this computer's credential manager, not in a file. The command-line pack-publisher uses the same key.
      </p>
      <label>Access key ID <input class="text" bind:value={keyId} autocomplete="off" /></label>
      <label>Secret access key <input class="text" type="password" bind:value={keySecret} autocomplete="off" /></label>
      <button class="primary" onclick={saveKeys} disabled={!keyId.trim() || !keySecret.trim()}>{keys?.saved ? "Replace key" : "Save key"}</button>
      {#if keyMsg}<p role="status">{keyMsg}</p>{/if}

      <h2 class="spaced">Display</h2>
      <label>
        Text size
        <select bind:value={textScale}>
          {#each [100, 115, 130, 150] as p}
            <option value={p}>{p}%{p === 100 ? " (default)" : ""}</option>
          {/each}
        </select>
      </label>
    </section>
  {/if}
  </div>
</main>
