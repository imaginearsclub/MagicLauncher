<script lang="ts">
  import { app } from "../lib/app.svelte";
  import CrashCard from "../components/CrashCard.svelte";
  import Icon from "../components/Icon.svelte";

  let showChanges = $state(false);
  let copied = $state(false);

  const changeCount = $derived(
    app.pack?.changes
      ? app.pack.changes.added.length + app.pack.changes.removed.length + app.pack.changes.updated.length
      : 0,
  );

  const playLabel = $derived(
    app.busy
      ? app.predownloading
        ? "Downloading update…"
        : "Working…"
      : app.blockedByUpdate
        ? "Update required"
        : app.blockedByMaintenance
          ? "Maintenance"
          : "Play",
  );

  async function copyAddress() {
    if (!app.info) return;
    await navigator.clipboard.writeText(app.info.server).catch(() => {});
    copied = true;
    setTimeout(() => (copied = false), 1500);
  }
</script>

<div class="home">
  <section class="hero">
    {#if app.crashInfo}<CrashCard />{/if}

    <div class="cockpit">
      <div class="cockpit-head">
        <span class="eyebrow">To all who come to this happy place: welcome</span>
        <h2 class="display">Imaginears Magic Pack</h2>
        <p class="cockpit-sub">
          {#if app.pack}
            <span class="mono">Pack {app.pack.latest}</span>
            {#if app.pack.installed && !app.pack.update_available}<span class="mono ok">· up to date</span>{/if}
          {:else if app.packState === "not_published"}
            <span class="mono muted">No pack on this channel yet</span>
          {:else}
            <span class="mono muted">Pack not checked yet</span>
          {/if}
        </p>
      </div>

      {#if app.code}
        <div class="card signin">
          <p>Go to <strong class="selectable">{app.code.verification_uri}</strong> and enter this code:</p>
          <p class="code">{app.code.user_code}</p>
          <button class="secondary" onclick={() => navigator.clipboard.writeText(app.code!.user_code)}>
            <Icon name="copy" size={18} />Copy code
          </button>
          <p class="muted">Waiting for you to finish signing in…</p>
        </div>
      {:else if !app.profile}
        <div class="card signin">
          <p>Sign in with the Microsoft account that owns Minecraft: Java Edition.</p>
          <button class="primary" onclick={() => app.signIn()}>Sign in with Microsoft</button>
          {#if app.info?.dev_build}
            <div class="dev">
              <label>Dev offline launch <input bind:value={app.offlineName} /></label>
              <button class="secondary" onclick={() => app.launch(true)} disabled={app.busy}>Play offline (dev)</button>
            </div>
          {/if}
        </div>
      {:else}
        <button
          class="play"
          onclick={() => app.launch()}
          disabled={app.busy || app.updating || app.blockedByUpdate || app.blockedByMaintenance}
        >
          <Icon name="play" size={34} />
          <span>{playLabel}</span>
        </button>
        {#if app.pack?.update_available && !app.busy}
          <span class="muted">
            {app.pack.installed
              ? `Pack update ${app.pack.installed} → ${app.pack.latest}`
              : `Installs pack ${app.pack.latest}`}
            when you press Play
          </span>
        {/if}
      {/if}

      {#if app.progress}
        <div class="progress">
          <div
            class="bar"
            role="progressbar"
            aria-label={app.progress.stage}
            aria-valuemin="0"
            aria-valuemax="100"
            aria-valuenow={app.percent}
          >
            <div style={`width:${app.percent}%`}></div>
          </div>
          <span aria-live="polite">
            {app.progress.stage}{app.progress.total > 1 ? ` · ${app.progress.done}/${app.progress.total}` : ""}
          </span>
        </div>
      {/if}
      {#if app.error}<p class="error" role="alert">{app.error}</p>{/if}
      {#if app.notice}<p class="notice" role="status">{app.notice}</p>{/if}
    </div>
  </section>

  <aside class="side">
    <div class="panel">
      <div class="panel-head">
        <span class={`dot ${app.server ? "on" : app.serverChecked ? "off" : ""}`} aria-hidden="true"></span>
        <h2>{app.info?.server ?? "Server"}</h2>
        {#if app.info}
          <button
            class="icon-button"
            onclick={copyAddress}
            aria-label={copied ? "Address copied" : `Copy server address ${app.info.server}`}
            title={copied ? "Copied" : "Copy address"}
          >
            <Icon name={copied ? "check" : "copy"} size={16} />
          </button>
        {/if}
      </div>
      {#if app.server}
        <p class="big">{app.server.online}&nbsp;<span class="muted">/ {app.server.max} online</span></p>
        <p class="muted small mono">{app.server.latency_ms} ms · {app.server.version}</p>
      {:else if app.serverChecked}
        <p class="muted">Offline or unreachable right now.</p>
      {:else}
        <p class="muted">Checking…</p>
      {/if}
    </div>

    <div class="panel">
      <div class="panel-head">
        <Icon name="extension" size={18} />
        <h2>Modpack</h2>
      </div>
      {#if app.pack}
        <p>
          <strong>{app.pack.latest}</strong>
          {#if app.pack.installed && !app.pack.update_available}<span class="muted"> · up to date</span>{/if}
          {#if app.pack.update_available}<span class="pill">update</span>{/if}
        </p>
        {#if app.pack.changes}
          {#if app.pack.changes.notes}<p class="small">{app.pack.changes.notes}</p>{/if}
          {#if changeCount > 0 || app.pack.changes.other_files > 0}
            <p class="muted small">
              {[
                app.pack.changes.added.length && `+${app.pack.changes.added.length} added`,
                app.pack.changes.removed.length && `−${app.pack.changes.removed.length} removed`,
                app.pack.changes.updated.length && `${app.pack.changes.updated.length} updated`,
                app.pack.changes.other_files && `${app.pack.changes.other_files} config changes`,
              ]
                .filter(Boolean)
                .join(" · ")}
            </p>
            {#if changeCount > 0}
              <button class="link" onclick={() => (showChanges = !showChanges)}>
                {showChanges ? "Hide details" : "What's new"}
              </button>
            {/if}
          {/if}
          {#if showChanges}
            <div class="changes">
              {#each [["Added", app.pack.changes.added], ["Removed", app.pack.changes.removed], ["Updated", app.pack.changes.updated]] as [title, list]}
                {#if (list as string[]).length}
                  <h3>{title}</h3>
                  <ul>{#each list as string[] as m}<li>{m}</li>{/each}</ul>
                {/if}
              {/each}
            </div>
          {/if}
        {/if}
        {#if app.pack.features.length}
          <button class="link" onclick={() => (app.view = "mods")}>
            <Icon name="tune" size={16} />Mods &amp; shaders
          </button>
        {/if}
      {:else if app.packState === "not_published"}
        <p class="muted">No modpack has been published on this channel yet.</p>
      {:else if app.packState === "unreachable"}
        <p class="muted">Couldn't check for pack updates.</p>
      {:else}
        <p class="muted">Checking for pack updates…</p>
      {/if}
    </div>
  </aside>
</div>
