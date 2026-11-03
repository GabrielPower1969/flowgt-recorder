import { beforeEach, describe, expect, it } from "vitest";
import { readdirSync, readFileSync } from "node:fs";
import { join, resolve } from "node:path";
import zh from "./i18n/zh.json";
import en from "./i18n/en.json";
import { dictionaries, getLocale, interpolate, hasKey, resolveLanguage, setLanguage, t, tCode } from "./stores/i18n.svelte";

const REPO = resolve(__dirname, "../..");

function rustCodes(): string[] {
  const dirs = [join(REPO, "src-tauri/src"), join(REPO, "src-tauri/crates/core/src")];
  const codes = new Set<string>();
  for (const dir of dirs) {
    let files: string[] = [];
    try {
      files = readdirSync(dir).filter((f) => f.endsWith(".rs"));
    } catch {
      continue;
    }
    for (const f of files) {
      const src = readFileSync(join(dir, f), "utf8");
      for (const m of src.matchAll(/"((?:warn|error)\.[a-zA-Z]+)"/g)) codes.add(m[1]);
    }
  }
  return [...codes].sort();
}

describe("i18n dictionaries", () => {
  it("zh and en have identical key sets", () => {
    const zhKeys = Object.keys(zh).sort();
    const enKeys = Object.keys(en).sort();
    expect(zhKeys).toEqual(enKeys);
    expect(zhKeys.length).toBeGreaterThan(100);
  });

  it("every value is a non-empty string in both languages", () => {
    for (const [k, v] of Object.entries(zh)) expect(typeof v === "string" && v.length > 0, `zh ${k}`).toBe(true);
    for (const [k, v] of Object.entries(en)) expect(typeof v === "string" && v.length > 0, `en ${k}`).toBe(true);
  });

  it("every warn.*/error.* code used in Rust exists in en.json", () => {
    const codes = rustCodes();
    expect(codes.length).toBeGreaterThan(5);
    const missing = codes.filter((c) => !(c in en));
    expect(missing).toEqual([]);
  });

  it("exposes both dictionaries through `dictionaries`", () => {
    expect(dictionaries.zh).toBe(zh);
    expect(dictionaries.en).toBe(en);
  });
});

describe("interpolate / t", () => {
  beforeEach(() => setLanguage("zh"));

  it("interpolates {n} placeholders", () => {
    expect(interpolate("{n} files", { n: 3 })).toBe("3 files");
    expect(interpolate("{a}-{b}", { a: "x", b: "y" })).toBe("x-y");
  });

  it("leaves unknown placeholders untouched", () => {
    expect(interpolate("{n} and {m}", { n: 1 })).toBe("1 and {m}");
    expect(interpolate("plain", undefined)).toBe("plain");
  });

  it("t() interpolates params into a real key", () => {
    const key = Object.keys(en).find((k) => en[k as keyof typeof en].includes("{"));
    expect(key).toBeDefined();
    const param = /\{(\w+)\}/.exec(en[key as keyof typeof en])![1];
    setLanguage("en");
    expect(t(key!, { [param]: 42 })).toContain("42");
    expect(t(key!, { [param]: 42 })).not.toContain(`{${param}}`);
  });

  it("t() returns the key itself when unknown", () => {
    expect(t("nope.missing")).toBe("nope.missing");
  });

  it("hasKey reflects en.json", () => {
    expect(hasKey("warn.lowDisk")).toBe(true);
    expect(hasKey("warn.doesNotExist")).toBe(false);
  });
});

describe("tCode", () => {
  beforeEach(() => setLanguage("zh"));

  it('tCode("warn","cameraLost") returns the zh text', () => {
    expect(tCode("warn", "cameraLost")).toBe(zh["warn.cameraLost"]);
    expect(tCode("warn", "cameraLost")).toBe("摄像头已断开，继续只录屏幕");
  });

  it("falls back to detail, then to the unknown message", () => {
    expect(tCode("error", "weird", "raw detail")).toBe("raw detail");
    expect(tCode("error", "weird", "")).toBe(zh["error.unknown"]);
    expect(tCode("warn", "weird")).toBe(zh["warn.unknown"] ?? "warn.unknown");
  });
});

describe("resolveLanguage / setLanguage", () => {
  it("auto resolves from the navigator language", () => {
    expect(resolveLanguage("auto", "en-US")).toBe("en");
    expect(resolveLanguage("auto", "zh-CN")).toBe("zh");
    expect(resolveLanguage("auto", "zh-TW")).toBe("zh");
    expect(resolveLanguage("auto", "")).toBe("en");
  });

  it("explicit languages win over the navigator", () => {
    expect(resolveLanguage("en", "zh-CN")).toBe("en");
    expect(resolveLanguage("zh", "en-US")).toBe("zh");
  });

  it("setLanguage updates locale and document lang", () => {
    setLanguage("en");
    expect(getLocale()).toBe("en");
    expect(document.documentElement.lang).toBe("en");
    setLanguage("zh");
    expect(getLocale()).toBe("zh");
    expect(document.documentElement.lang).toBe("zh-CN");
    setLanguage("auto"); // test-setup pins navigator.language to zh-CN
    expect(getLocale()).toBe("zh");
    expect(document.documentElement.lang).toBe("zh-CN");
  });
});
