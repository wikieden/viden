// @vitest-environment jsdom

/**
 * H2 hygiene — E1 defect 8: "the Welcome screen does not fill the window".
 *
 * The run saw Welcome's content *and the status bar* stop at roughly 525 px in
 * an 800 px, a 900 px and a 640 px window, identically. That shape — the whole
 * shell short, not just the centre pane — is what a content-sized flex chain
 * produces: `gui-kit.css`'s `.frame` is a flex column and wins `display` over
 * `.d1-frame`'s grid in the bundled cascade, so every row below the titlebar
 * was sized by its content once the composer and the permission host were not
 * mounted.
 *
 * This suite pins the chain that has to fill, at each of the three heights the
 * defect was observed at. It asserts *computed* values rather than the CSS
 * text, and it loads the real stylesheets in the hostile order (`gui-kit.css`
 * last, the way the bundle orders them), so a rule that only wins in dev fails
 * here.
 *
 * jsdom performs no layout, so this cannot measure pixels. It does not need
 * to: the defect is that a link in the chain is *content-sized*, and every
 * assertion below is that the link is a fill instead — and that the answer is
 * the same at 640, 800 and 900, which is exactly what "a magic number" would
 * not be.
 */

import { readFileSync } from "node:fs";
import { join } from "node:path";

import { beforeEach, describe, expect, test, vi } from "vitest";

import { renderD1Cockpit, type D1IntentResult } from "../src/screens/d1_cockpit";
import { D1_PROJECTION } from "./support/d1_projection";

/// The cascade the bundle produces: `d1_cockpit.css` first (minus its own
/// `@import`, which is inlined at the end here), then the design kit, whose
/// `.frame` therefore wins `display` exactly as it does in the built app.
function installStyles(): void {
  const read = (relative: string) => readFileSync(join(process.cwd(), relative), "utf8");
  const cockpit = read("src/screens/d1_cockpit.css").replace(/@import[^;]+;/g, "");
  const welcome = read("src/components/welcome_center.css");
  const kit = read("../../docs/viden-design/Viden/GUI/gui-kit.css").replace(/@import[^;]+;/g, "");
  for (const text of [cockpit, welcome, kit]) {
    const style = document.createElement("style");
    style.textContent = text;
    document.head.append(style);
  }
}

function mountWelcome(height: number) {
  Object.defineProperty(window, "innerHeight", { value: height, configurable: true });
  document.body.innerHTML = '<main id="app"></main>';
  const root = document.querySelector<HTMLElement>("#app")!;
  const result: D1IntentResult = {
    projection: D1_PROJECTION,
    pendingCommandId: null,
    outcome: { state: "confirmed", reason: null },
  };
  const controller = renderD1Cockpit(
    root,
    D1_PROJECTION,
    vi.fn(async () => result),
    vi.fn(async () => result),
    undefined,
    undefined,
    { poll: false, showWelcome: true, onOpenProject: vi.fn() },
  );
  return { root, controller };
}

const HEIGHTS = [640, 800, 900];

describe("Welcome fills the window at every observed height", () => {
  beforeEach(() => {
    document.head.innerHTML = "";
    document.body.innerHTML = "";
    installStyles();
  });

  test.each(HEIGHTS)("the shell frame claims the whole viewport at %ipx", (height) => {
    const { root, controller } = mountWelcome(height);
    const frame = root.querySelector<HTMLElement>(".d1-frame")!;
    expect(getComputedStyle(frame).height).toBe("100vh");
    controller.dispose();
  });

  test.each(HEIGHTS)("the body grows into the frame's spare space at %ipx", (height) => {
    // The link the defect broke. Under the kit's flex column a body with the
    // initial `flex: 0 1 auto` is sized by its content, which is how the
    // status bar ended up at 525 px instead of at the bottom.
    const { root, controller } = mountWelcome(height);
    const body = getComputedStyle(root.querySelector<HTMLElement>(".d1-body")!);
    expect(body.flexGrow).toBe("1");
    expect(body.minHeight).toBe("0");
    controller.dispose();
  });

  test.each(HEIGHTS)("the welcome main is one filling row at %ipx", (height) => {
    // Welcome mounts without the permission host and the composer, so the
    // other two rows must not survive: `.d1-main-welcome` has to *win* over
    // `.d1-main`, which it did not while it sat earlier in the same file.
    const { root, controller } = mountWelcome(height);
    const main = root.querySelector<HTMLElement>("[data-lane-work-surface]")!;
    expect(main.classList.contains("d1-main-welcome")).toBe(true);
    expect(getComputedStyle(main).gridTemplateRows).toBe("minmax(0, 1fr)");
    controller.dispose();
  });

  test.each(HEIGHTS)("the work surface stretches Welcome into that row at %ipx", (height) => {
    const { root, controller } = mountWelcome(height);
    const surface = getComputedStyle(root.querySelector<HTMLElement>("[data-work-surface]")!);
    expect(surface.display).toBe("grid");
    expect(surface.gridTemplateRows).toBe("minmax(0, 1fr)");
    controller.dispose();
  });

  test.each(HEIGHTS)("Welcome itself asks for the full column at %ipx", (height) => {
    const { root, controller } = mountWelcome(height);
    const welcome = getComputedStyle(root.querySelector<HTMLElement>("[data-d1-welcome]")!);
    expect(welcome.minHeight).toBe("100%");
    controller.dispose();
  });

  test("no link in the chain is a fixed pixel height", () => {
    // "Fix the CSS, not with a magic number": a height that was right at one
    // window size is exactly the regression this suite exists to catch.
    const { root, controller } = mountWelcome(900);
    for (const selector of [
      ".d1-body",
      "[data-lane-work-surface]",
      "[data-work-surface]",
      "[data-d1-welcome]",
      ".d1-welcome-content",
    ]) {
      const style = getComputedStyle(root.querySelector<HTMLElement>(selector)!);
      expect(style.height).not.toMatch(/^\d+px$/);
      expect(style.maxHeight).not.toMatch(/^\d+px$/);
    }
    controller.dispose();
  });
});
