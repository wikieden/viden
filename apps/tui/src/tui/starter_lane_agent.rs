//! Binding an Agent to a starter Lane at creation
//! (`runtime.starter_lane_agent`, GUI-CORE-030).
//!
//! This module is pure: it builds the adapter pick list from
//! `RuntimeViewState.agent_adapters` and turns one pick into the
//! `StarterLaneAgentBinding` the request carries. It sends nothing and starts
//! nothing.
//!
//! Three rules:
//!
//! - **The pick travels with the request.** Before this capability the TUI
//!   created the Lane and then sent `StartAgentSession` itself, which is still
//!   valid; what it could not do was tell Core which Agent the operator chose,
//!   so the Lane existed for a moment with no answer about its session. The
//!   binding is that answer.
//! - **Absence stays the two-step flow.** "No Agent" is a real pick, and a
//!   request without `agent` serializes to exactly the bytes it did before the
//!   field existed.
//! - **An unknown adapter is Core's answer, not this client's refusal.** Core
//!   echoes an `agent_id` absent from `agent_adapters` with a diagnostic and
//!   still creates the Lane; the receipt then says the session was refused.
//!   The pick list is built only from adapters Core published, so this client
//!   never *offers* an unknown one — but it never filters Core's echo either.

use viden_types::StarterLaneAgentBinding;

use super::state::TuiState;

/// The capability that carries the binding on the request, the preview, and
/// the receipt.
pub(super) const STARTER_LANE_AGENT_CAPABILITY: &str = "runtime.starter_lane_agent";

/// One row of the Agent pick offered while creating a Lane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum StarterLaneAgentPick {
    /// The two-step flow: create the Lane, start a session later or not at
    /// all. Always first, so the default pick is the behavior that predates
    /// this capability.
    None,
    /// Bind this adapter, optionally at one of the models it published.
    Adapter {
        agent_id: String,
        model: Option<String>,
    },
}

impl StarterLaneAgentPick {
    /// The binding this pick puts on the request, or `None` for the two-step
    /// flow.
    ///
    /// `task` is left unset: Core defaults it to the preset's Lane summary,
    /// and the composer text the operator typed is the *first turn*, which the
    /// creation flow already submits separately. Copying it into `task` as
    /// well would file one instruction as two.
    pub(super) fn binding(&self) -> Option<StarterLaneAgentBinding> {
        match self {
            Self::None => None,
            Self::Adapter { agent_id, model } => Some(StarterLaneAgentBinding {
                agent_id: agent_id.clone(),
                model: model.clone(),
                task: None,
            }),
        }
    }
}

/// The Agent picks offered for a starter Lane, in Core's own adapter order.
///
/// Every row comes from an adapter Core published. An adapter that publishes
/// models contributes one row per model beside its default row, so choosing a
/// model never means typing one this client could not verify exists.
///
/// Without the capability the list is the two-step flow alone: a binding this
/// Core would drop is worse than no binding, because the operator would
/// believe a session was starting.
pub(super) fn starter_lane_agent_picks(state: &TuiState) -> Vec<StarterLaneAgentPick> {
    let mut picks = vec![StarterLaneAgentPick::None];
    if !state.has_capability(STARTER_LANE_AGENT_CAPABILITY) {
        return picks;
    }
    for adapter in &state.runtime.agent_adapters {
        picks.push(StarterLaneAgentPick::Adapter {
            agent_id: adapter.agent_id.clone(),
            model: None,
        });
        picks.extend(
            adapter
                .models
                .iter()
                .map(|model| StarterLaneAgentPick::Adapter {
                    agent_id: adapter.agent_id.clone(),
                    model: Some(model.clone()),
                }),
        );
    }
    picks
}

/// The rows the Lane-creation Agent step renders.
pub(super) fn starter_lane_agent_rows(state: &TuiState, selected: usize) -> Vec<String> {
    let picks = starter_lane_agent_picks(state);
    let mut rows = vec![super::i18n::text(state, "starter_lane.agent.prompt")];
    if !state.has_capability(STARTER_LANE_AGENT_CAPABILITY) {
        rows.push(super::i18n::translate(
            state,
            "starter_lane.agent.unavailable",
            &[("capability", STARTER_LANE_AGENT_CAPABILITY)],
        ));
    }
    rows.extend(picks.into_iter().enumerate().map(|(index, pick)| {
        let marker = if index == selected { ">" } else { " " };
        match pick {
            StarterLaneAgentPick::None => {
                super::i18n::translate(state, "starter_lane.agent.none", &[("marker", marker)])
            }
            StarterLaneAgentPick::Adapter { agent_id, model } => {
                let startability = state
                    .runtime
                    .agent_adapters
                    .iter()
                    .find(|adapter| adapter.agent_id == agent_id)
                    .map(|adapter| startability_key(adapter.startability))
                    .unwrap_or("acp.status.unavailable");
                super::i18n::translate(
                    state,
                    "starter_lane.agent.row",
                    &[
                        ("marker", marker),
                        ("agent", agent_id.as_str()),
                        (
                            "model",
                            model.as_deref().unwrap_or(&super::i18n::text(
                                state,
                                "starter_lane.agent.default_model",
                            )),
                        ),
                        ("status", &super::i18n::text(state, startability)),
                    ],
                )
            }
        }
    }));
    rows
}

fn startability_key(startability: viden_core::AgentStartability) -> &'static str {
    match startability {
        viden_core::AgentStartability::Ready => "acp.status.ready",
        viden_core::AgentStartability::ProbeRequired => "acp.status.probe_required",
        viden_core::AgentStartability::InstallRequired => "acp.status.installation_required",
        viden_core::AgentStartability::AuthenticationRequired => {
            "acp.status.authentication_required"
        }
        viden_core::AgentStartability::Unavailable => "acp.status.unavailable",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use viden_types::CapabilityId;

    fn state_with_adapters() -> TuiState {
        let mut state = TuiState::default();
        state
            .capabilities
            .insert(CapabilityId(STARTER_LANE_AGENT_CAPABILITY.to_string()));
        state
            .runtime
            .agent_adapters
            .push(viden_core::AgentAdapterView {
                agent_id: "claude-code".to_string(),
                display_name: "Claude Code".to_string(),
                route: viden_core::AgentRoute::Acp,
                source: viden_core::AgentAdapterSource::Registry,
                availability: viden_core::AgentAvailability::Available,
                auth_state: viden_core::AgentAuthState::Ready,
                startability: viden_core::AgentStartability::Ready,
                capabilities: Vec::new(),
                models: vec!["claude-sonnet".to_string()],
                diagnostics: Vec::new(),
            });
        state
    }

    #[test]
    fn the_two_step_flow_is_always_the_first_pick() {
        let state = state_with_adapters();

        assert_eq!(
            starter_lane_agent_picks(&state).first(),
            Some(&StarterLaneAgentPick::None)
        );
        assert_eq!(StarterLaneAgentPick::None.binding(), None);
    }

    #[test]
    fn every_adapter_contributes_a_default_row_and_one_row_per_published_model() {
        let state = state_with_adapters();

        assert_eq!(
            starter_lane_agent_picks(&state),
            vec![
                StarterLaneAgentPick::None,
                StarterLaneAgentPick::Adapter {
                    agent_id: "claude-code".to_string(),
                    model: None,
                },
                StarterLaneAgentPick::Adapter {
                    agent_id: "claude-code".to_string(),
                    model: Some("claude-sonnet".to_string()),
                },
            ]
        );
    }

    #[test]
    fn a_pick_becomes_the_binding_core_carries_and_leaves_task_to_core() {
        assert_eq!(
            StarterLaneAgentPick::Adapter {
                agent_id: "claude-code".to_string(),
                model: Some("claude-sonnet".to_string()),
            }
            .binding(),
            Some(StarterLaneAgentBinding {
                agent_id: "claude-code".to_string(),
                model: Some("claude-sonnet".to_string()),
                task: None,
            })
        );
    }

    #[test]
    fn without_the_capability_only_the_two_step_flow_is_offered_and_the_gap_is_named() {
        let mut state = state_with_adapters();
        state.capabilities.clear();

        assert_eq!(
            starter_lane_agent_picks(&state),
            vec![StarterLaneAgentPick::None]
        );
        let rows = starter_lane_agent_rows(&state, 0).join("\n");
        assert!(rows.contains(STARTER_LANE_AGENT_CAPABILITY), "{rows}");
    }

    #[test]
    fn rows_name_the_adapter_the_model_and_cores_startability() {
        let state = state_with_adapters();

        let rows = starter_lane_agent_rows(&state, 1).join("\n");

        assert!(rows.contains("claude-code"), "{rows}");
        assert!(rows.contains("claude-sonnet"), "{rows}");
        assert!(rows.contains("Ready"), "{rows}");
    }
}
