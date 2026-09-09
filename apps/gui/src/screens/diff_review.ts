import { renderDiffBody } from "../components/diff_rows";
import { translate, type Locale, type MessageKey } from "../i18n/catalog";
import {
  STRUCTURED_DIFF_CAPABILITY,
  type WorkspaceDiffEntryProjection,
  type WorkspaceDiffProjection,
} from "../models/diff_review";
import {
  IDLE_OPERATOR_GIT,
  MAX_COMMIT_MESSAGE_BYTES,
  OPERATOR_GIT_CAPABILITY,
  commitMessageBytes,
  type OperatorGitProjection,
} from "../models/operator_git";
import "./diff_review.css";

/**
 * DiffReview — the in-cockpit per-file review of the working tree.
 *
 * The registered `D-RAILNAV ①` family: `.review > .filetree + .diffpane`. It
 * is a *view* inside D1's centre pane rather than a standalone D-screen,
 * because that is what the design registers and what the operator needs: the
 * transcript is one Escape away, not one navigation away.
 *
 * Read side only. Everything the pane draws comes from one
 * `WorkspaceDiffLoaded` page Core answered: the client never runs git, never
 * parses diff text, never re-sorts Core's order, and never derives `staged`
 * from the index classification Core already derived it from.
 *
 * The commit bar is the action half (`runtime.operator_git`, GUI-CORE-020) and
 * follows the same rule: every button sends one `RunOperatorGitAction` through
 * the CoreClient seam and renders the ordered answer. The client stages
 * nothing itself, commits nothing itself, and reads no fact out of git's
 * output. A bar that cannot act — no capability, no Core-published owner, an
 * action already in flight — stays visible, disabled, and labelled with the
 * reason, because an operator who can see *why* can fix it.
 */

export type { WorkspaceDiffProjection } from "../models/diff_review";

/**
 * The action half of the view (`runtime.operator_git`, GUI-CORE-020).
 *
 * Absent while no host is bound: the bar then keeps the design's shape and is
 * wholly inert, which is what a shell with nothing behind it owes the reader.
 */
export interface DiffReviewActionHandlers {
  /** Core's action projection: capability, owner, in flight, last answer. */
  state: OperatorGitProjection;
  /**
   * The operator's draft commit message.
   *
   * Owned by the caller, because an ordered Core refresh rebuilds this DOM and
   * a draft rebuilt from an empty string would silently discard what the
   * operator typed.
   */
  message: string;
  onMessageChange: (next: string) => void;
  /**
   * The commit half of "commit and push" did not complete, so the push was
   * never sent. Said out loud: a silently dropped second half would leave the
   * operator believing the branch was published.
   */
  pairStopped?: boolean;
  /** `Stage { paths: [] }` — Core's own "every changed path". */
  onStageAll: () => void;
  onCommit: () => void;
  /** Two sequential commands; the caller sends the push only on `Completed`. */
  onCommitPush: () => void;
  /** `staged` is Core's flag for the row, so the caller picks the inverse. */
  onToggleStaged: (path: string, staged: boolean) => void;
  /** The `NoUpstream` recovery: the same push with `set_upstream: true`. */
  onPushSetUpstream: () => void;
  /** The `NonFastForward` recovery. `pull` is excluded by contract in 0.3.3. */
  onFetch: () => void;
}

export interface DiffReviewHandlers {
  /** Re-reads the page from Core. Absent while no host is bound. */
  onRefresh?: () => void;
  /** Returns the centre pane to the transcript. */
  onClose?: () => void;
  /** Remembers the operator's file selection across an ordered Core refresh. */
  onSelect?: (path: string | null) => void;
  /** Path to open with, when the view is being rebuilt after a re-read. */
  selectedPath?: string | null;
  /** Operator source-control actions. Absent leaves the bar inert. */
  actions?: DiffReviewActionHandlers;
}

/// Core's audit verbs, each with its own localized noun for the bar's copy.
const ACTION_LABEL: Record<string, MessageKey> = {
  stage: "d1.review.verb.stage",
  unstage: "d1.review.verb.unstage",
  commit: "d1.review.verb.commit",
  push: "d1.review.verb.push",
  fetch: "d1.review.verb.fetch",
};

/**
 * Core's failure classes, each with the sentence and the recovery the contract
 * names for it.
 *
 * The class is the key, never git's English text: Core classifies stderr once,
 * in one place, so a localized client renders a message per class instead of
 * matching substrings that change with git's version and the operator's
 * locale. `authentication_required` deliberately offers no recovery — a retry
 * button there is a retry loop against a credential the client cannot supply.
 */
const FAILURE_COPY: Record<string, { key: MessageKey; recovery?: "fetch" | "set_upstream" }> = {
  nothing_to_commit: { key: "d1.review.failed.nothingToCommit" },
  non_fast_forward: { key: "d1.review.failed.nonFastForward", recovery: "fetch" },
  authentication_required: { key: "d1.review.failed.authenticationRequired" },
  remote_unreachable: { key: "d1.review.failed.remoteUnreachable" },
  no_upstream: { key: "d1.review.failed.noUpstream", recovery: "set_upstream" },
  path_outside_repository: { key: "d1.review.failed.pathOutsideRepository" },
  other: { key: "d1.review.failed.other" },
};

/// The design's `.ftrow .stat` glyphs, one per `WorkspaceChangeKind`.
const KIND_GLYPH: Record<string, { glyph: string; variant: string }> = {
  modified: { glyph: "M", variant: "m" },
  added: { glyph: "A", variant: "a" },
  deleted: { glyph: "D", variant: "d" },
  renamed: { glyph: "R", variant: "m" },
  untracked: { glyph: "?", variant: "a" },
};

/**
 * The kind glyph for one row.
 *
 * With scope `Both` a path changed on both sides carries its *worktree* rows,
 * so the worktree classification is the one that describes what the operator's
 * file holds right now; the index classification stands in when the change is
 * staged only. `diff.kind` is the last resort, and a row Core classified in no
 * way at all gets a neutral marker rather than a borrowed letter.
 */
function entryKind(entry: WorkspaceDiffEntryProjection): string | null {
  return entry.worktree ?? entry.index ?? entry.diff?.kind ?? null;
}

/// Splits a path so the distinctive tail can be kept while the leading
/// directories ellipsize. `README.md` has no directory half, which is fine.
function splitPath(path: string): { directories: string; base: string } {
  const cut = path.lastIndexOf("/");
  return cut < 0
    ? { directories: "", base: path }
    : { directories: path.slice(0, cut + 1), base: path.slice(cut + 1) };
}

function stateNote(
  host: HTMLElement,
  state: string,
  text: string,
  alert = false,
): HTMLElement {
  const note = document.createElement("p");
  note.className = "review-state";
  note.dataset.reviewState = state;
  if (alert) note.setAttribute("role", "alert");
  note.textContent = text;
  host.append(note);
  return note;
}

/**
 * Renders the view into `host`, replacing whatever was there.
 *
 * Selection is presentation state and lives with the caller: an ordered Core
 * refresh rebuilds this DOM, so the selected path is passed in and reported
 * back rather than being read out of the tree afterwards.
 */
export function renderDiffReview(
  host: HTMLElement,
  projection: WorkspaceDiffProjection,
  locale: Locale,
  handlers: DiffReviewHandlers = {},
): void {
  const entries = projection.entries;
  const requested = handlers.selectedPath ?? null;
  // A selection Core no longer publishes falls back to the head of the list
  // rather than leaving the pane pointed at a file that is gone.
  const selected =
    entries.find((entry) => entry.path === requested) ?? entries[0] ?? null;

  const review = document.createElement("section");
  review.className = "review";
  review.dataset.diffReview = "true";
  review.setAttribute("role", "region");
  review.setAttribute("aria-label", translate(locale, "d1.review.region", {}));

  /* ---- file tree ---- */

  const tree = document.createElement("div");
  tree.className = "filetree";

  const head = document.createElement("div");
  head.className = "fthead";
  const title = document.createElement("h3");
  title.textContent = translate(locale, "d1.review.title", {});
  head.append(title);

  // Totals are summed over the entries Core published rows for. An entry with
  // no diff contributes no counts, so the summary says so instead of quietly
  // under-reporting.
  const counted = entries.filter((entry) => entry.diff !== null);
  const additions = counted.reduce((total, entry) => total + (entry.diff?.additions ?? 0), 0);
  const deletions = counted.reduce((total, entry) => total + (entry.diff?.deletions ?? 0), 0);
  // Only a page that arrived has counts. Before one does — and after a
  // refusal — "0 files · +0 −0" would be a number Core never gave, and would
  // read as a clean tree.
  if (projection.loaded) {
    const count = document.createElement("span");
    count.className = "ct";
    count.textContent = translate(locale, "d1.review.count", {
      files: String(entries.length),
      additions: String(additions),
      deletions: String(deletions),
    });
    head.append(count);
  }
  if (projection.loaded && counted.length !== entries.length) {
    const partial = document.createElement("span");
    partial.className = "review-partial";
    partial.dataset.reviewCountPartial = "true";
    partial.textContent = "…";
    partial.title = translate(locale, "d1.review.countPartial", {});
    head.append(partial);
  }

  const refresh = document.createElement("button");
  refresh.type = "button";
  refresh.className = "review-icon";
  refresh.dataset.reviewRefresh = "true";
  refresh.textContent = "⟳";
  const refreshLabel = translate(locale, "d1.review.refresh", {});
  refresh.title = refreshLabel;
  refresh.setAttribute("aria-label", refreshLabel);
  // Visible and disabled without a host: the view is a read, and a read the
  // shell cannot issue must say so rather than look idle.
  refresh.disabled = !handlers.onRefresh;
  refresh.addEventListener("click", () => handlers.onRefresh?.());
  head.append(refresh);
  tree.append(head);

  const list = document.createElement("div");
  list.className = "ftlist";
  list.setAttribute("role", "list");
  // The registered row is one line with two independent affordances: open the
  // file, and stage or unstage it. A button cannot be nested inside a button
  // and stay focusable, so the line is the container and `.ftrow` keeps the
  // family's own styling as the opening half.
  const actions = handlers.actions;
  for (const entry of entries) {
    const line = document.createElement("div");
    line.className = "ftline";
    line.setAttribute("role", "listitem");

    const row = document.createElement("button");
    row.type = "button";
    row.className = entry === selected ? "ftrow on" : "ftrow";
    row.dataset.path = entry.path;
    row.setAttribute("aria-current", String(entry === selected));

    const kind = entryKind(entry);
    const marker = kind ? KIND_GLYPH[kind] : undefined;
    const stat = document.createElement("span");
    stat.className = marker ? `stat ${marker.variant}` : "stat";
    stat.textContent = marker?.glyph ?? "·";
    stat.title = kind ?? translate(locale, "d1.review.kindUnknown", {});
    row.append(stat);

    const name = document.createElement("span");
    name.className = "fn";
    name.title = entry.path;
    const { directories, base } = splitPath(entry.path);
    if (directories) {
      const head = document.createElement("span");
      head.className = "dir";
      head.textContent = directories;
      name.append(head);
    }
    const tail = document.createElement("span");
    tail.className = "base";
    tail.textContent = base;
    name.append(tail);
    row.append(name);

    if (entry.diff) {
      const counts = document.createElement("span");
      counts.className = "pm";
      // Real even for an omitted file: the counts are what make "not shown"
      // readable as something other than "unchanged".
      counts.textContent = `+${entry.diff.additions} −${entry.diff.deletions}`;
      row.append(counts);
    }
    if (entry.staged) {
      const staged = document.createElement("span");
      staged.className = "ck";
      staged.textContent = "✓";
      staged.title = translate(locale, "d1.review.staged", {});
      row.append(staged);
    }
    row.addEventListener("click", () => {
      handlers.onSelect?.(entry.path);
      renderDiffReview(host, projection, locale, {
        ...handlers,
        selectedPath: entry.path,
      });
    });
    line.append(row);

    if (actions) {
      // Which command the toggle sends follows Core's own `staged` flag; the
      // client never re-derives it from the index classification Core already
      // derived it from.
      const toggle = document.createElement("button");
      toggle.type = "button";
      toggle.className = "review-stage";
      toggle.dataset.reviewStageToggle = entry.path;
      toggle.dataset.reviewStaged = String(entry.staged);
      const toggleLabel = translate(
        locale,
        entry.staged ? "d1.review.unstage" : "d1.review.stage",
        {},
      );
      toggle.textContent = toggleLabel;
      toggle.setAttribute("aria-label", `${toggleLabel} ${entry.path}`);
      const blocked = actionBlockedReason(actions.state, locale);
      toggle.disabled = blocked !== null;
      toggle.title = blocked ?? toggleLabel;
      toggle.addEventListener("click", (event) => {
        // The toggle is an action on the row, not a way into it: opening a
        // file the operator only meant to stage would move the diff pane
        // under them.
        event.stopPropagation();
        actions.onToggleStaged(entry.path, entry.staged);
      });
      line.append(toggle);
    }
    list.append(line);
  }

  // The four non-row states, each with its own sentence. Only the last one is
  // ever drawn as "no changes".
  if (!projection.capabilityAvailable) {
    stateNote(
      list,
      "unavailable",
      translate(locale, "d1.review.unavailable", {
        capability: STRUCTURED_DIFF_CAPABILITY,
      }),
    );
  } else if (projection.outcome.state === "rejected") {
    // Core's own words, unedited: the reason carries the actionable hint.
    stateNote(
      list,
      "rejected",
      projection.outcome.reason ?? translate(locale, "d1.review.rejected", {}),
      true,
    );
  } else if (!projection.loaded) {
    stateNote(list, "pending", translate(locale, "d1.review.pending", {}));
  } else if (entries.length === 0) {
    stateNote(list, "empty", translate(locale, "d1.review.empty", {}));
  }
  tree.append(list);

  /* ---- diff pane ---- */

  const pane = document.createElement("div");
  pane.className = "diffpane";

  const paneHead = document.createElement("div");
  paneHead.className = "dphead";
  const paneName = document.createElement("span");
  paneName.className = "fn";
  if (selected) {
    paneName.title = selected.path;
    const { directories, base } = splitPath(selected.path);
    paneName.textContent = `${directories}${base}`;
  } else {
    paneName.textContent = translate(locale, "d1.review.selectFile", {});
  }
  paneHead.append(paneName);

  if (selected?.diff?.oldPath) {
    const renamed = document.createElement("span");
    renamed.className = "review-renamed";
    renamed.dataset.reviewRenamed = "true";
    renamed.textContent = translate(locale, "d1.review.renamed", {
      from: selected.diff.oldPath,
      to: selected.path,
    });
    paneHead.append(renamed);
  }

  const segmented = document.createElement("div");
  segmented.className = "seg";
  const unified = document.createElement("button");
  unified.type = "button";
  unified.className = "on";
  unified.dataset.diffView = "unified";
  unified.textContent = translate(locale, "d1.review.unified", {});
  unified.setAttribute("aria-pressed", "true");
  const split = document.createElement("button");
  split.type = "button";
  split.dataset.diffView = "split";
  split.textContent = translate(locale, "d1.review.split", {});
  // Visible and disabled rather than hidden: the design registers both halves
  // of this control, and only the unified body is built.
  split.disabled = true;
  split.title = translate(locale, "d1.review.splitUnavailable", {});
  split.setAttribute("aria-disabled", "true");
  segmented.append(unified, split);
  paneHead.append(segmented);

  const close = document.createElement("button");
  close.type = "button";
  close.className = "review-icon";
  close.dataset.reviewClose = "true";
  close.textContent = "✕";
  const closeLabel = translate(locale, "d1.review.close", {});
  close.title = closeLabel;
  close.setAttribute("aria-label", closeLabel);
  close.disabled = !handlers.onClose;
  close.addEventListener("click", () => handlers.onClose?.());
  paneHead.append(close);
  pane.append(paneHead);

  if (projection.truncated) {
    // A page bound is a fact about the page, not about the file on screen, so
    // it belongs above the body rather than inside one file's rows.
    const banner = document.createElement("p");
    banner.className = "review-banner";
    banner.dataset.reviewTruncated = "true";
    banner.textContent = translate(locale, "d1.review.truncated", {});
    pane.append(banner);
  }
  if (projection.stale) {
    // The rows stay: "re-read due" must never blank the only facts the
    // operator has in front of them.
    const banner = document.createElement("p");
    banner.className = "review-banner";
    banner.dataset.reviewStale = "true";
    banner.setAttribute("role", "status");
    banner.textContent = translate(locale, "d1.review.stale", {});
    pane.append(banner);
  }

  if (selected) {
    renderDiffBody(pane, selected.diff, locale);
  } else {
    const body = document.createElement("div");
    body.className = "diffbody";
    pane.append(body);
  }

  /* ---- operator actions (runtime.operator_git, GUI-CORE-020) ---- */

  renderActionState(pane, actions?.state ?? IDLE_OPERATOR_GIT, locale, actions);
  renderCommitBar(pane, locale, actions);

  review.append(tree, pane);
  host.replaceChildren(review);
}

/**
 * Why the bar cannot act right now, in the operator's language, or `null` when
 * it can.
 *
 * The order is the order the operator can do something about: an absent
 * capability is a fact about this Core build, an absent owner is a fact about
 * this client's Lane selection, and an action in flight is temporary. Each
 * leaves the control visible and disabled — hiding it would make the operator
 * hunt for a button that is right there.
 */
function actionBlockedReason(
  state: OperatorGitProjection,
  locale: Locale,
): string | null {
  if (!state.capabilityAvailable) {
    return translate(locale, "d1.review.actionUnavailable", {
      capability: OPERATOR_GIT_CAPABILITY,
    });
  }
  if (!state.ownerAvailable) {
    // This client's own words, not Core's: nothing was refused by Core here.
    return (
      state.ownerUnavailableReason ?? translate(locale, "d1.review.actionNoOwner", {})
    );
  }
  if (state.outcome.state === "pending") {
    return state.awaitingApproval
      ? translate(locale, "d1.review.actionAwaitingApproval", {})
      : translate(locale, "d1.review.actionBusy", {});
  }
  return null;
}

/// One localized noun for a Core audit verb, or the raw verb when this build
/// has no name for it — never a blank, which would read as "something".
function actionNoun(locale: Locale, verb: string | null): string {
  if (!verb) return translate(locale, "d1.review.verb.unknown", {});
  const key = ACTION_LABEL[verb];
  return key ? translate(locale, key, {}) : verb;
}

/**
 * The line above the bar: what is happening, or what happened.
 *
 * Four states with four sentences. The one thing they never do is share: a
 * `CommandRejected` means Core refused before anything ran, and a `Failed`
 * outcome means the effect was attempted and audited. Rendering them alike
 * would tell an operator "denied" about a problem in their own index.
 */
function renderActionState(
  pane: HTMLElement,
  state: OperatorGitProjection,
  locale: Locale,
  actions: DiffReviewActionHandlers | undefined,
): void {
  const block = document.createElement("div");
  block.className = "review-action";
  block.dataset.reviewAction = "true";

  if (state.outcome.state === "pending") {
    const line = document.createElement("p");
    line.className = "review-action-line";
    line.dataset.reviewActionState = state.awaitingApproval ? "awaiting_approval" : "pending";
    line.setAttribute("role", "status");
    const action = actionNoun(locale, state.pendingAction);
    line.textContent = state.awaitingApproval
      ? // The dock owns the decision from here; the bar only says so.
        translate(locale, "d1.review.actionAwaitingApprovalDetail", { action })
      : translate(locale, "d1.review.actionPending", { action });
    block.append(line);
    pane.append(block);
    return;
  }

  if (state.outcome.state === "rejected") {
    const line = document.createElement("p");
    line.className = "review-action-line";
    line.dataset.reviewActionState = "rejected";
    line.setAttribute("role", "alert");
    // Core's own words, unedited: the reason carries the actionable hint.
    line.textContent =
      state.outcome.reason ?? translate(locale, "d1.review.actionRejected", {});
    block.append(line);
    if (actions?.pairStopped) {
      const note = document.createElement("p");
      note.className = "review-action-note";
      note.dataset.reviewPairStopped = "true";
      note.setAttribute("role", "status");
      note.textContent = translate(locale, "d1.review.actionPairStopped", {});
      block.append(note);
    }
    pane.append(block);
    return;
  }

  const result = state.result;
  if (!result) {
    // Nothing has been attempted. An empty block is the honest render: there
    // is no outcome to describe and no placeholder that could be mistaken for
    // one.
    return;
  }

  const action = actionNoun(locale, result.action);
  if (result.kind === "completed") {
    const line = document.createElement("p");
    line.className = "review-action-line";
    line.dataset.reviewActionState = "completed";
    line.setAttribute("role", "status");
    // Every number here is Core's resampled `source`. Nothing is read out of
    // the transcript below, which is display text.
    const source = result.source;
    line.textContent = source
      ? translate(locale, "d1.review.actionCompletedResampled", {
          action,
          branch: source.branch ?? translate(locale, "d1.review.actionNoBranch", {}),
          ahead: String(source.ahead),
          behind: String(source.behind),
          tree: translate(
            locale,
            source.dirty ? "d1.review.actionDirty" : "d1.review.actionClean",
            {},
          ),
        })
      : translate(locale, "d1.review.actionCompleted", { action });
    block.append(line);

    if (result.output && result.output.trim().length > 0) {
      // Collapsed: a commit transcript is evidence, not the answer, and the
      // answer is the line above it.
      const details = document.createElement("details");
      details.className = "review-action-output";
      details.dataset.reviewActionOutput = "true";
      const summary = document.createElement("summary");
      summary.textContent = translate(locale, "d1.review.actionOutput", {});
      const body = document.createElement("pre");
      body.textContent = result.output;
      details.append(summary, body);
      block.append(details);
    }
    if (result.truncated) {
      const note = document.createElement("p");
      note.className = "review-action-note";
      note.dataset.reviewActionTruncated = "true";
      note.textContent = translate(locale, "d1.review.actionOutputTruncated", {});
      block.append(note);
    }
    pane.append(block);
    return;
  }

  if (actions?.pairStopped) {
    const note = document.createElement("p");
    note.className = "review-action-note";
    note.dataset.reviewPairStopped = "true";
    note.setAttribute("role", "status");
    note.textContent = translate(locale, "d1.review.actionPairStopped", {});
    block.append(note);
  }

  const failureClass = result.failureClass ?? "unknown";
  const copy = FAILURE_COPY[failureClass];
  const line = document.createElement("p");
  line.className = "review-action-line";
  line.dataset.reviewActionState = "failed";
  line.dataset.reviewFailureClass = failureClass;
  line.setAttribute("role", "alert");
  line.textContent = `${translate(locale, "d1.review.actionFailed", { action })} ${translate(
    locale,
    // An unrecognized class keeps its own sentence rather than borrowing the
    // nearest-looking one, which would offer the wrong recovery.
    copy?.key ?? "d1.review.failed.unknown",
    {},
  )}`;
  block.append(line);

  if (result.detail) {
    // Git's own message, verbatim. The class above is the machine-readable
    // half; this is the half a human reads.
    const detail = document.createElement("pre");
    detail.className = "review-action-detail";
    detail.dataset.reviewActionDetail = "true";
    detail.textContent = result.detail;
    block.append(detail);
  }

  if (copy?.recovery && actions) {
    const recovery = document.createElement("button");
    recovery.type = "button";
    recovery.className = "cbtn review-recovery";
    recovery.dataset.reviewRecovery = copy.recovery;
    recovery.textContent = translate(
      locale,
      copy.recovery === "fetch" ? "d1.review.recovery.fetch" : "d1.review.recovery.setUpstream",
      {},
    );
    const blocked = actionBlockedReason(actions.state, locale);
    recovery.disabled = blocked !== null;
    if (blocked) recovery.title = blocked;
    recovery.addEventListener("click", () => {
      if (copy.recovery === "fetch") actions.onFetch();
      else actions.onPushSetUpstream();
    });
    block.append(recovery);
  }
  pane.append(block);
}

/**
 * The registered `.commitbar`: a message box and three actions.
 *
 * `Commit & Push` is two sequential commands, not one: the caller sends the
 * push only after the commit reports `Completed`. That belongs to the caller
 * because only it can watch the ordered stream; the bar's job is to say which
 * button was pressed.
 */
function renderCommitBar(
  pane: HTMLElement,
  locale: Locale,
  actions: DiffReviewActionHandlers | undefined,
): void {
  const state = actions?.state ?? IDLE_OPERATOR_GIT;
  const commitBar = document.createElement("div");
  commitBar.className = "commitbar";
  commitBar.dataset.reviewCommit = "true";

  const input = document.createElement("input");
  input.type = "text";
  input.className = "msg-in";
  input.dataset.reviewCommitMessage = "true";
  input.value = actions?.message ?? "";
  const placeholder = translate(locale, "d1.review.commitPlaceholder", {});
  input.placeholder = placeholder;
  input.setAttribute("aria-label", placeholder);
  const blocked = actionBlockedReason(state, locale);
  input.disabled = !actions;
  if (!actions && blocked) input.title = blocked;
  commitBar.append(input);

  // The over-bound note lives beside the bar so the count sits next to the box
  // it describes. Hidden rather than absent, because it appears and disappears
  // as the operator types and a node that comes and goes moves the layout.
  const overflow = document.createElement("p");
  overflow.className = "review-action-note";
  overflow.dataset.reviewCommitOverflow = "true";
  overflow.setAttribute("role", "alert");

  const controls: Array<{ button: HTMLButtonElement; needsMessage: boolean }> = [];
  for (const [key, variant, action] of [
    ["d1.review.stageAll", "", "stage_all"],
    ["d1.review.commit", "commit", "commit"],
    ["d1.review.commitPush", "push", "commit_push"],
  ] as const) {
    const button = document.createElement("button");
    button.type = "button";
    button.className = variant ? `cbtn ${variant}` : "cbtn";
    button.dataset.reviewAction = action;
    button.textContent = translate(locale, key, {});
    // Staging needs no message; committing does. A disabled button fires no
    // click, so the handler is attached once and `disabled` is the only gate —
    // there is no second, drifting copy of the same rule.
    if (actions) {
      button.addEventListener("click", () => {
        if (action === "stage_all") actions.onStageAll();
        else if (action === "commit") actions.onCommit();
        else actions.onCommitPush();
      });
    }
    commitBar.append(button);
    controls.push({ button, needsMessage: action !== "stage_all" });
  }

  /**
   * Applies the message-dependent half of the bar's state.
   *
   * Called on every keystroke rather than re-rendering the view, because
   * rebuilding the DOM under a caret would move it. Core counts UTF-8 bytes,
   * so the client does too: a character-based check would let a Chinese
   * message past a bound Core then refuses.
   */
  const syncMessageState = (): void => {
    const message = input.value;
    const bytes = commitMessageBytes(message);
    const overLimit = bytes > MAX_COMMIT_MESSAGE_BYTES;
    const tooLong = translate(locale, "d1.review.commitTooLong", {
      bytes: String(bytes),
      limit: String(MAX_COMMIT_MESSAGE_BYTES),
    });
    overflow.textContent = tooLong;
    overflow.hidden = !overLimit;
    for (const { button, needsMessage } of controls) {
      const messageProblem = !needsMessage
        ? null
        : overLimit
          ? tooLong
          : message.trim().length > 0
            ? null
            : translate(locale, "d1.review.commitEmpty", {});
      const reason = blocked ?? messageProblem;
      button.disabled = !actions || reason !== null;
      button.title = reason ?? (button.textContent ?? "");
      if (button.disabled) button.setAttribute("aria-disabled", "true");
      else button.removeAttribute("aria-disabled");
    }
  };
  syncMessageState();
  if (actions) {
    input.addEventListener("input", () => {
      actions.onMessageChange(input.value);
      syncMessageState();
    });
  }

  pane.append(commitBar, overflow);
}
