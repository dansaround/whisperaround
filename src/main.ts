import { mount } from "svelte";
import "./app.css";
import App from "./App.svelte";

/** Render a fatal error visibly instead of leaving a blank (black) page. */
function showFatal(label: string, err: unknown) {
  const msg = err instanceof Error ? `${err.name}: ${err.message}\n${err.stack ?? ""}` : String(err);
  const el = document.getElementById("app");
  if (el) {
    el.innerHTML = `<pre style="color:#f87171;background:#1b1b1f;padding:16px;margin:0;white-space:pre-wrap;font:13px monospace;height:100vh;overflow:auto">[${label}]\n${msg}</pre>`;
  }
  console.error(`[${label}]`, err);
}

window.addEventListener("error", (e) => showFatal("window.error", e.error ?? e.message));
window.addEventListener("unhandledrejection", (e) => showFatal("unhandledrejection", e.reason));

let app: ReturnType<typeof mount> | undefined;
try {
  const target = document.getElementById("app");
  if (!target) throw new Error('No se encontró el contenedor #app en index.html');
  app = mount(App, { target });
} catch (err) {
  showFatal("mount", err);
}

export default app;
