/**
 * Theme handling shared by every window: resolves "auto" against the OS
 * appearance and exposes the result as `data-theme` on <html>, which the CSS
 * tokens in `app.css` key off.
 */
import type { Theme } from "./types";

const media = typeof window !== "undefined" ? window.matchMedia("(prefers-color-scheme: dark)") : null;
let current: Theme = "auto";

function paint() {
  const dark = current === "dark" || (current === "auto" && (media?.matches ?? true));
  document.documentElement.dataset.theme = dark ? "dark" : "light";
}

media?.addEventListener("change", () => current === "auto" && paint());

export function applyTheme(theme: Theme) {
  current = theme;
  paint();
}
