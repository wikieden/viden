// @vitest-environment jsdom

/**
 * H2 hygiene — E1 defect 7, reproduction attempt at the shell seam.
 *
 * The run reported: with a project bound, `⌘K` then `Escape` left the cockpit
 * showing `CONNECTING · Establishing the versioned Core connection. · Core
 * connection pending` with the titlebar project chip back at `—`, and about
 * ten seconds later the window and the process were gone, with no crash
 * report. Two mechanisms were named as candidates: the palette-close path
 * re-issuing a Core connect, and a dropped adapter terminating the app.
 *
 * This suite drives the first candidate through the production shell
 * (`hydrateShellFromCore` over a fake host) and pins the second's invariant:
 * whatever the host answers, the cockpit keeps the last facts Core published
 * and stays mounted. `bootstrapShell`'s `connecting` projection is the *only*
 * producer of "Core connection pending", so a test that closes the palette and
 * finds the bound cockpti still on screen rules that mechanism out.
 */

import { beforeEach, describe, expect, test, vi } from "vitest";

const invoke = vi.fn();
const openDialog = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (command: string, args?: unknown) => invoke(command, args),
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: (options?: unknown) => openDialog(options),
}));

import { bootstrapShell, hydrateShellFromCore } from "../src/main";
import { D1_PROJECTION } from "./support/d1_projection";

/// The host's own sentence when `DesktopState.adapter` is `None` — the string
/// every `#[tauri::command]` in `src-tauri/src/lib.rs` returns in that case.
const ADAPTER_GONE = "Core adapter is not connected";

function bound() {
  return structuredClone(D1_PROJECTION);
}

/// A host that answers every read from one bound projection. `adapterGone`
/// flips it to the failure the defect describes, for every command at once.
function host(): { adapterGone: () => void } {
  let gone = false;
  invoke.mockImplementation(async (command: string) => {
    if (gone) throw new Error(ADAPTER_GONE);
    switch (command) {
      case "resolved_preferences":
        return D1_PROJECTION.preferences;
      case "preferences_available":
        return true;
      case "d1_cockpit":
        return bound();
      case "d1_poll":
        return { projection: bound(), pendingCommandId: null, outcome: { state: "idle", reason: null } };
      case "transcript_rows":
      case "transcript_rows_poll":
        // Shaped like the host's own answer: an absent capability still comes
        // back as a projection, never as `null`.
        return {
          outcome: { state: "idle", reason: null },
          pendingCommandId: null,
          capabilityAvailable: false,
          loaded: false,
          rows: [],
          older: null,
          complete: false,
          scopeLaneId: null,
        };
      case "workspace_file":
      case "workspace_file_poll":
        // Same rule: a Core that publishes no file reads still answers with a
        // projection, and a fake that returns `null` tests the fake.
        return {
          outcome: { state: "idle", reason: null },
          pendingCommandId: null,
          capabilityAvailable: false,
          requestedPath: null,
          targetLaneId: null,
          file: null,
        };
      case "layout_preferences":
      case "layout_preferences_poll":
        // Shaped like the host's own answer for the same reason as below: a
        // Core that publishes no layout record still answers with a record.
        return {
          outcome: { state: "idle", reason: null },
          pendingCommandId: null,
          capabilityAvailable: false,
          laneSidebarMode: null,
          hiddenStatusbarSegments: [],
          persisted: null,
          diagnostics: [],
        };
      case "operator_git":
      case "operator_git_poll":
        // Shaped like the host's own answer. `null` is not a value this command
        // can return, and a fake that returns one tests the fake.
        return {
          outcome: { state: "idle", reason: null },
          availability: [],
          lastAction: null,
          pendingCommandId: null,
          capabilityAvailable: false,
          targetLaneId: null,
        };
      default:
        // Every other read is a capability probe this suite does not exercise;
        // an absent answer is a valid Core answer for all of them.
        return null;
    }
  });
  return {
    adapterGone: () => {
      gone = true;
    },
  };
}

async function settle(hops = 12): Promise<void> {
  for (let hop = 0; hop < hops; hop += 1) await Promise.resolve();
  await new Promise((resolve) => setTimeout(resolve, 0));
}

let root: HTMLElement;

beforeEach(() => {
  invoke.mockReset();
  openDialog.mockReset();
  document.body.innerHTML = '<main id="app"></main>';
  root = document.querySelector<HTMLElement>("#app")!;
});

describe("the palette close path with a bound project", () => {
  test("⌘K then Escape leaves the bound cockpit exactly where it was", async () => {
    host();
    await hydrateShellFromCore(root);
    await settle();
    expect(root.dataset.clientState).toBe("connected");
    expect(root.textContent).not.toContain("Core connection pending");

    window.dispatchEvent(new KeyboardEvent("keydown", { key: "k", metaKey: true }));
    await settle();
    // The chord must reach exactly one cockpit. A pre-hydration shell still
    // holding the window chords answers it too, and its render paints the
    // `connecting` projection over the bound one.
    expect(document.querySelectorAll("[data-command-palette]")).toHaveLength(1);
    expect(root.textContent).not.toContain("Core connection pending");
    expect(root.querySelector("[data-topbar-project], [data-project-selector]")?.textContent)
      .not.toBe("—");

    // The palette consumes its own Escape from the query field, which is where
    // it puts focus when it opens — the same path the operator takes.
    root
      .querySelector<HTMLInputElement>("[data-palette-input]")!
      .dispatchEvent(
        new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true }),
      );
    await settle();

    expect(document.querySelector("[data-command-palette]")).toBeNull();
    // The two facts the run reported losing.
    expect(root.dataset.clientState).toBe("connected");
    expect(root.textContent).not.toContain("Core connection pending");
    // The recovery stage is only mounted for a non-live connection, and never
    // for `connecting` here: this is what the run saw replace the centre pane.
    expect(root.querySelector("[data-d6-state]")?.getAttribute("data-d6-state") ?? "live").toBe(
      "live",
    );
    expect(root.querySelector("[data-screen]")?.getAttribute("data-screen")).toBe("d1-cockpit");
  });

  test("the pre-hydration shell stops answering the window chords once Core is live", async () => {
    // The mechanism directly: `bootstrapShell` mounts a cockpit whose chords
    // live on `window`, and the live cockpit replaces its DOM without
    // replacing those handlers unless the shell is disposed.
    host();
    bootstrapShell(root);
    await hydrateShellFromCore(root);
    await settle();

    for (const key of ["k", "l", "."]) {
      window.dispatchEvent(new KeyboardEvent("keydown", { key, metaKey: true }));
      await settle();
      expect(root.textContent).not.toContain("Core connection pending");
      expect(root.dataset.clientState).toBe("connected");
    }
  });

  test("closing the palette issues no second Core connect", async () => {
    // The `CONNECTING` projection is produced by `bootstrapShell` only, so a
    // re-issued handshake is the only way the palette could have caused it.
    host();
    await hydrateShellFromCore(root);
    await settle();
    const before = invoke.mock.calls.filter(
      ([command]) => command === "resolved_preferences" || command === "d1_cockpit",
    ).length;

    window.dispatchEvent(new KeyboardEvent("keydown", { key: "k", metaKey: true }));
    await settle();
    root
      .querySelector<HTMLInputElement>("[data-palette-input]")!
      .dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true }));
    await settle();

    const after = invoke.mock.calls.filter(
      ([command]) => command === "resolved_preferences" || command === "d1_cockpit",
    ).length;
    expect(after).toBe(before);
  });
});

describe("a host that loses its adapter after the project is bound", () => {
  test("keeps the last facts Core published and stays mounted", async () => {
    const fake = host();
    await hydrateShellFromCore(root);
    await settle();
    const lanesBefore = root.querySelectorAll("[data-lane-id]").length;
    expect(lanesBefore).toBeGreaterThan(0);

    fake.adapterGone();
    // Longer than the shell's 250ms drain, so every poll in the window the run
    // measured has failed at least once.
    for (let poll = 0; poll < 6; poll += 1) {
      await new Promise((resolve) => setTimeout(resolve, 60));
      await settle(4);
    }

    // Nothing is torn down and nothing is invented: the cockpit still shows the
    // last projection, and it does not claim to be reconnecting on its own.
    expect(root.querySelector("[data-screen]")?.getAttribute("data-screen")).toBe("d1-cockpit");
    expect(root.querySelectorAll("[data-lane-id]").length).toBe(lanesBefore);
    expect(root.dataset.clientState).toBe("connected");
    expect(root.isConnected).toBe(true);
  });

  test("a rejected read is never rendered as a Core fact", async () => {
    const fake = host();
    await hydrateShellFromCore(root);
    await settle();
    fake.adapterGone();
    for (let poll = 0; poll < 4; poll += 1) {
      await new Promise((resolve) => setTimeout(resolve, 60));
      await settle(4);
    }
    // The host's transport sentence is not display text; a client that painted
    // it into the transcript would be reporting a Core fact that never existed.
    expect(root.querySelector("[data-transcript]")?.textContent ?? "").not.toContain(
      ADAPTER_GONE,
    );
  });
});
