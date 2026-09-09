#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="${1:-"$ROOT/target/tui-previews/0.3.3"}"
THEME="${VIDEN_TUI_PREVIEW_THEME:-aurora}"
THEMES="${VIDEN_TUI_PREVIEW_THEMES:-aurora aurora-light ice ice-light mono mono-light amber phosphor}"
PROVIDER="${VIDEN_TUI_PREVIEW_PROVIDER:-openai}"
MODEL="${VIDEN_TUI_PREVIEW_MODEL:-gpt-4o}"

mkdir -p "$OUT_DIR"

run_preview() {
  local name="$1"
  shift
  env \
    -u DEEPSEEK_API_BASE \
    -u OPENAI_API_BASE \
    -u OPENROUTER_API_BASE \
    -u ANTHROPIC_API_BASE \
    -u DEEPSEEK_API_KEY \
    -u OPENAI_API_KEY \
    -u OPENROUTER_API_KEY \
    -u ANTHROPIC_API_KEY \
    cargo run -p viden-cli -- "$@" --provider "$PROVIDER" --model "$MODEL" >"$OUT_DIR/$name.txt"
  case "$name" in
    main-command-palette | main-setup-wizard | main-provider-selector | main-provider-detail | main-model-selector | main-lane-selector | main-lane | main-approval-hunks)
      perl -0pi -e 's/[ \t]+$//mg' "$OUT_DIR/$name.txt"
      ;;
  esac
}

run_ansi_preview() {
  local name="$1"
  shift
  env \
    -u DEEPSEEK_API_BASE \
    -u OPENAI_API_BASE \
    -u OPENROUTER_API_BASE \
    -u ANTHROPIC_API_BASE \
    -u DEEPSEEK_API_KEY \
    -u OPENAI_API_KEY \
    -u OPENROUTER_API_KEY \
    -u ANTHROPIC_API_KEY \
    cargo run -p viden-cli -- "$@" --tui-theme "$THEME" --provider "$PROVIDER" --model "$MODEL" >"$OUT_DIR/$name.ansi"
}

render_svg_preview() {
  local source="$1"
  local target="$2"
  python3 - "$source" "$target" <<'PY'
from html import escape
from pathlib import Path
import re
import sys
import unicodedata

source = Path(sys.argv[1])
target = Path(sys.argv[2])
raw_lines = source.read_text(encoding="utf-8").splitlines()
ansi_re = re.compile(r"\x1b\[([0-9;]*)m")
default_fg = "#c4e0ff"
default_bg = None

def rgb(parts, offset):
    return f"#{int(parts[offset]):02x}{int(parts[offset + 1]):02x}{int(parts[offset + 2]):02x}"

def apply_sgr(state, params):
    if params == [""] or "0" in params:
        state["fg"] = default_fg
        state["bg"] = default_bg
    index = 0
    while index < len(params):
        code = params[index]
        if code == "":
            index += 1
            continue
        value = int(code)
        if value == 39:
            state["fg"] = default_fg
        elif value == 49:
            state["bg"] = default_bg
        elif value == 38 and index + 4 < len(params) and params[index + 1] == "2":
            state["fg"] = rgb(params, index + 2)
            index += 4
        elif value == 48 and index + 4 < len(params) and params[index + 1] == "2":
            state["bg"] = rgb(params, index + 2)
            index += 4
        index += 1

def parse_ansi_line(line):
    state = {"fg": default_fg, "bg": default_bg}
    spans = []
    cursor = 0
    for match in ansi_re.finditer(line):
        if match.start() > cursor:
            spans.append((line[cursor:match.start()], state["fg"], state["bg"]))
        apply_sgr(state, match.group(1).split(";"))
        cursor = match.end()
    if cursor < len(line):
        spans.append((line[cursor:], state["fg"], state["bg"]))
    return spans

lines = [parse_ansi_line(line) for line in raw_lines]
plain_lines = ["".join(text for text, _fg, _bg in line) for line in lines]

def cell_width(char):
    if unicodedata.combining(char):
        return 0
    if char in ("\u200d", "\ufe0e", "\ufe0f"):
        return 0
    if unicodedata.east_asian_width(char) in ("F", "W"):
        return 2
    code = ord(char)
    if (
        0x1F300 <= code <= 0x1FAFF
        or 0x2600 <= code <= 0x27BF
    ):
        return 2
    return 1

def text_width(text):
    return sum(cell_width(char) for char in text)

def text_cells(text):
    pending = ""
    for char in text:
        width = cell_width(char)
        if width == 0:
            pending += char
            continue
        yield pending + char, width
        pending = ""
    if pending:
        yield pending, 0

cell_w = 10
cell_h = 19
pad_x = 24
pad_y = 30
width = pad_x * 2 + max(text_width(line) for line in plain_lines) * cell_w
height = pad_y * 2 + len(lines) * cell_h
rows = []
for index, line in enumerate(lines):
    y = pad_y + index * cell_h
    col = 0
    for text, fg, bg in line:
        if not text:
            continue
        x = pad_x + col * cell_w
        span_width = text_width(text)
        if bg:
            rows.append(
                f'<rect x="{x}" y="{y - 15}" width="{span_width * cell_w}" height="{cell_h}" fill="{bg}" opacity="0.82"/>'
            )
        for cell_text, cell_span in text_cells(text):
            if not cell_text:
                continue
            rows.append(f'<text x="{pad_x + col * cell_w}" y="{y}" class="term" fill="{fg}">{escape(cell_text)}</text>')
            col += max(cell_span, 1)
svg = f'''<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}">
  <defs>
    <filter id="glow" x="-20%" y="-20%" width="140%" height="140%">
      <feGaussianBlur stdDeviation="2.5" result="blur"/>
      <feMerge><feMergeNode in="blur"/><feMergeNode in="SourceGraphic"/></feMerge>
    </filter>
  </defs>
  <rect width="100%" height="100%" fill="#030c16"/>
  <rect x="8" y="8" width="{width - 16}" height="{height - 16}" fill="none" stroke="#1c5f84" stroke-width="2" filter="url(#glow)"/>
  <rect x="15" y="15" width="{width - 30}" height="{height - 30}" fill="none" stroke="#08233a" stroke-width="1"/>
  <style>
    .term {{ font-family: Menlo, Monaco, Consolas, monospace; font-size: 16px; white-space: pre; dominant-baseline: alphabetic; }}
  </style>
  {''.join(rows)}
</svg>
'''
target.write_text(svg, encoding="utf-8")
PY
}

cd "$ROOT"

run_preview main --tui-preview
run_preview main-idle --tui-preview-idle
run_preview main-live-turn --tui-preview-live-turn
run_preview main-resize --tui-preview-resize
run_preview main-cjk-input --tui-preview-cjk-input
run_preview main-command-palette --tui-preview-command-palette
run_preview main-setup-wizard --tui-preview-setup-wizard
run_preview main-provider-selector --tui-preview-provider-selector
run_preview main-provider-detail --tui-preview-provider-detail
run_preview main-model-selector --tui-preview-model-selector
run_preview main-lane-selector --tui-preview-lane-selector
run_preview main-approval-hunks --tui-preview-approval-hunks
run_preview main-git-picker --tui-preview-git-picker
run_preview main-git-outcome --tui-preview-git-outcome
run_preview main-lane --tui-preview-lane
run_preview side-1 --tui-preview-side
run_preview side-2 --tui-preview-side-2

run_ansi_preview main --tui-preview-ansi
run_ansi_preview main-idle --tui-preview-idle-ansi
run_ansi_preview main-live-turn --tui-preview-live-turn-ansi
run_ansi_preview main-resize --tui-preview-resize-ansi
run_ansi_preview main-cjk-input --tui-preview-cjk-input-ansi
run_ansi_preview main-command-palette --tui-preview-command-palette-ansi
run_ansi_preview main-setup-wizard --tui-preview-setup-wizard-ansi
run_ansi_preview main-provider-selector --tui-preview-provider-selector-ansi
run_ansi_preview main-provider-detail --tui-preview-provider-detail-ansi
run_ansi_preview main-model-selector --tui-preview-model-selector-ansi
run_ansi_preview main-lane-selector --tui-preview-lane-selector-ansi
run_ansi_preview main-approval-hunks --tui-preview-approval-hunks-ansi
run_ansi_preview main-git-picker --tui-preview-git-picker-ansi
run_ansi_preview main-git-outcome --tui-preview-git-outcome-ansi
run_ansi_preview main-lane --tui-preview-lane-ansi
run_ansi_preview side-1 --tui-preview-side-ansi
run_ansi_preview side-2 --tui-preview-side-2-ansi

for theme_name in $THEMES; do
  cargo run -p viden-cli -- --tui-preview-ansi --tui-theme "$theme_name" --provider "$PROVIDER" --model "$MODEL" >"$OUT_DIR/main.$theme_name.ansi"
done

render_svg_preview "$OUT_DIR/main.ansi" "$OUT_DIR/main.svg"
render_svg_preview "$OUT_DIR/main-idle.ansi" "$OUT_DIR/main-idle.svg"
render_svg_preview "$OUT_DIR/main-live-turn.ansi" "$OUT_DIR/main-live-turn.svg"
render_svg_preview "$OUT_DIR/main-resize.ansi" "$OUT_DIR/main-resize.svg"
render_svg_preview "$OUT_DIR/main-cjk-input.ansi" "$OUT_DIR/main-cjk-input.svg"
render_svg_preview "$OUT_DIR/main-command-palette.ansi" "$OUT_DIR/main-command-palette.svg"
render_svg_preview "$OUT_DIR/main-setup-wizard.ansi" "$OUT_DIR/main-setup-wizard.svg"
render_svg_preview "$OUT_DIR/main-provider-selector.ansi" "$OUT_DIR/main-provider-selector.svg"
render_svg_preview "$OUT_DIR/main-provider-detail.ansi" "$OUT_DIR/main-provider-detail.svg"
render_svg_preview "$OUT_DIR/main-model-selector.ansi" "$OUT_DIR/main-model-selector.svg"
render_svg_preview "$OUT_DIR/main-lane-selector.ansi" "$OUT_DIR/main-lane-selector.svg"
render_svg_preview "$OUT_DIR/main-approval-hunks.ansi" "$OUT_DIR/main-approval-hunks.svg"
render_svg_preview "$OUT_DIR/main-git-picker.ansi" "$OUT_DIR/main-git-picker.svg"
render_svg_preview "$OUT_DIR/main-git-outcome.ansi" "$OUT_DIR/main-git-outcome.svg"
render_svg_preview "$OUT_DIR/main-lane.ansi" "$OUT_DIR/main-lane.svg"
render_svg_preview "$OUT_DIR/side-1.ansi" "$OUT_DIR/side-1.svg"
render_svg_preview "$OUT_DIR/side-2.ansi" "$OUT_DIR/side-2.svg"

assert_contains() {
  local file="$1"
  local needle="$2"
  if ! grep -Fq "$needle" "$file"; then
    printf 'preview check failed: %s missing %s\n' "$file" "$needle" >&2
    exit 1
  fi
}

assert_not_contains() {
  local file="$1"
  local needle="$2"
  if grep -Fq "$needle" "$file"; then
    printf 'preview check failed: %s should not contain %s\n' "$file" "$needle" >&2
    exit 1
  fi
}

assert_ansi_truecolor() {
  local file="$1"
  if ! grep -q $'\033\\[38;2;' "$file"; then
    printf 'preview check failed: %s missing truecolor foreground sequences\n' "$file" >&2
    exit 1
  fi
  if ! grep -q $'\033\\[48;2;' "$file"; then
    printf 'preview check failed: %s missing truecolor background sequences\n' "$file" >&2
    exit 1
  fi
}

assert_ansi_contains() {
  local file="$1"
  local needle="$2"
  NEEDLE="$needle" perl -0ne '
    s/\e\[[0-9;]*m//g;
    exit(index($_, $ENV{"NEEDLE"}) >= 0 ? 0 : 1);
  ' "$file" || {
    printf 'preview check failed: %s missing ANSI text %s\n' "$file" "$needle" >&2
    exit 1
  }
}

assert_line_count() {
  local file="$1"
  local expected="$2"
  local actual
  actual="$(wc -l <"$file" | tr -d ' ')"
  if [[ "$actual" != "$expected" ]]; then
    printf 'preview check failed: %s has %s lines, expected %s\n' "$file" "$actual" "$expected" >&2
    exit 1
  fi
}

assert_balanced_chips() {
  local file="$1"
  awk '
    {
      left = gsub(/\[/, "[")
      right = gsub(/\]/, "]")
      if (left != right) {
        printf "preview check failed: %s:%d has unbalanced chips: %s\n", FILENAME, NR, $0 > "/dev/stderr"
        exit 1
      }
    }
  ' "$file"
}

assert_char_width() {
  local file="$1"
  local expected="$2"
  local start_line="${3:-1}"
  EXPECTED_WIDTH="$expected" START_LINE="$start_line" perl -CSDA -Mutf8 -ne '
    chomp;
    next if $. < $ENV{"START_LINE"};
    my $length = length($_);
    if ($length != $ENV{"EXPECTED_WIDTH"}) {
      print STDERR "preview check failed: $ARGV:$.: width $length, expected $ENV{EXPECTED_WIDTH}: $_\n";
      exit 1;
    }
  ' "$file"
}

assert_max_char_width() {
  local file="$1"
  local expected="$2"
  EXPECTED_WIDTH="$expected" perl -CSDA -Mutf8 -ne '
    chomp;
    my $length = length($_);
    if ($length > $ENV{"EXPECTED_WIDTH"}) {
      print STDERR "preview check failed: $ARGV:$.: width $length, expected <= $ENV{EXPECTED_WIDTH}: $_\n";
      exit 1;
    }
  ' "$file"
}

{
  printf 'MAIN 140x40'
  printf '%130s' ''
  printf '    SIDE-1 80x40'
  printf '%67s' ''
  printf '    SIDE-2 80x40\n'
  paste "$OUT_DIR/main.txt" "$OUT_DIR/side-1.txt" "$OUT_DIR/side-2.txt" | sed $'s/\t/    /g'
} >"$OUT_DIR/multiscreen.txt"

assert_line_count "$OUT_DIR/main.txt" 40
assert_line_count "$OUT_DIR/main-idle.txt" 40
assert_line_count "$OUT_DIR/main-live-turn.txt" 40
assert_line_count "$OUT_DIR/main-resize.txt" 30
assert_line_count "$OUT_DIR/main-cjk-input.txt" 30
assert_line_count "$OUT_DIR/main-command-palette.txt" 40
assert_line_count "$OUT_DIR/main-setup-wizard.txt" 40
assert_line_count "$OUT_DIR/main-provider-selector.txt" 40
assert_line_count "$OUT_DIR/main-provider-detail.txt" 40
assert_line_count "$OUT_DIR/main-model-selector.txt" 40
assert_line_count "$OUT_DIR/main-lane-selector.txt" 40
assert_line_count "$OUT_DIR/main-approval-hunks.txt" 40
assert_line_count "$OUT_DIR/main-git-picker.txt" 40
assert_line_count "$OUT_DIR/main-git-outcome.txt" 40
assert_line_count "$OUT_DIR/main-lane.txt" 40
assert_line_count "$OUT_DIR/side-1.txt" 40
assert_line_count "$OUT_DIR/side-2.txt" 40
assert_line_count "$OUT_DIR/multiscreen.txt" 41
assert_char_width "$OUT_DIR/main.txt" 140
assert_char_width "$OUT_DIR/main-idle.txt" 140
assert_char_width "$OUT_DIR/main-live-turn.txt" 140
assert_char_width "$OUT_DIR/main-resize.txt" 100
assert_max_char_width "$OUT_DIR/main-cjk-input.txt" 100
assert_max_char_width "$OUT_DIR/main-command-palette.txt" 140
assert_max_char_width "$OUT_DIR/main-setup-wizard.txt" 140
assert_max_char_width "$OUT_DIR/main-provider-selector.txt" 140
assert_max_char_width "$OUT_DIR/main-provider-detail.txt" 140
assert_max_char_width "$OUT_DIR/main-model-selector.txt" 140
assert_max_char_width "$OUT_DIR/main-lane-selector.txt" 140
assert_max_char_width "$OUT_DIR/main-approval-hunks.txt" 140
assert_max_char_width "$OUT_DIR/main-git-picker.txt" 140
assert_max_char_width "$OUT_DIR/main-git-outcome.txt" 140
assert_max_char_width "$OUT_DIR/main-lane.txt" 140
assert_char_width "$OUT_DIR/side-1.txt" 80
assert_char_width "$OUT_DIR/side-2.txt" 80
assert_char_width "$OUT_DIR/multiscreen.txt" 308 2
for preview_file in \
  "$OUT_DIR/main.txt" \
  "$OUT_DIR/main-idle.txt" \
  "$OUT_DIR/main-live-turn.txt" \
  "$OUT_DIR/main-resize.txt" \
  "$OUT_DIR/main-cjk-input.txt" \
  "$OUT_DIR/main-command-palette.txt" \
  "$OUT_DIR/main-setup-wizard.txt" \
  "$OUT_DIR/main-provider-selector.txt" \
  "$OUT_DIR/main-provider-detail.txt" \
  "$OUT_DIR/main-model-selector.txt" \
  "$OUT_DIR/main-lane-selector.txt" \
  "$OUT_DIR/main-approval-hunks.txt" \
  "$OUT_DIR/main-git-picker.txt" \
  "$OUT_DIR/main-git-outcome.txt" \
  "$OUT_DIR/main-lane.txt" \
  "$OUT_DIR/side-1.txt" \
  "$OUT_DIR/side-2.txt" \
  "$OUT_DIR/multiscreen.txt"; do
  assert_balanced_chips "$preview_file"
done
assert_contains "$OUT_DIR/multiscreen.txt" "MAIN 140x40"
assert_contains "$OUT_DIR/multiscreen.txt" "SIDE-1 80x40"
assert_contains "$OUT_DIR/multiscreen.txt" "SIDE-2 80x40"
assert_contains "$OUT_DIR/multiscreen.txt" "next wait for test result"
assert_contains "$OUT_DIR/main-idle.txt" "Ask anything"
assert_contains "$OUT_DIR/main-idle.txt" "ctrl+p commands"
assert_not_contains "$OUT_DIR/main-idle.txt" "TRANSCRIPT"
assert_contains "$OUT_DIR/main-live-turn.txt" "LIVE WORK"
assert_contains "$OUT_DIR/main-live-turn.txt" "live provider request"
assert_contains "$OUT_DIR/main-live-turn.txt" "input open"
assert_contains "$OUT_DIR/main-live-turn.txt" "[^J Queue]"
assert_contains "$OUT_DIR/main-live-turn.txt" "[^C Cancel]"
assert_contains "$OUT_DIR/main-command-palette.txt" "COMMANDS"
assert_contains "$OUT_DIR/main-command-palette.txt" "/help"
assert_contains "$OUT_DIR/main-command-palette.txt" "Show commands"
assert_contains "$OUT_DIR/main-setup-wizard.txt" "SETUP SELECTOR"
assert_contains "$OUT_DIR/main-setup-wizard.txt" "DRAFT viden.toml"
assert_contains "$OUT_DIR/main-setup-wizard.txt" "Preview exact draft through Core"
assert_contains "$OUT_DIR/main-setup-wizard.txt" 'pack = "robot-pack"'
assert_contains "$OUT_DIR/main-provider-selector.txt" "Connect a provider"
assert_contains "$OUT_DIR/main-provider-selector.txt" "DeepSeek"
assert_contains "$OUT_DIR/main-provider-selector.txt" "OpenRouter"
assert_not_contains "$OUT_DIR/main-provider-selector.txt" "PROVIDER CONFIG"
assert_contains "$OUT_DIR/main-provider-detail.txt" "PROVIDER openai healthy"
assert_contains "$OUT_DIR/main-provider-detail.txt" "TRUSTED INGRESS unavailable"
assert_not_contains "$OUT_DIR/main-provider-detail.txt" "API key"
assert_not_contains "$OUT_DIR/main-provider-detail.txt" "Enter submit"
assert_not_contains "$OUT_DIR/main-provider-detail.txt" "PROVIDER CONFIG"
assert_not_contains "$OUT_DIR/main-provider-detail.txt" "set default provider"
assert_contains "$OUT_DIR/main-model-selector.txt" "Select model"
assert_contains "$OUT_DIR/main-model-selector.txt" "deepseek-v4-flash"
assert_contains "$OUT_DIR/main-model-selector.txt" "DeepSeek"
assert_not_contains "$OUT_DIR/main-model-selector.txt" "OpenAI"
assert_not_contains "$OUT_DIR/main-model-selector.txt" "Fallback"
assert_not_contains "$OUT_DIR/main-model-selector.txt" "fallback-local"
assert_contains "$OUT_DIR/main-lane-selector.txt" "LANE ACTIONS"
assert_contains "$OUT_DIR/main-lane-selector.txt" "codex-review"
assert_contains "$OUT_DIR/main-lane-selector.txt" "inspect L1"
assert_contains "$OUT_DIR/main.txt" "LIVE WORK"
assert_contains "$OUT_DIR/main.txt" "next wait for test result"
assert_contains "$OUT_DIR/main-resize.txt" "LIVE WORK"
assert_contains "$OUT_DIR/main-resize.txt" "Resize-safe redraw check"
assert_contains "$OUT_DIR/main-cjk-input.txt" "你好，帮我检查当前变更"
assert_not_contains "$OUT_DIR/main.txt" "TELEMETRY"
assert_contains "$OUT_DIR/main.txt" "P:viden L:"
assert_contains "$OUT_DIR/main.txt" "PERM:Ask"
assert_contains "$OUT_DIR/main.txt" "⏸ 0 E:0"
assert_contains "$OUT_DIR/main.txt" "EVENTS"
if grep -Fq "APPROVAL REQUIRED" "$OUT_DIR/main.txt" "$OUT_DIR/main-idle.txt"; then
  printf 'preview check failed: %s should not contain approval modal\n' "$OUT_DIR/main-idle.txt" >&2
  exit 1
fi
assert_contains "$OUT_DIR/multiscreen.txt" "AGENT LANES"
assert_contains "$OUT_DIR/multiscreen.txt" "TESTS / LSP"
assert_contains "$OUT_DIR/multiscreen.txt" "MCP / CONTEXT"
assert_contains "$OUT_DIR/multiscreen.txt" "RECENT EVIDENCE"
assert_contains "$OUT_DIR/multiscreen.txt" "pty/01"
assert_contains "$OUT_DIR/main-approval-hunks.txt" "@@ -18,4 +18,5 @@"
assert_contains "$OUT_DIR/main-approval-hunks.txt" "computed against 9c1185a5"
assert_contains "$OUT_DIR/main-approval-hunks.txt" "rows omitted by the byte bound"
assert_ansi_contains "$OUT_DIR/main-approval-hunks.ansi" "@@ -18,4 +18,5 @@"
assert_contains "$OUT_DIR/main-git-picker.txt" "Stage all changes"
assert_contains "$OUT_DIR/main-git-picker.txt" "Commit…"
assert_contains "$OUT_DIR/main-git-picker.txt" "TARGET  workspace"
assert_ansi_contains "$OUT_DIR/main-git-picker.ansi" "Stage all changes"
assert_contains "$OUT_DIR/main-git-outcome.txt" "Commit completed"
assert_contains "$OUT_DIR/main-git-outcome.txt" "Push failed"
assert_contains "$OUT_DIR/main-git-outcome.txt" "push again with set upstream"
assert_ansi_contains "$OUT_DIR/main-git-outcome.ansi" "Commit completed"
assert_contains "$OUT_DIR/main-lane.txt" "LANE DETAIL"
assert_contains "$OUT_DIR/main-lane.txt" "ROUTE main→side-1"
assert_contains "$OUT_DIR/main-lane.txt" "CMD    codex exec test fixes"
assert_contains "$OUT_DIR/main-lane.txt" "ATTACH /lane tmux L1"
assert_contains "$OUT_DIR/main-lane.txt" "CONTROL [stop] [tmux] [pty] [send] [inspect]"
for ansi_file in \
  "$OUT_DIR/main.ansi" \
  "$OUT_DIR/main-idle.ansi" \
  "$OUT_DIR/main-live-turn.ansi" \
  "$OUT_DIR/main-resize.ansi" \
  "$OUT_DIR/main-cjk-input.ansi" \
  "$OUT_DIR/main-command-palette.ansi" \
  "$OUT_DIR/main-setup-wizard.ansi" \
  "$OUT_DIR/main-provider-selector.ansi" \
  "$OUT_DIR/main-provider-detail.ansi" \
  "$OUT_DIR/main-model-selector.ansi" \
  "$OUT_DIR/main-lane-selector.ansi" \
  "$OUT_DIR/main-approval-hunks.ansi" \
  "$OUT_DIR/main-git-picker.ansi" \
  "$OUT_DIR/main-git-outcome.ansi" \
  "$OUT_DIR/main-lane.ansi" \
  "$OUT_DIR/side-1.ansi" \
  "$OUT_DIR/side-2.ansi"; do
  assert_ansi_truecolor "$ansi_file"
done
for theme_name in $THEMES; do
  assert_ansi_truecolor "$OUT_DIR/main.$theme_name.ansi"
done
assert_ansi_contains "$OUT_DIR/main.ansi" "next wait for test result"
assert_ansi_contains "$OUT_DIR/main-idle.ansi" "Ask anything"
assert_ansi_contains "$OUT_DIR/main-idle.ansi" "ctrl+p commands"
assert_ansi_contains "$OUT_DIR/main-live-turn.ansi" "LIVE WORK"
assert_ansi_contains "$OUT_DIR/main-live-turn.ansi" "[^J Queue]"
assert_ansi_contains "$OUT_DIR/main-resize.ansi" "Resize-safe redraw check"
assert_ansi_contains "$OUT_DIR/main-cjk-input.ansi" "你好，帮我检查当前变更"
assert_ansi_contains "$OUT_DIR/main-command-palette.ansi" "COMMANDS"
assert_ansi_contains "$OUT_DIR/main-setup-wizard.ansi" "SETUP SELECTOR"
assert_ansi_contains "$OUT_DIR/main-provider-selector.ansi" "Connect a provider"
assert_ansi_contains "$OUT_DIR/main-provider-detail.ansi" "TRUSTED INGRESS unavailable"
assert_ansi_contains "$OUT_DIR/main-model-selector.ansi" "Select model"
assert_ansi_contains "$OUT_DIR/main-lane-selector.ansi" "LANE ACTIONS"
assert_ansi_contains "$OUT_DIR/main-lane.ansi" "LANE DETAIL"
assert_ansi_contains "$OUT_DIR/side-1.ansi" "AGENT LANES"
assert_ansi_contains "$OUT_DIR/side-2.ansi" "TESTS / LSP"

cat >"$OUT_DIR/README.md" <<EOF
# Generated TUI Previews

Generated by \`scripts/tui-previews.sh\`.

- Theme: \`$THEME\`
- Registered palette profiles: \`$THEMES\`
- Provider: \`$PROVIDER\`
- Model: \`$MODEL\`

Files:

- \`main.txt\` / \`main.ansi\`
- \`main-idle.txt\` / \`main-idle.ansi\`
- \`main-live-turn.txt\` / \`main-live-turn.ansi\`
- \`main-resize.txt\` / \`main-resize.ansi\`
- \`main-cjk-input.txt\` / \`main-cjk-input.ansi\`
- \`main-command-palette.txt\` / \`main-command-palette.ansi\`
- \`main-setup-wizard.txt\` / \`main-setup-wizard.ansi\`
- \`main-provider-selector.txt\` / \`main-provider-selector.ansi\`
- \`main-provider-detail.txt\` / \`main-provider-detail.ansi\`
- \`main-model-selector.txt\` / \`main-model-selector.ansi\`
- \`main-lane-selector.txt\` / \`main-lane-selector.ansi\`
- \`main-approval-hunks.txt\` / \`main-approval-hunks.ansi\`
- \`main-git-picker.txt\` / \`main-git-picker.ansi\`
- \`main-git-outcome.txt\` / \`main-git-outcome.ansi\`
- \`main-lane.txt\` / \`main-lane.ansi\`
- \`main.svg\` / \`main-idle.svg\` / \`main-live-turn.svg\` / \`main-resize.svg\` / \`main-cjk-input.svg\` / \`main-command-palette.svg\` / \`main-setup-wizard.svg\` / \`main-provider-selector.svg\` / \`main-provider-detail.svg\` / \`main-model-selector.svg\` / \`main-lane-selector.svg\` / \`main-lane.svg\` quick visual screenshots
- \`side-1.txt\` / \`side-1.ansi\` / \`side-1.svg\`
- \`side-2.txt\` / \`side-2.ansi\` / \`side-2.svg\`
- \`multiscreen.txt\` combined plain-text workstation preview
- \`main.<theme>.ansi\` for each generated theme variant
EOF

wc -l "$OUT_DIR"/main-approval-hunks.txt "$OUT_DIR"/main.txt "$OUT_DIR"/main-idle.txt "$OUT_DIR"/main-live-turn.txt "$OUT_DIR"/main-resize.txt "$OUT_DIR"/main-cjk-input.txt "$OUT_DIR"/main-command-palette.txt "$OUT_DIR"/main-setup-wizard.txt "$OUT_DIR"/main-provider-selector.txt "$OUT_DIR"/main-provider-detail.txt "$OUT_DIR"/main-model-selector.txt "$OUT_DIR"/main-lane-selector.txt "$OUT_DIR"/main-lane.txt "$OUT_DIR"/side-1.txt "$OUT_DIR"/side-2.txt
