<script lang="ts">
  import { app, type View } from "../lib/app.svelte";
  import type { IconName } from "../lib/icons";
  import Icon from "./Icon.svelte";

  const ITEMS: { id: View; label: string; icon: IconName; iconOn: IconName }[] = [
    { id: "play", label: "Play", icon: "rocket", iconOn: "rocketFill" },
    { id: "mods", label: "Mods & Shaders", icon: "extension", iconOn: "extensionFill" },
  ];

  const offCount = $derived(app.pack ? app.pack.features.filter((f) => !app.featureOn(f.id)).length : 0);
</script>

<nav class="rail" aria-label="Launcher sections">
  {#each ITEMS as item (item.id)}
    {@const on = app.view === item.id}
    <button
      class="rail-item"
      class:on
      aria-current={on ? "page" : undefined}
      onclick={() => (app.view = item.id)}
      title={item.label}
    >
      <Icon name={on ? item.iconOn : item.icon} size={22} />
      <span class="rail-label">{item.label}</span>
      {#if item.id === "play" && app.pack?.update_available}
        <span class="rail-badge" title="Pack update available">
          <span class="sr-only">Pack update available</span>
        </span>
      {:else if item.id === "mods" && offCount > 0}
        <span class="rail-count mono" title={`${offCount} optional mods off`}>{offCount} off</span>
      {/if}
    </button>
  {/each}
</nav>
