<script lang="ts">
  import { api } from "../lib/api";
  import { app } from "../lib/app.svelte";
  import Icon from "./Icon.svelte";

  let confirmShare = $state(false);
</script>

{#if app.crashInfo}
  {@const c = app.crashInfo}
  <div class="card crash" role="alert">
    <button class="crash-close" aria-label="Dismiss" title="Dismiss" onclick={() => (app.crashInfo = null)}>
      <Icon name="close" size={18} />
    </button>
    <div class="crash-head">
      <span class="crash-icon" aria-hidden="true"><Icon name="warning" size={18} /></span>
      <div>
        <h2>{c.title}</h2>
        {#if c.culprit}<span class="chip">Mod involved: {c.culprit}</span>{/if}
      </div>
    </div>
    <p class="crash-hint">{c.hint}</p>
    {#if confirmShare}
      <p class="small">
        This uploads the log to <strong>mclo.gs</strong> and makes it <strong>public</strong> to anyone
        with the link. Your Windows/macOS user name and login token are removed first.
      </p>
      <div class="crash-actions">
        <button
          class="primary"
          onclick={() => {
            confirmShare = false;
            app.shareCrashLog();
          }}>Upload and copy link</button
        >
        <button class="secondary" onclick={() => (confirmShare = false)}>Cancel</button>
      </div>
    {:else}
      <div class="crash-actions">
        {#if c.cause === "out_of_memory" || c.cause === "memory_setting"}
          <button class="primary" onclick={() => (app.showSettings = true)}>Memory settings</button>
        {/if}
        <button class="secondary" onclick={() => (confirmShare = true)}>Share link</button>
        <button class="secondary" onclick={() => app.copyCrashLog()}>Copy log</button>
        <button class="link" onclick={() => api.openLogsFolder()}>Open logs</button>
      </div>
    {/if}
    {#if app.crashMsg}<p class="crash-msg selectable">{app.crashMsg}</p>{/if}
  </div>
{/if}
