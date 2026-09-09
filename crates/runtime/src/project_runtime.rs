use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use sha2::{Digest, Sha256};
use viden_config::parse_project_config;
use viden_permissions::PermissionEngine;
use viden_types::{
    ApprovalResponse, CredentialHandle, PermissionDecision, PermissionPrompt, ProjectConfigPreview,
    ProjectConfigState, ProjectProbe, ProviderHealthView, RuntimeCommand, RuntimeEvent,
    RuntimeEventKind, ToolInput, ToolSpec, fresh_id,
};

use crate::{FileRollback, SessionEngine};
use viden_lanes::workspace_eligibility;

pub trait CredentialBackend: Send + Sync {
    /// Resolves a one-use opaque request already staged by the trusted backend.
    /// Secret bytes never cross this serialized runtime boundary.
    fn store(
        &self,
        provider_id: &str,
        backend_id: &str,
        credential_request_id: &str,
    ) -> Result<CredentialHandle, String>;
}

pub(crate) struct UnavailableCredentialBackend;

impl CredentialBackend for UnavailableCredentialBackend {
    fn store(
        &self,
        _provider_id: &str,
        _backend_id: &str,
        _credential_request_id: &str,
    ) -> Result<CredentialHandle, String> {
        Err("credential backend is not configured".to_string())
    }
}

#[derive(Debug, Clone)]
pub(crate) struct PendingProjectConfig {
    pub(crate) preview: ProjectConfigPreview,
    pub(crate) bytes: Vec<u8>,
}

pub(crate) enum SupervisorProjectMutationPreparation {
    Ready,
    Pending(PermissionPrompt),
}

impl SessionEngine {
    pub fn with_credential_backend(mut self, backend: Arc<dyn CredentialBackend>) -> Self {
        self.credential_backend = backend;
        self
    }

    pub fn preview_project_config(
        &mut self,
        contents: String,
    ) -> Result<ProjectConfigPreview, String> {
        let reviewed_contents = contents.clone();
        let bytes = contents.into_bytes();
        let parsed = parse_project_config(&bytes);
        let (project_name, pack, exact_contents, diagnostics) = match parsed {
            Ok(config) => (
                Some(config.project_name),
                Some(config.pack),
                Some(reviewed_contents),
                Vec::new(),
            ),
            Err(error) => (None, None, None, vec![error]),
        };
        let path = self.cwd.join("viden.toml");
        let base_content_sha256 = match fs::read(&path) {
            Ok(contents) => Some(content_sha256(&contents)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(format!("failed to read {}: {error}", path.display())),
        };
        let preview = ProjectConfigPreview {
            preview_id: fresh_id("project-preview"),
            relative_path: "viden.toml".to_string(),
            content_sha256: content_sha256(&bytes),
            byte_len: bytes.len() as u64,
            exact_contents,
            base_content_sha256,
            project_name,
            pack,
            diagnostics,
        };
        if preview.is_valid() {
            self.pending_project_previews.insert(
                preview.preview_id.clone(),
                PendingProjectConfig {
                    preview: preview.clone(),
                    bytes,
                },
            );
        }
        Ok(preview)
    }

    pub(crate) fn project_probe_events(&self) -> Vec<RuntimeEvent> {
        let path = self.cwd.join("viden.toml");
        let (config_state, project_name, pack, diagnostics) = match fs::read(&path) {
            Ok(contents) => match parse_project_config(&contents) {
                Ok(config) => (
                    ProjectConfigState::Valid,
                    Some(config.project_name),
                    Some(config.pack),
                    Vec::new(),
                ),
                Err(error) => (ProjectConfigState::Invalid, None, None, vec![error]),
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                (ProjectConfigState::Missing, None, None, Vec::new())
            }
            Err(error) => (
                ProjectConfigState::Invalid,
                None,
                None,
                vec![format!("failed to read {}: {error}", path.display())],
            ),
        };
        let git_root = git_root(&self.cwd);
        let probe = ProjectProbe {
            root: self.cwd.display().to_string(),
            is_git_repository: git_root.is_some(),
            git_root: git_root.map(|root| root.display().to_string()),
            config_path: path.display().to_string(),
            config_state,
            project_name,
            pack,
            diagnostics,
        };
        vec![
            RuntimeEvent::new(1, RuntimeEventKind::ProjectProbed { probe }),
            RuntimeEvent::new(
                2,
                RuntimeEventKind::WorkspaceEligibilityUpdated {
                    eligibility: workspace_eligibility(&self.cwd),
                },
            ),
            RuntimeEvent::new(
                3,
                RuntimeEventKind::ProviderHealthUpdated {
                    provider: self.project_provider_health(),
                },
            ),
        ]
    }

    pub(crate) fn preview_project_config_events(
        &mut self,
        contents: String,
    ) -> Result<Vec<RuntimeEvent>, String> {
        let preview = self.preview_project_config(contents)?;
        Ok(vec![RuntimeEvent::new(
            1,
            RuntimeEventKind::ProjectConfigPreviewed { preview },
        )])
    }

    pub(crate) fn confirm_project_config<F>(
        &mut self,
        preview_id: &str,
        expected_sha256: &str,
        approver: &mut F,
    ) -> Result<Vec<RuntimeEvent>, String>
    where
        F: FnMut(viden_types::PermissionPrompt) -> ApprovalResponse,
    {
        let pending = self
            .pending_project_previews
            .get(preview_id)
            .cloned()
            .ok_or_else(|| {
                "project config preview is missing or no longer confirmable".to_string()
            })?;
        if pending.preview.content_sha256 != expected_sha256
            || content_sha256(&pending.bytes) != expected_sha256
        {
            return Err("project config preview hash mismatch".to_string());
        }
        if !pending.preview.is_valid() || parse_project_config(&pending.bytes).is_err() {
            return Err("project config preview is invalid".to_string());
        }
        let permission_preview = format!("viden.toml sha256={expected_sha256}");
        if let Some(denial) = self.ensure_workflow_permission(
            "project_config_confirm",
            &permission_preview,
            approver,
        )? {
            return Err(denial);
        }

        let path = self.cwd.join("viden.toml");
        let current_hash = match fs::read(&path) {
            Ok(contents) => Some(content_sha256(&contents)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(format!("failed to read {}: {error}", path.display())),
        };
        if current_hash != pending.preview.base_content_sha256 {
            return Err("project config changed after preview".to_string());
        }
        self.stage_project_config_rollback(&path)?;
        write_exact_project_config(&path, &pending.bytes)?;
        self.pending_project_previews.remove(preview_id);
        self.confirmed_project_config = Some(pending.preview.clone());
        Ok(vec![RuntimeEvent::new(
            1,
            RuntimeEventKind::ProjectConfigConfirmed {
                preview: pending.preview,
            },
        )])
    }

    pub(crate) fn prepare_project_mutation_for_supervisor(
        &self,
        envelope_owner: &viden_types::RuntimeOwner,
        command: &RuntimeCommand,
    ) -> Result<SupervisorProjectMutationPreparation, String> {
        if let Some(command_actor) = supervised_command_actor(command)
            && command_actor != envelope_owner
        {
            return Err("runtime command actor does not match envelope owner".to_string());
        }
        let (action, preview) = match command {
            RuntimeCommand::ConfirmProjectConfig {
                preview_id,
                content_sha256,
            } => {
                self.validate_project_config_confirmation(preview_id, content_sha256)?;
                (
                    "project_config_confirm",
                    format!("viden.toml sha256={content_sha256}"),
                )
            }
            RuntimeCommand::StoreCredentialHandle {
                provider_id,
                backend_id,
                credential_request_id,
            } => {
                validate_safe_identifier("provider_id", provider_id)?;
                validate_safe_identifier("backend_id", backend_id)?;
                validate_safe_identifier("credential_request_id", credential_request_id)?;
                (
                    "credential_handle_store",
                    format!("provider={provider_id} backend={backend_id}"),
                )
            }
            _ => match self.ui_preference_mutation_descriptor(command)? {
                Some(descriptor) => descriptor,
                None => self
                    .trust_mutation_permission_descriptor(command)?
                    .ok_or_else(|| "command is not a supervised runtime mutation".to_string())?,
            },
        };
        let tool_name = format!("workflow_{action}");
        let tool = ToolSpec {
            name: tool_name.clone(),
            description: format!("Workflow mutation: {action}"),
            is_mutating: true,
            input_schema_hint: "workflow action".to_string(),
        };
        let mut input = ToolInput::new();
        input.insert("action".to_string(), action.to_string());
        input.insert("preview".to_string(), preview);
        match self.permissions.decide(&tool, &input) {
            PermissionDecision::Allow(_) => Ok(SupervisorProjectMutationPreparation::Ready),
            PermissionDecision::Ask(ask) => {
                let mut prompt = PermissionEngine::prompt_for(&tool_name, &ask, &input);
                // The supervised half of the same gate the in-process path
                // takes: a patch merge is the one supervised mutation whose
                // proposal Core already holds as bytes, so the operator sees
                // the multi-file change rather than a gate id
                // (`runtime.structured_diff`, GUI-CORE-012).
                if let RuntimeCommand::MergeAgentPatch { gate_id, actor, .. } = command
                    && let Ok(patch) = self.preflight_merge_agent_patch(gate_id, actor)
                {
                    prompt.decision_context =
                        Some(crate::decision_context::patch_decision_context(&patch));
                }
                Ok(SupervisorProjectMutationPreparation::Pending(prompt))
            }
            PermissionDecision::Deny(deny) => Err(deny.message),
        }
    }

    pub(crate) fn store_credential_handle<F>(
        &mut self,
        provider_id: &str,
        backend_id: &str,
        credential_request_id: &str,
        approver: &mut F,
    ) -> Result<Vec<RuntimeEvent>, String>
    where
        F: FnMut(viden_types::PermissionPrompt) -> ApprovalResponse,
    {
        validate_safe_identifier("provider_id", provider_id)?;
        validate_safe_identifier("backend_id", backend_id)?;
        validate_safe_identifier("credential_request_id", credential_request_id)?;
        let preview = format!("provider={provider_id} backend={backend_id}");
        if let Some(denial) =
            self.ensure_workflow_permission("credential_handle_store", &preview, approver)?
        {
            return Err(denial);
        }
        let handle =
            self.credential_backend
                .store(provider_id, backend_id, credential_request_id)?;
        if handle.provider_id != provider_id || handle.backend_id != backend_id {
            return Err("credential backend returned a mismatched handle".to_string());
        }
        if let Some(existing) = self.credential_handles.iter_mut().find(|existing| {
            existing.provider_id == handle.provider_id && existing.backend_id == handle.backend_id
        }) {
            *existing = handle.clone();
        } else {
            self.credential_handles.push(handle.clone());
        }
        Ok(vec![
            RuntimeEvent::new(
                1,
                RuntimeEventKind::CredentialHandleStored {
                    handle: handle.clone(),
                },
            ),
            RuntimeEvent::new(
                2,
                RuntimeEventKind::ProviderHealthUpdated {
                    provider: self.project_provider_health(),
                },
            ),
        ])
    }

    pub(crate) fn project_provider_health(&self) -> ProviderHealthView {
        let credential = self
            .credential_handles
            .iter()
            .rev()
            .find(|handle| handle.provider_id == self.provider_name())
            .cloned();
        let mut provider = crate::runtime_contract::provider_health_view(
            self.provider_name(),
            self.model_name(),
            &self.provider_telemetry,
        );
        if provider.request_count == 0 && provider.error_count == 0 {
            provider.status = "ready".to_string();
        }
        provider.credential = credential;
        provider
    }

    fn stage_project_config_rollback(&self, path: &Path) -> Result<(), String> {
        let root = fs::canonicalize(&self.cwd)
            .map_err(|error| format!("failed to resolve {}: {error}", self.cwd.display()))?;
        let rollback_path = root.join("viden.toml");
        let (contents, permissions) = match fs::symlink_metadata(&rollback_path) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() || !metadata.is_file() {
                    return Err(format!("unsafe project config target `{}`", path.display()));
                }
                (
                    Some(fs::read(&rollback_path).map_err(|error| {
                        format!("failed to read {}: {error}", rollback_path.display())
                    })?),
                    Some(metadata.permissions()),
                )
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (None, None),
            Err(error) => return Err(format!("failed to inspect {}: {error}", path.display())),
        };
        self.transaction_file_rollback
            .borrow_mut()
            .push(FileRollback {
                root,
                path: rollback_path,
                contents,
                permissions,
                created_parent_dirs: Vec::new(),
            });
        Ok(())
    }

    fn validate_project_config_confirmation(
        &self,
        preview_id: &str,
        expected_sha256: &str,
    ) -> Result<(), String> {
        let pending = self
            .pending_project_previews
            .get(preview_id)
            .ok_or_else(|| {
                "project config preview is missing or no longer confirmable".to_string()
            })?;
        if pending.preview.content_sha256 != expected_sha256
            || content_sha256(&pending.bytes) != expected_sha256
        {
            return Err("project config preview hash mismatch".to_string());
        }
        if !pending.preview.is_valid() || parse_project_config(&pending.bytes).is_err() {
            return Err("project config preview is invalid".to_string());
        }
        Ok(())
    }
}

fn supervised_command_actor(command: &RuntimeCommand) -> Option<&viden_types::RuntimeOwner> {
    match command {
        RuntimeCommand::CreateHandoff { owner, .. }
        | RuntimeCommand::RequestReview { owner, .. }
        | RuntimeCommand::ConfirmContract { owner, .. }
        | RuntimeCommand::SetDependency { owner, .. }
        | RuntimeCommand::BounceMergeConflict { owner, .. }
        | RuntimeCommand::RevertAppliedChange { owner, .. } => Some(owner),
        RuntimeCommand::DecideReview { actor, .. }
        | RuntimeCommand::AcceptMergeGate { actor, .. }
        | RuntimeCommand::AcceptAgentArtifact { actor, .. }
        | RuntimeCommand::RejectMergeGate { actor, .. }
        | RuntimeCommand::RejectAgentArtifact { actor, .. }
        | RuntimeCommand::MergeAgentPatch { actor, .. }
        | RuntimeCommand::RevalidateMergeConflict { actor, .. } => Some(actor),
        _ => None,
    }
}

fn content_sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn validate_safe_identifier(name: &str, value: &str) -> Result<(), String> {
    const MAX_OPAQUE_ID_BYTES: usize = 96;
    const SECRET_MARKERS: &[&str] = &[
        "sk-",
        "sk_",
        "token",
        "api_key",
        "apikey",
        "secret",
        "password",
        "bearer",
        "credential",
        "private_key",
        "access_key",
        "refresh_key",
    ];
    let normalized = value.to_ascii_lowercase();
    let grammar_is_safe = !value.is_empty()
        && value.len() <= MAX_OPAQUE_ID_BYTES
        && value.is_ascii()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
        && value
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphanumeric)
        && value
            .as_bytes()
            .last()
            .is_some_and(u8::is_ascii_alphanumeric)
        && !value.contains("..")
        && !value.contains("::")
        && !(value.len() >= 2
            && value.as_bytes()[0].is_ascii_alphabetic()
            && value.as_bytes()[1] == b':');
    if !grammar_is_safe
        || SECRET_MARKERS
            .iter()
            .any(|marker| normalized.contains(marker))
    {
        return Err(format!("invalid credential {name}"));
    }
    Ok(())
}

fn git_root(cwd: &Path) -> Option<PathBuf> {
    let output = Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let root = String::from_utf8(output.stdout).ok()?;
    let root = root.trim();
    (!root.is_empty()).then(|| PathBuf::from(root))
}

fn write_exact_project_config(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let temp_path = path.with_extension(format!("toml.tmp-{}", fresh_id("write")));
    let result = (|| {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp_path)
            .map_err(|error| format!("failed to create {}: {error}", temp_path.display()))?;
        file.write_all(bytes)
            .and_then(|_| file.sync_all())
            .map_err(|error| format!("failed to write {}: {error}", temp_path.display()))?;
        fs::rename(&temp_path, path).map_err(|error| {
            format!(
                "failed to install project config {}: {error}",
                path.display()
            )
        })
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp_path);
    }
    result
}
