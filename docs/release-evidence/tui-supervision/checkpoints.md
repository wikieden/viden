# TUI Supervision Decision Checkpoints

Chinese version: [checkpoints.zh-CN.md](checkpoints.zh-CN.md)

Date: 2026-08-30

This evidence describes local candidates only. Nothing here is published,
signed, notarized, pushed, merged, tagged, or certified against a live
provider. The work exists as commits on feature branches in isolated worktrees.

Stages 1 and 2 (supervision decisions) and stage 3 (the audit timeline, T1b-1)
are separate branches with separate bases; each stage's section names its own.

## Candidate Line

| Item | SHA / path |
| --- | --- |
| Base `origin/main` | `126a5321` |
| Stage 1 — supervision foundation | `2aafa99b` on `claude/tui-supervision-foundation` |
| Stage 2 — decision workflows | tip of `claude/tui-supervision-decisions` |
| Worktree | `.worktrees/tui-supervision-decisions` |
| Component | TUI `0.3.3`, `min_core_version` `0.3.4` |

Stage 2 branches off stage 1, not off `main`. Neither branch has been merged or
pushed.

## Surfaces Delivered

| Surface | Behavior |
| --- | --- |
| Decision Center (`OverlayKind::Decisions`) | Lists approvals, merge gates, pending review requests, and pending conflict bounces as one ordered pick list, with the registered `⏸` / `◌` / `⚠` glyphs and a selection marker. Approval rows still route to the pinned Approval overlay. |
| Supervision decision overlay (`OverlayKind::SupervisionDecision`) | New overlay parameterized by a `SupervisionTarget` (gate, review, or bounce). Keyboard-first action bar; arrows and number keys select, `Enter` confirms, `Esc` unwinds the reason line then the overlay. |
| Action availability | Derived from the full Core record re-read from `RuntimeViewState`, never from the compact projection row. Actions that can never apply to the record's current status are not listed. |
| Reason / feedback input | Single-line input inside the overlay. Reject, revert, and bounce require text; review feedback is optional for both verdicts. Empty required text and text over Core's 500-character trust-text limit are refused locally with nothing sent. |
| Dispatch and outcome | Every action sends one `RuntimeCommand` through the Core client and registers a confirm-on-fact expectation. Outcome renders in the overlay and echoes in the status bar: pending `◌`, confirmed `✓`, rejected `✗` with Core's own reason verbatim. |
| Reset and escape | A settled outcome resets to idle when the next supervision action is initiated or the overlay is closed. A pending outcome is never auto-reset. A `dismiss` action in the overlay and in the Decision Center footer clears a stranded pending correlation without settling it and without cancelling the Core command. |
| Wall-time display | Blind-lane run stats render wall time as milliseconds, seconds with one decimal, or minutes plus seconds, replacing the raw millisecond figure. |

## Payload Derivation Mirrored From The GUI

The TUI sends the same shapes the GUI integration-gate adapter sends, because
Core compares acceptance against those exact values
(`apps/gui/src-tauri/src/projection.rs`, `apps/gui/src-tauri/src/adapter.rs`):

| Command | Derivation |
| --- | --- |
| `AcceptMergeGate.actor` | `d12_accept_actor`: with a validator, the validator owner when it carries a Lane id, otherwise no actor; without a validator, the gate owner. |
| `AcceptMergeGate.reviewed_evidence` | `d12_reviewed_evidence`: with a validator, the review request's recorded `evidence_bindings` verbatim; a default-owner gate sends an empty list; otherwise the canonical bindings rebuilt from `latest_evidence`, sorted and deduped (`d12_canonical_bindings`). Any listed evidence without a canonical reference means no valid payload, so the action is refused locally. |
| `RejectMergeGate.actor` | `d12_reject_actor`: the gate owner when it is not the default owner, otherwise the validator owner when it has a Lane id and is not default. |
| `RevertAppliedChange.owner` | The gate owner, refused when it is the default owner. |
| `BounceMergeConflict` | Owner and `original_lane_id` both replayed from the gate owner, as `validate_conflict_bounce` requires. |
| `RevalidateMergeConflict` | `bounce_id` and `actor` from the pending conflict record; `evidence` is the one canonical binding whose `source_hash` differs from every baseline hash, as `validate_conflict_revalidation` requires. |
| `DecideReview.actor` | The gate validator's owner when it names this review, otherwise Core's own `reviewer_owner_from_requester` derivation: the requester owner re-pointed at the reviewer Lane with session and turn identity unclaimed. |

## Confirm-On-Fact Expectations

| Action | Confirming Core fact |
| --- | --- |
| Accept gate | `MergeGateUpdated` with this gate id and `Accepted` |
| Reject gate | `MergeGateUpdated` with this gate id and `NeedsChanges` |
| Revalidate conflict | `MergeGateUpdated` with this gate id and `CollectingEvidence` (`trust_loop::revalidate_merge_conflict` publishes `MergeConflictBounced` then returns the gate to evidence collection) |
| Revert | `RevertRecorded` for this gate |
| Bounce | `MergeConflictBounced` for this gate |
| Decide review | `ReviewRequestUpdated` with this review id and the status the verdict maps to |

`CommandAccepted` never confirms. `CommandRejected` for the same command id is
the only local failure path and carries Core's reason unchanged.

## Pinned Tests

Added to `scripts/tui-turn-controller-smoke.sh`:

```text
tui::pending::tests::abandon_clears_a_stranded_pending_decision_without_settling_it
tui::pending::tests::only_a_settled_outcome_resets_to_idle
tui::decision::tests::action_availability_follows_the_records_current_status
tui::decision::tests::accept_payload_mirrors_the_gui_actor_and_reviewed_evidence_derivation
tui::decision::tests::reject_revert_and_bounce_replay_core_owned_parties_and_require_a_reason
tui::decision::tests::revalidation_carries_the_bounce_identity_and_a_changed_canonical_receipt
tui::decision::tests::review_verdicts_carry_the_reviewer_lane_actor_and_optional_feedback
tui::decision::tests::decision_picks_list_approvals_gates_pending_reviews_then_pending_bounces
tui::app::tests::decision_center_lists_supervision_rows_and_routes_every_pick
tui::app::tests::supervision_overlay_unwinds_escape_in_order_and_yields_to_a_pinned_approval
tui::app::tests::supervision_overlay_only_lists_actions_the_gate_status_can_accept
tui::app::tests::a_required_reason_is_enforced_locally_and_nothing_is_sent
tui::app::tests::every_supervision_decision_round_trips_through_its_exact_core_fact
tui::app::tests::core_rejection_renders_its_own_reason_and_frees_the_decision_slot
tui::app::tests::a_second_supervision_action_while_one_is_pending_sends_nothing
tui::app::tests::dismiss_releases_a_stranded_pending_decision_without_sending_anything
tui::app::tests::a_settled_outcome_resets_on_the_next_action_and_on_overlay_close
tui::app::tests::composer_stays_editable_while_the_supervision_overlay_is_open_during_a_stream
tui::app::tests::blind_lane_wall_time_is_rendered_at_the_scale_an_operator_reads
tui::modal::tests::decisions_overlay_projects_typed_gates_recovery_and_pending_core_command
tui::modal::tests::blind_lane_inspector_shows_bounded_run_facts_and_never_fabricates_zeros
```

`every_supervision_decision_round_trips_through_its_exact_core_fact` drives all
seven commands through the fake Core client and asserts, for each one, the exact
`RuntimeCommandEnvelope` sent, that the receipt leaves the decision pending, and
that only the matching business fact confirms it.

## Deterministic Evidence

| Command | Result |
| --- | --- |
| `cargo test -p viden-tui` | PASS, 306 lib + 1 API test |
| `bash scripts/tui-turn-controller-smoke.sh` | PASS |
| `bash scripts/rc-tui-stability-smoke.sh` | PASS |
| `bash scripts/tui-regression.sh` | PASS |
| `bash scripts/tui-previews.sh` | PASS, all preview assertions hold |
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --workspace --all-targets` | PASS, no new warning in `viden-tui` |
| `cargo test --workspace --quiet` | PASS |
| `bash scripts/check-dependency-boundaries.sh` | PASS |
| `git diff --check` | PASS |
| `scripts/check-doc-pairs.sh` / `scripts/check-doc-links.sh` on changed Markdown | PASS |

The i18n catalog digests in `apps/tui/release-manifest.toml` were recomputed for
the added supervision keys and both locales keep exact key and parameter parity.

## T1b-1 — Audit Timeline Panel

Stage 3 of the same TUI supervision line. It closes the loop the earlier stages
opened: the decision surfaces answer "what is Core waiting on", and this one
answers "what already happened". The TUI is the **first** client of the Core
audit contract (`RuntimeCommand::QueryAudit` -> `RuntimeEventKind::
AuditPageLoaded`, landed in Core as `faad3fc5`); no other client consumes it
yet, so there was no precedent to copy.

| Item | SHA / path |
| --- | --- |
| Base | `7ff5139a` on `main` |
| Stage 3 — audit timeline | tip of `claude/tui-audit-panel` |
| Worktree | `.worktrees/tui-audit-panel` |
| Component | TUI `0.3.3`, `min_core_version` `0.3.4` |

Not merged, not pushed, not released.

### Surfaces Delivered

| Surface | Behavior |
| --- | --- |
| Audit timeline overlay (`OverlayKind::AuditTimeline`) | Read-only browsing surface. No global chord: it is reached from the supervision overlay or the Decision Center. Selection moves through rows; `Enter` on the trailing `Load older records` row asks Core for the next page; `Esc` closes to the base state. |
| Scoped entry | A new non-mutation `Audit trail` row on the supervision decision overlay, appended after every decision so it never renumbers one. A gate or conflict target scopes the query to `merge_gate:<gate id>`, a review target to `review_request:<review id>`, using `AuditObjectRef::KIND_*` constants rather than string literals. |
| Unscoped entry | An `Audit timeline (project)` pick appended last in the Decision Center. It leaves `project_id` and `lane_id` unset because Core's audit store is already scoped to the project's own workflow directory. |
| Pagination | First query is `limit` 100 with no cursor; the first page replaces, later pages append (records are newest-first, so older pages append at the end). `complete` hides the load-older row. A second query while one is in flight is refused locally with a message and nothing is sent. |
| Row rendering | `{time} {action} {outcome} {objects} {args}`. `time` is `HH:MM:SS` UTC (no existing TUI absolute-time precedent, and audit records are compared across machines). `action` is Core's raw dotted key, deliberately not localized. Outcome uses registered `✓` / `✗`; an unknown outcome renders literal ASCII `?`. Objects are comma-joined `kind:id`, args space-joined `k=v`, both truncated by display width. |
| Distinct states | Loading, empty (only once a page has arrived), error (Core's reason verbatim), and loaded, plus a footer stating the loaded count and whether older records remain. All chrome is localized in `en` and `zh-CN`; the `action` key is not. |
| Independence | Correlation is panel-local and deliberately does **not** reuse `SupervisionMachine`: an audit read never blocks, and is never blocked by, a pending supervision mutation, and an audit page never settles a decision. |
| Plan mode | `QueryAudit` mutates nothing and prompts for no permission, so the timeline is dispatched and rendered unchanged in Plan mode. |

### Honest Limitation

`AuditPageLoaded` carries no command id. A page produced by another client's
concurrent query can therefore be attributed to this panel's in-flight query.
This is documented in `apps/tui/src/tui/audit_panel.rs` and accepted for the
single-operator loop: the page is a real Core page, it is dropped when the
overlay closes, and the next query self-corrects. No speculative correlation
machinery was built. Removing the ambiguity is a **Core contract request** — a
command id on `AuditPageLoaded` — not a client-side guess.

Also recorded: the TUI has no general overlay stack. `OverlayState::
previous_overlay` exists but is used only by Global Jump, and the existing
Decision Center -> Approval and Decision Center -> supervision routes replace
the overlay rather than stacking it. The audit overlay matches that behavior:
`Esc` closes to the base state and does not return to the supervision overlay.
No stack was invented for this feature.

### Pinned Tests Added

```text
tui::audit_panel::tests::the_first_query_is_unscoped_or_object_scoped_and_pages_from_the_returned_cursor
tui::audit_panel::tests::the_first_page_replaces_and_older_pages_append_in_delivery_order
tui::audit_panel::tests::an_empty_page_is_emptiness_only_after_it_arrives
tui::audit_panel::tests::only_a_rejection_for_this_query_becomes_an_error_and_it_is_cores_own_reason
tui::audit_panel::tests::a_page_with_nothing_in_flight_belongs_to_another_reader_and_is_ignored
tui::audit_panel::tests::a_second_query_while_one_is_in_flight_is_refused_locally
tui::audit_panel::tests::selection_walks_records_then_the_load_older_row_and_never_leaves_the_list
tui::audit_panel::tests::a_row_renders_the_raw_action_key_registered_outcome_glyphs_objects_and_args
tui::audit_panel::tests::a_row_is_truncated_to_the_overlay_width_by_display_width
tui::audit_panel::tests::timestamps_render_as_utc_clock_time
tui::decision::tests::the_audit_row_is_offered_for_every_target_including_ones_with_no_decision_left
tui::decision::tests::audit_scope_uses_the_contracts_own_object_kind_constants
tui::app::tests::opening_the_timeline_scopes_the_query_to_the_record_or_to_the_whole_project
tui::app::tests::the_first_page_replaces_older_pages_append_and_the_footer_states_what_remains
tui::app::tests::a_rejected_query_shows_cores_reason_and_a_page_nobody_asked_for_is_ignored
tui::app::tests::a_second_page_request_while_one_is_in_flight_sends_nothing
tui::app::tests::confirming_a_record_row_does_nothing_and_escape_closes_to_the_base_state
tui::app::tests::the_audit_timeline_is_readable_in_plan_mode
tui::app::tests::composer_stays_editable_while_the_audit_timeline_is_open_during_a_stream
tui::app::tests::a_pending_supervision_decision_neither_blocks_nor_is_settled_by_an_audit_read
```

The unknown-outcome `?` fallback is compile-checked only: `AuditOutcome` is
`#[non_exhaustive]` and carries no serde `other` arm, so no unknown variant can
be constructed or deserialized from outside `viden-types`. The test asserts the
glyph for every known variant and that the fallback stays literal ASCII.

`tui::decision::tests::decision_picks_list_approvals_gates_pending_reviews_then_pending_bounces`
was updated, not weakened: it now asserts the audit pick is appended after the
dismiss escape, so neither non-decision entry can shift a real decision's index.

### Deterministic Evidence (T1b-1)

| Command | Result |
| --- | --- |
| `cargo test -p viden-tui` | PASS, 326 lib + 1 API test |
| `bash scripts/tui-turn-controller-smoke.sh` | PASS, 77 pinned tests |
| `bash scripts/rc-tui-stability-smoke.sh` | PASS |
| `bash scripts/tui-regression.sh` | PASS |
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --workspace --all-targets` | PASS, no `viden-tui` warning |
| `cargo test --workspace --quiet` | PASS |
| `git diff --check` | PASS |
| `scripts/check-doc-pairs.sh` / `scripts/check-doc-links.sh` on changed Markdown | PASS |

The i18n catalog digests in `apps/tui/release-manifest.toml` were recomputed for
the added audit keys and both locales keep exact key and parameter parity.

## Not Delivered Here

- Per-row actions inside the audit timeline (jump to the record, copy the audit
  id, filter by actor or action). The overlay browses; it decides nothing.
- Client-side audit filtering by lane, actor, or time range. Core's
  `AuditQuery` exposes `lane_id`, but no TUI surface sets it yet, and no filter
  is applied locally to a page.
- Creation flows for handoffs, contracts, and dependencies — explicitly
  deferred to 0.3.3 / T2; they are not in the plan's TUI P1 rows. Their intent
  builders exist in `apps/tui/src/tui/supervision.rs` and remain undispatched
  behind a local `#[allow(dead_code)]`.
- ~~A dedicated evidence inspector. The decision overlay shows evidence counts
  and identifiers, not evidence contents.~~ **Delivered 2026-09-10 (T1b)** on
  `runtime.evidence_reads` (GUI-CORE-025): the decisions overlay gained an
  "Evidence…" read row scoped to the record's own Core-published owner, and
  `/evidence` opens the same paged archive scoped to the focused Lane. See
  [TUI 0.3.4 source-control and conflict parity](../../release-tui-0.3.4-source-control-parity.md).
- Multi-select or batch supervision decisions. Exactly one supervision command
  may be in flight, by design.
- GUI consumption of the audit contract. The TUI is the only client so far.
