/**
 * The static half of the shell-path guard (H7).
 *
 * The shipped desktop path is webview -> `main.ts` port -> Tauri `invoke` ->
 * the `lib.rs` command -> the adapter. Three of its joints are name-matched at
 * runtime and silent when they drift:
 *
 * 1. `invoke("name")` against `generate_handler![...]`. A name that is not
 *    registered rejects the promise at runtime with a message no screen is
 *    written to render.
 * 2. `CoreClient` against `createTauriCoreClient`. A declared method with no
 *    implementation is `undefined` at the call site.
 * 3. `D1RenderOptions` against the object `main.ts` hands `renderD1Cockpit`.
 *    An unwired port is `undefined`, and every port call site in the cockpit
 *    is guarded with `if (!port) return` — so a missing wire is a **silent
 *    no-op**, which is exactly how E3d defect 36 lost an operator's text.
 *
 * These are compile-invisible: the options bag is optional by design, so
 * TypeScript accepts an omission. This walks the sources instead.
 */

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

import { describe, expect, test } from "vitest";

const read = (relative: string): string =>
  readFileSync(fileURLToPath(new URL(`../${relative}`, import.meta.url)), "utf8");

const TAURI_CLIENT = read("src/host/tauri_core_client.ts");
const CORE_CLIENT = read("src/host/core_client.ts");
const LIB_RS = read("src-tauri/src/lib.rs");
const COCKPIT = read("src/screens/d1_cockpit.ts");
const MAIN = read("src/main.ts");

/**
 * Commands `lib.rs` registers that no port calls.
 *
 * Allowed, but named: an unreachable command is dead weight rather than a
 * broken joint, and the list exists so a *new* one cannot appear unnoticed.
 */
const UNREACHABLE_COMMANDS = new Set([
  // The dock settles on the cockpit's ordered wake, so its own poll is unused.
  "permission_poll",
  // D11 and D4 run as full-window screens whose entry read is their `*_poll`.
  "d11_intake",
  "d4_lane_create",
]);

/**
 * Cockpit options the shell deliberately leaves at their default.
 *
 * None of them is a Core port: they are local presentation switches whose
 * default is the desktop behaviour, so an omission claims nothing.
 */
const SHELL_DEFAULTED_OPTIONS = new Set([
  // The browser harness's bounded long poll; the desktop uses `onCoreWake`.
  "poll",
  // Welcome is the shell's own route, not a cockpit state.
  "showWelcome",
  // Read from the Core-owned layout record, not passed in.
  "laneSidebarMode",
  // Superseded by the in-cockpit New Lane popover and the project picker.
  "onCreateLane",
  "onOpenProject",
]);

/** Every `invoke("name")` the host client makes. */
function invokedCommands(): Set<string> {
  return new Set(
    [...TAURI_CLIENT.matchAll(/invoke\(\s*"([a-z0-9_]+)"/g)].map((match) => match[1]),
  );
}

/** Every command name inside `tauri::generate_handler![...]`. */
function registeredCommands(): Set<string> {
  const block = /tauri::generate_handler!\[([\s\S]*?)\]/.exec(LIB_RS);
  expect(block, "lib.rs must register its commands in generate_handler!").not.toBeNull();
  return new Set(
    (block as RegExpExecArray)[1]
      .split(",")
      .map((name) => name.trim())
      .filter((name) => name.length > 0),
  );
}

/** The body of the first brace-balanced block starting at `open`. */
function balanced(source: string, open: number): string {
  let depth = 0;
  for (let index = open; index < source.length; index += 1) {
    if (source[index] === "{") depth += 1;
    else if (source[index] === "}") {
      depth -= 1;
      if (depth === 0) return source.slice(open, index + 1);
    }
  }
  throw new Error("unbalanced block");
}

function coreClientMembers(): Set<string> {
  const body = balanced(CORE_CLIENT, CORE_CLIENT.indexOf("{", CORE_CLIENT.indexOf("export interface CoreClient")));
  return new Set([...body.matchAll(/^ {2}([a-zA-Z0-9_]+)\s*[(?:]/gm)].map((match) => match[1]));
}

function tauriClientMembers(): Set<string> {
  return new Set([...TAURI_CLIENT.matchAll(/^ {4}([a-zA-Z0-9_]+)\s*:/gm)].map((match) => match[1]));
}

function cockpitOptionMembers(): Set<string> {
  const body = balanced(COCKPIT, COCKPIT.indexOf("{", COCKPIT.indexOf("export interface D1RenderOptions")));
  return new Set([...body.matchAll(/^ {2}([a-zA-Z0-9_]+)\??\s*[:(]/gm)].map((match) => match[1]));
}

/** The option keys `main.ts` actually hands `renderD1Cockpit`. */
function shellWiredOptions(): Set<string> {
  const call = MAIN.indexOf("renderD1Cockpit(");
  expect(call, "main.ts mounts the cockpit").toBeGreaterThan(-1);
  // The options bag is the last argument: the final brace-balanced object in
  // the call, which is the one that begins after the previous argument list.
  const optionsOpen = MAIN.indexOf("{", MAIN.indexOf("recoverD6,", call));
  const body = balanced(MAIN, optionsOpen);
  return new Set([...body.matchAll(/^ {12}([a-zA-Z0-9_]+)\s*[,:]/gm)].map((match) => match[1]));
}

describe("the shipped shell path is name-matched end to end", () => {
  test("every port method invokes a command lib.rs registers", () => {
    const missing = [...invokedCommands()].filter((name) => !registeredCommands().has(name));
    expect(
      missing,
      "an unregistered command rejects the invoke at runtime, and no screen renders that",
    ).toEqual([]);
  });

  test("every registered command is reachable from a port, or is named as unreachable", () => {
    const invoked = invokedCommands();
    const orphans = [...registeredCommands()].filter(
      (name) => !invoked.has(name) && !UNREACHABLE_COMMANDS.has(name),
    );
    expect(orphans, "a command no port calls is dead weight; name it or wire it").toEqual([]);
  });

  test("every CoreClient member is implemented by the Tauri host client", () => {
    const implemented = tauriClientMembers();
    const missing = [...coreClientMembers()].filter((name) => !implemented.has(name));
    expect(missing, "an unimplemented member is `undefined` at its call site").toEqual([]);
  });

  test("every cockpit option the shell owns is wired in main.ts", () => {
    const wired = shellWiredOptions();
    const missing = [...cockpitOptionMembers()].filter(
      (name) => !wired.has(name) && !SHELL_DEFAULTED_OPTIONS.has(name),
    );
    expect(
      missing,
      "an unwired port is undefined, and every cockpit call site returns silently on that",
    ).toEqual([]);
  });

  test("the ports that carry a Core command are wired, one by one", () => {
    // Named rather than derived: these are the joints whose absence is silent,
    // and the list is the one a reviewer can check against the cockpit.
    const wired = shellWiredOptions();
    for (const port of [
      "queueFollowUp",
      "operatorGit",
      "workspaceDiff",
      "agentProbe",
      "evidence",
      "transcriptRows",
      "workspaceFile",
      "layout",
      "preferences",
      "revokeStandingApproval",
      "sendComposerControl",
      "sendD6Intent",
      "onCoreWake",
    ]) {
      expect(wired.has(port), `main.ts must wire the \`${port}\` port`).toBe(true);
    }
  });
});
