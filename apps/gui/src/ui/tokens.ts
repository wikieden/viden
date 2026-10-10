// Token source metadata. TOKEN_SOURCE_SHA256 is the sha256 of
// docs/viden-design/Viden/tokens.css (`shasum -a 256 <path>`). When the design
// package changes that file, recompute it and update every live pin together:
// this constant, `[appearance].token_revision_sha256` in
// apps/gui/release-manifest.toml (a new GUI version's frozen snapshot copies
// it; shipped snapshots keep the digest they shipped with),
// `[source_revisions].tokens_css` in apps/tui/release-manifest.toml, and the
// literal its manifest test asserts in apps/tui/src/tui/app.rs.
// `scripts/tui-regression.sh` recomputes the TUI pin; nothing recomputes this
// one automatically, so it is checked by hand in the same change.
export const TOKEN_SOURCE_PATH = "docs/viden-design/Viden/tokens.css";
export const TOKEN_SOURCE_SHA256 = "2d80baa92bc7b83a188257a08dec27cb1fc143fc27237a02dfdbb61eee8b941a";
export const SEMANTIC_TOKEN_ROLES = [
  "accent",
  "bg-base",
  "bg-elev",
  "bg-panel",
  "bg-sel",
  "bg-topbar",
  "bg-void",
  "border",
  "border-active",
  "border-soft",
  "error",
  "fg-faint",
  "fg-muted",
  "fg-primary",
  "fg-secondary",
  "gold",
  "on-accent",
  "progress",
  "success",
  "warning",
] as const;
export const TOKEN_SKIN_MODE_PAIRS = [
  "aurora/dark",
  "aurora/light",
  "ice/dark",
  "ice/light",
  "mono/dark",
  "mono/light",
  "amber/dark",
  "phosphor/dark",
] as const;
export const TOKEN_DENSITIES = ["compact", "regular", "comfy"] as const;
