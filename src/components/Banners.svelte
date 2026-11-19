<script lang="ts">
  import { app } from "../lib/app.svelte";
  import Icon from "./Icon.svelte";
</script>

{#if app.update}
  <div class="banner update" role="status">
    {#if app.updating}
      <span>Updating the launcher… {app.updatePercent !== null ? `${app.updatePercent}%` : ""}</span>
      <div
        class="bar"
        role="progressbar"
        aria-label="Launcher update"
        aria-valuemin="0"
        aria-valuemax="100"
        aria-valuenow={app.updatePercent ?? undefined}
      >
        <div style={`width:${app.updatePercent ?? 100}%`}></div>
      </div>
    {:else}
      <span>
        {#if app.update.downgrade}
          Switching back to Release {app.update.version}.
        {:else}
          Launcher {app.update.version} is available{app.update.mandatory ? " and required to play" : ""}.
        {/if}
        {#if app.update.notes}<span class="muted">{app.update.notes}</span>{/if}
      </span>
      <button class="primary" onclick={() => app.runUpdate()} disabled={app.busy}>
        <Icon name="download" size={18} />{app.busy ? "Close the game to update" : "Update now"}
      </button>
    {/if}
    {#if app.updateError}<p class="error" role="alert">{app.updateError}</p>{/if}
  </div>
{/if}

{#if app.maintenance?.maintenance}
  <div class="banner maintenance" role="alert">
    <Icon name="build" size={20} />
    <strong>Park closed for refurbishment</strong>
    <span>{app.maintenance.message ?? "The server is being updated. Please check back soon."}</span>
    {#if app.maintenance.can_bypass}
      <span class="muted">You're an Alpha tester, so you can still play to test it.</span>
    {/if}
  </div>
{/if}
