// @vitest-environment jsdom

import { readFileSync } from "node:fs";
import { join } from "node:path";

import { describe, expect, test, vi } from "vitest";

import { renderD1Cockpit, type D1IntentResult } from "../../src/screens/d1_cockpit";
import { D1_PROJECTION } from "../support/d1_projection";

function renderActiveD1() {
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
    { poll: false },
  );
  return { root, controller };
}

describe("D1 accessibility and contrast semantics", () => {
  test("does not rely on color alone for operational state channels", () => {
    const { root, controller } = renderActiveD1();

    expect(root.querySelector("[data-lane-id='lane-core']")?.textContent).toContain("running");
    expect(root.querySelector("[data-lane-status]")?.getAttribute("aria-hidden")).toBe("true");
    expect(root.querySelector("[data-live-work-bar]")?.getAttribute("aria-description")).toContain(
      "Freeze contract",
    );
    expect(root.querySelector("[data-workspace-change], [data-check-run]")?.textContent).toContain(
      "Passed",
    );
    // Statusbar state channels are text-first: work mode and permission
    // level are named, never encoded by color alone.
    expect(root.querySelector("[data-statusbar]")?.textContent).toContain("MODE build");
    expect(root.querySelector("[data-statusbar]")?.textContent).toContain("PERM ask");
    controller.dispose();
  });

  test("keeps visible focus and reduced-motion rules in the GUI-owned CSS adapter", () => {
    const css = readFileSync(join(process.cwd(), "src/screens/d1_cockpit.css"), "utf8");
    const themeCss = readFileSync(join(process.cwd(), "src/ui/theme.css"), "utf8");
    const qaHtml = readFileSync(
      join(process.cwd(), "evidence/task-6-work-surface/qa.html"),
      "utf8",
    );

    expect(css).toContain(".d1-frame :focus-visible");
    expect(css).toContain("outline:");
    // The Settings overlay is portalled outside `.d1-frame`, so it carries
    // its own copy of the focus-ring rule rather than inheriting one that no
    // longer applies to it.
    const settingsCss = readFileSync(
      join(process.cwd(), "src/components/settings_panel.css"),
      "utf8",
    );
    expect(settingsCss).toContain(".gset-panel :focus-visible");
    expect(settingsCss).toContain("outline: 2px solid var(--accent)");
    expect(themeCss).toContain('[data-motion="reduced"]');
    expect(themeCss).toContain("prefers-reduced-motion");
    expect(themeCss).toContain("transition: none !important");
    expect(qaHtml).toContain('get("font_scale")');
    expect(qaHtml).toContain("document.documentElement.style.fontSize");
  });

  test("neutralises the user-agent button fill the registered kit never overrides", () => {
    // G10 drift. `gui-kit.css` styles `.tbtbtn`, `.gitchip` and friends without
    // a `background`, because the design package draws them as non-interactive
    // elements. The client renders them as real `<button>`s so they can be
    // reached from the keyboard, which exposes the UA's `ButtonFace` fill: the
    // three titlebar tool buttons painted an opaque grey slab on every
    // cockpit-bearing surface, in both themes, and no design token authorises
    // that colour. The reset is written with `:where()` so it carries zero
    // specificity and any real class background still wins.
    const themeCss = readFileSync(join(process.cwd(), "src/ui/theme.css"), "utf8");

    expect(themeCss).toContain(":where(#app) :where(button");
    expect(themeCss).toContain("background-color: transparent");
  });

  test("records the deterministic rc.3 accessibility evidence artifact", () => {
    const evidence = JSON.parse(
      readFileSync(
        join(process.cwd(), "evidence/0.1.0-rc.3/accessibility.json"),
        "utf8",
      ),
    );

    expect(evidence.component).toBe("viden-gui");
    expect(evidence.component_version).toBe("0.1.0-rc.3");
    expect(evidence.matrix.locales).toEqual(["en", "zh-CN"]);
    expect(evidence.matrix.skin_mode_pairs).toHaveLength(8);
    expect(evidence.matrix.densities).toEqual(["compact", "regular", "comfy"]);
    expect(evidence.matrix.motion).toEqual(["system", "reduced"]);
    expect(evidence.matrix.rendered_font_scale).toEqual(["100%", "200%"]);
    expect(evidence.browser_provenance.url).toBe(
      "http://127.0.0.1:4173/evidence/0.1.0-rc.3/d1-canonical-qa.html",
    );
    expect(evidence.browser_provenance.exact_viewport_harness).toBe(
      "http://127.0.0.1:4173/evidence/0.1.0-rc.3/d1-target-viewport-capture.html",
    );
    expect(evidence.browser_provenance.backend).toBe("Chrome extension via Browser runtime");
    expect(evidence.browser_provenance.observations).toHaveLength(4);
    expect(
      evidence.browser_provenance.observations.every(
        (observation: { rendered_font_scale: string; clipping_observation: string }) =>
          observation.rendered_font_scale === "100%" &&
          (observation.clipping_observation.includes("No horizontal document overflow") ||
            observation.clipping_observation.includes("Lower Context Dock facts")),
      ),
    ).toBe(true);
    expect(evidence.outcomes.keyboard_only).toBe("pass");
    expect(evidence.outcomes.cjk_ime).toBe("pass");
    expect(evidence.outcomes.browser_rendered_200_percent_font_scale).toBe("pass");
    expect(evidence.browser_provenance.drawer_probe.dock_drawer_open).toBe("true");
    expect(evidence.browser_provenance.drawer_probe.toggle_expanded).toBe("true");
    expect(evidence.browser_provenance.context_bottom_probe.scroll_top).toBeGreaterThan(0);
    expect(evidence.browser_provenance.context_bottom_probe.visible_tail_contains).toContain(
      "Task checklist",
    );
    expect(evidence.contract_limitations).toContain(
      "Core-resolved ui_preferences does not currently expose a font-scale field; 200% is certified only as rendered browser/local preview scale, not as a persisted GUI preference.",
    );
    expect(evidence.limitations).toContain(
      "Browser-controlled native accessibility tree audit was not executed in this artifact.",
    );
    expect(evidence.limitations).toContain(
      "Chrome capped the outer Browser viewport at 2560x1267 in this session; exact target-size visual evidence was captured through a Browser-rendered 5140x2650 iframe harness.",
    );
  });
});
