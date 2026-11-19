<script lang="ts">
  import { modal } from "../lib/a11y";
  import { app } from "../lib/app.svelte";
</script>

<div class="overlay">
  <div class="card consent" role="dialog" aria-modal="true" aria-labelledby="consent-title" use:modal>
    <h2 id="consent-title">Help improve the launcher?</h2>
    <p>
      Send <strong>anonymous</strong> usage and crash reports: launcher and pack version, your operating
      system, whether installs and launches worked, and what kind of crash happened.
    </p>
    <p class="muted small">
      Never your Minecraft name, account, IP address or game logs. You can change this any time in
      Settings.
    </p>
    <div class="row center">
      <button class="secondary" onclick={() => app.setTelemetry(false)}>No thanks</button>
      <button class="primary" onclick={() => app.setTelemetry(true)}>Yes, send reports</button>
    </div>
  </div>
</div>
