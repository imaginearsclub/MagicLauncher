<script lang="ts">
  import { ICONS, type IconName } from "../lib/icons";

  /** Decorative by default (hidden from screen readers); pass `label` when it stands alone. */
  let { name, size = 20, label }: { name: IconName; size?: number; label?: string } = $props();
</script>

<span
  class="icon"
  style={`--icon-size:${size}px`}
  role={label ? "img" : undefined}
  aria-label={label}
  aria-hidden={label ? undefined : "true"}
>
  <!-- Trusted: SVG files bundled from @material-symbols at build time. -->
  {@html ICONS[name]}
</span>

<style>
  .icon {
    display: inline-flex;
    width: var(--icon-size);
    height: var(--icon-size);
    flex: none;
  }
  .icon :global(svg) {
    width: 100%;
    height: 100%;
    fill: currentColor;
  }
</style>
