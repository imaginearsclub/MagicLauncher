// Keyboard and screen-reader helpers shared by the launcher and Imaginears PackPublisher.

import { getCurrentWebview } from "@tauri-apps/api/webview";

const FOCUSABLE =
  'button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

function focusables(node: HTMLElement): HTMLElement[] {
  return [...node.querySelectorAll<HTMLElement>(FOCUSABLE)].filter(
    (el) => el.offsetParent !== null || el === document.activeElement,
  );
}

export interface ModalOptions {
  /** Called on Esc. Leave out for dialogs that need an explicit choice. */
  onclose?: () => void;
}

/**
 * `use:modal` on a dialog: moves focus into it (the `autofocus` element or the first control),
 * keeps Tab / Shift+Tab inside, closes on Esc, and gives focus back to whatever opened it.
 */
export function modal(node: HTMLElement, options: ModalOptions = {}) {
  let opts = options;
  const opener = document.activeElement as HTMLElement | null;
  if (!node.hasAttribute("tabindex")) node.setAttribute("tabindex", "-1");
  queueMicrotask(() => {
    if (node.contains(document.activeElement)) return;
    (node.querySelector<HTMLElement>("[autofocus]") ?? focusables(node)[0] ?? node).focus();
  });

  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape" && opts.onclose) {
      e.preventDefault();
      e.stopPropagation();
      opts.onclose();
      return;
    }
    if (e.key !== "Tab") return;
    const f = focusables(node);
    if (!f.length) {
      e.preventDefault();
      return;
    }
    const first = f[0];
    const last = f[f.length - 1];
    if (e.shiftKey && (document.activeElement === first || document.activeElement === node)) {
      e.preventDefault();
      last.focus();
    } else if (!e.shiftKey && document.activeElement === last) {
      e.preventDefault();
      first.focus();
    }
  }

  node.addEventListener("keydown", onKey);
  return {
    update(o: ModalOptions) {
      opts = o;
    },
    destroy() {
      node.removeEventListener("keydown", onKey);
      if (opener && document.contains(opener)) opener.focus();
    },
  };
}

/**
 * `use:popover` on a small menu: Esc or a click outside closes it, and focus goes back to the
 * button that opened it. Tab moves through it normally.
 */
export function popover(node: HTMLElement, options: { onclose: () => void }) {
  let opts = options;
  const opener = document.activeElement as HTMLElement | null;
  queueMicrotask(() => focusables(node)[0]?.focus());

  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      opts.onclose();
    }
  }
  function onDown(e: PointerEvent) {
    const t = e.target as Node;
    if (!node.contains(t) && !opener?.contains(t)) opts.onclose();
  }
  function onFocusOut(e: FocusEvent) {
    const to = e.relatedTarget as Node | null;
    if (to && !node.contains(to) && !opener?.contains(to)) opts.onclose();
  }
  node.addEventListener("keydown", onKey);
  node.addEventListener("focusout", onFocusOut);
  document.addEventListener("pointerdown", onDown, true);
  return {
    update(o: { onclose: () => void }) {
      opts = o;
    },
    destroy() {
      node.removeEventListener("keydown", onKey);
      node.removeEventListener("focusout", onFocusOut);
      document.removeEventListener("pointerdown", onDown, true);
      if (opener && document.contains(opener) && node.contains(document.activeElement)) opener.focus();
    },
  };
}

/**
 * Text size: real webview zoom (like Ctrl + Plus in a browser), so the layout reflows to fit
 * the window. CSS `zoom` would also enlarge `100vh` and push the page past the window.
 */
export async function applyTextScale(percent: number | null | undefined) {
  const p = Math.min(200, Math.max(80, percent || 100));
  try {
    await getCurrentWebview().setZoom(p / 100);
  } catch {
    // Outside Tauri (plain browser preview): no webview zoom available.
  }
}
