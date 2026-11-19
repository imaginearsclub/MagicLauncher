<script lang="ts">
  import { app } from "../lib/app.svelte";
  import Icon from "../components/Icon.svelte";

  const presetId = $derived(app.settings?.preset ?? "");

  function pick(id: string) {
    app.choosePreset(id);
    app.saveQuietly();
  }

  function toggle(id: string, on: boolean) {
    // Switching single mods means "Custom", starting from what the current preset has.
    if (app.settings?.preset !== "custom") app.choosePreset("custom");
    app.toggleFeature(id, on);
    app.saveQuietly();
  }

  const onCount = $derived(app.pack ? app.pack.features.filter((f) => app.featureOn(f.id)).length : 0);
</script>

<div class="page">
  <div class="page-head">
    <span class="eyebrow">Approved by the Imaginears</span>
    <h2 class="display">Mods &amp; Shaders</h2>
    <p class="muted">
      Pick how much your PC runs. Mods everyone needs are always on and aren't listed here. Changes
      apply the next time you press Play.
    </p>
  </div>

  {#if !app.pack || !app.settings}
    <p class="muted">The modpack hasn't been checked yet.</p>
  {:else}
    {#if app.pack.presets.length}
      <section aria-labelledby="presets-title">
        <h3 id="presets-title" class="section-title"><Icon name="sparkles" size={18} />Graphics preset</h3>
        <div class="preset-grid" role="radiogroup" aria-labelledby="presets-title">
          {#each [{ id: "", name: "Pack default", description: "What the Imaginears recommend.", disable: [] as string[] }, ...app.pack.presets] as p (p.id)}
            {@const on = presetId === p.id}
            <button
              class="preset"
              class:on
              role="radio"
              aria-checked={on}
              onclick={() => pick(p.id)}
            >
              <span class="preset-name">{p.name}</span>
              {#if p.description}<span class="preset-desc">{p.description}</span>{/if}
              {#if p.disable.length}
                <span class="preset-off mono">
                  Off: {p.disable.map((id) => app.pack!.features.find((f) => f.id === id)?.name ?? id).join(", ")}
                </span>
              {/if}
            </button>
          {/each}
          {#if app.pack.features.length}
            <button
              class="preset"
              class:on={presetId === "custom"}
              role="radio"
              aria-checked={presetId === "custom"}
              onclick={() => pick("custom")}
            >
              <span class="preset-name">Custom</span>
              <span class="preset-desc">Choose each optional mod yourself.</span>
            </button>
          {/if}
        </div>
      </section>
    {/if}

    {#if app.pack.features.length}
      <section aria-labelledby="optional-title">
        <h3 id="optional-title" class="section-title">
          <Icon name="tune" size={18} />Optional mods
          <span class="mono muted count">{onCount} of {app.pack.features.length} on</span>
        </h3>
        <ul class="toggles mods-list">
          {#each app.pack.features as f (f.id)}
            {@const on = app.featureOn(f.id)}
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
                  onchange={(e) => toggle(f.id, e.currentTarget.checked)}
                />
              </label>
            </li>
          {/each}
        </ul>
        <p class="muted small">Changing a switch picks the Custom preset.</p>
      </section>
    {:else}
      <p class="muted">This pack has no optional mods.</p>
    {/if}
  {/if}
</div>
