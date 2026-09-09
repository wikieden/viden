//! Viden's production desktop client boundary.

mod adapter;
mod agent_content;
mod d1;
mod d10;
mod d12;
mod d13;
mod d14;
mod d2;
mod d4;
mod d6;
mod permission;
mod presentation;
mod projection;
mod recent_work;
mod ui_preferences;
mod workspace_files;

use std::sync::Mutex;
use std::thread;
use std::time::Duration;
use std::{env, ffi::OsString, path::Path};

pub use adapter::{D11Intent, D11IntentResult, GuiCoreAdapter, open_local_workspace};
pub use agent_content::{agent_content_data_url, resolve_agent_content_reference};
pub use d1::{
    ComposerControlIntent, D1_OWNER_CAPABILITY, D1_OWNER_CARDINALITY_CODE,
    D1AgentSessionInputProjection, D1AgentSessionProjection, D1ChecklistItemProjection,
    D1CockpitProjection, D1ContentPartProjection, D1ContextDockProjection,
    D1ContextUsageProjection, D1CostUsageProjection, D1CursorProjection, D1Intent, D1IntentResult,
    D1LaneAgentProjection, D1OutcomeProjection, D1ProviderHealthProjection,
    D1RuntimeServiceProjection, D1StarterLaneReceiptProjection, D1StatusbarContextProjection,
    D1StatusbarLaneProjection, D1StatusbarLatencyProjection, D1StatusbarProjection,
    D1StatusbarRequestsProjection, D1StatusbarTokensProjection, D1TopbarSourceProjection,
    D1WorkspaceSourceProjection,
};
pub use d2::{
    D2_CONTRACT_DECIDED_CODE, D2_KIND_CONTRACT, D2_KIND_GATE, D2_KIND_REVIEW,
    D2_REVIEW_FEEDBACK_MAX_CHARS, D2_REVIEW_NO_ACTOR_CODE, D2_REVIEW_SETTLED_CODE,
    D2ActionProjection, D2ContextProjection, D2DecisionsProjection, D2DetailProjection,
    D2EvidenceProjection, D2GroupProjection, D2Intent, D2IntentResult, D2QueueItemProjection,
    D2UnavailableProjection,
};
pub use d4::{
    D4_STARTER_LANE_CAPABILITY, D4ApprovalIntent, D4Intent, D4IntentResult, D4LaneCreateProjection,
    D4LaneRequest, D4Preset,
};
pub use d6::{
    D6ActionProjection, D6ConnectionState, D6Intent, D6IntentResult, D6RecoveryProjection, D6State,
};
pub use d10::{
    D10_EVENT_TICKER_LIMIT, D10AgentProjection, D10EvidenceProjection, D10LaneMonitorProjection,
    D10LaneProjection, D10RunStatsProjection,
};
pub use d12::{
    D12ActionProjection, D12BounceProjection, D12CheckProjection, D12GateDetailProjection,
    D12GateProjection, D12IntegrationGateProjection, D12Intent, D12IntentResult,
    D12RevertProjection, D12ReviewedEvidenceInput, d12_action_code,
};
pub use d13::{
    D13BlockerProjection, D13FleetWorkflowProjection, D13HandoffProjection, D13NodeProjection,
    D13WorkflowProjection,
};
pub use d14::{
    AUDIT_CAPABILITY, D14_AUDIT_PAGE_LIMIT, D14AuditArgProjection, D14AuditObjectProjection,
    D14AuditProjection, D14AuditRowProjection, D14AuditScopeInput, D14AuditScopeProjection,
    D14AuditTimelineProjection, D14RowProjection,
};
pub use permission::{
    PermissionActionProjection, PermissionChoice, PermissionDockProjection, PermissionIntent,
    PermissionIntentResult, PermissionOutcomeProjection, PermissionRequestProjection,
    PermissionTargetProjection,
};

pub use presentation::{
    ComposerAction, ComposerDraft, GuiPreferences, TranscriptRow, TranscriptViewport,
    WorkspaceSelection,
};
pub use projection::{
    D11IntakeProjection, PreferenceDiagnosticProjection, ResolvedPreferencesProjection,
    RuntimeProjection,
};
pub use recent_work::{
    RECENT_WORK_CAPABILITY, RecentProjectProjection, RecentSessionProjection, RecentWorkResult,
};
pub use ui_preferences::{
    PreferenceIntent, PreferenceIntentResult, PreferencePatchInput,
    UI_PREFERENCE_PERSISTENCE_CAPABILITY,
};
pub use workspace_files::{
    WORKSPACE_FILES_CAPABILITY, WORKSPACE_FILES_PAGE_LIMIT, WorkspaceFileRowProjection,
    WorkspaceFilesProjection,
};

struct DesktopState {
    adapter: Mutex<Option<GuiCoreAdapter>>,
}

#[tauri::command]
fn open_workspace(root: String, state: tauri::State<'_, DesktopState>) -> Result<(), String> {
    // Build and connect the replacement before taking the shared slot so a
    // failed folder validation never discards the currently open workspace.
    let replacement = open_local_workspace(&root)?;
    let mut adapter = state
        .adapter
        .lock()
        .map_err(|_| "GUI Core adapter lock is unavailable".to_string())?;
    *adapter = Some(replacement);
    Ok(())
}

#[tauri::command]
fn resolved_preferences(
    state: tauri::State<'_, DesktopState>,
) -> Option<ResolvedPreferencesProjection> {
    state
        .adapter
        .lock()
        .ok()?
        .as_ref()?
        .projection()
        .preferences()
}

/// Whether Core's handshake published `ui.preference_persistence`.
///
/// The Settings panel reads this before it renders: without the capability it
/// shows the read-only unavailable state instead of controls that cannot
/// reach Core. `false` also covers "no adapter is connected", which is the
/// same operator-visible fact.
#[tauri::command]
fn preferences_available(state: tauri::State<'_, DesktopState>) -> bool {
    state
        .adapter
        .lock()
        .ok()
        .and_then(|guard| {
            guard
                .as_ref()
                .map(|adapter| adapter.supports_ui_preference_persistence())
        })
        .unwrap_or(false)
}

/// Sends one personal preference change as `SetUiPreferences`.
///
/// The patch carries only the axes the operator selected. Persistence,
/// permission gating, precedence, and the skin/mode rule all stay in Core; the
/// result confirms only what `UiPreferencesUpdated` reported.
#[tauri::command]
fn preferences_save(
    command_id: String,
    patch: PreferencePatchInput,
    state: tauri::State<'_, DesktopState>,
) -> Result<PreferenceIntentResult, String> {
    state
        .adapter
        .lock()
        .map_err(|_| "GUI Core adapter lock is unavailable".to_string())?
        .as_mut()
        .ok_or_else(|| "Core adapter is not connected".to_string())?
        .send_preference_intent_and_wait(
            &command_id,
            PreferenceIntent::Save { patch },
            Duration::from_millis(250),
        )
}

/// Drains ordered Core events for a preference command still in flight.
///
/// A slow Core can leave the send call without its receipt; the panel keeps
/// waiting through this rather than reporting a persistence result Core has
/// not published.
#[tauri::command]
fn preferences_poll(
    state: tauri::State<'_, DesktopState>,
) -> Result<PreferenceIntentResult, String> {
    state
        .adapter
        .lock()
        .map_err(|_| "GUI Core adapter lock is unavailable".to_string())?
        .as_mut()
        .ok_or_else(|| "Core adapter is not connected".to_string())?
        .poll_preferences(Duration::from_millis(250))
}

/// Sends `ResetUiPreferences`, dropping the user `[ui]` table.
///
/// A confirmed restore reports `persisted: false` while the resolved fallback
/// still renders, exactly as the contract describes.
#[tauri::command]
fn preferences_restore(
    command_id: String,
    state: tauri::State<'_, DesktopState>,
) -> Result<PreferenceIntentResult, String> {
    state
        .adapter
        .lock()
        .map_err(|_| "GUI Core adapter lock is unavailable".to_string())?
        .as_mut()
        .ok_or_else(|| "Core adapter is not connected".to_string())?
        .send_preference_intent_and_wait(
            &command_id,
            PreferenceIntent::Restore,
            Duration::from_millis(250),
        )
}

/// Sends one `QueryRecentWork` and waits briefly for Core's ordered answer.
///
/// The read is bounded by Core, available in Plan mode, and never approves
/// anything. The Welcome "Recent" section and the project picker are its only
/// callers; neither may fall back to scanning the session home itself.
#[tauri::command]
fn query_recent_work(
    command_id: String,
    limit: u16,
    state: tauri::State<'_, DesktopState>,
) -> Result<RecentWorkResult, String> {
    state
        .adapter
        .lock()
        .map_err(|_| "GUI Core adapter lock is unavailable".to_string())?
        .as_mut()
        .ok_or_else(|| "Core adapter is not connected".to_string())?
        .query_recent_work_and_wait(&command_id, limit, Duration::from_millis(250))
}

/// Drains ordered Core events for a recent-work read still in flight.
///
/// A slow Core can leave the send call without its answer; the caller keeps
/// waiting through this rather than rendering an empty inventory.
#[tauri::command]
fn recent_work_poll(state: tauri::State<'_, DesktopState>) -> Result<RecentWorkResult, String> {
    state
        .adapter
        .lock()
        .map_err(|_| "GUI Core adapter lock is unavailable".to_string())?
        .as_mut()
        .ok_or_else(|| "Core adapter is not connected".to_string())?
        .poll_recent_work(Duration::from_millis(250))
}

/// Sends one `QueryWorkspaceFiles` and waits briefly for Core's ordered answer.
///
/// The read is permission-gated by Core, bounded, and available in Plan mode
/// because it mutates nothing. The `~` palette scope is its only caller, and it
/// must never fall back to walking the workspace itself.
#[tauri::command]
fn query_workspace_files(
    command_id: String,
    state: tauri::State<'_, DesktopState>,
) -> Result<WorkspaceFilesProjection, String> {
    state
        .adapter
        .lock()
        .map_err(|_| "GUI Core adapter lock is unavailable".to_string())?
        .as_mut()
        .ok_or_else(|| "Core adapter is not connected".to_string())?
        .query_workspace_files_and_wait(&command_id, Duration::from_millis(250))
}

/// Drains ordered Core events for an inventory read still in flight.
///
/// A slow Core can leave the send call without its answer; the caller keeps
/// waiting through this rather than rendering an empty file list.
#[tauri::command]
fn workspace_files_poll(
    state: tauri::State<'_, DesktopState>,
) -> Result<WorkspaceFilesProjection, String> {
    state
        .adapter
        .lock()
        .map_err(|_| "GUI Core adapter lock is unavailable".to_string())?
        .as_mut()
        .ok_or_else(|| "Core adapter is not connected".to_string())?
        .poll_workspace_files(Duration::from_millis(250))
}

#[tauri::command]
fn d11_intake(state: tauri::State<'_, DesktopState>) -> Option<D11IntakeProjection> {
    state
        .adapter
        .lock()
        .ok()?
        .as_ref()?
        .projection()
        .d11_intake()
}

#[tauri::command]
fn d11_send_intent(
    command_id: String,
    intent: D11Intent,
    state: tauri::State<'_, DesktopState>,
) -> Result<D11IntentResult, String> {
    state
        .adapter
        .lock()
        .map_err(|_| "GUI Core adapter lock is unavailable".to_string())?
        .as_mut()
        .ok_or_else(|| "Core adapter is not connected".to_string())?
        .send_d11_intent_and_wait(&command_id, intent, Duration::from_millis(250))
}

#[tauri::command]
fn d11_poll(state: tauri::State<'_, DesktopState>) -> Result<D11IntentResult, String> {
    state
        .adapter
        .lock()
        .map_err(|_| "GUI Core adapter lock is unavailable".to_string())?
        .as_mut()
        .ok_or_else(|| "Core adapter is not connected".to_string())?
        .poll_d11(Duration::ZERO)
}

#[tauri::command]
fn d4_lane_create(state: tauri::State<'_, DesktopState>) -> Option<D4LaneCreateProjection> {
    state.adapter.lock().ok()?.as_ref()?.d4_lane_create()
}

#[tauri::command]
fn d4_send_intent(
    command_id: String,
    intent: D4Intent,
    state: tauri::State<'_, DesktopState>,
) -> Result<D4IntentResult, String> {
    state
        .adapter
        .lock()
        .map_err(|_| "GUI Core adapter lock is unavailable".to_string())?
        .as_mut()
        .ok_or_else(|| "Core adapter is not connected".to_string())?
        .send_d4_intent_and_wait(&command_id, intent, Duration::from_millis(250))
}

#[tauri::command]
fn d4_poll(state: tauri::State<'_, DesktopState>) -> Result<D4IntentResult, String> {
    state
        .adapter
        .lock()
        .map_err(|_| "GUI Core adapter lock is unavailable".to_string())?
        .as_mut()
        .ok_or_else(|| "Core adapter is not connected".to_string())?
        .poll_d4(Duration::ZERO)
}

#[tauri::command]
fn d1_cockpit(
    selected_lane_id: Option<String>,
    state: tauri::State<'_, DesktopState>,
) -> Option<D1CockpitProjection> {
    state
        .adapter
        .lock()
        .ok()?
        .as_ref()?
        .d1_cockpit(selected_lane_id.as_deref())
}

#[tauri::command]
fn d1_send_intent(
    command_id: String,
    intent: D1Intent,
    state: tauri::State<'_, DesktopState>,
) -> Result<D1IntentResult, String> {
    state
        .adapter
        .lock()
        .map_err(|_| "GUI Core adapter lock is unavailable".to_string())?
        .as_mut()
        .ok_or_else(|| "Core adapter is not connected".to_string())?
        .send_d1_intent_and_wait(&command_id, intent, Duration::from_millis(250))
}

#[tauri::command]
fn d1_poll(
    selected_lane_id: Option<String>,
    wait_for_event: bool,
    state: tauri::State<'_, DesktopState>,
) -> Result<D1IntentResult, String> {
    let mut guard = state
        .adapter
        .lock()
        .map_err(|_| "GUI Core adapter lock is unavailable".to_string())?;
    let adapter = guard
        .as_mut()
        .ok_or_else(|| "Core adapter is not connected".to_string())?;
    let timeout = if wait_for_event {
        Duration::from_millis(250)
    } else {
        Duration::ZERO
    };
    adapter.poll_d1(selected_lane_id.as_deref(), timeout)
}

/// Routes one composer control through the shared adapter boundary and
/// returns the refreshed D1 result, whose projection carries the work mode,
/// permission level, provider/model, and adapter model options Core now
/// publishes.
fn send_composer_control(
    command_id: &str,
    intent: ComposerControlIntent,
    selected_lane_id: Option<&str>,
    state: &tauri::State<'_, DesktopState>,
) -> Result<D1IntentResult, String> {
    state
        .adapter
        .lock()
        .map_err(|_| "GUI Core adapter lock is unavailable".to_string())?
        .as_mut()
        .ok_or_else(|| "Core adapter is not connected".to_string())?
        .send_composer_control_and_wait(
            command_id,
            intent,
            selected_lane_id,
            Duration::from_millis(250),
        )
}

#[tauri::command]
fn set_work_mode(
    command_id: String,
    mode: String,
    selected_lane_id: Option<String>,
    state: tauri::State<'_, DesktopState>,
) -> Result<D1IntentResult, String> {
    send_composer_control(
        &command_id,
        ComposerControlIntent::SetWorkMode { mode },
        selected_lane_id.as_deref(),
        &state,
    )
}

#[tauri::command]
fn set_permission_level(
    command_id: String,
    level: String,
    selected_lane_id: Option<String>,
    state: tauri::State<'_, DesktopState>,
) -> Result<D1IntentResult, String> {
    send_composer_control(
        &command_id,
        ComposerControlIntent::SetPermissionLevel { level },
        selected_lane_id.as_deref(),
        &state,
    )
}

#[tauri::command]
fn select_model(
    command_id: String,
    provider_id: String,
    model: String,
    selected_lane_id: Option<String>,
    state: tauri::State<'_, DesktopState>,
) -> Result<D1IntentResult, String> {
    send_composer_control(
        &command_id,
        ComposerControlIntent::SelectModel { provider_id, model },
        selected_lane_id.as_deref(),
        &state,
    )
}

#[tauri::command]
fn permission_send_intent(
    command_id: String,
    intent: PermissionIntent,
    state: tauri::State<'_, DesktopState>,
) -> Result<PermissionIntentResult, String> {
    state
        .adapter
        .lock()
        .map_err(|_| "GUI Core adapter lock is unavailable".to_string())?
        .as_mut()
        .ok_or_else(|| "Core adapter is not connected".to_string())?
        .send_permission_intent_and_wait(&command_id, intent, Duration::from_millis(250))
}

#[tauri::command]
fn permission_poll(
    state: tauri::State<'_, DesktopState>,
) -> Result<PermissionIntentResult, String> {
    state
        .adapter
        .lock()
        .map_err(|_| "GUI Core adapter lock is unavailable".to_string())?
        .as_mut()
        .ok_or_else(|| "Core adapter is not connected".to_string())?
        .poll_permission(Duration::ZERO)
}

#[tauri::command]
fn d13_fleet_workflow(
    state: tauri::State<'_, DesktopState>,
) -> Result<Option<D13FleetWorkflowProjection>, String> {
    Ok(state
        .adapter
        .lock()
        .map_err(|_| "GUI Core adapter lock is unavailable".to_string())?
        .as_ref()
        .ok_or_else(|| "Core adapter is not connected".to_string())?
        .d13_fleet_workflow())
}

#[tauri::command]
fn d14_audit_timeline(
    after: Option<String>,
    limit: u32,
    state: tauri::State<'_, DesktopState>,
) -> Result<D14AuditTimelineProjection, String> {
    state
        .adapter
        .lock()
        .map_err(|_| "GUI Core adapter lock is unavailable".to_string())?
        .as_mut()
        .ok_or_else(|| "Core adapter is not connected".to_string())?
        .d14_audit_timeline(after.as_deref(), limit)
}

/// Reads the newest page of the Core audit timeline, optionally object-scoped.
#[tauri::command]
fn d14_audit_query(
    command_id: String,
    scope: Option<D14AuditScopeInput>,
    state: tauri::State<'_, DesktopState>,
) -> Result<D14AuditProjection, String> {
    state
        .adapter
        .lock()
        .map_err(|_| "GUI Core adapter lock is unavailable".to_string())?
        .as_mut()
        .ok_or_else(|| "Core adapter is not connected".to_string())?
        .query_audit_and_wait(&command_id, scope, Duration::from_millis(250))
}

/// Reads the page older than the cursor Core handed back.
#[tauri::command]
fn d14_audit_load_older(
    command_id: String,
    state: tauri::State<'_, DesktopState>,
) -> Result<D14AuditProjection, String> {
    state
        .adapter
        .lock()
        .map_err(|_| "GUI Core adapter lock is unavailable".to_string())?
        .as_mut()
        .ok_or_else(|| "Core adapter is not connected".to_string())?
        .load_older_audit_and_wait(&command_id, Duration::from_millis(250))
}

/// Drains ordered Core events while an audit read is still pending.
#[tauri::command]
fn d14_audit_poll(state: tauri::State<'_, DesktopState>) -> Result<D14AuditProjection, String> {
    state
        .adapter
        .lock()
        .map_err(|_| "GUI Core adapter lock is unavailable".to_string())?
        .as_mut()
        .ok_or_else(|| "Core adapter is not connected".to_string())?
        .poll_audit(Duration::from_millis(250))
}

/// Reads the Agent content Core persisted for a message part.
///
/// The webview cannot load a workspace path, so the shell reads the file Core
/// wrote and returns an inline data URL. The workspace root comes from Core's
/// own project probe, never from the caller.
#[tauri::command]
fn agent_content(
    reference: String,
    state: tauri::State<'_, DesktopState>,
) -> Result<String, String> {
    let root = state
        .adapter
        .lock()
        .map_err(|_| "GUI Core adapter lock is unavailable".to_string())?
        .as_ref()
        .ok_or_else(|| "Core adapter is not connected".to_string())?
        .workspace_root()
        .ok_or_else(|| "gui.agentContent.noWorkspace".to_string())?;
    agent_content::agent_content_data_url(Path::new(&root), &reference)
}

/// Reads the D10 event ticker from the Core audit timeline.
///
/// GUI-CORE-014: one bounded newest-first page over the whole workspace,
/// ordered by Core across projects. The screen never rebuilds a timeline by
/// diffing successive snapshots.
#[tauri::command]
fn d10_events(
    command_id: String,
    state: tauri::State<'_, DesktopState>,
) -> Result<D14AuditProjection, String> {
    state
        .adapter
        .lock()
        .map_err(|_| "GUI Core adapter lock is unavailable".to_string())?
        .as_mut()
        .ok_or_else(|| "Core adapter is not connected".to_string())?
        .d10_events_and_wait(&command_id, Duration::from_millis(250))
}

/// Drains ordered Core events for a D10 ticker read still in flight.
#[tauri::command]
fn d10_events_poll(state: tauri::State<'_, DesktopState>) -> Result<D14AuditProjection, String> {
    state
        .adapter
        .lock()
        .map_err(|_| "GUI Core adapter lock is unavailable".to_string())?
        .as_mut()
        .ok_or_else(|| "Core adapter is not connected".to_string())?
        .poll_audit(Duration::from_millis(250))
}

#[tauri::command]
fn d12_integration_gate(
    selected_gate_id: Option<String>,
    state: tauri::State<'_, DesktopState>,
) -> Result<Option<D12IntegrationGateProjection>, String> {
    let guard = state
        .adapter
        .lock()
        .map_err(|_| "GUI Core adapter lock is unavailable".to_string())?;
    let adapter = guard
        .as_ref()
        .ok_or_else(|| "Core adapter is not connected".to_string())?;
    Ok(match selected_gate_id {
        Some(gate_id) => adapter.d12_integration_gate_for(&gate_id),
        None => adapter.d12_integration_gate(),
    })
}

/// Sends one merge-gate decision (`AcceptMergeGate` / `RejectMergeGate`) and
/// waits briefly for the ordered Core receipt.
///
/// The gate is re-resolved against the current Core view before the command
/// leaves the host, so a decision on a gate that vanished or closed fails here
/// rather than becoming a command Core has to reject.
#[tauri::command]
fn d12_send_intent(
    command_id: String,
    intent: D12Intent,
    state: tauri::State<'_, DesktopState>,
) -> Result<D12IntentResult, String> {
    state
        .adapter
        .lock()
        .map_err(|_| "GUI Core adapter lock is unavailable".to_string())?
        .as_mut()
        .ok_or_else(|| "Core adapter is not connected".to_string())?
        .send_d12_intent_and_wait(&command_id, intent, Duration::from_millis(250))
}

#[tauri::command]
fn d10_lane_monitor(
    state: tauri::State<'_, DesktopState>,
) -> Result<Option<D10LaneMonitorProjection>, String> {
    Ok(state
        .adapter
        .lock()
        .map_err(|_| "GUI Core adapter lock is unavailable".to_string())?
        .as_ref()
        .ok_or_else(|| "Core adapter is not connected".to_string())?
        .d10_lane_monitor())
}

#[tauri::command]
fn d2_decisions(
    selected_id: Option<String>,
    state: tauri::State<'_, DesktopState>,
) -> Result<Option<D2DecisionsProjection>, String> {
    let guard = state
        .adapter
        .lock()
        .map_err(|_| "GUI Core adapter lock is unavailable".to_string())?;
    let adapter = guard
        .as_ref()
        .ok_or_else(|| "Core adapter is not connected".to_string())?;
    Ok(match selected_id {
        Some(id) => adapter.d2_decisions_for(&id),
        None => adapter.d2_decisions(),
    })
}

#[tauri::command]
fn d2_send_intent(
    command_id: String,
    intent: D2Intent,
    state: tauri::State<'_, DesktopState>,
) -> Result<D2IntentResult, String> {
    state
        .adapter
        .lock()
        .map_err(|_| "GUI Core adapter lock is unavailable".to_string())?
        .as_mut()
        .ok_or_else(|| "Core adapter is not connected".to_string())?
        // A review verdict settles on an ordered `ReviewRequestUpdated`, so the
        // command waits the same brief budget the merge-gate decision does.
        .d2_send_intent_and_wait(&command_id, intent, Duration::from_millis(250))
}

#[tauri::command]
fn d6_recover(state: tauri::State<'_, DesktopState>) -> Result<D6RecoveryProjection, String> {
    let mut guard = state
        .adapter
        .lock()
        .map_err(|_| "GUI Core adapter lock is unavailable".to_string())?;
    let adapter = guard
        .as_mut()
        .ok_or_else(|| "Core adapter is not connected".to_string())?;
    adapter.recover().map_err(|error| error.to_string())?;
    Ok(adapter.d6_recovery())
}

#[tauri::command]
fn d6_send_intent(
    command_id: String,
    intent: D6Intent,
    state: tauri::State<'_, DesktopState>,
) -> Result<D6IntentResult, String> {
    state
        .adapter
        .lock()
        .map_err(|_| "GUI Core adapter lock is unavailable".to_string())?
        .as_mut()
        .ok_or_else(|| "Core adapter is not connected".to_string())?
        .send_d6_intent_and_wait(&command_id, intent, Duration::from_millis(250))
}

pub fn run() {
    install_desktop_command_path();
    let adapter = match adapter::default_local_adapter() {
        Ok(adapter) => adapter,
        Err(error) => {
            eprintln!("Viden Core workspace bootstrap failed: {error}");
            None
        }
    };
    run_with_adapter(adapter);
}

pub fn run_with_adapter(adapter: Option<GuiCoreAdapter>) {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(DesktopState {
            adapter: Mutex::new(adapter),
        })
        .invoke_handler(tauri::generate_handler![
            open_workspace,
            resolved_preferences,
            preferences_available,
            preferences_save,
            preferences_restore,
            preferences_poll,
            query_recent_work,
            recent_work_poll,
            query_workspace_files,
            workspace_files_poll,
            d11_intake,
            d11_send_intent,
            d11_poll,
            d4_lane_create,
            d4_send_intent,
            d4_poll,
            d1_cockpit,
            d1_send_intent,
            d1_poll,
            set_work_mode,
            set_permission_level,
            select_model,
            permission_send_intent,
            permission_poll,
            d2_decisions,
            d2_send_intent,
            d10_lane_monitor,
            d10_events,
            d10_events_poll,
            d12_integration_gate,
            d12_send_intent,
            d13_fleet_workflow,
            d14_audit_timeline,
            d14_audit_query,
            d14_audit_load_older,
            d14_audit_poll,
            agent_content,
            d6_recover,
            d6_send_intent
        ])
        .setup(|app| {
            spawn_core_event_pump(app.handle().clone());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("failed to run the Viden desktop client");
}

/// Name of the ordered-event wake the frontend listens for.
///
/// The payload carries nothing: a wake means "Core advanced, re-read the
/// projection you care about". Screens stay the only readers of their own
/// projection, so the pump never has to know which screen is mounted.
pub const CORE_EVENT_WAKE: &str = "viden://core-advanced";

/// Drains ordered Core events off the UI thread and wakes the frontend.
///
/// The adapter lock is held only for the bounded drain, never across an emit,
/// so a command issued from the UI thread cannot be blocked behind a wake.
fn spawn_core_event_pump(app: tauri::AppHandle) {
    thread::spawn(move || {
        loop {
            let advanced = {
                use tauri::Manager;
                let state = app.state::<DesktopState>();
                let Ok(mut guard) = state.adapter.lock() else {
                    break;
                };
                match guard.as_mut() {
                    Some(adapter) => adapter.pump_events(Duration::from_millis(250)),
                    // No workspace is bound yet; wait for one without burning
                    // the CPU on an empty lock.
                    None => {
                        drop(guard);
                        thread::sleep(Duration::from_millis(250));
                        continue;
                    }
                }
            };
            if advanced {
                use tauri::Emitter;
                let _ = app.emit(CORE_EVENT_WAKE, ());
            }
        }
    });
}

fn install_desktop_command_path() {
    let current = env::var_os("PATH");
    let home = env::var_os("HOME").map(std::path::PathBuf::from);
    let path = desktop_command_path(current.clone(), home.as_deref());
    if Some(&path) != current.as_ref() {
        // Startup is still single-threaded here. Core discovery and every ACP
        // child spawned afterward must observe the same resolved command path.
        unsafe { env::set_var("PATH", path) };
    }
}

fn desktop_command_path(current: Option<OsString>, home: Option<&Path>) -> OsString {
    let mut entries = Vec::new();
    if let Some(home) = home {
        for relative in [
            ".local/bin",
            ".bun/bin",
            ".volta/bin",
            ".asdf/shims",
            ".local/share/mise/shims",
            ".local/share/fnm/aliases/default/bin",
        ] {
            let candidate = home.join(relative);
            if candidate.is_dir() && !entries.contains(&candidate) {
                entries.push(candidate);
            }
        }
    }
    #[cfg(target_os = "macos")]
    for candidate in [Path::new("/opt/homebrew/bin"), Path::new("/usr/local/bin")] {
        if candidate.is_dir() && !entries.iter().any(|entry| entry == candidate) {
            entries.push(candidate.to_path_buf());
        }
    }
    if let Some(current) = current.as_deref() {
        for entry in env::split_paths(current) {
            if !entries.contains(&entry) {
                entries.push(entry);
            }
        }
    }
    env::join_paths(entries).unwrap_or_else(|_| current.unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use std::{env, fs};

    use super::desktop_command_path;

    #[test]
    fn desktop_command_path_recovers_user_local_bin_from_restricted_path() {
        let home = env::temp_dir().join(format!("viden-gui-desktop-path-{}", std::process::id()));
        let user_bin = home.join(".local/bin");
        fs::create_dir_all(&user_bin).expect("create user-local bin fixture");

        let path = desktop_command_path(Some("/usr/bin:/bin".into()), Some(&home));
        let entries = env::split_paths(&path).collect::<Vec<_>>();

        assert_eq!(entries.first(), Some(&user_bin));
        assert!(entries.contains(&"/usr/bin".into()));
        assert!(entries.contains(&"/bin".into()));
        fs::remove_dir_all(home).expect("remove desktop path fixture");
    }
}
