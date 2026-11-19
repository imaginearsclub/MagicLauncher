<script lang="ts">
  import { applyTextScale, modal } from "../lib/a11y";
  import { app, BIG_WINDOW_MEMORY_MB, CHANNEL_LABEL, SIZE_LABEL, WINDOW_SIZES } from "../lib/app.svelte";

  let confirmGoBack = $state(false);
  let confirmRepair = $state(false);

  function openModPicker() {
    app.showModPicker = true;
  }
</script>

{#if app.settings}
  {@const settings = app.settings}
  {@const pack = app.pack}
  {@const status = app.status}
  <div
    class="sheet"
    role="dialog"
    aria-modal="true"
    aria-labelledby="settings-title"
    use:modal={{ onclose: () => (app.showSettings = false) }}
  >
    <h2 id="settings-title">Settings</h2>
    <label>
      <input
        type="checkbox"
        checked={settings.memory_mb === null}
        onchange={(e) => (settings.memory_mb = e.currentTarget.checked ? null : 4096)}
      />
      Choose memory automatically
    </label>
    {#if settings.memory_mb !== null}
      <label>
        Memory: <span class="mono gold">{(settings.memory_mb / 1024).toFixed(1)} GB</span>
        <input type="range" min="2048" max="16384" step="512" bind:value={settings.memory_mb} />
      </label>
    {/if}
    {#if app.largeWindow}
      {#if settings.memory_mb === null}
        <span class="muted small">Your window size is large, so automatic memory adds 2 GB.</span>
      {:else if settings.memory_mb < BIG_WINDOW_MEMORY_MB}
        <div class="hint-row">
          <span class="small">4K and ultrawide work best with 10 GB or more.</span>
          <button class="secondary small" onclick={() => (settings.memory_mb = BIG_WINDOW_MEMORY_MB)}>
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
        <select id="preset-select" value={settings.preset ?? ""} onchange={(e) => app.choosePreset(e.currentTarget.value)}>
          <option value="">Pack default</option>
          {#each pack.presets as p (p.id)}
            <option value={p.id}>{p.name}</option>
          {/each}
          {#if pack.features.length}<option value="custom">Custom</option>{/if}
        </select>
        {#if settings.preset === "custom"}
          {@const total = pack.features.length}
          {@const onCount = pack.features.filter((f) => !settings.custom_features_off.includes(f.id)).length}
          <div class="hint-row custom-row">
            <span class="small">{onCount} of {total} optional mods on</span>
            <button class="secondary small" onclick={openModPicker}>Choose mods…</button>
          </div>
          <span class="muted small">Mods the server needs are always on. Applies the next time you press Play.</span>
        {:else}
          {@const chosen = pack.presets.find((p) => p.id === settings.preset)}
          {@const off = chosen?.disable ?? []}
          <span class="muted small">
            {chosen?.description ? `${chosen.description} ` : ""}{off.length
              ? `Turns off: ${off.map((id) => pack.features.find((f) => f.id === id)?.name ?? id).join(", ")}.`
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
          settings.text_scale = Number(e.currentTarget.value);
          applyTextScale(settings.text_scale);
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
          settings.window_width = w || null;
          settings.window_height = h || null;
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
          onchange={(e) => (settings.extra_jvm_args = e.currentTarget.value.split(/\s+/).filter(Boolean))}
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
          <option value="alpha">Alpha (Cast Members and testers)</option>
        {/if}
      </select>
    </label>
    {#if settings.channel === "alpha" && !status?.alpha_allowed}
      <p class="muted">Alpha isn't available for this account, so the launcher uses Beta.</p>
    {:else if settings.channel !== "release"}
      <p class="muted">
        Test builds can be unstable. They use their own game folder, so your Release game folder
        isn't touched. You stay on this channel until Release catches up.
      </p>
    {/if}

    {#if status && status.running !== "release" && !app.info?.dev_build}
      {#if confirmGoBack}
        <div class="warn">
          <p>
            This reinstalls the newest Release launcher, which can be <strong>older</strong> than the
            {CHANNEL_LABEL[status.running]} version you're running. Your Release game folder is untouched;
            the {CHANNEL_LABEL[status.running]} folder stays on disk.
          </p>
          <div class="row">
            <button class="secondary" onclick={() => (confirmGoBack = false)}>Cancel</button>
            <button
              class="danger"
              onclick={() => {
                confirmGoBack = false;
                app.goBackToRelease();
              }}
              disabled={app.busy}>Go back to Release</button
            >
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
        onchange={(e) => app.setTelemetry(e.currentTarget.checked)}
      />
      Send anonymous usage and crash reports
    </label>

    {#if app.profile}
      {#if confirmRepair}
        <div class="warn soft">
          <p>
            Re-checks Java, Minecraft, Fabric and every modpack file and re-downloads anything broken.
            Your settings, saves and screenshots are kept. Can take a few minutes.
          </p>
          <div class="row">
            <button class="secondary" onclick={() => (confirmRepair = false)}>Cancel</button>
            <button
              class="primary"
              onclick={() => {
                confirmRepair = false;
                app.runRepair();
              }}
              disabled={app.busy}>Repair now</button
            >
          </div>
        </div>
      {:else}
        <button class="link" onclick={() => (confirmRepair = true)} disabled={app.busy}>Repair game files…</button>
      {/if}
    {/if}

    <div class="row">
      <button class="secondary" onclick={() => (app.showSettings = false)}>Cancel</button>
      <button class="primary" onclick={() => app.saveSettings()}>Save</button>
    </div>
  </div>
{/if}
