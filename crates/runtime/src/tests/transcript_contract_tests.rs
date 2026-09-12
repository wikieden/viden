//! Contract: model-visible means logged.
//!
//! Everything the provider request derives from the in-memory message history
//! must be reconstructible from the append-only JSONL transcript alone. These
//! tests run a live turn (including a tool call), rebuild the session from the
//! persisted transcript, and require the model-visible projection of both
//! histories to be identical. Message ids and timestamps are runtime-local and
//! intentionally excluded: providers never see them.

use std::collections::BTreeMap;

use viden_types::{ApprovalResponse, Message, ModelEvent, Role, ToolCall};

use crate::{RuntimeResumeRequest, SessionEngine};

use super::{SequenceProvider, temp_dir};

/// The fields of a message a provider adapter is allowed to render into a
/// model request. Anything outside this tuple must never influence what the
/// model sees.
fn model_visible_projection(
    messages: &[Message],
) -> Vec<(Role, String, Option<String>, Option<String>)> {
    messages
        .iter()
        .map(|message| {
            (
                message.role.clone(),
                message.content.clone(),
                message.tool_name.clone(),
                message.tool_call_id.clone(),
            )
        })
        .collect()
}

#[test]
fn model_visible_messages_are_reconstructible_from_transcript() {
    let home = temp_dir("log_contract_home");
    let cwd = temp_dir("log_contract_cwd");
    std::fs::write(cwd.join("note.txt"), "hello contract\n").unwrap();
    let mut approver = |_prompt| ApprovalResponse::allow_once(None);

    let mut glob_input = BTreeMap::new();
    glob_input.insert("pattern".to_string(), "*.txt".to_string());
    let provider = Box::new(SequenceProvider::new(vec![
        vec![ModelEvent::ToolCall(ToolCall {
            id: "call_contract_1".to_string(),
            name: "glob".to_string(),
            input: glob_input,
        })],
        vec![ModelEvent::AssistantText {
            content: "found the note".to_string(),
        }],
    ]));
    let mut live_engine = SessionEngine::new_with_home(&cwd, provider, Some(home.clone())).unwrap();
    let session_id = live_engine.session_id().to_string();
    live_engine
        .process_input_with_approval("list text files", &mut approver)
        .unwrap();
    let live = model_visible_projection(&live_engine.messages);

    // The live turn must have produced the full shape under test: user input,
    // assistant tool call, tool result, and final assistant text.
    assert!(
        live.iter().any(|(role, _, _, tool_call_id)| {
            *role == Role::Assistant && tool_call_id.as_deref() == Some("call_contract_1")
        }),
        "expected an assistant tool-call message in the live history: {live:?}"
    );
    assert!(
        live.iter().any(|(role, _, _, tool_call_id)| {
            *role == Role::Tool && tool_call_id.as_deref() == Some("call_contract_1")
        }),
        "expected a tool result message in the live history: {live:?}"
    );
    assert!(
        live.iter()
            .any(|(role, content, _, _)| *role == Role::Assistant && content == "found the note"),
        "expected the final assistant reply in the live history: {live:?}"
    );

    let rebuilt_provider = Box::new(SequenceProvider::new(vec![]));
    let mut rebuilt_engine =
        SessionEngine::new_with_home(&cwd, rebuilt_provider, Some(home)).unwrap();
    rebuilt_engine
        .resume_session(RuntimeResumeRequest::exact_session_id(session_id))
        .unwrap();
    let rebuilt = model_visible_projection(&rebuilt_engine.messages);

    assert_eq!(
        live, rebuilt,
        "model-visible history rebuilt from the JSONL transcript must match the live history"
    );
}

#[test]
fn denied_tool_call_history_is_reconstructible_from_transcript() {
    let home = temp_dir("log_contract_deny_home");
    let cwd = temp_dir("log_contract_deny_cwd");
    // Denials inject synthetic tool results and system messages; those are
    // model-visible on the next request and must survive the log round trip.
    let mut approver = |_prompt| ApprovalResponse::deny(Some("not now".to_string()));

    let mut shell_input = BTreeMap::new();
    shell_input.insert("command".to_string(), "rm -rf target".to_string());
    let provider = Box::new(SequenceProvider::new(vec![
        vec![ModelEvent::ToolCall(ToolCall {
            id: "call_contract_denied".to_string(),
            name: "shell".to_string(),
            input: shell_input,
        })],
        vec![ModelEvent::AssistantText {
            content: "understood, stopping".to_string(),
        }],
    ]));
    let mut live_engine = SessionEngine::new_with_home(&cwd, provider, Some(home.clone())).unwrap();
    let session_id = live_engine.session_id().to_string();
    live_engine
        .process_input_with_approval("clean the build directory", &mut approver)
        .unwrap();
    let live = model_visible_projection(&live_engine.messages);
    assert!(
        live.iter().any(|(role, _, _, tool_call_id)| {
            *role == Role::Tool && tool_call_id.as_deref() == Some("call_contract_denied")
        }),
        "expected the synthetic denial tool result in the live history: {live:?}"
    );

    let rebuilt_provider = Box::new(SequenceProvider::new(vec![]));
    let mut rebuilt_engine =
        SessionEngine::new_with_home(&cwd, rebuilt_provider, Some(home)).unwrap();
    rebuilt_engine
        .resume_session(RuntimeResumeRequest::exact_session_id(session_id))
        .unwrap();
    let rebuilt = model_visible_projection(&rebuilt_engine.messages);

    assert_eq!(
        live, rebuilt,
        "model-visible history after a denial must match after a transcript rebuild"
    );
}

/// Compatibility follow-up 12, reader one of three: resume rebuilds the
/// model-visible history from the JSONL alone, so a non-ASCII prompt and reply
/// must come back exactly as they were written. Latin-1 decoding replayed them
/// as mojibake, which a provider would then see on the next request.
#[test]
fn non_ascii_transcript_text_survives_a_session_rebuild() {
    let home = temp_dir("log_contract_utf8_home");
    let cwd = temp_dir("log_contract_utf8_cwd");
    let prompt = "请用三种文字回答: café, 你好, ✓";
    let reply = "café 你好 — ✓ 🚀";
    let provider = Box::new(SequenceProvider::new(vec![vec![
        ModelEvent::AssistantText {
            content: reply.to_string(),
        },
    ]]));
    let mut approver = |_prompt| ApprovalResponse::allow_once(None);
    let mut live_engine = SessionEngine::new_with_home(&cwd, provider, Some(home.clone())).unwrap();
    let session_id = live_engine.session_id().to_string();
    live_engine
        .process_input_with_approval(prompt, &mut approver)
        .unwrap();
    let live = model_visible_projection(&live_engine.messages);

    let rebuilt_provider = Box::new(SequenceProvider::new(vec![]));
    let mut rebuilt_engine =
        SessionEngine::new_with_home(&cwd, rebuilt_provider, Some(home)).unwrap();
    rebuilt_engine
        .resume_session(RuntimeResumeRequest::exact_session_id(session_id))
        .unwrap();
    let rebuilt = model_visible_projection(&rebuilt_engine.messages);

    assert_eq!(
        live, rebuilt,
        "a non-ASCII history rebuilt from the JSONL must match the live history"
    );
    assert!(
        rebuilt
            .iter()
            .any(|(role, content, _, _)| *role == Role::Assistant && content == reply),
        "the resumed assistant reply must be byte-equal to the persisted one: {rebuilt:?}"
    );
    assert!(
        rebuilt
            .iter()
            .any(|(role, content, _, _)| *role == Role::User && content == prompt),
        "the resumed prompt must be byte-equal to the persisted one: {rebuilt:?}"
    );
}

#[test]
fn resume_surfaces_quarantined_transcript_lines_as_a_system_fact() {
    let home = temp_dir("log_contract_quarantine_home");
    let cwd = temp_dir("log_contract_quarantine_cwd");
    let provider = Box::new(SequenceProvider::new(vec![vec![
        ModelEvent::AssistantText {
            content: "logged reply".to_string(),
        },
    ]]));
    let mut approver = |_prompt| ApprovalResponse::allow_once(None);
    let mut live_engine = SessionEngine::new_with_home(&cwd, provider, Some(home.clone())).unwrap();
    let session_id = live_engine.session_id().to_string();
    live_engine
        .process_input_with_approval("say something", &mut approver)
        .unwrap();

    // A newer build appended an event this build cannot reduce. Replay must
    // keep the readable history and report the skipped line, not fail.
    let transcript_path = live_engine.store.transcript_path().to_path_buf();
    let mut contents = std::fs::read_to_string(&transcript_path).unwrap();
    contents.push_str(
        "{\"type\":\"runtime_event\",\"event\":{\"sequence\":9001,\"timestamp\":1,\"kind\":{\"type\":\"not_yet_invented\",\"payload\":{}}}}\n",
    );
    std::fs::write(&transcript_path, contents).unwrap();

    let rebuilt_provider = Box::new(SequenceProvider::new(vec![]));
    let mut rebuilt_engine =
        SessionEngine::new_with_home(&cwd, rebuilt_provider, Some(home)).unwrap();
    rebuilt_engine
        .resume_session(RuntimeResumeRequest::exact_session_id(session_id))
        .unwrap();

    assert!(
        rebuilt_engine.messages.iter().any(|message| {
            message.role == Role::System && message.content.contains("quarantined")
        }),
        "resume must surface quarantined transcript lines: {:?}",
        rebuilt_engine
            .messages
            .iter()
            .map(|message| (message.role.clone(), message.content.clone()))
            .collect::<Vec<_>>()
    );
    assert!(
        rebuilt_engine
            .messages
            .iter()
            .any(|message| message.content == "say something"),
        "the readable history before the quarantined line must still load"
    );
}
