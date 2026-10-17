/**
 * Shortcut bindings ↔ display chips ↔ keyboard events.
 *
 * Stored formats (see `src-tauri/src/shortcuts.rs`): an accelerator such as
 * `"Super+Shift+KeyX"` (key names are `KeyboardEvent.code` values, which the
 * Rust parser accepts), `"mod:MetaRight"` for a lone modifier, `"mouse:3"` for
 * an extra mouse button (NSEvent numbering), or `""` when unbound. The last
 * two need a native backend (`NATIVE_SHORTCUTS`: macOS and Windows).
 */
import { IS_MAC } from "./ipc";
import { t } from "./i18n/index.svelte";

export interface Chip {
  label: string;
  /** "L"/"R" superscript for one-sided modifiers. */
  side?: string;
  wide?: boolean;
}

const MOD_GLYPH: Record<string, string> = IS_MAC
  ? { Control: "⌃", Alt: "⌥", Shift: "⇧", Super: "⌘" }
  : { Control: "Ctrl", Alt: "Alt", Shift: "Shift", Super: "Win" };

const MOD_ALIASES: Record<string, string> = {
  CTRL: "Control",
  CONTROL: "Control",
  ALT: "Alt",
  OPTION: "Alt",
  SHIFT: "Shift",
  SUPER: "Super",
  CMD: "Super",
  COMMAND: "Super",
  META: "Super",
  CMDORCTRL: IS_MAC ? "Super" : "Control",
  COMMANDORCONTROL: IS_MAC ? "Super" : "Control",
  COMMANDORCTRL: IS_MAC ? "Super" : "Control",
  CMDORCONTROL: IS_MAC ? "Super" : "Control",
};

/** macOS order: ⌃ ⌥ ⇧ ⌘. */
const MOD_ORDER = ["Control", "Alt", "Shift", "Super"];

const MAC_KEY_LABELS: Record<string, string> = {
  Escape: "esc",
  Space: "space",
  Enter: "↩",
  Backspace: "⌫",
  Delete: "⌦",
  Tab: "⇥",
  ArrowUp: "↑",
  ArrowDown: "↓",
  ArrowLeft: "←",
  ArrowRight: "→",
  Minus: "-",
  Equal: "=",
  BracketLeft: "[",
  BracketRight: "]",
  Backslash: "\\",
  Semicolon: ";",
  Quote: "'",
  Backquote: "`",
  Comma: ",",
  Period: ".",
  Slash: "/",
  Home: "↖",
  End: "↘",
  PageUp: "⇞",
  PageDown: "⇟",
};

/** Windows spells the special keys out, as printed on PC keyboards. */
const PC_KEY_LABELS: Record<string, string> = {
  ...MAC_KEY_LABELS,
  Escape: "Esc",
  Space: "Space",
  Enter: "Enter",
  Backspace: "Backspace",
  Delete: "Del",
  Tab: "Tab",
  Home: "Home",
  End: "End",
  PageUp: "PgUp",
  PageDown: "PgDn",
};

const KEY_LABELS = IS_MAC ? MAC_KEY_LABELS : PC_KEY_LABELS;

/** Modifier names for hints ("Add a modifier (…)"). */
export const MODIFIER_LIST = IS_MAC ? "⌘ ⌥ ⌃ ⇧" : "Ctrl, Alt, Shift, Win";

function keyLabel(code: string): string {
  if (/^Key[A-Z]$/.test(code)) return code.slice(3);
  if (/^Digit\d$/.test(code)) return code.slice(5);
  if (/^Numpad\d$/.test(code)) return code.slice(6);
  if (KEY_LABELS[code]) return KEY_LABELS[code];
  if (code.length === 1) return code.toUpperCase();
  return code.replace(/^Arrow/, "");
}

const MODIFIER_CODES: Record<string, { label: string; side?: string }> = {
  MetaLeft: { label: MOD_GLYPH.Super, side: "L" },
  MetaRight: { label: MOD_GLYPH.Super, side: "R" },
  AltLeft: { label: MOD_GLYPH.Alt, side: "L" },
  AltRight: { label: MOD_GLYPH.Alt, side: "R" },
  ShiftLeft: { label: MOD_GLYPH.Shift, side: "L" },
  ShiftRight: { label: MOD_GLYPH.Shift, side: "R" },
  ControlLeft: { label: MOD_GLYPH.Control, side: "L" },
  ControlRight: { label: MOD_GLYPH.Control, side: "R" },
  Fn: { label: "fn" },
};

/** Resolved at call time so the label follows the UI language. */
function mouseLabel(n: string): string {
  switch (n) {
    case "2":
      return t("configuration.mouseButtons.middle");
    case "3":
      return t("configuration.mouseButtons.back");
    case "4":
      return t("configuration.mouseButtons.forward");
    default:
      return t("configuration.mouseButtons.other", { n: Number(n) + 1 });
  }
}

/** Display chips for a stored binding. */
export function chips(binding: string): Chip[] {
  const b = binding.trim();
  if (!b) return [];
  if (b.startsWith("mod:")) {
    const m = MODIFIER_CODES[b.slice(4)];
    return m ? [m] : [{ label: b.slice(4) }];
  }
  if (b.startsWith("mouse:")) {
    const n = b.slice(6);
    return [{ label: mouseLabel(n), wide: true }];
  }
  const mods: string[] = [];
  let key = "";
  for (const raw of b.split("+")) {
    const mod = MOD_ALIASES[raw.trim().toUpperCase()];
    if (mod) mods.push(mod);
    else key = raw.trim();
  }
  mods.sort((a, c) => MOD_ORDER.indexOf(a) - MOD_ORDER.indexOf(c));
  const out: Chip[] = mods.map((m) => ({ label: MOD_GLYPH[m] }));
  if (key) out.push({ label: keyLabel(key) });
  return out;
}

/** Plain-text rendering ("⌥⇧K"), for tooltips and hints. */
export function shortcutText(binding: string): string {
  return chips(binding)
    .map((c) => c.label + (c.side ? (c.side === "R" ? "ᴿ" : "ᴸ") : ""))
    .join(IS_MAC ? "" : "+");
}

export function isModifierCode(code: string): boolean {
  return code in MODIFIER_CODES || code === "OSLeft" || code === "OSRight";
}

/** Normalize WebKit's OS* names to Meta*. */
export function normalizeModifierCode(code: string): string {
  return code === "OSLeft" ? "MetaLeft" : code === "OSRight" ? "MetaRight" : code;
}

const ALLOWED_BARE = /^(F\d{1,2}|Pause|ScrollLock|Insert|Help)$/;

export type CaptureResult = { binding: string } | { error: string } | null;

/**
 * Accelerator for a non-modifier keydown, or an error when it would hijack
 * normal typing (letters / digits / space without a modifier).
 */
export function acceleratorFromEvent(e: KeyboardEvent, allowBareEscape: boolean): CaptureResult {
  if (isModifierCode(e.code)) return null;
  const mods: string[] = [];
  if (e.ctrlKey) mods.push("Control");
  if (e.altKey) mods.push("Alt");
  if (e.shiftKey) mods.push("Shift");
  if (e.metaKey) mods.push("Super");
  const code = e.code;
  if (!code) return null;
  if (mods.length === 0) {
    if (code === "Escape" && allowBareEscape) return { binding: "Escape" };
    if (!ALLOWED_BARE.test(code)) return { error: t("configuration.recorder.addModifier", { mods: MODIFIER_LIST }) };
  }
  return { binding: [...mods, code].join("+") };
}

/** Webview `MouseEvent.button` → NSEvent `buttonNumber` (middle/back/forward). */
export function mouseBinding(button: number): string | null {
  if (button === 1) return "mouse:2";
  if (button === 3) return "mouse:3";
  if (button === 4) return "mouse:4";
  return null;
}
