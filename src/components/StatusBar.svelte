<script lang="ts">
  import { api } from "../lib/api";
  import { app, CHANNEL_LABEL } from "../lib/app.svelte";
  import Icon from "./Icon.svelte";
</script>

<footer class="statusbar">
  <span class="mono muted">
    Launcher v{app.info?.version ?? "…"}
    {#if app.status && app.status.running !== "release"}
      <span class={`badge ${app.status.running}`}>{CHANNEL_LABEL[app.status.running].toUpperCase()}</span>
    {/if}
    {#if app.status && app.status.effective !== "release"}
      · {CHANNEL_LABEL[app.status.effective]} channel{app.status.selected !== app.status.effective
        ? ` (${CHANNEL_LABEL[app.status.selected]} not available)`
        : ""}
    {/if}
    {#if app.server}<span class="hide-narrow"> · {app.server.version}</span>{/if}
  </span>
  <div class="statusbar-actions">
    <button class="link" onclick={() => api.openGameFolder()}>
      <Icon name="folder" size={16} />Game folder
    </button>
    <button class="link" onclick={() => (app.showSettings = !app.showSettings)}>
      <Icon name="settings" size={16} />Settings
    </button>
  </div>
</footer>
