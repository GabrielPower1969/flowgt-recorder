/**
 * Translations. Flat dictionaries keyed "section.key"; `{param}` interpolation.
 * zh is the default when language = auto and the browser speaks Chinese.
 */
import type { Language } from "../types";
import zhJson from "../i18n/zh.json";
import enJson from "../i18n/en.json";

export type Locale = "zh" | "en";
export type Params = Record<string, string | number>;

export const dictionaries: Record<Locale, Record<string, string>> = {
  zh: zhJson as Record<string, string>,
  en: enJson as Record<string, string>,
};

export function resolveLanguage(lang: Language, navLang?: string): Locale {
  if (lang === "zh" || lang === "en") return lang;
  const nav = navLang ?? (typeof navigator !== "undefined" ? navigator.language : "") ?? "";
  return nav.toLowerCase().startsWith("zh") ? "zh" : "en";
}

let locale = $state<Locale>(resolveLanguage("auto"));

export function getLocale(): Locale {
  return locale;
}

export function setLanguage(lang: Language): void {
  locale = resolveLanguage(lang);
  if (typeof document !== "undefined") {
    document.documentElement.lang = locale === "zh" ? "zh-CN" : "en";
  }
}

export function interpolate(template: string, params?: Params): string {
  if (!params) return template;
  return template.replace(/\{(\w+)\}/g, (whole, key: string) => (key in params ? String(params[key]) : whole));
}

/** Translate `key` in the current locale; falls back to en, then to the key itself. */
export function t(key: string, params?: Params): string {
  const text = dictionaries[locale][key] ?? dictionaries.en[key] ?? key;
  return interpolate(text, params);
}

export function hasKey(key: string): boolean {
  return key in dictionaries.en;
}

/** Translate a backend warning/error code ("lowDisk" → warn.lowDisk) with a detail fallback. */
export function tCode(prefix: "warn" | "error" | "info", code: string, detail?: string): string {
  const key = `${prefix}.${code}`;
  if (hasKey(key)) return t(key);
  return detail && detail.length > 0 ? detail : t(`${prefix}.unknown`, {});
}
