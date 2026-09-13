// @vitest-environment node

/**
 * G10 design-fidelity gate: the GUI-4 rule "token mapping is checkable;
 * components do not write raw colors, scattered type sizes, or temporary
 * spacing" turned into a check that can fail.
 *
 * Two properties are asserted over every GUI-owned stylesheet:
 *
 * 1. No stylesheet writes a raw color literal (`#hex`, `rgb()`, `rgba()`).
 *    The design package's `tokens.css` is the only palette the client has.
 * 2. Every `var(--name)` reference names a custom property that actually
 *    resolves at runtime — one registered in `tokens.css`, one the GUI
 *    defines in its own CSS, or one the GUI sets from TypeScript through
 *    `style.setProperty`.
 *
 * Property 2 is the one that matters most and the one a screenshot cannot
 * catch on its own. A `var(--not-a-token)` with no fallback is invalid at
 * computed-value time, so the *whole declaration* is dropped: a background
 * silently disappears, and a `font:` shorthand silently falls back to the
 * browser's 16px serif default. Nothing errors; the screen simply stops
 * matching the design package.
 */

import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative, resolve } from "node:path";

import { describe, expect, test } from "vitest";

const GUI_ROOT = resolve(process.cwd());
const REPO_ROOT = resolve(GUI_ROOT, "..", "..");
const SRC_ROOT = join(GUI_ROOT, "src");
const TOKENS_CSS = join(REPO_ROOT, "docs", "viden-design", "Viden", "tokens.css");

function walk(dir: string, suffix: string): string[] {
  const out: string[] = [];
  for (const entry of readdirSync(dir)) {
    const full = join(dir, entry);
    if (statSync(full).isDirectory()) out.push(...walk(full, suffix));
    else if (entry.endsWith(suffix)) out.push(full);
  }
  return out.sort();
}

/** Custom-property names a stylesheet declares (`--name:`). */
function declaredIn(source: string): Set<string> {
  return new Set(source.match(/--[A-Za-z0-9_-]+(?=\s*:)/g) ?? []);
}

const cssFiles = walk(SRC_ROOT, ".css");
const tsFiles = walk(SRC_ROOT, ".ts");

/** The design package is the single palette, type scale, and spacing source. */
const registered = declaredIn(readFileSync(TOKENS_CSS, "utf8"));

/** Properties the GUI legitimately owns: declared in its own CSS… */
const guiDeclared = new Set<string>();
for (const file of cssFiles) {
  for (const name of declaredIn(readFileSync(file, "utf8"))) guiDeclared.add(name);
}

/** …or written at runtime, which is how the anchored overlays position. */
const runtimeSet = new Set<string>();
for (const file of tsFiles) {
  for (const match of readFileSync(file, "utf8").matchAll(
    /setProperty\(\s*["'](--[A-Za-z0-9_-]+)/g,
  )) {
    runtimeSet.add(match[1]);
  }
}

describe("GUI stylesheets resolve against the registered design tokens", () => {
  test("no GUI stylesheet writes a raw color literal", () => {
    const offences: string[] = [];
    for (const file of cssFiles) {
      const lines = readFileSync(file, "utf8").split("\n");
      lines.forEach((line, index) => {
        // `color-mix(in srgb, var(--accent) 46%, transparent)` is a token
        // operation, not a literal, so only `#hex` and `rgb(a)(…)` count.
        if (!/#[0-9a-fA-F]{3,8}\b|rgba?\(/.test(line)) return;
        offences.push(`${relative(REPO_ROOT, file)}:${index + 1}: ${line.trim()}`);
      });
    }
    expect(offences).toEqual([]);
  });

  test("every var() reference names a property that resolves", () => {
    const offences: string[] = [];
    for (const file of cssFiles) {
      const lines = readFileSync(file, "utf8").split("\n");
      lines.forEach((line, index) => {
        for (const match of line.matchAll(/var\(\s*(--[A-Za-z0-9_-]+)/g)) {
          const name = match[1];
          if (registered.has(name) || guiDeclared.has(name) || runtimeSet.has(name)) continue;
          offences.push(`${relative(REPO_ROOT, file)}:${index + 1}: ${name}`);
        }
      });
    }
    expect(offences).toEqual([]);
  });

  test("the check has a corpus to check, so an empty glob cannot pass it", () => {
    expect(cssFiles.length).toBeGreaterThan(10);
    expect(registered.size).toBeGreaterThan(80);
  });
});
