<script lang="ts">
  import { modal } from "../lib/a11y";
  import { app } from "../lib/app.svelte";
  import Icon from "./Icon.svelte";

  let modSearch = $state("");
  const filtered = $derived.by(() => {
    const q = modSearch.trim().toLowerCase();
    const all = app.pack?.features ?? [];
    return q ? all.filter((f) => `${f.name} ${f.description ?? ""}`.toLowerCase().includes(q)) : all;
  });
  const close = () => (app.showModPicker = false);
</script>

{#if app.settings && app.pack}
  {@const settings = app.settings}
  {@const pack = app.pack}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="overlay" onclick={(e) => e.target === e.currentTarget && close()}>
    <div class="card picker" role="dialog" aria-modal="true" aria-labelledby="picker-title" use:modal={{ onclose: close }}>
      <div class="picker-head">
        <h2 id="picker-title">Optional mods</h2>
        <button class="crash-close" aria-label="Close" title="Close" onclick={close}><Icon name="close" size={18} /></button>
      </div>
      <p class="muted small">Switch off what your PC doesn't need. Mods the server needs aren't listed and are always on.</p>
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
        <button class="link small" onclick={() => app.setAll(filtered.map((f) => f.id), true)}>All on</button>
        <button class="link small" onclick={() => app.setAll(filtered.map((f) => f.id), false)}>All off</button>
      </div>
      <ul class="toggles picker-list" aria-label="Optional mods">
        {#each filtered as f (f.id)}
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
                onchange={(e) => app.toggleFeature(f.id, e.currentTarget.checked)}
              />
            </label>
          </li>
        {:else}
          <li class="picker-empty muted small">No mods match "{modSearch}".</li>
        {/each}
      </ul>
      <div class="picker-foot">
        <span class="muted small">
          {pack.features.filter((f) => !settings.custom_features_off.includes(f.id)).length} of {pack.features.length} on
        </span>
        <button class="primary" onclick={close}>Done</button>
      </div>
    </div>
  </div>
{/if}
