//! Durable work evidence for the paths that produce it without a gate
//! (`runtime.durable_work_evidence`, C7, closes GUI-CORE-028 and E1 defect 4).
//!
//! Before this capability an applied mutation left the archive empty. The
//! trust loop's `RecordAgentEvidence` was the only producer of a `patch` row,
//! so a real edit made by a native turn or by an external Agent published a
//! live `WorkspaceChangeUpdated` a client could render and nothing a reviewer
//! could ever read back: `QueryEvidence` answered with an empty archive after
//! a session that had genuinely changed files. Three separate gaps caused it
//! and all three are closed here.
//!
//! 1. **Nothing built a patch row for a native tool call.** [`native_patch_evidence`]
//!    does, from the same `ToolResult::diff` the cockpit change already
//!    carries, and it stores those exact bytes in the canonical ContextStore so
//!    the row's `source_hash` names content that can be served and verified
//!    rather than a summary of content nobody kept.
//! 2. **The external Agent adapters cannot store bytes.** `viden-agents` is a
//!    leaf below the runtime with no ContextStore of its own, so it publishes
//!    its patch facts with `canonical: None` and carries the diff in the
//!    fact's `metadata`. [`canonicalize_agent_patch_evidence`] is the runtime's
//!    ingestion of that: it stores the bytes and completes the row.
//! 3. **Supervised work never reached the durable projection.** Only
//!    `SessionEngine::handle_runtime_command` persisted its domain facts; a
//!    turn driven by `RuntimeSupervisor` published to the live bus and stopped
//!    there. `SessionEngine::absorb_supervised_events` is the entry point that
//!    closes it, in `session_lifecycle`.
//!
//! **Honesty rules this module keeps.** A row is recorded with
//! `canonical: None` and a summary that says why whenever Core does not hold
//! servable bytes — an over-bound diff, a store that refused the write — so
//! "no canonical bytes" is never the same fact as "bytes we did not mention".
//! `permission_snapshot_id` is `None` unless an operator approval actually
//! allowed this tool call, because naming a receipt that does not exist would
//! let a merge gate accept a mutation nobody approved. And the producer task is
//! the owner's task when the turn is bound to one, never a task id invented for
//! a session-scoped turn: that is what makes a composer edit unable to satisfy
//! a Lane's merge gate.

use std::path::Path;
use std::sync::{Arc, Mutex};

use sha2::{Digest, Sha256};
use viden_context::{ContextEngine, ContextPutRequest};
use viden_types::{
    CanonicalEvidenceReference, ContextContentKind, ContextScope, EvidenceProducer,
    EvidenceQualityFacts, EvidenceQualityStatus, EvidenceVerificationState, EvidenceView,
    MAX_EVIDENCE_CONTENT_BYTES, RuntimeEvent, RuntimeEventKind, RuntimeOwner, now_timestamp,
};

/// Evidence kind whose canonical bytes are a unified diff.
///
/// The same string the merge gate reduces required evidence on and the read
/// path parses as a diff, named once so a producer cannot drift from either.
pub(crate) const PATCH_EVIDENCE_KIND: &str = "patch";

/// Producer identity for a mutation the built-in runtime made itself.
const NATIVE_PRODUCER_IDENTITY: &str = "native";

/// Producer identity for a mutation an external agent adapter reported.
const AGENT_PRODUCER_IDENTITY: &str = "agent";

/// The role a patch-producing turn acts in.
///
/// `EvidenceProducer::role` is validated against [`viden_types::AgentRole`] on
/// the external ingestion path, so a producer role Core writes itself must come
/// from the same vocabulary rather than being free text.
const PATCH_PRODUCER_ROLE: &str = "coder";

/// The approval receipt the current tool call may claim.
///
/// A shared slot rather than a return value because the two halves live on
/// different borrows: the supervisor's approver closure learns the audit id
/// when an operator answers, and the engine's tool loop needs it one step
/// later, while the engine itself is mutably borrowed by the turn. It is
/// deliberately *consumed* by the next completed tool call
/// ([`SessionEngine::take_permission_receipt`]): an `allow_session` or
/// `allow_repo` scope lets later tool calls run with no approval at all, and a
/// receipt left in the slot would attach the first approval's audit id to every
/// mutation after it.
pub(crate) type PermissionReceiptSlot = Arc<Mutex<Option<String>>>;

/// The native turn a patch row is attributed to.
///
/// Installed for the length of one turn by whichever driver is running it —
/// `run_native_turn_sequence` for the direct path, `run_one_supervised_native_turn`
/// for the supervised one — because the tool loop is several call frames below
/// both and the owner is not a parameter of any of them. `None` means no turn
/// is open, in which case a tool call publishes no patch row: an unattributed
/// mutation is exactly what the archive must not contain.
#[derive(Debug, Clone)]
pub(crate) struct NativeTurnContext {
    pub(crate) owner: RuntimeOwner,
    pub(crate) permission_receipt: PermissionReceiptSlot,
}

/// Everything [`native_patch_evidence`] needs that is not the store.
pub(crate) struct NativePatchEvidenceInput<'a> {
    /// The turn's owner, carrying `turn_id`.
    pub(crate) owner: &'a RuntimeOwner,
    pub(crate) tool_call_id: &'a str,
    pub(crate) path: &'a str,
    /// The unified diff the tool produced. Stored verbatim.
    pub(crate) diff: &'a str,
    /// The context bundle the turn built, when it built one. Not invented: a
    /// gate that cannot find a bundle under this id blocks the row with
    /// `MissingSource`, which is the truth about it.
    pub(crate) bundle_id: &'a str,
    /// The audit id of the approval that allowed this tool call, or `None` when
    /// a rule allowed it without asking anybody.
    pub(crate) permission_snapshot_id: Option<String>,
}

/// The task a patch row names as its producer.
///
/// The owner's task when the turn is bound to one — a Lane's native turn
/// working on a task — then the turn, then the tool call. The order is the
/// contract: a merge gate accepts a row only when `producer.task_id` equals the
/// gate's task, so a session-scoped composer turn (which names no task and
/// therefore falls through to its turn id) can never satisfy one. That is the
/// intended refusal, not a gap.
pub(crate) fn patch_producer_task_id(owner: &RuntimeOwner, tool_call_id: &str) -> String {
    owner
        .task_id
        .as_deref()
        .filter(|id| !id.is_empty())
        .or_else(|| owner.turn_id.as_deref().filter(|id| !id.is_empty()))
        .unwrap_or(tool_call_id)
        .to_string()
}

/// Builds the archived `patch` row for one applied native mutation.
///
/// The row is published whatever happens to the bytes; only `canonical` and the
/// summary differ, because "this turn changed that file" is a fact worth
/// archiving even when Core cannot keep a servable copy of the change.
pub(crate) fn native_patch_evidence(
    context_engine_root: &Path,
    input: NativePatchEvidenceInput<'_>,
) -> EvidenceView {
    let task_id = patch_producer_task_id(input.owner, input.tool_call_id);
    let scope = ContextScope::Task(task_id.clone());
    let (additions, deletions) = count_diff_lines(input.diff);
    let stored = store_canonical_diff(context_engine_root, &scope, input.diff, input.tool_call_id);
    let summary = match &stored {
        Ok(_) => format!(
            "native {} edit: {} (+{additions}/-{deletions})",
            short_identity(input.owner),
            input.path
        ),
        Err(reason) => format!(
            "native {} edit: {} (+{additions}/-{deletions}); no canonical bytes: {reason}",
            short_identity(input.owner),
            input.path
        ),
    };
    EvidenceView {
        id: format!("patch-{}", input.tool_call_id),
        kind: PATCH_EVIDENCE_KIND.to_string(),
        summary,
        path: Some(input.path.to_string()),
        source: Some(NATIVE_PRODUCER_IDENTITY.to_string()),
        canonical: stored.ok().map(|item_id| CanonicalEvidenceReference {
            item_id,
            bundle_id: input.bundle_id.to_string(),
            source_hash: sha256_hex(input.diff.as_bytes()),
            producer: EvidenceProducer {
                identity: NATIVE_PRODUCER_IDENTITY.to_string(),
                role: PATCH_PRODUCER_ROLE.to_string(),
                task_id,
            },
            permission_snapshot_id: input.permission_snapshot_id,
            permission_scope: scope.clone(),
            evidence_scope: scope,
            // The bytes were verified on the way in: `store_canonical_diff`
            // re-reads and re-hashes the item it just wrote, so `Verified` here
            // is a check that ran rather than an assumption about the store.
            verification: EvidenceVerificationState::Verified,
            quality: EvidenceQualityFacts {
                status: EvidenceQualityStatus::Pass,
                reason_codes: Vec::new(),
            },
        }),
        metadata: None,
        timestamp: Some(now_timestamp()),
        owner: Some(input.owner.clone()),
    }
}

/// Completes the agent patch facts in one ingested batch.
///
/// `viden-agents` builds its patch rows with `canonical: None` and the diff in
/// `metadata.diff`, because it owns no ContextStore. This stores those bytes and
/// publishes the existing `EvidenceCanonicalized` beside the completed row, so
/// a client that saw the patch fact also learns the bytes behind it are now
/// servable. The returned vector carries only the pairs that were completed, so
/// a caller can archive exactly what changed.
///
/// The producer task is read from the `MergeGateUpdated` fact the same batch
/// carries — the gate this evidence was just attached to — rather than derived
/// from the evidence id, so a gate accepting the row and the row naming the gate
/// cannot disagree. A batch with no gate names the owner's task, then nothing:
/// an agent patch Core cannot attribute is left uncanonicalized rather than
/// stored under an invented task, because the scope is what the gate checks.
pub(crate) fn canonicalize_agent_patch_evidence(
    context_engine_root: &Path,
    events: &mut Vec<RuntimeEvent>,
) -> Vec<RuntimeEvent> {
    let batch_task_id = events.iter().rev().find_map(|event| match &event.kind {
        RuntimeEventKind::MergeGateUpdated { gate } => Some(gate.task_id.clone()),
        _ => None,
    });
    let mut completed = Vec::new();
    let mut insertions = Vec::new();
    for (index, event) in events.iter_mut().enumerate() {
        let RuntimeEventKind::EvidenceRecorded { evidence } = &mut event.kind else {
            continue;
        };
        if evidence.kind != PATCH_EVIDENCE_KIND || evidence.canonical.is_some() {
            continue;
        }
        let Some(diff) = agent_patch_diff(evidence) else {
            continue;
        };
        let Some(task_id) = batch_task_id.clone().or_else(|| {
            evidence
                .owner
                .as_ref()
                .and_then(|owner| owner.task_id.clone())
                .filter(|id| !id.is_empty())
        }) else {
            continue;
        };
        let scope = ContextScope::Task(task_id.clone());
        let Ok(item_id) = store_canonical_diff(context_engine_root, &scope, &diff, &evidence.id)
        else {
            continue;
        };
        let canonical = CanonicalEvidenceReference {
            item_id: item_id.clone(),
            bundle_id: item_id,
            source_hash: sha256_hex(diff.as_bytes()),
            producer: EvidenceProducer {
                identity: AGENT_PRODUCER_IDENTITY.to_string(),
                role: PATCH_PRODUCER_ROLE.to_string(),
                task_id,
            },
            // An adapter-reported patch carries no operator approval receipt:
            // the ACP permission bridge decides per request and mints no audit
            // id this fact can name. `None` is what keeps a merge gate from
            // treating it as approved evidence.
            permission_snapshot_id: None,
            permission_scope: scope.clone(),
            evidence_scope: scope,
            verification: EvidenceVerificationState::Verified,
            quality: EvidenceQualityFacts {
                status: EvidenceQualityStatus::Pass,
                reason_codes: Vec::new(),
            },
        };
        evidence.canonical = Some(canonical.clone());
        let evidence_id = evidence.id.clone();
        completed.push(RuntimeEvent::new(
            0,
            RuntimeEventKind::EvidenceRecorded {
                evidence: evidence.clone(),
            },
        ));
        let canonicalized = RuntimeEvent::new(
            0,
            RuntimeEventKind::EvidenceCanonicalized {
                evidence_id,
                item_id: canonical.item_id.clone(),
                content_sha256: canonical.source_hash.clone(),
            },
        );
        completed.push(canonicalized.clone());
        insertions.push((index + 1, canonicalized));
    }
    // Inserted back to front so an earlier insertion cannot shift a later
    // index, and each `EvidenceCanonicalized` lands immediately after the row
    // it completes rather than at the end of the batch.
    for (index, event) in insertions.into_iter().rev() {
        events.insert(index, event);
    }
    completed
}

/// The unified diff an agent adapter carried in its patch fact's metadata.
fn agent_patch_diff(evidence: &EvidenceView) -> Option<String> {
    evidence
        .metadata
        .as_ref()?
        .get("diff")?
        .as_str()
        .map(str::to_string)
        .filter(|diff| !diff.trim().is_empty())
}

/// Stores one diff and re-verifies it, returning the stored item id.
///
/// `Err` carries the reason the row will publish in its summary. Two of them
/// matter and are distinguished because an operator's next action differs: a
/// diff over [`MAX_EVIDENCE_CONTENT_BYTES`] was never offered to the store, and
/// a store failure means Core tried and could not keep the bytes. The 64 KiB
/// cockpit patch bound deliberately does not apply here — that bound exists so
/// a live event stays small, while the archive's job is to hold the whole
/// change a reviewer has to read.
fn store_canonical_diff(
    context_engine_root: &Path,
    scope: &ContextScope,
    diff: &str,
    evidence_id: &str,
) -> Result<String, String> {
    let limit = MAX_EVIDENCE_CONTENT_BYTES as usize;
    if diff.len() > limit {
        return Err(format!(
            "the diff is {} bytes, over the {limit} byte canonical evidence bound",
            diff.len()
        ));
    }
    let mut engine = ContextEngine::open(context_engine_root)
        .map_err(|err| format!("the canonical store could not be opened: {err}"))?;
    let stored = engine
        .store(ContextPutRequest {
            scope: scope.clone(),
            kind: ContextContentKind::Diff,
            content: diff.as_bytes(),
            evidence_id: Some(evidence_id.to_string()),
        })
        .map_err(|err| format!("the canonical store refused the write: {err}"))?;
    // Verify on the way in rather than trusting the write: the row about to be
    // published claims `Verified`, and the read path will re-verify against the
    // same hash. A row that claimed verification without one would be the one
    // kind of evidence a reviewer must never be shown.
    engine
        .verify_item(&stored.item.item_id, &stored.item.content_sha256, scope)
        .map_err(|err| format!("the stored canonical bytes did not verify: {err}"))?;
    Ok(stored.item.item_id)
}

/// `+`/`-` line counts, for the row's summary only.
fn count_diff_lines(diff: &str) -> (usize, usize) {
    diff.lines().fold((0, 0), |(additions, deletions), line| {
        if line.starts_with("+++") || line.starts_with("---") {
            (additions, deletions)
        } else if line.starts_with('+') {
            (additions + 1, deletions)
        } else if line.starts_with('-') {
            (additions, deletions + 1)
        } else {
            (additions, deletions)
        }
    })
}

/// How a summary names the work that produced the change.
///
/// The Lane when there is one, otherwise "session": the two are different
/// facts for a reviewer, and a summary that called a composer edit a Lane's
/// would misattribute it.
fn short_identity(owner: &RuntimeOwner) -> String {
    owner
        .lane_id
        .as_deref()
        .filter(|id| !id.is_empty())
        .map(|id| format!("lane {id}"))
        .unwrap_or_else(|| "session".to_string())
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

impl crate::SessionEngine {
    /// Opens the attribution window for one native turn.
    ///
    /// Returns the receipt slot the caller's approver writes an allowed
    /// approval's audit id into. Both halves are installed together because a
    /// patch row without an owner and a patch row without a permission receipt
    /// are the two ways the archive lies about who changed a file.
    pub(crate) fn begin_native_turn(&mut self, owner: RuntimeOwner) -> PermissionReceiptSlot {
        let permission_receipt: PermissionReceiptSlot = Arc::new(Mutex::new(None));
        self.active_native_turn = Some(NativeTurnContext {
            owner,
            permission_receipt: Arc::clone(&permission_receipt),
        });
        permission_receipt
    }

    /// Closes it. Called on every exit of the turn, including the failing and
    /// cancelled ones, so a later tool call outside a turn cannot inherit the
    /// previous turn's owner.
    pub(crate) fn end_native_turn(&mut self) {
        self.active_native_turn = None;
    }

    /// Consumes the approval receipt the running tool call may claim.
    ///
    /// Taken once per completed tool call whatever the tool was, so a receipt
    /// from an approval that allowed a `shell` command cannot be attached to an
    /// unapproved `edit_file` that follows it.
    pub(crate) fn take_permission_receipt(&self) -> Option<String> {
        self.active_native_turn
            .as_ref()
            .and_then(|turn| turn.permission_receipt.lock().ok()?.take())
    }

    /// The archived `patch` facts for one completed tool call, if it applied a
    /// mutation inside an open turn.
    ///
    /// Empty for every other case, and deliberately so: a failed tool changed
    /// nothing, a non-mutating tool produced no diff, and a tool call outside a
    /// turn has no owner to attribute. The events are returned rather than
    /// remembered here — the archive is what was persisted, so the durable
    /// write is what admits a row into it.
    pub(crate) fn native_patch_evidence_events(
        &self,
        call: &viden_types::ToolCall,
        result: &viden_types::ToolResult,
        permission_snapshot_id: Option<String>,
    ) -> Vec<RuntimeEvent> {
        if !result.success || !matches!(call.name.as_str(), "write_file" | "edit_file") {
            return Vec::new();
        }
        let Some(turn) = self.active_native_turn.as_ref() else {
            return Vec::new();
        };
        let Some(path) = call
            .input
            .get("path")
            .filter(|path| !path.trim().is_empty())
        else {
            return Vec::new();
        };
        let Some(diff) = result
            .diff
            .as_deref()
            .filter(|diff| !diff.trim().is_empty())
        else {
            return Vec::new();
        };
        let bundle_id = self
            .last_context_bundle
            .as_ref()
            .map(|bundle| bundle.bundle_id.clone())
            .unwrap_or_default();
        let evidence = native_patch_evidence(
            self.context_engine_root(),
            NativePatchEvidenceInput {
                owner: &turn.owner,
                tool_call_id: &result.tool_call_id,
                path,
                diff,
                bundle_id: &bundle_id,
                permission_snapshot_id,
            },
        );
        let canonicalized = evidence.canonical.as_ref().map(|canonical| {
            RuntimeEvent::new(
                0,
                RuntimeEventKind::EvidenceCanonicalized {
                    evidence_id: evidence.id.clone(),
                    item_id: canonical.item_id.clone(),
                    content_sha256: canonical.source_hash.clone(),
                },
            )
        });
        let mut events = vec![RuntimeEvent::new(
            0,
            RuntimeEventKind::EvidenceRecorded { evidence },
        )];
        events.extend(canonicalized);
        events
    }
}
