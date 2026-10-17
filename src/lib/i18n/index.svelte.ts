/**
 * UI translations.
 *
 * `t("home.getStarted")` looks a dotted key up in the active locale, falling
 * back to English. It reads reactive state, so any template (or `$derived`)
 * that calls it re-renders when the language changes. Strings computed once at
 * module/script init will NOT update — compute them in a function or `$derived`.
 *
 * Placeholders use `{name}`: `t("vocabulary.remove", { word })`.
 * Plurals: `tn("history.words", n)` picks `<key>.one` / `<key>.other`
 * (Intl.PluralRules), with `{count}` available in the text.
 */
import en from "./locales/en";
import es from "./locales/es";
import pt from "./locales/pt";
import fr from "./locales/fr";
import de from "./locales/de";
import it from "./locales/it";
import ko from "./locales/ko";
import ja from "./locales/ja";
import zh from "./locales/zh";

export type Locale = "en" | "es" | "pt" | "fr" | "de" | "it" | "ko" | "ja" | "zh";

/** Language picker entries, in their own language. */
export const LOCALES: { code: Locale; label: string }[] = [
  { code: "en", label: "English" },
  { code: "es", label: "Español" },
  { code: "pt", label: "Português" },
  { code: "fr", label: "Français" },
  { code: "de", label: "Deutsch" },
  { code: "it", label: "Italiano" },
  { code: "ko", label: "한국어" },
  { code: "ja", label: "日本語" },
  { code: "zh", label: "简体中文" },
];

/** BCP-47 tags for `Intl` formatting and `<html lang>`. */
export const BCP47: Record<Locale, string> = {
  en: "en",
  es: "es",
  pt: "pt-BR",
  fr: "fr",
  de: "de",
  it: "it",
  ko: "ko",
  ja: "ja",
  zh: "zh-Hans",
};

type Widen<T> = T extends string ? string : { [K in keyof T]: Widen<T[K]> };
type DeepPartial<T> = T extends string ? string : { [K in keyof T]?: DeepPartial<T[K]> };

/** Shape every complete locale must have. */
export type Dict = Widen<typeof en>;
/** A locale that may still be missing keys (falls back to English). */
export type PartialDict = DeepPartial<Dict>;

/** Every dotted path to a string in the English dictionary. */
type Leaves<T, P extends string = ""> = {
  [K in keyof T & string]: T[K] extends string ? `${P}${K}` : Leaves<T[K], `${P}${K}.`>;
}[keyof T & string];
export type Key = Leaves<typeof en>;

const DICTS: Record<Locale, PartialDict> = { en, es, pt, fr, de, it, ko, ja, zh };

class I18n {
  locale = $state<Locale>("en");
}
export const i18n = new I18n();

export function setLocale(code: string) {
  const l = (LOCALES.some((x) => x.code === code) ? code : "en") as Locale;
  i18n.locale = l;
  if (typeof document !== "undefined") document.documentElement.lang = BCP47[l];
}

/** BCP-47 tag of the active locale (for `Intl` / `toLocale*String`). */
export function intlLocale(): string {
  return BCP47[i18n.locale];
}

function get(dict: PartialDict | undefined, key: string): string | undefined {
  let node: unknown = dict;
  for (const part of key.split(".")) {
    if (node == null || typeof node !== "object") return undefined;
    node = (node as Record<string, unknown>)[part];
  }
  return typeof node === "string" ? node : undefined;
}

function fill(s: string, params?: Record<string, string | number>): string {
  if (!params) return s;
  return s.replace(/\{(\w+)\}/g, (m, k) => (k in params ? String(params[k]) : m));
}

/** Translate a static key. */
export function t(key: Key, params?: Record<string, string | number>): string {
  const s = get(DICTS[i18n.locale], key) ?? get(en as PartialDict, key) ?? key;
  return fill(s, params);
}

/** Translate a key built at runtime (e.g. `common.modelDesc.${id}`). */
export function td(key: string, params?: Record<string, string | number>): string {
  return t(key as Key, params);
}

/** Plural: uses `<key>.one` / `<key>.other` (and `.zero`/`.few`/… if present). */
export function tn(key: string, count: number, params?: Record<string, string | number>): string {
  // CLDR files 0 under "one" in Portuguese, but Brazilian UI copy says
  // "0 palavras"; zero takes the plural there.
  const cat = count === 0 && i18n.locale === "pt" ? "other" : new Intl.PluralRules(BCP47[i18n.locale]).select(count);
  const all = { count: count.toLocaleString(BCP47[i18n.locale]), ...params };
  const hit = get(DICTS[i18n.locale], `${key}.${cat}`) ?? get(DICTS[i18n.locale], `${key}.other`);
  return fill(hit ?? get(en as PartialDict, `${key}.${count === 1 ? "one" : "other"}`) ?? key, all);
}
