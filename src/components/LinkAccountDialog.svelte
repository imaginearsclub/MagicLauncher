<script lang="ts">
  import { modal } from "../lib/a11y";
  import { app } from "../lib/app.svelte";

  // Any web page can send an imaginears://link/… link, including one made with someone
  // else's website account, so the player always sees both names and confirms.
  const d = $derived(app.linkDialog);
  // Staff account recovery: proves the player owns the account already linked; links nothing.
  function purposeOf(dialog: typeof d) {
    if (!dialog || dialog.step === "loading") return "link";
    if ("prompt" in dialog) return dialog.prompt.purpose;
    return dialog.purpose ?? "link";
  }
  const recovery = $derived(purposeOf(d) === "recovery");
</script>

{#if d}
  <div class="overlay">
    <div
      class="card link-dialog"
      role="dialog"
      aria-modal="true"
      aria-labelledby="link-title"
      aria-busy={d.step === "loading" || d.step === "linking"}
      use:modal={{ onclose: () => app.closeLink() }}
    >
      {#if d.step === "loading"}
        <h2 id="link-title">Link your Minecraft account</h2>
        <p class="muted">Checking the link with the website…</p>
      {:else if (d.step === "confirm" || d.step === "linking") && recovery}
        <h2 id="link-title">Prove you own this account?</h2>
        <dl class="link-names">
          <dt>Minecraft account</dt>
          <dd><strong>{d.prompt.minecraft_name}</strong></dd>
          <dt>Website account</dt>
          <dd><strong>{d.prompt.website_account}</strong></dd>
        </dl>
        <p>
          Imaginears Club staff asked you to prove that you own <strong>{d.prompt.minecraft_name}</strong>, the
          Minecraft account linked to that website account, so they can help you get back into it. Nothing is
          linked or changed.
        </p>
        <p class="muted small">
          Only continue if Imaginears Club staff sent you this link, for example in your support ticket. Your
          Minecraft login is only sent to Mojang, never to the website. If this isn't the linked Minecraft account,
          cancel, switch accounts, and ask staff for a new link.
        </p>
        <div class="row center">
          <button class="secondary" onclick={() => app.closeLink()} disabled={d.step === "linking"}>Cancel</button>
          <button class="primary" onclick={() => app.confirmLink()} disabled={d.step === "linking"}>
            {d.step === "linking" ? "Checking…" : "Prove ownership"}
          </button>
        </div>
      {:else if d.step === "confirm" || d.step === "linking"}
        <h2 id="link-title">Link your Minecraft account?</h2>
        <dl class="link-names">
          <dt>Minecraft account</dt>
          <dd><strong>{d.prompt.minecraft_name}</strong></dd>
          <dt>Website account</dt>
          <dd><strong>{d.prompt.website_account}</strong></dd>
        </dl>
        <p>
          This proves to the Imaginears Club website that you own <strong>{d.prompt.minecraft_name}</strong>
          and links it to that website account.
        </p>
        <p class="muted small">
          Only continue if you just clicked <strong>Verify with MagicLauncher</strong> on the website yourself.
          Your Minecraft login is only sent to Mojang, never to the website. To link a different Minecraft
          account, cancel, switch accounts, and start again on the website.
        </p>
        <div class="row center">
          <button class="secondary" onclick={() => app.closeLink()} disabled={d.step === "linking"}>Cancel</button>
          <button class="primary" onclick={() => app.confirmLink()} disabled={d.step === "linking"}>
            {d.step === "linking" ? "Linking…" : "Link account"}
          </button>
        </div>
      {:else if d.step === "done" && recovery}
        <h2 id="link-title">Ownership confirmed</h2>
        <p role="status">
          The website confirmed you own <strong>{d.minecraftName}</strong>. Let staff know in your support ticket so
          they can continue.
        </p>
        <div class="row center">
          <button class="primary" onclick={() => app.closeLink()}>Done</button>
        </div>
      {:else if d.step === "done"}
        <h2 id="link-title">Account linked</h2>
        <p role="status">
          <strong>{d.minecraftName}</strong> is now verified on the Imaginears Club website. You can go back to your
          browser.
        </p>
        <div class="row center">
          <button class="primary" onclick={() => app.closeLink()}>Done</button>
        </div>
      {:else if d.step === "error"}
        <h2 id="link-title">{recovery ? "Couldn't confirm your account" : "Couldn't link your account"}</h2>
        <p class="error" role="alert">{d.message}</p>
        <div class="row center">
          <button class="secondary" onclick={() => app.closeLink()}>Close</button>
          {#if d.signIn}
            <button class="primary" onclick={() => app.signInForLink()}>Sign in</button>
          {/if}
        </div>
      {/if}
    </div>
  </div>
{/if}

<style>
  .link-names {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 0.25rem 1rem;
    margin: 0 0 0.75rem;
  }
  .link-names dt {
    color: var(--muted);
  }
  .link-names dd {
    margin: 0;
  }
</style>
