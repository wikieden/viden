# TUI 监督决策检查点

英文版：[checkpoints.md](checkpoints.md)

日期：2026-08-30

本证据只描述本地候选。这里没有任何内容被发布、签名、公证、push、merge、tag 或经过
live provider 认证。工作以提交的形式存在于隔离 worktree 中的特性分支上。

阶段 1、2（监督决策）与阶段 3（审计时间线，T1b-1）是各自独立、基线不同的分支；
每个阶段的章节都各自标明自己的基线与分支。

## 候选线

| 项目 | SHA / 路径 |
| --- | --- |
| 基线 `origin/main` | `126a5321` |
| 阶段 1 — 监督基础 | `claude/tui-supervision-foundation` 上的 `2aafa99b` |
| 阶段 2 — 决策工作流 | `claude/tui-supervision-decisions` 的分支头 |
| Worktree | `.worktrees/tui-supervision-decisions` |
| 组件 | TUI `0.3.3`，`min_core_version` `0.3.4` |

阶段 2 从阶段 1 分出，而不是从 `main` 分出。两个分支都未被 merge，也未被 push。

## 交付的界面

| 界面 | 行为 |
| --- | --- |
| 决策中心（`OverlayKind::Decisions`） | 将审批、merge gate、待裁决 review request、待重新验证 conflict bounce 合并为一个有序可选列表，使用登记过的 `⏸` / `◌` / `⚠` 字形并带选中标记。审批行仍然路由到固定的 Approval 覆盖层。 |
| 监督决策覆盖层（`OverlayKind::SupervisionDecision`） | 新覆盖层，由 `SupervisionTarget`（gate、review 或 bounce）参数化。键盘优先的动作栏：方向键与数字键选择，`Enter` 确认，`Esc` 先关闭原因输入行再关闭覆盖层。 |
| 动作可用性 | 由每帧从 `RuntimeViewState` 重新读取的完整 Core 记录推导，绝不使用精简投影行。对记录当前状态永远不适用的动作不会列出。 |
| 原因 / 反馈输入 | 覆盖层内的单行输入。拒绝、回滚、回弹必须填写文本；两种 review 结论的反馈都是选填。必填文本为空或超过 Core 的 500 字符 trust-text 上限时在本地拒绝，且不发送任何命令。 |
| 分发与结果 | 每个动作通过 Core 客户端发送一条 `RuntimeCommand` 并登记 confirm-on-fact 期望。结果同时在覆盖层与状态栏渲染：pending `◌`、confirmed `✓`、rejected `✗` 并原样展示 Core 给出的原因。 |
| 重置与逃生 | 已裁决结果在发起下一次监督动作或关闭覆盖层时重置为 idle；pending 绝不自动重置。覆盖层与决策中心页脚提供 `dismiss` 动作，清除滞留的 pending 归属但不裁决结果，也不取消 Core 命令。 |
| 墙钟时间展示 | cost-blind lane 的运行统计把墙钟时间渲染为毫秒、带一位小数的秒或分加秒，取代原来的原始毫秒数。 |

## 与 GUI 对齐的载荷推导

TUI 发送与 GUI 集成门适配器完全相同的载荷形状，因为 Core 就是按这些精确取值校验接受操作的
（`apps/gui/src-tauri/src/projection.rs`、`apps/gui/src-tauri/src/adapter.rs`）：

| 命令 | 推导 |
| --- | --- |
| `AcceptMergeGate.actor` | `d12_accept_actor`：存在 validator 时，取带 Lane id 的 validator owner，否则没有可用 actor；不存在 validator 时取 gate owner。 |
| `AcceptMergeGate.reviewed_evidence` | `d12_reviewed_evidence`：存在 validator 时原样使用 review request 记录的 `evidence_bindings`；默认 owner 的 gate 发送空列表；其余情况按 `d12_canonical_bindings` 从 `latest_evidence` 重建规范绑定并排序去重。任一列出的证据缺少规范引用即表示没有合法载荷，该动作在本地被拒绝。 |
| `RejectMergeGate.actor` | `d12_reject_actor`：gate owner 非默认时取 gate owner，否则取带 Lane id 且非默认的 validator owner。 |
| `RevertAppliedChange.owner` | gate owner；为默认 owner 时拒绝。 |
| `BounceMergeConflict` | owner 与 `original_lane_id` 都从 gate owner 回放，符合 `validate_conflict_bounce` 的要求。 |
| `RevalidateMergeConflict` | `bounce_id` 与 `actor` 来自待处理的 conflict 记录；`evidence` 取 `source_hash` 与全部 baseline 哈希都不同的那条规范绑定，符合 `validate_conflict_revalidation` 的要求。 |
| `DecideReview.actor` | gate validator 指向该 review 时取其 owner，否则复现 Core 自身的 `reviewer_owner_from_requester`：把 requester owner 指向 reviewer Lane，并清空 session 与 turn 身份。 |

## Confirm-On-Fact 期望

| 动作 | 确认用的 Core 事实 |
| --- | --- |
| 接受 gate | 命中该 gate id 且状态为 `Accepted` 的 `MergeGateUpdated` |
| 拒绝 gate | 命中该 gate id 且状态为 `NeedsChanges` 的 `MergeGateUpdated` |
| 重新验证冲突 | 命中该 gate id 且状态为 `CollectingEvidence` 的 `MergeGateUpdated`（`trust_loop::revalidate_merge_conflict` 先发布 `MergeConflictBounced`，再把 gate 退回证据收集态） |
| 回滚 | 该 gate 的 `RevertRecorded` |
| 回弹 | 该 gate 的 `MergeConflictBounced` |
| 裁决 review | 命中该 review id 且状态与结论对应的 `ReviewRequestUpdated` |

`CommandAccepted` 绝不确认。相同 command id 的 `CommandRejected` 是唯一的本地失败路径，
并原样携带 Core 给出的原因。

## 固定测试

已加入 `scripts/tui-turn-controller-smoke.sh`：

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

`every_supervision_decision_round_trips_through_its_exact_core_fact` 通过伪 Core 客户端驱动
全部七条命令，并对每一条断言：发送的 `RuntimeCommandEnvelope` 精确匹配、回执只保持 pending、
只有匹配的业务事实才确认。

## 确定性证据

| 命令 | 结果 |
| --- | --- |
| `cargo test -p viden-tui` | PASS，306 个库测试 + 1 个 API 测试 |
| `bash scripts/tui-turn-controller-smoke.sh` | PASS |
| `bash scripts/rc-tui-stability-smoke.sh` | PASS |
| `bash scripts/tui-regression.sh` | PASS |
| `bash scripts/tui-previews.sh` | PASS，全部 preview 断言成立 |
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --workspace --all-targets` | PASS，`viden-tui` 无新增告警 |
| `cargo test --workspace --quiet` | PASS |
| `bash scripts/check-dependency-boundaries.sh` | PASS |
| `git diff --check` | PASS |
| 对改动 Markdown 运行 `scripts/check-doc-pairs.sh` / `scripts/check-doc-links.sh` | PASS |

`apps/tui/release-manifest.toml` 中的 i18n catalog 摘要已按新增的监督键重算，两个语言包保持
精确的键与参数对等。

## T1b-1 — 审计时间线面板

同一条 TUI 监督线的第 3 阶段。它闭合了前两个阶段打开的循环：决策界面回答“Core 还在
等什么”，本阶段回答“已经发生过什么”。TUI 是 Core 审计契约
（`RuntimeCommand::QueryAudit` -> `RuntimeEventKind::AuditPageLoaded`，Core 侧落于
`faad3fc5`）的**第一个**消费者；目前没有其他客户端消费它，因此没有先例可循。

| 项 | SHA / 路径 |
| --- | --- |
| 基线 | `main` 上的 `7ff5139a` |
| 阶段 3 — 审计时间线 | `claude/tui-audit-panel` 分支顶端 |
| Worktree | `.worktrees/tui-audit-panel` |
| 组件 | TUI `0.3.3`，`min_core_version` `0.3.4` |

未合并、未推送、未发布。

### 交付的界面

| 界面 | 行为 |
| --- | --- |
| 审计时间线覆盖层（`OverlayKind::AuditTimeline`） | 只读浏览界面。没有全局快捷键：只能从监督覆盖层或决策中心进入。选择在各行之间移动；在末尾的 `Load older records` 行上按 `Enter` 向 Core 请求下一页；`Esc` 关闭并回到基础状态。 |
| 有范围入口 | 监督决策覆盖层新增的非变更行 `Audit trail`，追加在全部决策之后，因此绝不会给任何决策重新编号。gate 或 conflict 目标将查询范围限定为 `merge_gate:<gate id>`，review 目标限定为 `review_request:<review id>`，使用 `AuditObjectRef::KIND_*` 常量而非字符串字面量。 |
| 无范围入口 | 决策中心末尾追加的 `Audit timeline (project)` 选项。它不设置 `project_id` 与 `lane_id`，因为 Core 的审计存储已经限定在本项目自己的 workflow 目录内。 |
| 分页 | 首次查询 `limit` 为 100 且不带游标；首页替换，后续页追加（记录由新到旧，因此更早的页追加在末尾）。`complete` 时隐藏加载更早行。已有查询在途时的第二次查询在本地被拒绝并给出提示，不发送任何命令。 |
| 行渲染 | `{time} {action} {outcome} {objects} {args}`。`time` 为 `HH:MM:SS` UTC（TUI 此前没有绝对时间先例，而审计记录需要跨机器比对）。`action` 是 Core 原始的点分 key，刻意不本地化。结果使用注册字形 `✓` / `✗`；未知结果渲染为字面 ASCII `?`。objects 为逗号连接的 `kind:id`，args 为空格连接的 `k=v`，两者均按显示宽度截断。 |
| 状态区分 | loading、empty（仅在某一页已到达后）、error（原样展示 Core 的原因）与已加载列表，另有页脚说明已加载条数以及是否仍有更早的记录。全部 chrome 在 `en` 与 `zh-CN` 中本地化；`action` key 不本地化。 |
| 独立性 | 关联是面板局部的，并刻意**不**复用 `SupervisionMachine`：审计读取绝不阻塞在途的监督变更，也绝不被其阻塞，且审计页绝不裁决任何决策。 |
| Plan 模式 | `QueryAudit` 不产生变更也不触发权限提示，因此在 Plan 模式下照常发送并渲染。 |

### 如实记录的限制

`AuditPageLoaded` 不携带 command id。因此另一个客户端并发查询产生的页可能被归属到本面板
在途的查询上。该限制已记录在 `apps/tui/src/tui/audit_panel.rs` 中，并在单操作者闭环下被
接受：该页是真实的 Core 页，覆盖层关闭时即被丢弃，下一次查询会自行纠正。未构建任何投机性
关联机制。彻底消除这一歧义属于 **Core 契约请求**——在 `AuditPageLoaded` 上携带 command
id——而不是客户端的猜测。

同时记录：TUI 没有通用的覆盖层栈。`OverlayState::previous_overlay` 存在，但仅供 Global
Jump 使用，现有的决策中心 -> 审批与决策中心 -> 监督路径都是替换覆盖层而非入栈。审计覆盖层
与该行为保持一致：`Esc` 关闭并回到基础状态，不返回监督覆盖层。本功能没有为此发明新的栈。

### 新增的固定测试

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

未知结果的 `?` 回退仅在编译期被检查：`AuditOutcome` 是 `#[non_exhaustive]` 且没有 serde
`other` 分支，因此在 `viden-types` 之外无法构造或反序列化出未知变体。测试断言了每个已知变体
的字形，以及该回退保持为字面 ASCII。

`tui::decision::tests::decision_picks_list_approvals_gates_pending_reviews_then_pending_bounces`
是被更新而非削弱：它现在断言审计选项追加在放弃逃生口之后，因此两个非决策条目都不会移动真实
决策的下标。

### 确定性证据（T1b-1）

| 命令 | 结果 |
| --- | --- |
| `cargo test -p viden-tui` | PASS，326 个库测试 + 1 个 API 测试 |
| `bash scripts/tui-turn-controller-smoke.sh` | PASS，77 个固定测试 |
| `bash scripts/rc-tui-stability-smoke.sh` | PASS |
| `bash scripts/tui-regression.sh` | PASS |
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --workspace --all-targets` | PASS，`viden-tui` 无告警 |
| `cargo test --workspace --quiet` | PASS |
| `git diff --check` | PASS |
| 对改动 Markdown 运行 `scripts/check-doc-pairs.sh` / `scripts/check-doc-links.sh` | PASS |

`apps/tui/release-manifest.toml` 中的 i18n catalog 摘要已按新增的审计键重算，两个语言包保持
精确的键与参数对等。

## 本阶段未交付

- 审计时间线内的逐行动作（跳转到记录、复制 audit id、按 actor 或 action 过滤）。该覆盖层
  只负责浏览，不做任何决策。
- 客户端侧按 lane、actor 或时间范围的审计过滤。Core 的 `AuditQuery` 暴露了 `lane_id`，但
  目前没有 TUI 界面设置它，也不会在本地对某一页做过滤。
- handoff、contract、dependency 的创建流程——明确推迟到 0.3.3 / T2；它们不在计划的 TUI P1
  行内。其 intent 构造器已存在于 `apps/tui/src/tui/supervision.rs`，尚未被分发，暂由局部
  `#[allow(dead_code)]` 标注。
- ~~专用的证据检视器。决策覆盖层展示证据数量与标识，不展示证据内容。~~
  **已于 2026-09-10 交付（T1b）**，基于 `runtime.evidence_reads`（GUI-CORE-025）：
  决策覆盖层新增"查看证据…"读取行，作用域为记录自身由 Core 发布的 owner；`/evidence`
  以聚焦 Lane 为作用域打开同一份分页归档。参见
  [TUI 0.3.4 源码控制与冲突对等](../../release-tui-0.3.4-source-control-parity.zh-CN.md)。
- 多选或批量监督决策。按设计，同一时刻只允许一条监督命令在途。
- GUI 对审计契约的消费。目前 TUI 是唯一的客户端。
