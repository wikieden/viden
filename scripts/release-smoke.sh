#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VERSION=""
TARGET=""
OUT_DIR=""
QUICK=0
RUN_DEEPSEEK=0
PROVIDER_SMOKE_ID=""
PROVIDER_SMOKE_MODEL=""
RUN_GITHUB_ACTIONS=0
RUN_GITHUB_RELEASE_ASSETS=0
RUN_HOMEBREW=0
SKIP_PACKAGE=0

usage() {
  cat <<'EOF'
Usage: scripts/release-smoke.sh [options]

Runs the Viden release smoke matrix and stores logs in an evidence directory.

Options:
  --version <version>     Release version to package; defaults to Cargo package version.
  --target <triple>       Release target triple; defaults to the local rustc host.
  --out-dir <dir>         Evidence directory; defaults to /tmp/viden-release-smoke-...
  --quick                 Run a faster local check set without full workspace tests or package smoke.
  --skip-package          Skip host package archive smoke.
  --deepseek              Run the opt-in DeepSeek development scenario smoke. Requires DEEPSEEK_API_KEY.
  --provider-smoke <id>   Run a live provider smoke through scripts/provider-live-smoke.sh.
  --provider-smoke-model <model>
                          Model for --provider-smoke; provider default if omitted.
  --github-actions        Dispatch release.yml with upload_to_release=false. Requires gh auth.
  --github-release-assets Validate the published GitHub release assets and checksums.
  --homebrew              Validate the published Homebrew tap formula.
  -h, --help              Show this help.
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --version)
      VERSION="${2:-}"
      shift 2
      ;;
    --target)
      TARGET="${2:-}"
      shift 2
      ;;
    --out-dir)
      OUT_DIR="${2:-}"
      shift 2
      ;;
    --quick)
      QUICK=1
      SKIP_PACKAGE=1
      shift
      ;;
    --skip-package)
      SKIP_PACKAGE=1
      shift
      ;;
    --deepseek)
      RUN_DEEPSEEK=1
      shift
      ;;
    --provider-smoke)
      PROVIDER_SMOKE_ID="${2:-}"
      shift 2
      ;;
    --provider-smoke-model)
      PROVIDER_SMOKE_MODEL="${2:-}"
      shift 2
      ;;
    --github-actions)
      RUN_GITHUB_ACTIONS=1
      shift
      ;;
    --github-release-assets)
      RUN_GITHUB_RELEASE_ASSETS=1
      shift
      ;;
    --homebrew)
      RUN_HOMEBREW=1
      shift
      ;;
    -h | --help)
      usage
      exit 0
      ;;
    *)
      printf 'unknown option: %s\n\n' "$1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

cd "$ROOT"

if [[ -z "$VERSION" ]]; then
  VERSION="$(cargo pkgid -p viden-cli | sed 's/.*#//')"
fi

if [[ -z "$TARGET" ]]; then
  TARGET="$(rustc -Vv | awk '/host:/ { print $2 }')"
fi

if [[ -z "$OUT_DIR" ]]; then
  OUT_DIR="$(mktemp -d "/tmp/viden-release-smoke-v${VERSION}.XXXXXX")"
fi

mkdir -p "$OUT_DIR"

SUMMARY="$OUT_DIR/summary.md"
STEP_RESULTS="$OUT_DIR/step-results.tsv"
EVIDENCE_JSON="$OUT_DIR/release-evidence.json"
: >"$SUMMARY"
: >"$STEP_RESULTS"

log() {
  printf '[smoke] %s\n' "$*"
}

record() {
  printf '%s\n' "$*" >>"$SUMMARY"
}

record_step_result() {
  local name="$1"
  local status="$2"
  local exit_code="$3"
  local log_file="$4"
  printf '%s\t%s\t%s\t%s\n' "$name" "$status" "$exit_code" "$log_file" >>"$STEP_RESULTS"
}

run_step() {
  local name="$1"
  shift
  local log_file="$OUT_DIR/${name}.log"
  log "START $name"
  if "$@" >"$log_file" 2>&1; then
    log "PASS  $name"
    record "- PASS \`$name\`"
    record_step_result "$name" "pass" "0" "$log_file"
  else
    local rc=$?
    log "FAIL  $name (exit $rc)"
    record "- FAIL \`$name\` (exit $rc, log: \`$log_file\`)"
    record_step_result "$name" "fail" "$rc" "$log_file"
    write_evidence_json
    tail -80 "$log_file" >&2 || true
    exit "$rc"
  fi
}

run_bash_step() {
  local name="$1"
  local script="$2"
  run_step "$name" bash -lc "$script"
}

fallback_cli_smoke() {
  local work_dir="$OUT_DIR/fallback-workspace"
  local transcript="$OUT_DIR/fallback-cli-transcript.log"
  rm -rf "$work_dir"
  mkdir -p "$work_dir"
  (
    cd "$work_dir"
    git init >/dev/null
    git config user.email smoke@example.com
    git config user.name "Viden Smoke"
    printf 'smoke\n' >README.md
    git add README.md
    git commit -m initial >/dev/null
    printf '/test printf smoke-ok\ny\n/status\n/exit\n' |
      cargo run -p viden-cli --manifest-path "$ROOT/Cargo.toml" --quiet -- \
        --no-tui \
        --provider fallback \
        --model test-local
  ) >"$transcript" 2>&1

  grep -Fq "status: passed" "$transcript"
  grep -Fq "smoke-ok" "$transcript"
  grep -Fq "Last test: passed" "$transcript"
  cat "$transcript"
}

deepseek_dev_scenario_smoke() {
  if [[ -z "${DEEPSEEK_API_KEY:-}" ]]; then
    printf 'DEEPSEEK_API_KEY is required for --deepseek\n' >&2
    return 2
  fi
  local smoke_dir="$OUT_DIR/deepseek-dev-scenario"
  scripts/deepseek-dev-scenario-smoke.sh --out-dir "$smoke_dir"
  cat "$smoke_dir/summary.md"
}

provider_live_smoke() {
  local args=(--provider "$PROVIDER_SMOKE_ID" --out-dir "$OUT_DIR/provider-live-smoke-$PROVIDER_SMOKE_ID")
  if [[ -n "$PROVIDER_SMOKE_MODEL" ]]; then
    args+=(--model "$PROVIDER_SMOKE_MODEL")
  fi
  scripts/provider-live-smoke.sh "${args[@]}"
}

lane_operator_smoke() {
  run_step "lane-operator-loop-smoke" scripts/smoke-lane-operator-loop.sh
}

daily_loop_smoke() {
  run_step "daily-loop-smoke" scripts/daily-loop-smoke.sh "$OUT_DIR/daily-loop-smoke"
}

package_smoke() {
  local package_log="$OUT_DIR/package-archive.log"
  local archive
  archive="$(scripts/package-release.sh "$VERSION" "$TARGET" | tee "$package_log" | tail -1)"
  [[ -f "$archive" ]]
  [[ -f "$archive.sha256" ]]

  (
    cd "$(dirname "$archive")"
    if command -v shasum >/dev/null 2>&1; then
      shasum -a 256 -c "$(basename "$archive").sha256"
    else
      sha256sum -c "$(basename "$archive").sha256"
    fi
  ) >>"$package_log" 2>&1

  local extract_dir="$OUT_DIR/package-extract"
  rm -rf "$extract_dir"
  mkdir -p "$extract_dir"
  tar -xzf "$archive" -C "$extract_dir"

  local package_dir="$extract_dir/viden-v${VERSION}-${TARGET}"
  local binary="$package_dir/viden"
  if [[ "$TARGET" == *"windows"* ]]; then
    binary="$package_dir/viden.exe"
  fi

  [[ -x "$binary" || -f "$binary" ]]
  "$binary" --version >>"$package_log" 2>&1
  "$binary" --help >>"$package_log" 2>&1
  grep -Fq "viden $VERSION" "$package_log"
  cat "$package_log"
}

github_actions_validation() {
  command -v gh >/dev/null
  gh workflow run release.yml \
    --repo wikieden/viden \
    -f "tag=v${VERSION}" \
    -f "upload_to_release=false"
}

github_release_asset_validation() {
  command -v gh >/dev/null
  local release_json="$OUT_DIR/github-release-v${VERSION}.json"
  local attempt
  for attempt in 1 2 3; do
    if gh release view "v${VERSION}" \
      --repo wikieden/viden \
      --json tagName,url,isDraft,isPrerelease,publishedAt,assets \
      >"$release_json" \
      && [[ -s "$release_json" ]]; then
      break
    fi
    if [[ "$attempt" == "3" ]]; then
      printf 'failed to fetch GitHub release metadata for v%s\n' "$VERSION" >&2
      return 1
    fi
    sleep 2
  done
  python3 - "$release_json" <<'PY'
import json
import sys
from pathlib import Path

release = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
assets = release.get("assets", [])
names = {asset.get("name", "") for asset in assets}
if release.get("isDraft") or release.get("isPrerelease"):
    raise SystemExit("release must not be draft or prerelease")
# `viden-v` is the CLI/TUI archive from the `build` matrix. The remaining
# prefixes are the GUI bundles from the `gui-bundle` matrix added in 0.3.5:
# `Viden-<version>-<target>.app.zip`, `Viden_<version>_<arch>.dmg`,
# `Viden_<version>_x64_en-US.msi`, `Viden_<version>_x64-setup.exe`,
# `Viden_<version>_amd64.deb`, `Viden_<version>_amd64.AppImage`, and one
# `SHA256SUMS-gui-<target>` per GUI target.
expected_prefixes = [
    "viden-v",
    "Viden-",
    "Viden_",
    "SHA256SUMS-gui-",
]
if len(assets) < 2:
    raise SystemExit("expected release assets and sha256 files")
tarballs = {name for name in names if name.endswith(".tar.gz")}
checksums = {name for name in names if name.endswith(".tar.gz.sha256")}
missing = sorted(f"{name}.sha256" for name in tarballs if f"{name}.sha256" not in checksums)
if missing:
    raise SystemExit(f"missing checksum assets: {missing}")
if not all(any(name.startswith(prefix) for prefix in expected_prefixes) for name in names):
    raise SystemExit("unexpected asset naming")
gui_bundle_suffixes = (".app.zip", ".dmg", ".msi", "-setup.exe", ".deb", ".AppImage")
gui_bundles = {name for name in names if name.endswith(gui_bundle_suffixes)}
gui_checksums = {name for name in names if name.startswith("SHA256SUMS-gui-")}
# A GUI bundle without its checksum file is unverifiable, so the pair is
# required in the same direction as the CLI archive/.sha256 pair above.
if gui_bundles and not gui_checksums:
    raise SystemExit("GUI bundles present without any SHA256SUMS-gui-<target> asset")
if gui_checksums and not gui_bundles:
    raise SystemExit("SHA256SUMS-gui-<target> present without any GUI bundle")
print(json.dumps({
    "tag": release.get("tagName"),
    "url": release.get("url"),
    "asset_count": len(assets),
    "tarballs": sorted(tarballs),
    "gui_bundles": sorted(gui_bundles),
    "gui_checksums": sorted(gui_checksums),
}, ensure_ascii=False))
PY
}

homebrew_validation() {
  command -v brew >/dev/null
  HOMEBREW_NO_AUTO_UPDATE=1 brew tap wikieden/tap >/dev/null
  local tap_repo
  tap_repo="$(brew --repository wikieden/tap)"
  git -C "$tap_repo" pull --ff-only origin main
  HOMEBREW_NO_AUTO_UPDATE=1 brew fetch --formula wikieden/tap/viden
  HOMEBREW_NO_AUTO_UPDATE=1 brew info wikieden/tap/viden --json=v2 \
    >"$OUT_DIR/homebrew-viden.json"
  python3 - "$OUT_DIR/homebrew-viden.json" "$VERSION" <<'PY'
import json
import sys
from pathlib import Path

payload = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
expected = sys.argv[2]
formulae = payload.get("formulae") or []
if not formulae:
    raise SystemExit("missing formula metadata")
formula = formulae[0]
actual = (formula.get("versions") or {}).get("stable")
if actual != expected:
    raise SystemExit(f"expected Homebrew stable {expected}, got {actual}")
print(f"{formula.get('full_name')} {actual}")
PY
}

write_evidence_json() {
  python3 - "$STEP_RESULTS" "$EVIDENCE_JSON" "$VERSION" "$TARGET" "$QUICK" "$OUT_DIR" <<'PY'
import json
import sys
from datetime import datetime, timezone
from pathlib import Path

results_path = Path(sys.argv[1])
target_path = Path(sys.argv[2])
version = sys.argv[3]
target = sys.argv[4]
quick = sys.argv[5] == "1"
out_dir = sys.argv[6]
steps = []
if results_path.exists():
    for line in results_path.read_text(encoding="utf-8").splitlines():
        if not line.strip():
            continue
        name, status, exit_code, log_file = line.split("\t", 3)
        steps.append({
            "name": name,
            "status": status,
            "exit_code": int(exit_code),
            "log": log_file,
        })
payload = {
    "version": version,
    "target": target,
    "quick": quick,
    "out_dir": out_dir,
    "generated_at": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
    "summary": {
        "passed": sum(1 for step in steps if step["status"] == "pass"),
        "failed": sum(1 for step in steps if step["status"] == "fail"),
        "skipped": sum(1 for step in steps if step["status"] == "skip"),
    },
    "steps": steps,
}
target_path.write_text(json.dumps(payload, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
PY
}

record "# Viden Release Smoke"
record ""
record "- Version: \`$VERSION\`"
record "- Target: \`$TARGET\`"
record "- Quick: \`$QUICK\`"
record "- Evidence directory: \`$OUT_DIR\`"
record ""
record "## Results"

run_step "cargo-fmt" cargo fmt --check
run_step "tdd-testing-contract-smoke" scripts/tdd-testing-contract-smoke.sh
run_step "tui-turn-controller-smoke" scripts/tui-turn-controller-smoke.sh
run_step "rc-tui-stability-contract-smoke" scripts/rc-tui-stability-contract-smoke.sh
run_step "final-zero-bug-contract-smoke" scripts/final-zero-bug-contract-smoke.sh
run_step "cargo-clippy" cargo clippy --workspace --all-targets -- -D warnings

if [[ "$QUICK" == "1" ]]; then
  run_bash_step "viden-tui-terminal-tests" "cargo test -p viden-tui tui::terminal::tests -- --nocapture"
else
  run_bash_step "viden-terminal-entry-tests" "cargo test -p viden-cli --quiet -- --test-threads=1"
  run_bash_step "workspace-tests" "cargo test --workspace --quiet -- --test-threads=1"
fi

run_step "tui-regression" scripts/tui-regression.sh "$OUT_DIR/tui-regression"
run_step "fallback-cli-smoke" fallback_cli_smoke
run_step "plan-mode-smoke" scripts/plan-mode-smoke.sh "$OUT_DIR/plan-mode-smoke"
daily_loop_smoke
run_step "codex-app-server-protocol-fixture" scripts/smoke-codex-app-server-protocol-fixture.sh
run_step "codex-app-server-write-guard" scripts/smoke-codex-app-server-write-guard.sh
lane_operator_smoke
run_step "rc-tui-stability-smoke" scripts/rc-tui-stability-smoke.sh "$OUT_DIR/rc-tui-stability"

if [[ "$SKIP_PACKAGE" == "0" ]]; then
  run_step "package-smoke" package_smoke
else
  log "SKIP  package-smoke"
  record "- SKIP \`package-smoke\`"
  record_step_result "package-smoke" "skip" "0" ""
fi

if [[ "$RUN_DEEPSEEK" == "1" ]]; then
  run_step "deepseek-dev-scenario-smoke" deepseek_dev_scenario_smoke
else
  log "SKIP  deepseek-dev-scenario-smoke (use --deepseek)"
  record "- SKIP \`deepseek-dev-scenario-smoke\` (use \`--deepseek\`)"
  record_step_result "deepseek-dev-scenario-smoke" "skip" "0" ""
fi

if [[ -n "$PROVIDER_SMOKE_ID" ]]; then
  run_step "provider-live-smoke-$PROVIDER_SMOKE_ID" provider_live_smoke
else
  log "SKIP  provider-live-smoke (use --provider-smoke <id>)"
  record "- SKIP \`provider-live-smoke\` (use \`--provider-smoke <id>\`)"
  record_step_result "provider-live-smoke" "skip" "0" ""
fi

if [[ "$RUN_GITHUB_ACTIONS" == "1" ]]; then
  run_step "github-actions-release-validation" github_actions_validation
else
  log "SKIP  github-actions-release-validation (use --github-actions)"
  record "- SKIP \`github-actions-release-validation\` (use \`--github-actions\`)"
  record_step_result "github-actions-release-validation" "skip" "0" ""
fi

if [[ "$RUN_GITHUB_RELEASE_ASSETS" == "1" ]]; then
  run_step "github-release-assets-validation" github_release_asset_validation
else
  log "SKIP  github-release-assets-validation (use --github-release-assets)"
  record "- SKIP \`github-release-assets-validation\` (use \`--github-release-assets\`)"
  record_step_result "github-release-assets-validation" "skip" "0" ""
fi

if [[ "$RUN_HOMEBREW" == "1" ]]; then
  run_step "homebrew-validation" homebrew_validation
else
  log "SKIP  homebrew-validation (use --homebrew)"
  record "- SKIP \`homebrew-validation\` (use \`--homebrew\`)"
  record_step_result "homebrew-validation" "skip" "0" ""
fi

record ""
record "Generated at: $(date -u '+%Y-%m-%dT%H:%M:%SZ')"
record "- Structured evidence: \`$EVIDENCE_JSON\`"
write_evidence_json
log "DONE  evidence: $OUT_DIR"
printf '%s\n' "$OUT_DIR"
