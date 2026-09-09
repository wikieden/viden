//! Append-only audit timeline contract.
//!
//! The audit timeline is the operator-facing answer to "who changed what, on
//! which objects, and with what outcome". Three invariants make it usable as
//! evidence rather than as decoration:
//!
//! 1. **Append-only.** A record is written once and never rewritten. The
//!    canonical store is JSONL; any index is derived and rebuildable.
//! 2. **Stable keys, not prose.** `action` is a dotted message key and `args`
//!    are bounded stable tokens. Localized or free-text prose (summaries,
//!    reasons, model output) never enters the timeline, because a frontend
//!    must be able to translate and a reader must be able to diff.
//! 3. **No secret bytes.** Arguments are charset- and length-bounded and are
//!    rejected outright when they look like credentials or path traversal.
//!    Sanitization rejects; it never silently strips, so a caller can never
//!    believe it logged something it did not.
//!
//! [`AuditRecord::sanitized`] is the only supported constructor for emission
//! sites, and the store re-validates on append so a hand-built record cannot
//! bypass the bounds.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::RuntimeOwner;

/// Identity of one audit record.
///
/// This is a `String` alias rather than a newtype because trust facts
/// (`HandoffRecord::audit_id`, `MergeGateDecision::audit_id`, ...) already
/// store the same value as a `String`; an audit record reuses the id its trust
/// fact minted so the two can be joined without a translation table.
pub type AuditId = String;

/// Maximum number of `args` entries on one audit record.
pub const MAX_AUDIT_ARGS: usize = 32;
/// Maximum byte length of one `args` key.
pub const MAX_AUDIT_ARG_KEY_BYTES: usize = 64;
/// Maximum byte length of one `args` value.
pub const MAX_AUDIT_ARG_VALUE_BYTES: usize = 512;
/// Maximum number of linked objects on one audit record.
pub const MAX_AUDIT_OBJECTS: usize = 32;
/// Maximum byte length of an object kind key.
pub const MAX_AUDIT_OBJECT_KIND_BYTES: usize = 64;
/// Maximum byte length of an identity (audit id, object id).
pub const MAX_AUDIT_ID_BYTES: usize = 128;
/// Maximum byte length of the dotted action key.
pub const MAX_AUDIT_ACTION_BYTES: usize = 64;
/// Largest page a single [`AuditQuery`] may return.
pub const MAX_AUDIT_PAGE_SIZE: u32 = 500;

/// Who performed the audited action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum AuditActor {
    /// A human operator acting through a client surface.
    Operator,
    /// The runtime itself, with no direct human or agent trigger.
    System,
    /// An agent lane acting under a delegated policy.
    Agent { agent_id: String },
}

/// One object the audited action touched.
///
/// `kind` is a stable key, not a closed enum: an unknown kind stays valid so a
/// newer producer can link an object this build has never heard of. Only the
/// charset is enforced.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct AuditObjectRef {
    pub kind: String,
    pub id: String,
}

impl AuditObjectRef {
    pub const KIND_LANE: &'static str = "lane";
    pub const KIND_TASK: &'static str = "task";
    pub const KIND_HANDOFF: &'static str = "handoff";
    pub const KIND_REVIEW_REQUEST: &'static str = "review_request";
    pub const KIND_CONTRACT: &'static str = "contract";
    pub const KIND_DEPENDENCY: &'static str = "dependency";
    pub const KIND_MERGE_GATE: &'static str = "merge_gate";
    pub const KIND_CONFLICT: &'static str = "conflict";
    pub const KIND_EVIDENCE: &'static str = "evidence";
    pub const KIND_APPLIED_CHANGE: &'static str = "applied_change";
    pub const KIND_REVERT: &'static str = "revert";
    pub const KIND_PERMISSION: &'static str = "permission";
    pub const KIND_SESSION: &'static str = "session";
    /// A source-control target: the workspace root, or a Lane's worktree named
    /// by that Lane's id. Operator git actions are audited against it
    /// (`runtime.operator_git`, GUI-CORE-020).
    pub const KIND_SOURCE: &'static str = "source";

    pub fn new(kind: impl Into<String>, id: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            id: id.into(),
        }
    }
}

/// Result of the audited action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum AuditOutcome {
    Success,
    Denied,
    Failed,
}

/// One durable, append-only audit fact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditRecord {
    pub audit_id: AuditId,
    pub timestamp: u64,
    pub owner: RuntimeOwner,
    pub actor: AuditActor,
    /// Stable dotted message key, e.g. `handoff.created`, `gate.decided`,
    /// `change.reverted` — never localized prose.
    pub action: String,
    #[serde(default)]
    pub objects: Vec<AuditObjectRef>,
    pub outcome: AuditOutcome,
    /// Bounded, sanitized arguments: stable tokens only, never secret bytes
    /// and never user or model prose.
    #[serde(default)]
    pub args: BTreeMap<String, String>,
}

impl AuditRecord {
    /// Builds a bounded, sanitized audit record or rejects the input.
    ///
    /// Rejection is deliberate: silently stripping a field would let an
    /// emission site believe it recorded a fact it did not record. Emission
    /// sites must go through this constructor, and the store re-validates on
    /// append.
    #[allow(clippy::too_many_arguments)]
    pub fn sanitized(
        audit_id: AuditId,
        timestamp: u64,
        owner: RuntimeOwner,
        actor: AuditActor,
        action: String,
        objects: Vec<AuditObjectRef>,
        outcome: AuditOutcome,
        args: BTreeMap<String, String>,
    ) -> Result<Self, String> {
        validate_audit_identity("audit id", &audit_id)?;
        validate_audit_action(&action)?;
        validate_audit_owner(&owner)?;
        if objects.len() > MAX_AUDIT_OBJECTS {
            return Err(format!(
                "audit objects exceed the {MAX_AUDIT_OBJECTS} entry bound"
            ));
        }
        for object in &objects {
            validate_audit_object_kind(&object.kind)?;
            validate_audit_identity("audit object id", &object.id)?;
        }
        if args.len() > MAX_AUDIT_ARGS {
            return Err(format!(
                "audit arguments exceed the {MAX_AUDIT_ARGS} entry bound"
            ));
        }
        for (key, value) in &args {
            validate_audit_arg_key(key)?;
            validate_audit_arg_value(key, value)?;
        }
        if let AuditActor::Agent { agent_id } = &actor {
            validate_audit_identity("audit agent id", agent_id)?;
        }
        Ok(Self {
            audit_id,
            timestamp,
            owner,
            actor,
            action,
            objects,
            outcome,
            args,
        })
    }

    /// Stable newest-first sort key. `audit_id` breaks ties so two records
    /// written in the same second still paginate deterministically.
    pub fn cursor(&self) -> AuditCursor {
        AuditCursor {
            timestamp: self.timestamp,
            audit_id: self.audit_id.clone(),
        }
    }
}

/// Exclusive newest-first pagination position.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct AuditCursor {
    pub timestamp: u64,
    pub audit_id: String,
}

/// Which actor a query keeps.
///
/// A separate type from [`AuditActor`] because the two answer different
/// questions: the record names one concrete actor, while the filter also has to
/// express "any agent lane" — the operator-facing chip that means "not a human
/// and not the runtime" without naming a lane.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum AuditActorFilter {
    /// Only human operator actions.
    Operator,
    /// Only runtime actions with no direct human or agent trigger.
    System,
    /// Any [`AuditActor::Agent`], whatever its lane.
    AnyAgent,
    /// Exactly one agent lane, matched on the full id — never a prefix.
    Agent { agent_id: String },
}

impl AuditActorFilter {
    /// Whether this filter keeps `actor`.
    ///
    /// Fail-closed on both sides of the `#[non_exhaustive]` boundary: an actor
    /// variant this build cannot classify, and a filter variant it does not
    /// know, both match nothing. A filter that quietly kept records it could
    /// not classify would report them under the wrong actor; the honest answer
    /// for an unclassifiable record is to leave the filter off.
    pub fn matches(&self, actor: &AuditActor) -> bool {
        match (self, actor) {
            (Self::Operator, AuditActor::Operator) => true,
            (Self::System, AuditActor::System) => true,
            (Self::AnyAgent, AuditActor::Agent { .. }) => true,
            (Self::Agent { agent_id }, AuditActor::Agent { agent_id: actual }) => {
                agent_id == actual
            }
            _ => false,
        }
    }
}

/// Read-only audit timeline query. Every filter is an AND.
///
/// Every filter here is applied by Core *before* pagination, so `complete` and
/// `next_before` on the returned [`AuditPage`] describe the filtered timeline.
/// This is the reason the filters are a contract addition rather than client
/// work: a client filtering a page it already holds cannot know whether a
/// matching record sits on a page it never loaded, so its "no records for this
/// actor" is a guess.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct AuditQuery {
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub lane_id: Option<String>,
    #[serde(default)]
    pub object: Option<AuditObjectRef>,
    /// Keeps only records performed by this actor.
    #[serde(default)]
    pub actor: Option<AuditActorFilter>,
    /// Inclusive lower bound on `AuditRecord::timestamp`, unix seconds.
    #[serde(default)]
    pub from: Option<u64>,
    /// Exclusive upper bound on `AuditRecord::timestamp`, unix seconds.
    ///
    /// Half-open with `from` so two adjacent windows tile the timeline without
    /// overlapping or dropping a record on the boundary second.
    #[serde(default)]
    pub until: Option<u64>,
    /// Exclusive upper bound; pagination walks newest to oldest.
    #[serde(default)]
    pub before: Option<AuditCursor>,
    /// Clamped to `1..=MAX_AUDIT_PAGE_SIZE` at query time rather than
    /// rejected, so a malformed client request still gets a well-formed page.
    pub limit: u32,
}

impl AuditQuery {
    pub fn clamped_limit(&self) -> usize {
        self.limit.clamp(1, MAX_AUDIT_PAGE_SIZE) as usize
    }

    /// Rejects a query that cannot mean what it says.
    ///
    /// Unlike `limit`, an inverted time range is not clamped: the empty page it
    /// would produce is indistinguishable from "nothing happened in that
    /// window", and an audit answer must never be readable as evidence of
    /// absence when it is really a malformed request.
    pub fn validate(&self) -> Result<(), String> {
        if let (Some(from), Some(until)) = (self.from, self.until)
            && from > until
        {
            return Err(format!(
                "audit query time range is inverted: from {from} is after until {until}"
            ));
        }
        Ok(())
    }
}

/// One newest-first page of the audit timeline.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct AuditPage {
    /// Newest first.
    pub records: Vec<AuditRecord>,
    /// Cursor to pass as the next query's `before`. `None` when complete.
    #[serde(default)]
    pub next_before: Option<AuditCursor>,
    /// True when no older record matches the query.
    pub complete: bool,
}

/// Safe-identifier charset shared by audit ids and object ids: ASCII
/// alphanumerics plus `-`, `_`, `.`, `:`, with no `..` traversal sequence and
/// no separator at either end.
fn validate_audit_identity(name: &str, value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > MAX_AUDIT_ID_BYTES
        || !value.is_ascii()
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
        || value.contains("..")
        || value.starts_with(['.', ':', '-', '_'])
        || value.ends_with(['.', ':', '-', '_'])
    {
        return Err(format!("invalid {name} `{value}`"));
    }
    Ok(())
}

/// Object kinds are lowercase stable keys. Unknown kinds stay valid so a newer
/// producer keeps forward compatibility; only the charset is closed.
fn validate_audit_object_kind(kind: &str) -> Result<(), String> {
    if kind.is_empty()
        || kind.len() > MAX_AUDIT_OBJECT_KIND_BYTES
        || !kind
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
        || kind.starts_with('_')
        || kind.ends_with('_')
    {
        return Err(format!("invalid audit object kind `{kind}`"));
    }
    Ok(())
}

/// Actions are dotted lowercase message keys such as `gate.decided`. Rejecting
/// spaces and capitals is what keeps localized prose out of the timeline.
fn validate_audit_action(action: &str) -> Result<(), String> {
    if action.is_empty()
        || action.len() > MAX_AUDIT_ACTION_BYTES
        || !action.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'.')
        })
        || action.starts_with(['.', '_'])
        || action.ends_with(['.', '_'])
        || action.contains("..")
        || !action.contains('.')
    {
        return Err(format!("invalid audit action key `{action}`"));
    }
    Ok(())
}

fn validate_audit_arg_key(key: &str) -> Result<(), String> {
    if key.is_empty()
        || key.len() > MAX_AUDIT_ARG_KEY_BYTES
        || !key.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'.')
        })
        || key.starts_with(['.', '_'])
        || key.ends_with(['.', '_'])
    {
        return Err(format!("invalid audit argument key `{key}`"));
    }
    Ok(())
}

fn validate_audit_arg_value(key: &str, value: &str) -> Result<(), String> {
    if value.len() > MAX_AUDIT_ARG_VALUE_BYTES {
        return Err(format!(
            "audit argument `{key}` exceeds {MAX_AUDIT_ARG_VALUE_BYTES} bytes"
        ));
    }
    if value.chars().any(char::is_control) {
        return Err(format!(
            "audit argument `{key}` contains control characters"
        ));
    }
    if value.contains("..") {
        return Err(format!(
            "audit argument `{key}` contains a path traversal sequence"
        ));
    }
    if looks_like_secret(value) {
        return Err(format!("audit argument `{key}` looks like a secret"));
    }
    Ok(())
}

/// Conservative credential-shape guard.
///
/// The audit timeline must never persist secret bytes, and an emission site
/// that accidentally forwards a token should fail loudly instead of writing it
/// to a durable log. This is a deliberate over-rejection: audit arguments are
/// stable tokens, so a value that looks like a credential is a bug either way.
fn looks_like_secret(value: &str) -> bool {
    const SECRET_PREFIXES: [&str; 8] = [
        "sk-",
        "sk_",
        "ghp_",
        "gho_",
        "github_pat_",
        "xoxb-",
        "AKIA",
        "-----BEGIN",
    ];
    const SECRET_MARKERS: [&str; 7] = [
        "api_key=",
        "api-key=",
        "apikey=",
        "access_token=",
        "token=",
        "password=",
        "secret=",
    ];
    if SECRET_PREFIXES
        .iter()
        .any(|prefix| value.starts_with(prefix))
    {
        return true;
    }
    let lowered = value.to_ascii_lowercase();
    if lowered.starts_with("bearer ") {
        return true;
    }
    SECRET_MARKERS.iter().any(|marker| lowered.contains(marker))
}

/// Owner identities are validated leniently: an unset (empty) workspace or
/// project is legal because runtime facts may be recorded before an owner
/// scope is claimed, but any value that is present must use the safe charset.
fn validate_audit_owner(owner: &RuntimeOwner) -> Result<(), String> {
    for (name, value) in [
        ("audit owner workspace id", owner.workspace_id.as_str()),
        ("audit owner project id", owner.project_id.as_str()),
    ] {
        if !value.is_empty() {
            validate_audit_identity(name, value)?;
        }
    }
    for (name, value) in [
        ("audit owner lane id", owner.lane_id.as_deref()),
        ("audit owner session id", owner.session_id.as_deref()),
        ("audit owner task id", owner.task_id.as_deref()),
        ("audit owner turn id", owner.turn_id.as_deref()),
    ] {
        if let Some(value) = value.filter(|value| !value.is_empty()) {
            validate_audit_identity(name, value)?;
        }
    }
    Ok(())
}
