import { mount } from "svelte";
import App from "./App.svelte";
// Fonts are bundled (the CSP allows no font hosts): storybook headings, warm body text and
// monospaced numbers, as in the MagicLauncher design system.
import "@fontsource/cinzel-decorative/latin-700.css";
import "@fontsource/varela-round/latin-400.css";
import "@fontsource/spline-sans-mono/latin-500.css";
import "@fontsource/spline-sans-mono/latin-600.css";
import "./app.css";

// macOS: the native title bar is an overlay (tauri.conf.json); the top bar makes room for
// the window buttons.
if (/Mac OS X|Macintosh/.test(navigator.userAgent)) {
  document.documentElement.classList.add("platform-macos");
}

const app = mount(App, { target: document.getElementById("app")! });

export default app;
