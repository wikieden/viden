#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="${1:-"$ROOT/target/tui-regression/0.3.3"}"
SCREENSHOT_DIR="$OUT_DIR/screenshots"
VERSION="${VIDEN_TUI_SCREENSHOT_VERSION:-}"
if [[ -z "$VERSION" ]]; then
  VERSION="$(cargo pkgid -p viden-tui | sed 's/.*@//')"
fi

mkdir -p "$SCREENSHOT_DIR"

"$ROOT/scripts/tui-previews.sh" "$OUT_DIR"

python3 - "$OUT_DIR" <<'PY'
from pathlib import Path
import sys

out_dir = Path(sys.argv[1])
for path in out_dir.glob("*.txt"):
    lines = path.read_text(encoding="utf-8").splitlines()
    path.write_text("\n".join(line.rstrip() for line in lines) + "\n", encoding="utf-8")
PY

CERT_LOG_DIR="$OUT_DIR/certification-tests"
CERT_RESULTS="$OUT_DIR/certification-tests.tsv"
mkdir -p "$CERT_LOG_DIR"
: >"$CERT_RESULTS"

run_certification_test() {
  local id="$1"
  local test_name="$2"
  local log_file="$CERT_LOG_DIR/$id.log"
  local output
  local rc
  set +e
  output="$(cargo test -p viden-tui "$test_name" -- --exact --nocapture 2>&1)"
  rc=$?
  set -e
  printf '%s\n' "$output" >"$log_file"
  if [[ "$rc" -ne 0 ]] || ! grep -Eq 'test result: ok\. 1 passed' "$log_file"; then
    printf 'tui certification failed: %s did not match one passing test\n' "$test_name" >&2
    tail -80 "$log_file" >&2 || true
    exit 1
  fi
  printf '%s\t%s\t%s\n' "$id" "$test_name" "$log_file" >>"$CERT_RESULTS"
}

run_certification_test contract_manifest \
  tui::app::tests::release_manifest_declares_requested_and_effective_presentation_inputs
run_certification_test shared_fixture_replay \
  tui::client::tests::shared_frontend_fixtures_reduce_to_core_expected_facts
run_certification_test extension_fixture_owner \
  tui::client::tests::lane_runtime_owner_extension_fixture_replays_the_exact_owner
run_certification_test stream_composer \
  tui::app::tests::composer_stays_editable_while_events_stream
run_certification_test tool_composer \
  tui::app::tests::runtime_provider_turn_starts_without_blocking_ui_thread
run_certification_test approval_composer \
  tui::app::tests::active_approval_does_not_swallow_composer_typing
run_certification_test input_modes \
  tui::keymap::tests::escape_unwinds_overlay_then_selection_then_insert
run_certification_test mode_badges \
  tui::statusbar::tests::status_bar_tracks_insert_and_overlay_ownership
run_certification_test paste_no_send \
  tui::app::tests::paste_normalizes_crlf_preserves_leading_slash_and_never_submits
run_certification_test cjk_cursor \
  tui::composer::tests::cursor_sits_on_middle_input_row_for_ime_candidate_placement
run_certification_test approval_actions \
  tui::input::tests::approval_keyboard_focus_reaches_deny_diff_and_approve
run_certification_test exact_owner_cancel \
  tui::app::tests::owner_scoped_cancel_uses_the_exact_live_lane_owner_without_denying_approval
run_certification_test width_matrix \
  tui::preview::tests::all_lenses_have_deterministic_80_112_160_render_models
run_certification_test locale_catalogs \
  tui::i18n::tests::catalogs_have_exact_key_and_parameter_parity
run_certification_test locale_projection \
  tui::app::tests::runtime_replacement_switches_locale_without_cached_tui_authority
run_certification_test palette_depth_matrix \
  tui::palette::tests::all_eight_palettes_map_across_truecolor_ansi256_and_ansi16
run_certification_test density_matrix \
  tui::render::appearance_tests::core_density_changes_the_rendered_right_rail_geometry
run_certification_test reduced_motion \
  tui::render::appearance_tests::reduced_motion_keeps_the_live_indicator_static
run_certification_test settings_receipts \
  tui::preferences::tests::apply_and_reset_wait_for_matching_core_receipts
run_certification_test invalid_fallback \
  tui::preferences::tests::invalid_or_partial_appearance_falls_back_atomically
run_certification_test local_color_depth \
  tui::preferences::tests::settings_build_only_typed_dirty_patch_and_keep_color_depth_local
run_certification_test source_boundary \
  tui::state::tests::tui_source_has_no_authoritative_runtime_effects

CHECKPOINT="54965464e87860f9c39a1fb656c2f528e354da94"
git cat-file -e "$CHECKPOINT^{commit}"
SOURCE_HEAD="$(git rev-parse HEAD)"
SOURCE_BRANCH="$(git branch --show-current)"

python3 - "$ROOT" "$OUT_DIR" "$VERSION" "$SOURCE_HEAD" "$SOURCE_BRANCH" <<'PY'
from pathlib import Path
import hashlib
import json
import re
import sys

root = Path(sys.argv[1])
out_dir = Path(sys.argv[2])
version = sys.argv[3]
source_head = sys.argv[4]
source_branch = sys.argv[5]

manifest_path = root / "apps/tui/release-manifest.toml"

def parse_manifest(path: Path) -> dict:
    # This certification reads only the manifest's scalar and JSON-compatible
    # array subset, keeping the release gate independent of Python 3.11.
    parsed = {}
    section = None
    lines = iter(path.read_text(encoding="utf-8").splitlines())
    for raw_line in lines:
        line = raw_line.strip()
        if not line or line.startswith("#"):
            continue
        if line.startswith("[") and line.endswith("]"):
            section = line[1:-1]
            parsed[section] = {}
            continue
        key, value = (part.strip() for part in line.split("=", 1))
        while value.startswith("[") and not value.endswith("]"):
            value += " " + next(lines).strip()
        value = re.sub(r",\s*]$", "]", value)
        if value in {"true", "false"}:
            decoded = value == "true"
        else:
            decoded = json.loads(value)
        parsed[section][key] = decoded
    return parsed

manifest = parse_manifest(manifest_path)
release = manifest["release"]
base_capabilities = manifest["compatibility"]["required_capabilities"]
extension_capabilities = manifest["extensions"]["capabilities"]
presentation = manifest["presentation"]
fixture_revisions = manifest["fixture_revisions"]

if version != release["version"]:
    raise SystemExit(f"TUI certification version mismatch: {version} vs {release['version']}")
if release["base_core_checkpoint"] != "54965464e87860f9c39a1fb656c2f528e354da94":
    raise SystemExit("TUI certification Core checkpoint is not Core 0.3.4")
if release["supported_schema_versions"] != [1]:
    raise SystemExit("TUI certification schema set is not [1]")
# These counts track the manifest's own [compatibility] and [extensions] lists,
# which in turn track FRONTEND_V1_CAPABILITIES / FRONTEND_V1_EXTENSION_CAPABILITIES;
# bump them in the same change that adds a capability.
# 23 completes the 0.3.3 contract increment's 19 -> 23 gate
# (docs/release-0.3.3-contract-design.md, "Capability gating"): structured_diff,
# operator_git, conflict_content, and evidence_reads, in that landing order.
if len(base_capabilities) != 15 or len(extension_capabilities) != 23:
    raise SystemExit("TUI certification capability counts are not base 15 + extension 23")
if set(base_capabilities) & set(extension_capabilities):
    raise SystemExit("TUI certification base and extension capabilities overlap")

def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()

fixture_root = root / "crates/types/tests/fixtures/frontend-contract-v1"
base_fixture_names = [
    "approval-allow-deny.json",
    "context-pressure-cost-blind.json",
    "d1-vertical-slice.json",
    "dag-blocker.json",
    "merge-gate.json",
    "multi-lane.json",
    "plan-denial.json",
    "queued-follow-up.json",
    "stream-tool.json",
]
base_fixture_digests = {
    name: sha256(fixture_root / name) for name in base_fixture_names
}
digest_lines = "".join(
    f"{digest}  {name}\n" for name, digest in sorted(base_fixture_digests.items())
)
corpus_digest = hashlib.sha256(digest_lines.encode("utf-8")).hexdigest()
manifest_fixture_digests = [
    f"{name}:{digest}" for name, digest in sorted(base_fixture_digests.items())
]
if fixture_revisions.get("algorithm") != "sha256":
    raise SystemExit("base fixture revision algorithm is not sha256")
if fixture_revisions.get("base_fixture_sha256") != manifest_fixture_digests:
    raise SystemExit("base fixture set or per-file digest does not match the TUI manifest")
if fixture_revisions.get("base_corpus_sha256") != corpus_digest:
    raise SystemExit("base fixture corpus digest does not match the TUI manifest")
(out_dir / "shared-fixture-digests.sha256").write_text(digest_lines, encoding="utf-8")

extension_fixture = fixture_root / "frontend-host-services.json"
extension_fixture_digest = sha256(extension_fixture)
if extension_fixture_digest != release["extension_fixture_sha256"]:
    raise SystemExit("extension fixture digest does not match the TUI manifest")

revision_paths = {
    "tokens_css": root / "docs/viden-design/Viden/tokens.css",
    "catalog_en": root / "apps/tui/i18n/en.json",
    "catalog_zh_cn": root / "apps/tui/i18n/zh-CN.json",
}
source_revisions = {name: sha256(path) for name, path in revision_paths.items()}
if {"algorithm": "sha256", **source_revisions} != manifest["source_revisions"]:
    raise SystemExit("token or catalog digest does not match the TUI manifest")

forbidden_symbols = [
    "std::process::Command",
    "process::Command",
    "OpenOptions",
    "git worktree",
    "git apply",
    "tmux new-session",
    "SessionEngine",
    ".viden/lanes",
    "std::fs::write",
    "fs::write(",
    "File::create",
    "create_dir_all",
    "write_all(",
    "serde_json::to_writer",
]
boundary_violations = []
for path in sorted((root / "apps/tui/src").rglob("*.rs")):
    production = re.split(
        r"#\[cfg\(test\)\]\s*mod\s+\w+\s*\{",
        path.read_text(encoding="utf-8"),
        maxsplit=1,
    )[0]
    for symbol in forbidden_symbols:
        if symbol in production:
            boundary_violations.append(f"{path.relative_to(root)}: {symbol}")

cargo_toml = (root / "apps/tui/Cargo.toml").read_text(encoding="utf-8")
for dependency in [
    "viden-runtime",
    "viden-provider",
    "viden-tools",
    "viden-permissions",
    "viden-session",
    "viden-workflows",
]:
    if dependency in cargo_toml:
        boundary_violations.append(f"apps/tui/Cargo.toml: {dependency}")
if boundary_violations:
    raise SystemExit("TUI boundary violations:\n" + "\n".join(boundary_violations))
(out_dir / "tui-boundary-report.txt").write_text(
    "status=passed\nauthoritative_effects=absent\nprivate_persistence=absent\n"
    "runtime_internal_dependencies=absent\n",
    encoding="utf-8",
)

test_results = {}
for line in (out_dir / "certification-tests.tsv").read_text(encoding="utf-8").splitlines():
    test_id, test_name, log_file = line.split("\t")
    test_results[test_id] = {
        "status": "passed",
        "test": test_name,
        "log": log_file,
    }

profile_names = {
    "aurora:dark": "aurora",
    "aurora:light": "aurora-light",
    "ice:dark": "ice",
    "ice:light": "ice-light",
    "mono:dark": "mono",
    "mono:light": "mono-light",
    "amber:dark": "amber",
    "phosphor:dark": "phosphor",
}
palette_depth_matrix = [
    {
        "palette": palette,
        "depth": depth,
        "evidence": test_results["palette_depth_matrix"]["test"],
        "truecolor_preview": (
            str(out_dir / f"main.{profile_names[palette]}.ansi")
            if depth == "truecolor"
            else None
        ),
    }
    for palette in presentation["valid_skin_modes"]
    for depth in presentation["effective_tui_color_depth"]
]

design_sources = [
    root / "docs/viden-design/Viden/index.html",
    root / "docs/viden-design/Viden/TUI/Viden - 设计稿索引 (TUI).html",
    root / "docs/viden-design/Viden/TUI/Viden - 统一原型 (TUI).html",
    root / "docs/viden-design/reference-shots/TUI-组件库.png",
    root / "docs/viden-design/reference-shots/TUI-统一原型驾驶舱.png",
]

certification = {
    "status": "passed",
    "component": "tui",
    "version": version,
    "source": {
        "branch": source_branch,
        "worktree": str(root),
        "head_at_evidence_run": source_head,
        "final_commit_rule": "Report the final commit SHA in handoff; do not self-reference it here.",
    },
    "core": {
        "checkpoint": release["base_core_checkpoint"],
        "schema": 1,
        "contract_payload_sha": release["contract_payload_sha"],
        "base_capabilities": base_capabilities,
        "base_capability_count": len(base_capabilities),
        "extension_capabilities": extension_capabilities,
        "extension_capability_count": len(extension_capabilities),
        "extension_fixture_sha256": extension_fixture_digest,
    },
    "fixtures": {
        "manifest_pinned": True,
        "shared_replay_parity": test_results["shared_fixture_replay"],
        "base_fixture_sha256": base_fixture_digests,
        "aggregate_sha256": corpus_digest,
        "extension_fixture": {
            "file": str(extension_fixture.relative_to(root)),
            "sha256": extension_fixture_digest,
            "replay": test_results["extension_fixture_owner"],
        },
    },
    "behavior": {
        "composer_during_stream_tool_approval": [
            test_results["stream_composer"],
            test_results["tool_composer"],
            test_results["approval_composer"],
        ],
        "normal_insert_overlay": [test_results["input_modes"], test_results["mode_badges"]],
        "paste_no_send": test_results["paste_no_send"],
        "cjk_cursor": test_results["cjk_cursor"],
        "approval_actions": test_results["approval_actions"],
        "exact_lane_owner_cancel": test_results["exact_owner_cancel"],
        "settings_apply_reset_receipts": test_results["settings_receipts"],
        "invalid_fallback": test_results["invalid_fallback"],
    },
    "appearance": {
        "widths": [80, 112, 160],
        "width_evidence": test_results["width_matrix"],
        "locales": ["en", "zh-CN"],
        "locale_evidence": [test_results["locale_catalogs"], test_results["locale_projection"]],
        "palette_depth_matrix": palette_depth_matrix,
        "densities": presentation["densities"],
        "density_evidence": test_results["density_matrix"],
        "reduced_motion": test_results["reduced_motion"],
    },
    "boundary": {
        "authoritative_effects": "absent",
        "private_persistence": "absent",
        "local_color_depth_only": test_results["local_color_depth"],
        "source_scan": test_results["source_boundary"],
        "report": str(out_dir / "tui-boundary-report.txt"),
    },
    "source_revisions": source_revisions,
    "accepted_design_references": [
        {
            "path": str(path.relative_to(root)),
            "sha256": sha256(path),
            "mode": "read-only",
        }
        for path in design_sources
    ],
    "tests": test_results,
}
(out_dir / f"tui-{version}-certification.json").write_text(
    json.dumps(certification, indent=2, ensure_ascii=False) + "\n",
    encoding="utf-8",
)
PY

copy_artifact() {
  local source="$1"
  local target="$2"
  if [[ -f "$source" ]]; then
    cp "$source" "$target"
  else
    printf 'tui regression failed: missing artifact %s\n' "$source" >&2
    exit 1
  fi
}

copy_artifact "$OUT_DIR/main.svg" "$SCREENSHOT_DIR/${VERSION}-tui-main.svg"
copy_artifact "$OUT_DIR/main-idle.svg" "$SCREENSHOT_DIR/${VERSION}-tui-main-idle.svg"
copy_artifact "$OUT_DIR/main-live-turn.svg" "$SCREENSHOT_DIR/${VERSION}-tui-live-turn.svg"
copy_artifact "$OUT_DIR/main-resize.svg" "$SCREENSHOT_DIR/${VERSION}-tui-main-resize.svg"
copy_artifact "$OUT_DIR/main-cjk-input.svg" "$SCREENSHOT_DIR/${VERSION}-tui-cjk-input.svg"
copy_artifact "$OUT_DIR/main-command-palette.svg" "$SCREENSHOT_DIR/${VERSION}-tui-command-palette.svg"
copy_artifact "$OUT_DIR/main-setup-wizard.svg" "$SCREENSHOT_DIR/${VERSION}-tui-setup-wizard.svg"
copy_artifact "$OUT_DIR/main-provider-selector.svg" "$SCREENSHOT_DIR/${VERSION}-tui-provider-selector.svg"
copy_artifact "$OUT_DIR/main-provider-detail.svg" "$SCREENSHOT_DIR/${VERSION}-tui-provider-detail.svg"
copy_artifact "$OUT_DIR/main-model-selector.svg" "$SCREENSHOT_DIR/${VERSION}-tui-model-selector.svg"
copy_artifact "$OUT_DIR/main-lane-selector.svg" "$SCREENSHOT_DIR/${VERSION}-tui-lane-selector.svg"
copy_artifact "$OUT_DIR/main-approval-hunks.svg" "$SCREENSHOT_DIR/${VERSION}-tui-approval-hunks.svg"
copy_artifact "$OUT_DIR/main-git-picker.svg" "$SCREENSHOT_DIR/${VERSION}-tui-git-picker.svg"
copy_artifact "$OUT_DIR/main-git-outcome.svg" "$SCREENSHOT_DIR/${VERSION}-tui-git-outcome.svg"
copy_artifact "$OUT_DIR/main-conflict-detail.svg" "$SCREENSHOT_DIR/${VERSION}-tui-conflict-detail.svg"
copy_artifact "$OUT_DIR/main-evidence-list.svg" "$SCREENSHOT_DIR/${VERSION}-tui-evidence-list.svg"
copy_artifact "$OUT_DIR/main-evidence-detail.svg" "$SCREENSHOT_DIR/${VERSION}-tui-evidence-detail.svg"
copy_artifact "$OUT_DIR/main-lane.svg" "$SCREENSHOT_DIR/${VERSION}-tui-lane-detail.svg"
copy_artifact "$OUT_DIR/side-1.svg" "$SCREENSHOT_DIR/${VERSION}-tui-side-1.svg"
copy_artifact "$OUT_DIR/side-2.svg" "$SCREENSHOT_DIR/${VERSION}-tui-side-2.svg"

python3 - "$OUT_DIR" "$SCREENSHOT_DIR" "$VERSION" <<'PY'
from pathlib import Path
import json
import sys

out_dir = Path(sys.argv[1])
screenshot_dir = Path(sys.argv[2])
version = sys.argv[3]
required = {
    "main": out_dir / "main.txt",
    "main_idle": out_dir / "main-idle.txt",
    "main_live_turn": out_dir / "main-live-turn.txt",
    "main_resize": out_dir / "main-resize.txt",
    "main_cjk_input": out_dir / "main-cjk-input.txt",
    "command_palette": out_dir / "main-command-palette.txt",
    "setup_wizard": out_dir / "main-setup-wizard.txt",
    "provider_selector": out_dir / "main-provider-selector.txt",
    "provider_detail": out_dir / "main-provider-detail.txt",
    "model_selector": out_dir / "main-model-selector.txt",
    "lane_selector": out_dir / "main-lane-selector.txt",
    "approval_hunks": out_dir / "main-approval-hunks.txt",
    "git_picker": out_dir / "main-git-picker.txt",
    "git_outcome": out_dir / "main-git-outcome.txt",
    "conflict_detail": out_dir / "main-conflict-detail.txt",
    "evidence_list": out_dir / "main-evidence-list.txt",
    "evidence_detail": out_dir / "main-evidence-detail.txt",
    "lane_detail": out_dir / "main-lane.txt",
    "side_1": out_dir / "side-1.txt",
    "side_2": out_dir / "side-2.txt",
    "multiscreen": out_dir / "multiscreen.txt",
    "certification": out_dir / f"tui-{version}-certification.json",
}

def fail(message: str) -> None:
    raise SystemExit(f"tui regression failed: {message}")

for name, path in required.items():
    if not path.exists():
        fail(f"missing {name} preview at {path}")

certification = json.loads(required["certification"].read_text(encoding="utf-8"))
if certification.get("status") != "passed":
    fail(f"TUI {version} certification status is not passed")
if certification.get("core", {}).get("checkpoint") != "54965464e87860f9c39a1fb656c2f528e354da94":
    fail("Core checkpoint does not match the reviewed Core 0.3.4 checkpoint")
if certification.get("core", {}).get("schema") != 1:
    fail("frontend schema is not 1")
core_certification = certification.get("core", {})
if core_certification.get("base_capability_count") != 15:
    fail("frozen base capability count is not 15")
if core_certification.get("extension_capability_count") != len(
    core_certification.get("extension_capabilities", [])
):
    fail("feature extension capability count does not match the recorded capabilities")
if len(certification.get("appearance", {}).get("palette_depth_matrix", [])) != 24:
    fail("appearance evidence does not cover eight palettes across three depths")
for field, expected in {
    "widths": [80, 112, 160],
    "locales": ["en", "zh-CN"],
    "densities": ["compact", "regular", "comfy"],
}.items():
    if certification.get("appearance", {}).get(field) != expected:
        fail(f"appearance evidence has the wrong {field} matrix")

main_lines = required["main"].read_text(encoding="utf-8").splitlines()
if len(main_lines) != 40:
    fail(f"main preview has {len(main_lines)} lines, expected 40")
if not any("LIVE WORK" in line for line in main_lines):
    fail("main preview does not show live work activity")
if not any("Viden >" in line for line in main_lines):
    fail("main preview does not show composer title")
if not any("› " in line for line in main_lines):
    fail("main preview does not show composer prompt")

main_live_turn = required["main_live_turn"].read_text(encoding="utf-8")
if (
    "LIVE WORK" not in main_live_turn
    or "live provider request" not in main_live_turn
    or "input open" not in main_live_turn
    or "[^J Queue]" not in main_live_turn
    or "[^C Cancel]" not in main_live_turn
):
    fail("main live-turn preview does not show provider activity evidence")

main_resize = required["main_resize"].read_text(encoding="utf-8")
if "Resize-safe redraw check" not in main_resize or "LIVE WORK" not in main_resize:
    fail("resize preview does not show redraw/inline activity evidence")

main_cjk_input = required["main_cjk_input"].read_text(encoding="utf-8")
if "你好，帮我检查当前变更" not in main_cjk_input:
    fail("CJK input preview does not show Chinese composer input")

provider_selector = required["provider_selector"].read_text(encoding="utf-8")
if "Connect a provider" not in provider_selector or "DeepSeek" not in provider_selector:
    fail("provider selector preview does not show the provider panel evidence")
if "DEEPSEEK_API_KEY" in provider_selector or "default endpoint" in provider_selector:
    fail("provider selector preview should not show provider config details")

provider_detail = required["provider_detail"].read_text(encoding="utf-8")
if "PROVIDER openai healthy" not in provider_detail or "TRUSTED INGRESS unavailable" not in provider_detail:
    fail("provider detail preview does not show safe Core health and trusted-ingress evidence")
if "API key" in provider_detail or "Enter submit" in provider_detail or "/provider key" in provider_detail:
    fail("provider detail preview must not expose raw credential entry")

setup_wizard = required["setup_wizard"].read_text(encoding="utf-8")
if "SETUP SELECTOR" not in setup_wizard or "DRAFT viden.toml" not in setup_wizard:
    fail("setup preview does not show the Core-backed project draft actions")

model_selector = required["model_selector"].read_text(encoding="utf-8")
if "Select model" not in model_selector or "deepseek-v4-flash" not in model_selector:
    fail("model selector preview does not show direct grouped model panel evidence")
if "OpenAI" in model_selector or "Fallback" in model_selector or "fallback-local" in model_selector:
    fail("model selector preview should only show configured providers and active models")

lane_selector = required["lane_selector"].read_text(encoding="utf-8")
if "LANE ACTIONS" not in lane_selector or "inspect L1" not in lane_selector:
    fail("lane selector preview does not show lane action evidence")

side_1 = required["side_1"].read_text(encoding="utf-8")
side_2 = required["side_2"].read_text(encoding="utf-8")
if "AGENT LANES" not in side_1:
    fail("side-1 missing agent lanes")
if "TESTS / LSP" not in side_2:
    fail("side-2 missing tests/lsp evidence")

artifacts = sorted(str(path) for path in screenshot_dir.glob(f"{version}-tui-*.svg"))
if len(artifacts) < 6:
    fail("expected at least six screenshot artifacts")

(out_dir / "tui-regression-evidence.json").write_text(
    json.dumps(
        {
            "status": "passed",
            "preview_dir": str(out_dir),
            "screenshots": artifacts,
            "checks": [
                f"TUI {version} CoreClient boundary certification",
                "Core checkpoint and capability manifest",
                "shared frontend fixture replay parity",
                "appearance and interaction matrix exact tests",
                "line counts",
                "live work activity",
                "composer visibility",
                "live provider turn evidence",
                "resize redraw evidence",
                "CJK composer input",
                "provider command stays compact before submit",
                "provider detail is handle-only with trusted ingress unavailable",
                "model command stays compact before submit",
                "side-1 lanes",
                "side-2 tests/lsp",
                "screenshot artifact export",
            ],
        },
        indent=2,
        ensure_ascii=False,
    )
    + "\n",
    encoding="utf-8",
)
PY

cat >"$SCREENSHOT_DIR/README.md" <<EOF
# ${VERSION} TUI Regression Screenshots

Generated by \`scripts/tui-regression.sh\`.

Each SVG is a deterministic visual artifact for product review:

- \`${VERSION}-tui-main.svg\`: active main cockpit
- \`${VERSION}-tui-main-idle.svg\`: first-launch welcome composer
- \`${VERSION}-tui-live-turn.svg\`: live provider request evidence
- \`${VERSION}-tui-main-resize.svg\`: resized 100x30 redraw evidence
- \`${VERSION}-tui-cjk-input.svg\`: CJK input and cursor-placement evidence
- \`${VERSION}-tui-command-palette.svg\`: slash-command suggestion surface
- \`${VERSION}-tui-setup-wizard.svg\`: Core-backed project draft and preview actions
- \`${VERSION}-tui-provider-selector.svg\`: provider supplier picker with first-level ids only
- \`${VERSION}-tui-provider-detail.svg\`: safe provider health and trusted-ingress status
- \`${VERSION}-tui-model-selector.svg\`: provider-grouped model selector evidence
- \`${VERSION}-tui-lane-selector.svg\`: lane action selector evidence
- \`${VERSION}-tui-approval-hunks.svg\`: approval overlay with Core decision-context hunk rows
- \`${VERSION}-tui-git-picker.svg\`: /git operator source-control picker
- \`${VERSION}-tui-git-outcome.svg\`: completed and failed operator git outcomes
- \`${VERSION}-tui-conflict-detail.svg\`: read-only structured conflict content modal
- \`${VERSION}-tui-evidence-list.svg\`: paged evidence archive inspector
- \`${VERSION}-tui-evidence-detail.svg\`: evidence detail with canonical content
- \`${VERSION}-tui-lane-detail.svg\`: focused lane detail
- \`${VERSION}-tui-side-1.svg\`: lane side screen
- \`${VERSION}-tui-side-2.svg\`: ops/test side screen

Structured evidence: \`../tui-regression-evidence.json\`

Client-boundary certification: \`../tui-${VERSION}-certification.json\`

Fixture and boundary evidence: \`../shared-fixture-digests.sha256\` and
\`../tui-boundary-report.txt\`
EOF

printf '%s\n' "$OUT_DIR/tui-regression-evidence.json"
