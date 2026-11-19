<script lang="ts">
  // Draws a Minecraft player's face (8x8 at 8,8) plus the hat layer (8x8 at 40,8) from their
  // skin image, pixel-sharp. Falls back to the first letter of the name.
  let { url, name, size = 32 }: { url: string | null; name: string; size?: number } = $props();

  let canvas = $state<HTMLCanvasElement | null>(null);
  let failed = $state(false);

  // Reset only when the URL changes, so a re-render doesn't flash the letter.
  $effect.pre(() => {
    void url;
    failed = false;
  });

  $effect(() => {
    if (!url || !canvas) return;
    const c = canvas;
    const img = new Image();
    img.onload = () => {
      const ctx = c.getContext("2d");
      if (!ctx) return;
      ctx.imageSmoothingEnabled = false;
      ctx.clearRect(0, 0, size, size);
      ctx.drawImage(img, 8, 8, 8, 8, 0, 0, size, size);
      ctx.drawImage(img, 40, 8, 8, 8, 0, 0, size, size);
    };
    img.onerror = () => (failed = true);
    img.src = url;
  });
</script>

{#if url && !failed}
  <canvas bind:this={canvas} width={size} height={size} class="head" aria-label={`${name}'s skin`}></canvas>
{:else}
  <span class="head fallback" style={`width:${size}px;height:${size}px`} aria-hidden="true">
    {name.slice(0, 1).toUpperCase()}
  </span>
{/if}

<style>
  .head {
    border-radius: 6px;
    image-rendering: pixelated;
    box-shadow: 0 0 0 2px rgba(255, 255, 255, 0.15);
  }
  .fallback {
    display: inline-grid;
    place-items: center;
    background: var(--accent);
    color: #fff;
    font-weight: 700;
  }
</style>
