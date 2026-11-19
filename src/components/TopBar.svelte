<script lang="ts">
  import logo from "../assets/logo.png";
  import PlayerHead from "../lib/PlayerHead.svelte";
  import { popover } from "../lib/a11y";
  import { app } from "../lib/app.svelte";
  import Icon from "./Icon.svelte";
</script>

<!-- Drag regions: the window can be moved by the empty parts of the bar (on macOS it is the
     title bar). Buttons inside keep working. -->
<header class="topbar" data-tauri-drag-region>
  <div class="brand" data-tauri-drag-region>
    <img src={logo} alt="" class="logo" data-tauri-drag-region />
    <div class="brand-text" data-tauri-drag-region>
      <h1>Imaginears Club</h1>
      <span class="tagline">MagicLauncher</span>
    </div>
  </div>

  <div class="server-pill" role="status" aria-label="Server status">
    <span class={`dot ${app.server ? "on" : app.serverChecked ? "off" : ""}`} aria-hidden="true"></span>
    {#if app.server}
      <span class="mono">
        <strong class="gold">{app.server.online}</strong>/{app.server.max} guests
        <span class="sep" aria-hidden="true">|</span>
        {app.server.latency_ms} ms
      </span>
    {:else if app.serverChecked}
      <span class="mono">Server offline</span>
    {:else}
      <span class="mono">Checking…</span>
    {/if}
  </div>

  {#if app.profile}
    <div class="account">
      <button
        class="account-button"
        onclick={() => (app.showAccounts = !app.showAccounts)}
        aria-expanded={app.showAccounts}
        aria-controls="account-menu"
        aria-label={`Account: ${app.profile.name}. Switch or add accounts`}
      >
        <PlayerHead url={app.skin} name={app.profile.name} size={26} />
        <span class="account-name">{app.profile.name}</span>
        <Icon name="arrowDown" size={18} />
      </button>
      {#if app.showAccounts}
        <div
          class="account-menu"
          id="account-menu"
          role="group"
          aria-label="Accounts"
          use:popover={{ onclose: () => (app.showAccounts = false) }}
        >
          {#each app.accountList?.list ?? [] as a (a.uuid)}
            {@const current = a.uuid === app.profile.uuid}
            <button
              class:current
              aria-current={current ? "true" : undefined}
              onclick={() => !current && app.switchTo(a.uuid)}
              disabled={app.busy || current}
            >
              <Icon name={current ? "check" : "account"} size={18} />
              {a.name}
            </button>
          {/each}
          <hr />
          <button onclick={() => app.addAccount()} disabled={app.busy}>
            <Icon name="personAdd" size={18} /> Add account…
          </button>
          <button onclick={() => app.signOut()} disabled={app.busy}>
            <Icon name="logout" size={18} /> Sign out of {app.profile.name}
          </button>
        </div>
      {/if}
    </div>
  {:else}
    <span class="account-placeholder" aria-hidden="true"></span>
  {/if}
</header>
