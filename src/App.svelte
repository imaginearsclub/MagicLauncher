<script lang="ts">
  import { onMount } from "svelte";
  import logo from "./assets/logo.png";
  import PlayerHead from "./lib/PlayerHead.svelte";
  import { applyTextScale, modal, popover } from "./lib/a11y";
  import { startSentry, stopSentry } from "./lib/sentry";
  import {
    api,
    DOWNLOAD_PAGE,
    onGameCrashed,
    onGameExited,
    onProgress,
    onUpdateProgress,
    type AppInfo,
    type Channel,
    type Accounts,
    type ChannelStatus,
    type CrashInfo,
    type DeviceCode,
    type MaintenanceStatus,
    type PackOverview,
    type Profile,
    type Progress,
    type ServerStatus,
    type Settings,
    type UpdateInfo,
  } from "./lib/api";

  /** Mirrors launcher-core LARGE_WINDOW_PIXELS: above 2560×1440 needs more heap. */
  const LARGE_WINDOW_PIXELS = 2560 * 1440;
  const BIG_WINDOW_MEMORY_MB = 10240;
  const WINDOW_SIZES = [
    [1280, 720],
    [1600, 900],
    [1920, 1080],
    [2560, 1080],
    [2560, 1440],
    [3440, 1440],
    [3840, 2160],
  ] as const;
  const SIZE_LABEL: Record<string, string> = {
    "1920x1080": "Full HD",
    "2560x1080": "ultrawide",
    "2560x1440": "QHD",
    "3440x1440": "ultrawide",
    "3840x2160": "4K",
  };

  const CHANNEL_LABEL: Record<Channel, string> = { release: "Release", beta: "Beta", alpha: "Alpha" };
  const STATUS_REFRESH_MS = 60_000;

  let info = $state<AppInfo | null>(null);
  let profile = $state<Profile | null>(null);
  let skin = $state<string | null>(null);
  let settings = $state<Settings | null>(null);
  let status = $state<ChannelStatus | null>(null);
  let server = $state<ServerStatus | null>(null);
  let serverChecked = $state(false);
  let pack = $state<PackOverview | null>(null);
  let maintenance = $state<MaintenanceStatus | null>(null);
  let code = $state<DeviceCode | null>(null);
  let progress = $state<Progress | null>(null);
  let busy = $state(false);
  let error = $state<string | null>(null);
  let notice = $state<string | null>(null);
  let showSettings = $state(false);
  let largeWindow = $derived(
    !!settings?.window_width &&
      !!settings?.window_height &&
      settings.window_width * settings.window_height > LARGE_WINDOW_PIXELS,
  );
  let showChanges = $state(false);
  let confirmGoBack = $state(false);
  let confirmRepair = $state(false);
  let offlineName = $state("Dev");
  let crashInfo = $state<CrashInfo | null>(null);
  let crashMsg = $state<string | null>(null);
  let confirmShare = $state(false);
  let predownloading = $state(false);
  let accountList = $state<Accounts | null>(null);
  let showAccounts = $state(false);
  let update = $state<UpdateInfo | null>(null);
  let updating = $state(false);
  let updatePercent = $state<number | null>(null);
  let updateError = $state<string | null>(null);

  const percent = $derived(
    progress && progress.total > 0 ? Math.round((progress.done / progress.total) * 100) : 0,
  );
  const blockedByUpdate = $derived(!!update?.mandatory && !updateError);
  const blockedByMaintenance = $derived(!!maintenance?.maintenance && !maintenance.can_bypass);
  const changeCount = $derived(
    pack?.changes
      ? pack.changes.added.length + pack.changes.removed.length + pack.changes.updated.length
      : 0,
  );

  onMount(() => {
    const unlisten = [
      onProgress((p) => (progress = p)),
      onUpdateProgress((p) => {
        updatePercent = p.total ? Math.min(100, Math.round((p.downloaded / p.total) * 100)) : null;
      }),
      onGameCrashed((c) => {
        crashInfo = c;
        crashMsg = null;
        confirmShare = false;
      }),
      onGameExited(() => {
        busy = false;
        progress = null;
        notice = null;
        refreshHome();
      }),
    ];
    (async () => {
      info = await api.appInfo();
      settings = await api.getSettings();
      applyTextScale(settings.text_scale);
      if (settings.telemetry === true || info.sentry_test) {
        startSentry(info.sentry_dsn_frontend, info.version, info.dev_build);
      }
      if (info.sentry_test) {
        // Dev only: a real uncaught UI error, to verify the frontend Sentry setup.
        setTimeout(() => {
          throw new Error(`Sentry frontend test error ${Date.now()}`);
        }, 1500);
      }
      refreshServer();
      try {
        profile = await api.account();
      } catch (e) {
        error = String(e);
      }
      accountList = await api.accounts().catch(() => null);
      await refreshChannel();
      refreshHome();
    })();
    const timer = setInterval(() => {
      if (!document.hidden) refreshServer();
    }, STATUS_REFRESH_MS);
    return () => {
      clearInterval(timer);
      unlisten.forEach((u) => u.then((f) => f()));
    };
  });

  async function refreshServer() {
    try {
      server = await api.serverStatus();
    } catch {
      server = null;
    }
    serverChecked = true;
  }

  /** Pack overview, maintenance and the player's skin: things that depend on the account/channel. */
  async function refreshHome() {
    api
      .packOverview()
      .then((p) => {
        pack = p;
        maybePredownload();
      })
      .catch(() => (pack = null));
    api.maintenanceStatus().then((m) => (maintenance = m)).catch(() => (maintenance = null));
    // The skin comes from a local cache (Mojang is asked at most every few hours), so keep the
    // current head on a failed lookup instead of flashing back to the letter.
    if (profile) api.skinUrl().then((u) => (skin = u)).catch(() => {});
    else skin = null;
  }

  /** Pack update available and the launcher idle: download it now so Play starts at once. */
  async function maybePredownload() {
    if (!pack?.update_available || !profile || busy || !settings?.background_updates) return;
    if (maintenance?.maintenance && !maintenance.can_bypass) return;
    busy = true;
    predownloading = true;
    try {
      const v = await api.prepareUpdate();
      if (v) {
        notice = `Pack ${v} is downloaded and ready.`;
        pack = await api.packOverview();
      }
    } catch (e) {
      // Not fatal: Play will try again and show the error then.
      console.warn("background download failed", e);
    } finally {
      busy = false;
      predownloading = false;
      progress = null;
    }
  }

  /** Channel access depends on the signed-in account; re-check after sign-in/out and saves. */
  async function refreshChannel() {
    try {
      status = await api.channelStatus();
    } catch (e) {
      console.warn("channel status failed", e);
    }
    // Dev builds never self-update (a release would replace your dev binary).
    if (info && !info.dev_build) {
      try {
        update = await api.checkUpdate(false);
      } catch (e) {
        // Offline or no feed yet: behave as "no update"; Play must keep working.
        console.warn("update check failed", e);
        update = null;
      }
    }
  }

  async function signIn() {
    error = null;
    try {
      code = await api.loginStart();
      profile = await api.loginFinish(code);
      await afterAccountChange();
    } catch (e) {
      error = String(e);
    } finally {
      code = null;
    }
  }

  /** Sign out of the active account; another saved account (if any) takes over. */
  async function signOut() {
    showAccounts = false;
    await api.logout();
    profile = null;
    try {
      profile = await api.account();
    } catch (e) {
      error = String(e);
    }
    await afterAccountChange();
  }

  async function switchTo(uuid: string) {
    showAccounts = false;
    error = null;
    try {
      profile = await api.switchAccount(uuid);
    } catch (e) {
      error = String(e);
    }
    await afterAccountChange();
  }

  /** Add another account: the normal sign-in flow, which makes the new one active. */
  async function addAccount() {
    showAccounts = false;
    await signIn();
  }

  async function afterAccountChange() {
    accountList = await api.accounts().catch(() => null);
    skin = null;
    await refreshChannel();
    refreshHome();
  }

  async function launch(offline = false) {
    error = null;
    notice = null;
    busy = true;
    try {
      const packVersion = offline ? await api.playOffline(offlineName) : await api.play();
      notice = `Pack ${packVersion} is running. Have fun!`;
      progress = null;
      refreshHome();
    } catch (e) {
      error = String(e);
      busy = false;
      progress = null;
    }
  }

  async function runRepair() {
    confirmRepair = false;
    showSettings = false;
    error = null;
    notice = null;
    busy = true;
    try {
      const v = await api.repair();
      notice = `Repair finished: pack ${v} and all game files are verified.`;
      refreshHome();
    } catch (e) {
      error = `Repair failed: ${e}`;
    } finally {
      busy = false;
      progress = null;
    }
  }

  async function runUpdate() {
    if (!update || busy) return;
    updating = true;
    updateError = null;
    updatePercent = null;
    try {
      await api.installUpdate(); // restarts the launcher on success
    } catch (e) {
      updating = false;
      updateError = `The update couldn't be installed automatically (${e}). Download it from ${DOWNLOAD_PAGE}`;
    }
  }

  /** First-start prompt and the Settings toggle. */
  async function setTelemetry(enabled: boolean) {
    if (!settings || !info) return;
    await api.setTelemetry(enabled);
    settings.telemetry = enabled;
    if (enabled) startSentry(info.sentry_dsn_frontend, info.version, info.dev_build);
    else await stopSentry();
  }

  async function copyCrashLog() {
    try {
      await navigator.clipboard.writeText(await api.crashLog());
      crashMsg = "Log copied. Paste it in the Imaginears Club Discord support channel.";
    } catch (e) {
      crashMsg = `Couldn't copy the log: ${e}`;
    }
  }

  async function shareCrashLog() {
    confirmShare = false;
    crashMsg = "Uploading…";
    try {
      const url = await api.shareCrashLog();
      await navigator.clipboard.writeText(url).catch(() => {});
      crashMsg = `Link copied: ${url}. Send it to staff on Discord.`;
    } catch (e) {
      crashMsg = `Couldn't upload the log: ${e}`;
    }
  }

  /** Pick a preset; "custom" starts from what the current choice switches off. */
  function choosePreset(id: string) {
    if (!settings || !pack) return;
    if (id === "custom" && settings.preset !== "custom") {
      const current = pack.presets.find((p) => p.id === settings!.preset);
      settings.custom_features_off = current
        ? [...current.disable]
        : pack.features.filter((f) => !f.default).map((f) => f.id);
    }
    settings.preset = id || null;
  }

  let showModPicker = $state(false);
  let modSearch = $state("");
  let filteredFeatures = $derived.by(() => {
    const q = modSearch.trim().toLowerCase();
    const all = pack?.features ?? [];
    return q
      ? all.filter((f) => `${f.name} ${f.description ?? ""}`.toLowerCase().includes(q))
      : all;
  });

  function openModPicker() {
    modSearch = "";
    showModPicker = true;
  }

  /** All on/off for the mods currently shown (respects the search). */
  function setAllShown(on: boolean) {
    if (!settings) return;
    const ids = new Set(filteredFeatures.map((f) => f.id));
    const rest = settings.custom_features_off.filter((x) => !ids.has(x));
    settings.custom_features_off = on ? rest : [...rest, ...ids];
  }

  function toggleFeature(id: string, on: boolean) {
    if (!settings) return;
    const off = settings.custom_features_off.filter((x) => x !== id);
    settings.custom_features_off = on ? off : [...off, id];
  }

  async function saveSettings() {
    if (!settings) return;
    await api.saveSettings($state.snapshot(settings));
    applyTextScale(settings.text_scale);
    showSettings = false;
    await refreshChannel();
    refreshHome();
  }

  /** Confirmed "go back to Release": switch channel, then install the newest Release build. */
  async function goBackToRelease() {
    if (!settings) return;
    confirmGoBack = false;
    settings.channel = "release";
    await api.saveSettings($state.snapshot(settings));
    showSettings = false;
    updateError = null;
    try {
      update = await api.checkUpdate(true);
      if (update) await runUpdate();
      else notice = "No Release build is available right now.";
    } catch (e) {
      updateError = `Couldn't switch back to Release (${e}). Download it from ${DOWNLOAD_PAGE}`;
    }
    status = await api.channelStatus();
  }
</script>

<main>
  <header>
    <div class="brand">
      <img src={logo} alt="" class="logo" />
      <div>
        <h1>Imaginears Club</h1>
        <span class="tagline">MagicLauncher</span>
      </div>
    </div>
    {#if profile}
      <div class="account">
        <button
          class="account-button"
          onclick={() => (showAccounts = !showAccounts)}
          aria-expanded={showAccounts}
          aria-controls="account-menu"
          aria-label={`Account: ${profile.name}. Switch or add accounts`}
        >
          <PlayerHead url={skin} name={profile.name} size={30} />
          <span>{profile.name}</span>
          <span class="caret" aria-hidden="true">▾</span>
        </button>
        {#if showAccounts}
          <div
            class="account-menu"
            id="account-menu"
            role="group"
            aria-label="Accounts"
            use:popover={{ onclose: () => (showAccounts = false) }}
          >
            {#each accountList?.list ?? [] as a (a.uuid)}
              <button
                class:current={a.uuid === profile.uuid}
                aria-current={a.uuid === profile.uuid ? "true" : undefined}
                onclick={() => a.uuid !== profile!.uuid && switchTo(a.uuid)}
                disabled={busy || a.uuid === profile.uuid}
              >
                {a.name}{a.uuid === profile.uuid ? " ✓" : ""}
              </button>
            {/each}
            <hr />
            <button onclick={addAccount} disabled={busy}>Add account…</button>
            <button onclick={signOut} disabled={busy}>Sign out of {profile.name}</button>
          </div>
        {/if}
      </div>
    {/if}
  </header>

  {#if update}
    <div class="banner update" role="status">
      {#if updating}
        <span>Updating the launcher… {updatePercent !== null ? `${updatePercent}%` : ""}</span>
        <div
          class="bar"
          role="progressbar"
          aria-label="Launcher update"
          aria-valuemin="0"
          aria-valuemax="100"
          aria-valuenow={updatePercent ?? undefined}
        >
          <div style={`width:${updatePercent ?? 100}%`}></div>
        </div>
      {:else}
        <span>
          {#if update.downgrade}
            Switching back to Release {update.version}.
          {:else}
            Launcher {update.version} is available{update.mandatory ? " and required to play" : ""}.
          {/if}
          {#if update.notes}<span class="muted">{update.notes}</span>{/if}
        </span>
        <button class="primary" onclick={runUpdate} disabled={busy}>
          {busy ? "Close the game to update" : "Update now"}
        </button>
      {/if}
      {#if updateError}<p class="error" role="alert">{updateError}</p>{/if}
    </div>
  {/if}

  {#if maintenance?.maintenance}
    <div class="banner maintenance" role="alert">
      <strong>Server maintenance</strong>
      <span>{maintenance.message ?? "The server is being updated. Please check back soon."}</span>
      {#if maintenance.can_bypass}
        <span class="muted">You're an Alpha tester, so you can still play to test it.</span>
      {/if}
    </div>
  {/if}

  <div class="home">
    <section class="hero">
    {#if crashInfo}
      <div class="card crash" role="alert">
        <button class="crash-close" aria-label="Dismiss" title="Dismiss" onclick={() => (crashInfo = null)}>×</button>
        <div class="crash-head">
          <span class="crash-icon" aria-hidden="true">!</span>
          <div>
            <h2>{crashInfo.title}</h2>
            {#if crashInfo.culprit}<span class="chip">Mod involved: {crashInfo.culprit}</span>{/if}
          </div>
        </div>
        <p class="crash-hint">{crashInfo.hint}</p>
        {#if confirmShare}
          <p class="small">
            This uploads the log to <strong>mclo.gs</strong> and makes it <strong>public</strong> to anyone
            with the link. Your Windows/macOS user name and login token are removed first.
          </p>
          <div class="crash-actions">
            <button class="primary" onclick={shareCrashLog}>Upload and copy link</button>
            <button class="secondary" onclick={() => (confirmShare = false)}>Cancel</button>
          </div>
        {:else}
          <div class="crash-actions">
            {#if crashInfo.cause === "out_of_memory" || crashInfo.cause === "memory_setting"}
              <button class="primary" onclick={() => (showSettings = true)}>Memory settings</button>
            {/if}
            <button class="secondary" onclick={() => (confirmShare = true)}>Share link</button>
            <button class="secondary" onclick={copyCrashLog}>Copy log</button>
            <button class="link" onclick={() => api.openLogsFolder()}>Open logs</button>
          </div>
        {/if}
        {#if crashMsg}<p class="crash-msg selectable">{crashMsg}</p>{/if}
      </div>
    {/if}

      {#if code}
        <div class="card">
          <p>Go to <strong>{code.verification_uri}</strong> and enter this code:</p>
          <p class="code">{code.user_code}</p>
          <button class="secondary" onclick={() => navigator.clipboard.writeText(code!.user_code)}>Copy code</button>
          <p class="muted">Waiting for you to finish signing in…</p>
        </div>
      {:else if !profile}
        <div class="card">
          <p>Sign in with the Microsoft account that owns Minecraft: Java Edition.</p>
          <button class="primary" onclick={signIn}>Sign in with Microsoft</button>
          {#if info?.dev_build}
            <div class="dev">
              <label>Dev offline launch <input bind:value={offlineName} /></label>
              <button class="secondary" onclick={() => launch(true)} disabled={busy}>Play offline (dev)</button>
            </div>
          {/if}
        </div>
      {:else}
        <button
          class="play"
          onclick={() => launch()}
          disabled={busy || updating || blockedByUpdate || blockedByMaintenance}
        >
          {busy
            ? predownloading
              ? "Downloading update…"
              : "Working…"
            : blockedByUpdate
              ? "Update required"
              : blockedByMaintenance
                ? "Maintenance"
                : "Play"}
        </button>
        {#if pack?.update_available && !busy}
          <span class="muted">
            {pack.installed ? `Pack update ${pack.installed} → ${pack.latest}` : `Installs pack ${pack.latest}`}
            when you press Play
          </span>
        {/if}
      {/if}

      {#if progress}
        <div class="progress">
          <div
            class="bar"
            role="progressbar"
            aria-label={progress.stage}
            aria-valuemin="0"
            aria-valuemax="100"
            aria-valuenow={Math.round(percent)}
          >
            <div style={`width:${percent}%`}></div>
          </div>
          <span aria-live="polite">{progress.stage}{progress.total > 1 ? ` · ${progress.done}/${progress.total}` : ""}</span>
        </div>
      {/if}
      {#if error}<p class="error" role="alert">{error}</p>{/if}
      {#if notice}<p class="notice" role="status">{notice}</p>{/if}
    </section>

    <aside class="side">
      <div class="panel">
        <div class="panel-head">
          <span class={`dot ${server ? "on" : serverChecked ? "off" : ""}`} aria-hidden="true"></span>
          <h2>{info?.server ?? "Server"}</h2>
        </div>
        {#if server}
          <p class="big">{server.online}&nbsp;<span class="muted">/ {server.max} online</span></p>
          <p class="muted small">{server.latency_ms} ms · {server.version}</p>
        {:else if serverChecked}
          <p class="muted">Offline or unreachable right now.</p>
        {:else}
          <p class="muted">Checking…</p>
        {/if}
      </div>

      <div class="panel">
        <div class="panel-head"><h2>Modpack</h2></div>
        {#if pack}
          <p>
            <strong>{pack.latest}</strong>
            {#if pack.installed && !pack.update_available}<span class="muted"> · up to date</span>{/if}
            {#if pack.update_available}<span class="pill">update</span>{/if}
          </p>
          {#if pack.changes}
            {#if pack.changes.notes}<p class="small">{pack.changes.notes}</p>{/if}
            {#if changeCount > 0 || pack.changes.other_files > 0}
              <p class="muted small">
                {[
                  pack.changes.added.length && `+${pack.changes.added.length} added`,
                  pack.changes.removed.length && `−${pack.changes.removed.length} removed`,
                  pack.changes.updated.length && `${pack.changes.updated.length} updated`,
                  pack.changes.other_files && `${pack.changes.other_files} config changes`,
                ].filter(Boolean).join(" · ")}
              </p>
              {#if changeCount > 0}
                <button class="link" onclick={() => (showChanges = !showChanges)}>
                  {showChanges ? "Hide details" : "What's new"}
                </button>
              {/if}
            {/if}
            {#if showChanges}
              <div class="changes">
                {#each [["Added", pack.changes.added], ["Removed", pack.changes.removed], ["Updated", pack.changes.updated]] as [title, list]}
                  {#if (list as string[]).length}
                    <h3>{title}</h3>
                    <ul>{#each list as string[] as m}<li>{m}</li>{/each}</ul>
                  {/if}
                {/each}
              </div>
            {/if}
          {/if}
        {:else}
          <p class="muted">Couldn't check for pack updates.</p>
        {/if}
      </div>
    </aside>
  </div>

  <footer>
    <span class="muted">
      Launcher v{info?.version ?? "…"}
      {#if status && status.running !== "release"}
        <span class={`badge ${status.running}`}>{CHANNEL_LABEL[status.running].toUpperCase()}</span>
      {/if}
      {#if status && status.effective !== "release"}
        · {CHANNEL_LABEL[status.effective]} channel{status.selected !== status.effective
          ? ` (${CHANNEL_LABEL[status.selected]} not available)`
          : ""}
      {/if}
    </span>
    <div>
      <button class="link" onclick={() => api.openGameFolder()}>Game folder</button>
      <button class="link" onclick={() => (showSettings = !showSettings)}>Settings</button>
    </div>
  </footer>

  {#if settings && settings.telemetry === null && info && !info.sentry_test}
    <div class="overlay">
      <div class="card consent" role="dialog" aria-modal="true" aria-labelledby="consent-title" use:modal>
        <h2 id="consent-title">Help improve the launcher?</h2>
        <p>
          Send <strong>anonymous</strong> usage and crash reports: launcher and pack version, your
          operating system, whether installs and launches worked, and what kind of crash happened.
        </p>
        <p class="muted small">
          Never your Minecraft name, account, IP address or game logs. You can change this any
          time in Settings.
        </p>
        <div class="row center">
          <button class="secondary" onclick={() => setTelemetry(false)}>No thanks</button>
          <button class="primary" onclick={() => setTelemetry(true)}>Yes, send reports</button>
        </div>
      </div>
    </div>
  {/if}

  {#if showModPicker && settings && pack}
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
    <div class="overlay" onclick={(e) => e.target === e.currentTarget && (showModPicker = false)}>
      <div
        class="card picker"
        role="dialog"
        aria-modal="true"
        aria-labelledby="picker-title"
        use:modal={{ onclose: () => (showModPicker = false) }}
      >
        <div class="picker-head">
          <h2 id="picker-title">Optional mods</h2>
          <button class="crash-close" aria-label="Close" title="Close" onclick={() => (showModPicker = false)}>×</button>
        </div>
        <p class="muted small">
          Switch off what your PC doesn't need. Mods the server needs aren't listed and are always on.
        </p>
        <div class="picker-tools">
          <!-- svelte-ignore a11y_autofocus -->
          <input
            class="text"
            type="search"
            placeholder="Search mods"
            aria-label="Search optional mods"
            bind:value={modSearch}
            autofocus
          />
          <button class="link small" onclick={() => setAllShown(true)}>All on</button>
          <button class="link small" onclick={() => setAllShown(false)}>All off</button>
        </div>
        <ul class="toggles picker-list" aria-label="Optional mods">
          {#each filteredFeatures as f (f.id)}
            {@const on = !settings.custom_features_off.includes(f.id)}
            <li>
              <label class="toggle" class:off={!on}>
                <span class="toggle-text">
                  <span class="toggle-name">{f.name}</span>
                  {#if f.description}<span class="toggle-desc">{f.description}</span>{/if}
                </span>
                <input
                  type="checkbox"
                  role="switch"
                  class="switch"
                  checked={on}
                  onchange={(e) => toggleFeature(f.id, e.currentTarget.checked)}
                />
              </label>
            </li>
          {:else}
            <li class="picker-empty muted small">No mods match "{modSearch}".</li>
          {/each}
        </ul>
        <div class="picker-foot">
          <span class="muted small">
            {pack.features.filter((f) => !settings!.custom_features_off.includes(f.id)).length} of {pack.features.length} on
          </span>
          <button class="primary" onclick={() => (showModPicker = false)}>Done</button>
        </div>
      </div>
    </div>
  {/if}

  {#if showSettings && settings}
    <div
      class="sheet"
      role="dialog"
      aria-modal="true"
      aria-labelledby="settings-title"
      use:modal={{ onclose: () => (showSettings = false) }}
    >
      <h2 id="settings-title">Settings</h2>
      <label>
        <input
          type="checkbox"
          checked={settings.memory_mb === null}
          onchange={(e) => (settings!.memory_mb = e.currentTarget.checked ? null : 6144)}
        />
        Choose memory automatically
      </label>
      {#if settings.memory_mb !== null}
        <label>
          Memory: {(settings.memory_mb / 1024).toFixed(1)} GB
          <input type="range" min="2048" max="16384" step="512" bind:value={settings.memory_mb} />
        </label>
      {/if}
      {#if largeWindow}
        {#if settings.memory_mb === null}
          <span class="muted small">Your window size is large, so automatic memory adds 2 GB.</span>
        {:else if settings.memory_mb < BIG_WINDOW_MEMORY_MB}
          <div class="hint-row">
            <span class="small">4K and ultrawide work best with 10 GB or more.</span>
            <button class="secondary small" onclick={() => (settings!.memory_mb = BIG_WINDOW_MEMORY_MB)}>
              Use 10 GB
            </button>
          </div>
        {/if}
      {/if}
      <label><input type="checkbox" bind:checked={settings.auto_join} /> Join the server automatically</label>
      <label><input type="checkbox" bind:checked={settings.close_on_launch} /> Hide launcher while playing</label>

      {#if pack?.presets.length}
        <div class="field">
          <label for="preset-select">Graphics preset</label>
          <select
            id="preset-select"
            value={settings.preset ?? ""}
            onchange={(e) => choosePreset(e.currentTarget.value)}
          >
            <option value="">Pack default</option>
            {#each pack.presets as p (p.id)}
              <option value={p.id}>{p.name}</option>
            {/each}
            {#if pack.features.length}<option value="custom">Custom</option>{/if}
          </select>
          {#if settings.preset === "custom"}
            {@const total = pack.features.length}
            {@const onCount = pack.features.filter((f) => !settings!.custom_features_off.includes(f.id)).length}
            <div class="hint-row custom-row">
              <span class="small">{onCount} of {total} optional mods on</span>
              <button class="secondary small" onclick={openModPicker}>Choose mods…</button>
            </div>
            <span class="muted small">Mods the server needs are always on. Applies the next time you press Play.</span>
          {:else}
            {@const chosen = pack.presets.find((p) => p.id === settings!.preset)}
            {@const off = chosen?.disable ?? []}
            <span class="muted small">
              {chosen?.description ? `${chosen.description} ` : ""}{off.length
                ? `Turns off: ${off.map((id) => pack!.features.find((f) => f.id === id)?.name ?? id).join(", ")}.`
                : chosen
                  ? ""
                  : "Everything on."}
              Applies the next time you press Play.
            </span>
          {/if}
        </div>
      {/if}

      <label>
        Text size
        <select
          value={String(settings.text_scale ?? 100)}
          onchange={(e) => {
            settings!.text_scale = Number(e.currentTarget.value);
            applyTextScale(settings!.text_scale);
          }}
        >
          {#each [100, 115, 130, 150] as p}
            <option value={String(p)}>{p}%{p === 100 ? " (default)" : ""}</option>
          {/each}
        </select>
      </label>

      <label>
        Game window size
        <select
          value={settings.window_width ? `${settings.window_width}x${settings.window_height}` : ""}
          onchange={(e) => {
            const [w, h] = e.currentTarget.value.split("x").map(Number);
            settings!.window_width = w || null;
            settings!.window_height = h || null;
          }}
        >
          <option value="">Minecraft default</option>
          {#each WINDOW_SIZES as [w, h]}
            <option value={`${w}x${h}`}>
              {w} × {h}{SIZE_LABEL[`${w}x${h}`] ? ` (${SIZE_LABEL[`${w}x${h}`]})` : ""}
            </option>
          {/each}
        </select>
      </label>
      <label><input type="checkbox" bind:checked={settings.fullscreen} /> Start in fullscreen</label>
      <label>
        <input type="checkbox" bind:checked={settings.background_updates} />
        Download modpack updates in the background
      </label>
      <label>
        <input type="checkbox" bind:checked={settings.discord_presence} />
        Show "Playing Imaginears Club" on Discord
      </label>

      <details class="advanced">
        <summary>Advanced</summary>
        <label>
          Extra Java arguments
          <input
            class="text"
            placeholder="e.g. -XX:+UseZGC"
            value={settings.extra_jvm_args.join(" ")}
            onchange={(e) => (settings!.extra_jvm_args = e.currentTarget.value.split(/\s+/).filter(Boolean))}
          />
          <span class="muted small">Only change this if you know what it does. Memory is set above.</span>
        </label>
      </details>

      <label>
        Update channel
        <select bind:value={settings.channel}>
          <option value="release">Release</option>
          <option value="beta">Beta (public testing)</option>
          {#if status?.alpha_allowed || settings.channel === "alpha"}
            <option value="alpha">Alpha (staff and testers)</option>
          {/if}
        </select>
      </label>
      {#if settings.channel === "alpha" && !status?.alpha_allowed}
        <p class="muted">Alpha isn't available for this account, so the launcher uses Beta.</p>
      {:else if settings.channel !== "release"}
        <p class="muted">
          Test builds can be unstable. They use their own game folder, so your Release
          game folder isn't touched. You stay on this channel until Release catches up.
        </p>
      {/if}

      {#if status && status.running !== "release" && !info?.dev_build}
        {#if confirmGoBack}
          <div class="warn">
            <p>
              This reinstalls the newest Release launcher, which can be <strong>older</strong>
              than the {CHANNEL_LABEL[status.running]} version you're running. Your Release game
              folder is untouched; the {CHANNEL_LABEL[status.running]} folder stays on disk.
            </p>
            <div class="row">
              <button class="secondary" onclick={() => (confirmGoBack = false)}>Cancel</button>
              <button class="danger" onclick={goBackToRelease} disabled={busy}>Go back to Release</button>
            </div>
          </div>
        {:else}
          <button class="link" onclick={() => (confirmGoBack = true)}>Go back to Release…</button>
        {/if}
      {/if}

      <label>
        <input
          type="checkbox"
          checked={settings.telemetry === true}
          onchange={(e) => setTelemetry(e.currentTarget.checked)}
        />
        Send anonymous usage and crash reports
      </label>

      {#if profile}
        {#if confirmRepair}
          <div class="warn soft">
            <p>
              Re-checks Java, Minecraft, Fabric and every modpack file and re-downloads anything
              broken. Your settings, saves and screenshots are kept. Can take a few minutes.
            </p>
            <div class="row">
              <button class="secondary" onclick={() => (confirmRepair = false)}>Cancel</button>
              <button class="primary" onclick={runRepair} disabled={busy}>Repair now</button>
            </div>
          </div>
        {:else}
          <button class="link" onclick={() => (confirmRepair = true)} disabled={busy}>Repair game files…</button>
        {/if}
      {/if}

      <div class="row">
        <button class="secondary" onclick={() => (showSettings = false)}>Cancel</button>
        <button class="primary" onclick={saveSettings}>Save</button>
      </div>
    </div>
  {/if}
</main>
