<script lang="ts">
  import { onMount } from "svelte";
  import { app } from "./lib/app.svelte";
  import Banners from "./components/Banners.svelte";
  import ConsentDialog from "./components/ConsentDialog.svelte";
  import ModPicker from "./components/ModPicker.svelte";
  import NavRail from "./components/NavRail.svelte";
  import SettingsSheet from "./components/SettingsSheet.svelte";
  import StatusBar from "./components/StatusBar.svelte";
  import TopBar from "./components/TopBar.svelte";
  import ModsView from "./views/ModsView.svelte";
  import PlayView from "./views/PlayView.svelte";

  onMount(() => app.start());
</script>

<main>
  <TopBar />
  <Banners />

  <div class="body">
    <NavRail />
    <div class="view">
      {#if app.view === "mods"}
        <ModsView />
      {:else}
        <PlayView />
      {/if}
    </div>
  </div>

  <StatusBar />

  {#if app.settings && app.settings.telemetry === null && app.info && !app.info.sentry_test}
    <ConsentDialog />
  {/if}
  {#if app.showModPicker}<ModPicker />{/if}
  {#if app.showSettings}<SettingsSheet />{/if}
</main>
