//! Static plugin registry boundary for Viden runtime integrations.
//!
//! Dynamic loading stays in provider-specific code for now. This host crate is
//! the shared place for plugin discovery, validation, and lifecycle contracts as
//! tools, agents, workflows, and providers move behind the plugin API.

use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

const CONTEXT_REDUCER_PROCESS_STDOUT_HARD_CAP_BYTES: usize = 1024 * 1024;
const CONTEXT_REDUCER_PROCESS_STDERR_HARD_CAP_BYTES: usize = 4 * 1024;
const CONTEXT_REDUCER_PROCESS_REQUEST_HARD_CAP_BYTES: usize = 64 * 1024;

use viden_plugin_api::{
    AgentAuthMode, AgentCommandSpec, AgentEnvRef, AgentPermissionProfile, AgentPluginCapability,
    AgentPluginDescriptor, AgentProtocolVersion, AgentRegistryPackage, AgentSource, AgentTransport,
    CONTEXT_REDUCER_SCHEMA_VERSION, ContextReducerAdapterConfig, ContextReducerContentKind,
    ContextReducerDescriptor, ContextReducerHealthMetadata, ContextReducerHealthStatus,
    ContextReducerProcessAuthorization, ContextReducerProcessDescriptor, ContextReducerRequest,
    ContextReducerResponse, PluginKind, PluginManifest,
};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StaticPluginRegistry {
    manifests: Vec<PluginManifest>,
    agent_descriptors: Vec<AgentPluginDescriptor>,
    context_reducers: Vec<ContextReducerDescriptor>,
}

impl StaticPluginRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, manifest: PluginManifest) {
        self.manifests.push(manifest);
    }

    pub fn register_agent(&mut self, descriptor: AgentPluginDescriptor) {
        self.agent_descriptors.push(descriptor);
    }

    pub fn register_context_reducer(
        &mut self,
        mut descriptor: ContextReducerDescriptor,
    ) -> Result<(), PluginHostError> {
        validate_context_reducer_identity(&descriptor)?;
        if let Some(process) = descriptor.process.take() {
            descriptor.process = Some(validate_context_reducer_process(
                &descriptor.reducer_id,
                &descriptor.version,
                process,
            )?);
        }
        if self
            .context_reducers
            .iter()
            .any(|registered| registered.reducer_id == descriptor.reducer_id)
        {
            return Err(PluginHostError::DuplicateContextReducer {
                reducer_id: descriptor.reducer_id,
            });
        }
        self.context_reducers.push(descriptor);
        Ok(())
    }

    pub fn manifests(&self) -> &[PluginManifest] {
        &self.manifests
    }

    pub fn agent_descriptors(&self) -> &[AgentPluginDescriptor] {
        &self.agent_descriptors
    }

    pub fn context_reducers(&self) -> &[ContextReducerDescriptor] {
        &self.context_reducers
    }

    pub fn agent_by_id(&self, id: &str) -> Option<&AgentPluginDescriptor> {
        self.agent_descriptors
            .iter()
            .find(|descriptor| descriptor.agent_id == id)
    }

    pub fn by_kind(&self, kind: PluginKind) -> impl Iterator<Item = &PluginManifest> {
        self.manifests
            .iter()
            .filter(move |manifest| manifest.kind == kind)
    }

    pub fn negotiate_context_reducer(
        &self,
        config: &ContextReducerAdapterConfig,
        kind: ContextReducerContentKind,
        schema_version: u32,
    ) -> Option<&ContextReducerDescriptor> {
        if !config.enabled {
            return None;
        }
        self.context_reducers.iter().find(|descriptor| {
            config
                .preferred_reducer_id
                .as_deref()
                .is_none_or(|preferred| preferred == descriptor.reducer_id)
                && descriptor
                    .supported_schema_versions
                    .contains(&schema_version)
                && descriptor.content_kinds.contains(&kind)
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginHostError {
    DuplicateContextReducer { reducer_id: String },
    InvalidContextReducer { reducer_id: String, reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContextReducerHostError {
    Timeout,
    AdapterAbsent,
    AdapterCrash,
    PolicyRejected(String),
    ProcessCrash(String),
    MalformedResponse,
    OversizeResponse,
}

type ContextReducerInProcessExecutor = Box<
    dyn FnOnce(ContextReducerRequest) -> Result<ContextReducerResponse, ContextReducerHostError>
        + Send
        + 'static,
>;

pub enum ContextReducerExecutor {
    TrustedInProcess(ContextReducerInProcessExecutor),
    Process(Box<ContextReducerProcessDescriptor>),
}

impl ContextReducerExecutor {
    pub fn trusted_in_process_for_test(
        executor: impl FnOnce(
            ContextReducerRequest,
        ) -> Result<ContextReducerResponse, ContextReducerHostError>
        + Send
        + 'static,
    ) -> Self {
        Self::TrustedInProcess(Box::new(executor))
    }

    pub fn process(process: ContextReducerProcessDescriptor) -> Self {
        Self::Process(Box::new(process))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextReducerHostOutcome {
    pub response: ContextReducerResponse,
    pub health: ContextReducerHealthMetadata,
    pub used_native_fallback: bool,
}

#[derive(Debug, Clone, Default)]
pub struct ContextReducerCircuitBreaker {
    states: HashMap<String, ContextReducerBreakerState>,
}

#[derive(Debug, Clone, Default)]
struct ContextReducerBreakerState {
    failures: u32,
    open_until: Option<Instant>,
}

impl ContextReducerCircuitBreaker {
    pub fn failure_count(&self, reducer_id: &str) -> u32 {
        self.states
            .get(reducer_id)
            .map(|state| state.failures)
            .unwrap_or(0)
    }

    fn record_success(&mut self, reducer_id: &str) {
        self.states.remove(reducer_id);
    }

    fn record_failure(&mut self, reducer_id: &str, config: &ContextReducerAdapterConfig) {
        const MAX_CONTEXT_REDUCER_BACKOFF_MS: u64 = 5 * 60 * 1_000;
        let state = self.states.entry(reducer_id.to_string()).or_default();
        state.failures = state.failures.saturating_add(1);
        let threshold = config.circuit_breaker.failure_threshold.max(1);
        if state.failures >= threshold {
            let backoff_ms = config
                .circuit_breaker
                .backoff_ms
                .min(MAX_CONTEXT_REDUCER_BACKOFF_MS);
            state.open_until = Some(Instant::now() + Duration::from_millis(backoff_ms));
        }
    }

    fn is_open(&mut self, reducer_id: &str) -> bool {
        let Some(state) = self.states.get_mut(reducer_id) else {
            return false;
        };
        match state.open_until {
            Some(open_until) if Instant::now() < open_until => true,
            Some(_) => {
                state.open_until = None;
                false
            }
            None => false,
        }
    }
}

pub fn execute_context_reducer<N>(
    config: &ContextReducerAdapterConfig,
    descriptor: &ContextReducerDescriptor,
    request: ContextReducerRequest,
    executor: Option<ContextReducerExecutor>,
    native_fallback: N,
) -> ContextReducerHostOutcome
where
    N: FnOnce(&ContextReducerRequest) -> ContextReducerResponse,
{
    let mut breaker = ContextReducerCircuitBreaker::default();
    execute_context_reducer_with_breaker(
        config,
        descriptor,
        request,
        executor,
        native_fallback,
        &mut breaker,
    )
}

pub fn execute_context_reducer_with_breaker<N>(
    config: &ContextReducerAdapterConfig,
    descriptor: &ContextReducerDescriptor,
    request: ContextReducerRequest,
    executor: Option<ContextReducerExecutor>,
    native_fallback: N,
    breaker: &mut ContextReducerCircuitBreaker,
) -> ContextReducerHostOutcome
where
    N: FnOnce(&ContextReducerRequest) -> ContextReducerResponse,
{
    if !config.enabled {
        return fallback_outcome(
            &request,
            native_fallback,
            health(
                ContextReducerHealthStatus::Disabled,
                0,
                "context reducer disabled",
            ),
        );
    }
    if let Err(message) = validate_request(&request) {
        return fallback_outcome(
            &request,
            native_fallback,
            health(ContextReducerHealthStatus::PolicyRejected, 0, message),
        );
    }
    if !descriptor
        .supported_schema_versions
        .contains(&request.schema_version)
        || request.schema_version != CONTEXT_REDUCER_SCHEMA_VERSION
    {
        return fallback_outcome(
            &request,
            native_fallback,
            health(
                ContextReducerHealthStatus::VersionMismatch,
                0,
                "unsupported context reducer schema version",
            ),
        );
    }
    if !descriptor.content_kinds.contains(&request.content_kind) {
        return fallback_outcome(
            &request,
            native_fallback,
            health(
                ContextReducerHealthStatus::PolicyRejected,
                0,
                "content kind not supported by reducer",
            ),
        );
    }
    if breaker.is_open(&descriptor.reducer_id) {
        return fallback_outcome(
            &request,
            native_fallback,
            health(
                ContextReducerHealthStatus::CircuitOpen,
                0,
                "context reducer circuit breaker open",
            ),
        );
    }
    let Some(executor) = executor else {
        breaker.record_failure(&descriptor.reducer_id, config);
        return fallback_outcome(
            &request,
            native_fallback,
            health(
                ContextReducerHealthStatus::AdapterAbsent,
                0,
                "context reducer executor absent",
            ),
        );
    };

    let execution = match execute_adapter(config, descriptor, request.clone(), executor) {
        Ok(execution) => execution,
        Err(ContextReducerHostError::Timeout) => {
            breaker.record_failure(&descriptor.reducer_id, config);
            return fallback_outcome(
                &request,
                native_fallback,
                health(
                    ContextReducerHealthStatus::Timeout,
                    config.timeout_ms,
                    "context reducer timed out",
                ),
            );
        }
        Err(ContextReducerHostError::AdapterAbsent) => {
            breaker.record_failure(&descriptor.reducer_id, config);
            return fallback_outcome(
                &request,
                native_fallback,
                health(
                    ContextReducerHealthStatus::AdapterAbsent,
                    0,
                    "context reducer executable absent",
                ),
            );
        }
        Err(ContextReducerHostError::AdapterCrash) => {
            breaker.record_failure(&descriptor.reducer_id, config);
            return fallback_outcome(
                &request,
                native_fallback,
                health(
                    ContextReducerHealthStatus::Crash,
                    0,
                    "context reducer crashed",
                ),
            );
        }
        Err(ContextReducerHostError::PolicyRejected(message)) => {
            breaker.record_failure(&descriptor.reducer_id, config);
            return fallback_outcome(
                &request,
                native_fallback,
                health(ContextReducerHealthStatus::PolicyRejected, 0, message),
            );
        }
        Err(ContextReducerHostError::ProcessCrash(message)) => {
            breaker.record_failure(&descriptor.reducer_id, config);
            return fallback_outcome(
                &request,
                native_fallback,
                health(ContextReducerHealthStatus::Crash, 0, message),
            );
        }
        Err(ContextReducerHostError::MalformedResponse) => {
            breaker.record_failure(&descriptor.reducer_id, config);
            return fallback_outcome(
                &request,
                native_fallback,
                health(
                    ContextReducerHealthStatus::Malformed,
                    0,
                    "context reducer returned malformed JSON",
                ),
            );
        }
        Err(ContextReducerHostError::OversizeResponse) => {
            breaker.record_failure(&descriptor.reducer_id, config);
            return fallback_outcome(
                &request,
                native_fallback,
                health(
                    ContextReducerHealthStatus::Oversize,
                    0,
                    "context reducer response exceeds byte limit",
                ),
            );
        }
    };

    let mut response = execution.response;
    response.health.latency_ms = execution.latency_ms;
    match validate_response(
        config,
        descriptor,
        &request,
        &response,
        execution.latency_ms,
    ) {
        Ok(()) => {
            breaker.record_success(&descriptor.reducer_id);
            ContextReducerHostOutcome {
                health: response.health.clone(),
                response,
                used_native_fallback: false,
            }
        }
        Err(health) => {
            breaker.record_failure(&descriptor.reducer_id, config);
            fallback_outcome(&request, native_fallback, health)
        }
    }
}

fn execute_adapter_isolated(
    reducer_id: String,
    request: ContextReducerRequest,
    executor: ContextReducerInProcessExecutor,
    timeout: Duration,
) -> Result<ContextReducerExecution, ContextReducerHostError> {
    let (sender, receiver) = mpsc::sync_channel(1);
    let thread_name = format!("viden-context-reducer-{reducer_id}");
    let builder = std::thread::Builder::new().name(thread_name);
    let started = Instant::now();
    builder
        .spawn(move || {
            let result = catch_unwind(AssertUnwindSafe(|| executor(request)))
                .unwrap_or(Err(ContextReducerHostError::AdapterCrash));
            let _ = sender.send(result);
        })
        .map_err(|_| ContextReducerHostError::AdapterCrash)?;
    let response = receiver.recv_timeout(timeout).map_err(|err| match err {
        mpsc::RecvTimeoutError::Timeout => ContextReducerHostError::Timeout,
        mpsc::RecvTimeoutError::Disconnected => ContextReducerHostError::AdapterCrash,
    })??;
    Ok(ContextReducerExecution {
        response,
        latency_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
    })
}

struct ContextReducerExecution {
    response: ContextReducerResponse,
    latency_ms: u64,
}

fn validate_context_reducer_identity(
    descriptor: &ContextReducerDescriptor,
) -> Result<(), PluginHostError> {
    for (name, value) in [
        ("reducer id", descriptor.reducer_id.as_str()),
        ("reducer version", descriptor.version.as_str()),
    ] {
        if !is_safe_identifier(value) {
            return Err(PluginHostError::InvalidContextReducer {
                reducer_id: descriptor.reducer_id.clone(),
                reason: format!("{name} contains unsafe characters"),
            });
        }
    }
    Ok(())
}

fn validate_context_reducer_process(
    reducer_id: &str,
    reducer_version: &str,
    process: ContextReducerProcessDescriptor,
) -> Result<ContextReducerProcessDescriptor, PluginHostError> {
    let trusted_root = canonical_absolute(&process.trusted_root).map_err(|reason| {
        PluginHostError::InvalidContextReducer {
            reducer_id: reducer_id.to_string(),
            reason,
        }
    })?;
    let executable = canonical_absolute(&process.executable).map_err(|reason| {
        PluginHostError::InvalidContextReducer {
            reducer_id: reducer_id.to_string(),
            reason,
        }
    })?;
    if !executable.starts_with(&trusted_root) {
        return Err(PluginHostError::InvalidContextReducer {
            reducer_id: reducer_id.to_string(),
            reason: "context reducer executable escapes trusted plugin root".to_string(),
        });
    }
    let cwd = match process.cwd {
        Some(cwd) => {
            let cwd = canonical_absolute(&cwd).map_err(|reason| {
                PluginHostError::InvalidContextReducer {
                    reducer_id: reducer_id.to_string(),
                    reason,
                }
            })?;
            if !cwd.starts_with(&trusted_root) {
                return Err(PluginHostError::InvalidContextReducer {
                    reducer_id: reducer_id.to_string(),
                    reason: "context reducer cwd escapes trusted plugin root".to_string(),
                });
            }
            Some(cwd.display().to_string())
        }
        None => None,
    };
    let authorization =
        process
            .authorization
            .ok_or_else(|| PluginHostError::InvalidContextReducer {
                reducer_id: reducer_id.to_string(),
                reason: "context reducer process authorization missing".to_string(),
            })?;
    validate_process_authorization_values(
        reducer_id,
        reducer_version,
        executable.as_path(),
        &authorization,
    )
    .map_err(|reason| PluginHostError::InvalidContextReducer {
        reducer_id: reducer_id.to_string(),
        reason,
    })?;
    Ok(ContextReducerProcessDescriptor {
        executable: executable.display().to_string(),
        args: process.args,
        cwd,
        trusted_root: trusted_root.display().to_string(),
        env_allowlist: process.env_allowlist,
        max_stderr_bytes: process.max_stderr_bytes,
        authorization: Some(authorization),
    })
}

fn validate_process_authorization(
    reducer_id: &str,
    reducer_version: &str,
    process: &ContextReducerProcessDescriptor,
) -> Result<(), ContextReducerHostError> {
    let executable = canonical_absolute(&process.executable).map_err(|reason| {
        if reason.contains("not found") {
            ContextReducerHostError::AdapterAbsent
        } else {
            ContextReducerHostError::PolicyRejected(reason)
        }
    })?;
    let trusted_root = canonical_absolute(&process.trusted_root)
        .map_err(ContextReducerHostError::PolicyRejected)?;
    if !executable.starts_with(&trusted_root) {
        return Err(ContextReducerHostError::PolicyRejected(
            "context reducer executable escapes trusted plugin root".to_string(),
        ));
    }
    if let Some(cwd) = process.cwd.as_deref() {
        let cwd = canonical_absolute(cwd).map_err(ContextReducerHostError::PolicyRejected)?;
        if !cwd.starts_with(&trusted_root) {
            return Err(ContextReducerHostError::PolicyRejected(
                "context reducer cwd escapes trusted plugin root".to_string(),
            ));
        }
    }
    let authorization = process.authorization.as_ref().ok_or_else(|| {
        ContextReducerHostError::PolicyRejected(
            "context reducer process authorization missing".to_string(),
        )
    })?;
    validate_process_authorization_values(
        reducer_id,
        reducer_version,
        executable.as_path(),
        authorization,
    )
    .map_err(ContextReducerHostError::PolicyRejected)
}

fn validate_process_authorization_values(
    reducer_id: &str,
    reducer_version: &str,
    executable: &Path,
    authorization: &ContextReducerProcessAuthorization,
) -> Result<(), String> {
    if authorization.adapter_id != reducer_id
        || authorization.adapter_version != reducer_version
        || authorization.executable_identity != executable.display().to_string()
        || authorization.permission_snapshot_ref.trim().is_empty()
        || contains_path_or_secret(&authorization.permission_snapshot_ref)
    {
        return Err(
            "context reducer process authorization does not bind adapter identity".to_string(),
        );
    }
    Ok(())
}

fn canonical_absolute(path: &str) -> Result<PathBuf, String> {
    let path = Path::new(path);
    if !path.is_absolute() {
        return Err("context reducer process path must be absolute".to_string());
    }
    path.canonicalize()
        .map_err(|_| "context reducer process path not found".to_string())
}

fn execute_adapter(
    config: &ContextReducerAdapterConfig,
    descriptor: &ContextReducerDescriptor,
    request: ContextReducerRequest,
    executor: ContextReducerExecutor,
) -> Result<ContextReducerExecution, ContextReducerHostError> {
    match executor {
        ContextReducerExecutor::TrustedInProcess(executor) => execute_adapter_isolated(
            descriptor.reducer_id.clone(),
            request,
            executor,
            Duration::from_millis(config.timeout_ms),
        ),
        ContextReducerExecutor::Process(process) => {
            let max_stdout_bytes = config
                .max_output_bytes
                .min(descriptor.limits.max_output_bytes)
                .min(request.policy.max_output_bytes)
                .min(CONTEXT_REDUCER_PROCESS_STDOUT_HARD_CAP_BYTES);
            let started = Instant::now();
            let response = execute_process_adapter(
                &descriptor.reducer_id,
                &descriptor.version,
                &process,
                max_stdout_bytes,
                config.timeout_ms,
                request,
            )?;
            Ok(ContextReducerExecution {
                response,
                latency_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
            })
        }
    }
}

fn execute_process_adapter(
    reducer_id: &str,
    reducer_version: &str,
    config: &ContextReducerProcessDescriptor,
    max_stdout_bytes: usize,
    timeout_ms: u64,
    request: ContextReducerRequest,
) -> Result<ContextReducerResponse, ContextReducerHostError> {
    validate_process_authorization(reducer_id, reducer_version, config)?;
    let request_json =
        serde_json::to_vec(&request).map_err(|_| ContextReducerHostError::MalformedResponse)?;
    if request_json.len() > CONTEXT_REDUCER_PROCESS_REQUEST_HARD_CAP_BYTES {
        return Err(ContextReducerHostError::PolicyRejected(
            "context reducer request exceeds byte limit".to_string(),
        ));
    }
    let io =
        ProcessAdapterIo::new(&request_json).map_err(|_| ContextReducerHostError::AdapterCrash)?;
    let stdin = io
        .open_stdin()
        .map_err(|_| ContextReducerHostError::AdapterCrash)?;
    let stdout = io
        .open_stdout()
        .map_err(|_| ContextReducerHostError::AdapterCrash)?;
    let stderr = io
        .open_stderr()
        .map_err(|_| ContextReducerHostError::AdapterCrash)?;
    let mut command = Command::new(&config.executable);
    command
        .args(&config.args)
        .stdin(Stdio::from(stdin))
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr))
        .env_clear();
    for name in &config.env_allowlist {
        if let Ok(value) = std::env::var(name)
            && !contains_path_or_secret(name)
            && !contains_path_or_secret(&value)
        {
            command.env(name, value);
        }
    }
    if let Some(cwd) = config.cwd.as_deref() {
        command.current_dir(cwd);
    }
    configure_process_group(&mut command);

    let started = Instant::now();
    let mut child = match command.spawn() {
        Ok(child) => ProcessChildGuard::new(child),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return Err(ContextReducerHostError::AdapterAbsent);
        }
        Err(_) => return Err(ContextReducerHostError::AdapterCrash),
    };
    let stderr_limit = config
        .max_stderr_bytes
        .min(CONTEXT_REDUCER_PROCESS_STDERR_HARD_CAP_BYTES);
    let deadline = started + Duration::from_millis(timeout_ms);
    let status = loop {
        if io
            .stdout_reached_sentinel(max_stdout_bytes)
            .map_err(|_| ContextReducerHostError::AdapterCrash)?
        {
            child.kill_and_wait();
            return Err(ContextReducerHostError::OversizeResponse);
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() >= deadline => {
                // Production isolation is process based: kill and reap before
                // returning so late writes cannot affect native fallback.
                child.kill_and_wait();
                return Err(ContextReducerHostError::Timeout);
            }
            Ok(None) => thread::sleep(Duration::from_millis(5)),
            Err(_) => {
                child.kill_and_wait();
                return Err(ContextReducerHostError::AdapterCrash);
            }
        }
    };
    child.disarm_after_exit();
    let stdout = io
        .read_stdout_bounded(max_stdout_bytes)
        .map_err(|_| ContextReducerHostError::AdapterCrash)?;
    let stderr = io
        .read_stderr_bounded(stderr_limit)
        .map_err(|_| ContextReducerHostError::AdapterCrash)?;
    if !status.success() {
        let stderr = bounded_redacted_stderr(&stderr.bytes);
        return Err(ContextReducerHostError::ProcessCrash(format!(
            "context reducer process exited nonzero: {stderr}"
        )));
    }
    if stdout.exceeded {
        return Err(ContextReducerHostError::OversizeResponse);
    }
    serde_json::from_slice::<ContextReducerResponse>(&stdout.bytes)
        .map_err(|_| ContextReducerHostError::MalformedResponse)
}

struct ProcessChildGuard {
    child: Option<std::process::Child>,
}

impl ProcessChildGuard {
    fn new(child: std::process::Child) -> Self {
        Self { child: Some(child) }
    }

    fn try_wait(&mut self) -> std::io::Result<Option<std::process::ExitStatus>> {
        self.child
            .as_mut()
            .expect("process child is present")
            .try_wait()
    }

    fn kill_and_wait(&mut self) {
        if let Some(mut child) = self.child.take() {
            kill_process_tree(&mut child);
            let _ = child.wait();
        }
    }

    fn disarm_after_exit(&mut self) {
        let _ = self.child.take();
    }
}

impl Drop for ProcessChildGuard {
    fn drop(&mut self) {
        self.kill_and_wait();
    }
}

#[cfg(unix)]
fn configure_process_group(command: &mut Command) {
    use std::os::unix::process::CommandExt;
    command.process_group(0);
}

#[cfg(not(unix))]
fn configure_process_group(_command: &mut Command) {}

#[cfg(unix)]
fn kill_process_tree(child: &mut std::process::Child) {
    let pgid = child.id() as libc::pid_t;
    if pgid > 0 {
        // Adapters run in their own process group; negative pgid kill is a
        // defense-in-depth containment boundary for accidental descendants.
        unsafe {
            libc::kill(-pgid, libc::SIGKILL);
        }
    }
    let _ = child.kill();
}

#[cfg(not(unix))]
fn kill_process_tree(child: &mut std::process::Child) {
    let _ = child.kill();
}

struct ProcessAdapterIo {
    dir: PathBuf,
    stdin_path: PathBuf,
    stdout_path: PathBuf,
    stderr_path: PathBuf,
}

impl ProcessAdapterIo {
    fn new(request_json: &[u8]) -> std::io::Result<Self> {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let mut dir = std::env::temp_dir();
        dir.push(format!(
            "viden-context-reducer-io-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::SeqCst)
        ));
        fs::create_dir(&dir)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&dir, fs::Permissions::from_mode(0o700))?;
        }
        let io = Self {
            stdin_path: dir.join("request.json"),
            stdout_path: dir.join("stdout.json"),
            stderr_path: dir.join("stderr.txt"),
            dir,
        };
        {
            let mut file = private_write_file(&io.stdin_path)?;
            file.write_all(request_json)?;
            file.write_all(b"\n")?;
        }
        private_write_file(&io.stdout_path)?;
        private_write_file(&io.stderr_path)?;
        Ok(io)
    }

    fn open_stdin(&self) -> std::io::Result<File> {
        File::open(&self.stdin_path)
    }

    fn open_stdout(&self) -> std::io::Result<File> {
        private_truncate_file(&self.stdout_path)
    }

    fn open_stderr(&self) -> std::io::Result<File> {
        private_truncate_file(&self.stderr_path)
    }

    fn stdout_reached_sentinel(&self, max_stdout_bytes: usize) -> std::io::Result<bool> {
        let sentinel = max_stdout_bytes.saturating_add(1);
        let len = fs::metadata(&self.stdout_path)?.len();
        Ok(len >= u64::try_from(sentinel).unwrap_or(u64::MAX))
    }

    fn read_stdout_bounded(&self, max_bytes: usize) -> std::io::Result<BoundedRead> {
        read_bounded(File::open(&self.stdout_path)?, max_bytes)
    }

    fn read_stderr_bounded(&self, max_bytes: usize) -> std::io::Result<BoundedRead> {
        read_bounded(File::open(&self.stderr_path)?, max_bytes)
    }

    #[cfg(test)]
    fn dir_for_test(&self) -> &Path {
        &self.dir
    }
}

impl Drop for ProcessAdapterIo {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.stdin_path);
        let _ = fs::remove_file(&self.stdout_path);
        let _ = fs::remove_file(&self.stderr_path);
        let _ = fs::remove_dir(&self.dir);
    }
}

fn private_write_file(path: &Path) -> std::io::Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}

fn private_truncate_file(path: &Path) -> std::io::Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}

struct BoundedRead {
    bytes: Vec<u8>,
    exceeded: bool,
}

fn read_bounded<R: Read>(mut reader: R, max_bytes: usize) -> Result<BoundedRead, std::io::Error> {
    let mut output = Vec::new();
    let mut chunk = [0_u8; 4096];
    let sentinel_bytes = max_bytes.saturating_add(1);
    loop {
        let read = reader.read(&mut chunk)?;
        if read == 0 {
            break;
        }
        if output.len() < sentinel_bytes {
            let remaining = sentinel_bytes.saturating_sub(output.len());
            output.extend_from_slice(&chunk[..read.min(remaining)]);
            if output.len() > max_bytes {
                return Ok(BoundedRead {
                    bytes: output,
                    exceeded: true,
                });
            }
        }
    }
    let exceeded = output.len() > max_bytes;
    Ok(BoundedRead {
        bytes: output,
        exceeded,
    })
}

fn bounded_redacted_stderr(stderr: &[u8]) -> String {
    let text = String::from_utf8_lossy(stderr);
    redact_health_text(&text)
}

fn redact_health_text(value: &str) -> String {
    value
        .split_whitespace()
        .map(|token| {
            if contains_path_or_secret(token) {
                "<redacted>"
            } else {
                token
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(160)
        .collect()
}

fn validate_request(request: &ContextReducerRequest) -> Result<(), &'static str> {
    if request.schema_version != CONTEXT_REDUCER_SCHEMA_VERSION {
        return Err("unsupported context reducer request version");
    }
    for value in [
        request.canonical.reference.as_str(),
        request.permission_snapshot_ref.as_str(),
        request.canonical.item_id.as_str(),
    ] {
        if contains_path_or_secret(value) {
            return Err("context reducer request contains path-like or secret-like data");
        }
    }
    Ok(())
}

fn validate_response(
    config: &ContextReducerAdapterConfig,
    descriptor: &ContextReducerDescriptor,
    request: &ContextReducerRequest,
    response: &ContextReducerResponse,
    observed_latency_ms: u64,
) -> Result<(), ContextReducerHealthMetadata> {
    if response.schema_version != request.schema_version {
        return Err(health(
            ContextReducerHealthStatus::VersionMismatch,
            observed_latency_ms,
            "context reducer response schema mismatch",
        ));
    }
    if response.request_id != request.request_id
        || response.canonical_hash != request.canonical.content_sha256
        || response.permission_snapshot_ref != request.permission_snapshot_ref
        || response.scope != request.scope
        || response.content_kind != request.content_kind
        || response.reducer_id != descriptor.reducer_id
        || response.reducer_version != descriptor.version
    {
        return Err(health(
            ContextReducerHealthStatus::BindingMismatch,
            observed_latency_ms,
            "context reducer response does not bind request/hash/scope",
        ));
    }
    let max_output_bytes = config
        .max_output_bytes
        .min(descriptor.limits.max_output_bytes)
        .min(request.policy.max_output_bytes);
    let max_output_items = config
        .max_output_items
        .min(descriptor.limits.max_output_items);
    if response.reduced_content.len() > max_output_bytes
        || response.omissions.len() > max_output_items
        || response.reduced_content.lines().count() > max_output_items
        || max_structural_depth(&response.reduced_content)
            > config.max_depth.min(descriptor.limits.max_depth)
    {
        return Err(health(
            ContextReducerHealthStatus::Oversize,
            observed_latency_ms,
            "context reducer response exceeds bounded output limits",
        ));
    }
    if !response.quality.passed
        || response.quality.score_microunits < config.min_quality_score_microunits
        || response.quality.evidence_recall_microunits < config.min_evidence_recall_microunits
    {
        return Err(health(
            ContextReducerHealthStatus::QualityFailed,
            observed_latency_ms,
            "context reducer quality threshold failed",
        ));
    }
    if contains_path_or_secret(&response.reduced_content) {
        return Err(health(
            ContextReducerHealthStatus::PolicyRejected,
            observed_latency_ms,
            "context reducer response contains path-like or secret-like data",
        ));
    }
    Ok(())
}

fn fallback_outcome<N>(
    request: &ContextReducerRequest,
    native_fallback: N,
    fallback_health: ContextReducerHealthMetadata,
) -> ContextReducerHostOutcome
where
    N: FnOnce(&ContextReducerRequest) -> ContextReducerResponse,
{
    ContextReducerHostOutcome {
        response: native_fallback(request),
        health: fallback_health,
        used_native_fallback: true,
    }
}

fn health(
    status: ContextReducerHealthStatus,
    latency_ms: u64,
    message: impl Into<String>,
) -> ContextReducerHealthMetadata {
    ContextReducerHealthMetadata {
        latency_ms,
        status,
        message: Some(message.into()),
    }
}

fn contains_path_or_secret(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    value.contains("/Users/")
        || value.contains("\\Users\\")
        || value.contains("://")
        || lower.contains("storage_path")
        || lower.contains("api_key")
        || lower.contains("credential")
        || lower.contains("password")
        || value.contains("sk-")
}

fn is_safe_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 80
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
}

fn max_structural_depth(value: &str) -> usize {
    let mut depth = 0_usize;
    let mut max_depth = 0_usize;
    let mut in_string = false;
    let mut escaped = false;
    for ch in value.chars() {
        if escaped {
            escaped = false;
            continue;
        }
        if ch == '\\' && in_string {
            escaped = true;
            continue;
        }
        if ch == '"' {
            in_string = !in_string;
            continue;
        }
        if in_string {
            continue;
        }
        match ch {
            '[' | '{' | '(' => {
                depth = depth.saturating_add(1);
                max_depth = max_depth.max(depth);
            }
            ']' | '}' | ')' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    max_depth
}

pub fn builtin_agent_descriptors() -> Vec<AgentPluginDescriptor> {
    vec![
        registry_acp_agent(
            "claude-acp",
            "Claude Agent",
            "0.60.0",
            "@agentclientprotocol/claude-agent-acp",
            &[
                AgentPluginCapability::SessionPrompt,
                AgentPluginCapability::SessionLoad,
                AgentPluginCapability::SessionCancel,
                AgentPluginCapability::StreamingUpdates,
                AgentPluginCapability::ToolCalls,
            ],
        ),
        registry_acp_agent(
            "codex-acp",
            "Codex",
            "1.1.7",
            "@agentclientprotocol/codex-acp",
            &[
                AgentPluginCapability::SessionPrompt,
                AgentPluginCapability::SessionCancel,
                AgentPluginCapability::StreamingUpdates,
                AgentPluginCapability::ToolCalls,
            ],
        ),
        local_kiro_agent(),
    ]
}

fn registry_acp_agent(
    agent_id: &str,
    display_name: &str,
    version: &str,
    package: &str,
    capabilities: &[AgentPluginCapability],
) -> AgentPluginDescriptor {
    AgentPluginDescriptor {
        agent_id: agent_id.to_string(),
        display_name: display_name.to_string(),
        version: version.to_string(),
        transport: AgentTransport::Acp,
        source: AgentSource::Registry,
        command: AgentCommandSpec {
            command: "npx".to_string(),
            args: vec!["-y".to_string(), format!("{package}@{version}")],
            env: Vec::new(),
        },
        registry_package: Some(AgentRegistryPackage {
            package: package.to_string(),
            version: version.to_string(),
        }),
        protocol_versions: vec![AgentProtocolVersion::AcpV1],
        auth_modes: vec![AgentAuthMode::AgentNative],
        capabilities: capabilities.to_vec(),
        permission_profile: AgentPermissionProfile::RuntimeGated,
        experimental_methods: Vec::new(),
        config_schema_version: 1,
    }
}

fn local_kiro_agent() -> AgentPluginDescriptor {
    AgentPluginDescriptor {
        agent_id: "kiro-cli".to_string(),
        display_name: "Kiro CLI".to_string(),
        version: "local".to_string(),
        transport: AgentTransport::Acp,
        source: AgentSource::LocalCommand,
        command: AgentCommandSpec {
            command: "kiro-cli".to_string(),
            args: vec!["acp".to_string()],
            env: vec![
                AgentEnvRef {
                    name: "VIDEN_KIRO_AGENT".to_string(),
                    required: false,
                },
                AgentEnvRef {
                    name: "VIDEN_KIRO_MODEL".to_string(),
                    required: false,
                },
                AgentEnvRef {
                    name: "VIDEN_KIRO_EFFORT".to_string(),
                    required: false,
                },
                AgentEnvRef {
                    name: "VIDEN_KIRO_TRUST_ALL_TOOLS".to_string(),
                    required: false,
                },
                AgentEnvRef {
                    name: "VIDEN_KIRO_TRUST_TOOLS".to_string(),
                    required: false,
                },
                AgentEnvRef {
                    name: "VIDEN_KIRO_AGENT_ENGINE".to_string(),
                    required: false,
                },
            ],
        },
        registry_package: None,
        protocol_versions: vec![AgentProtocolVersion::AcpV1],
        auth_modes: vec![AgentAuthMode::AgentNative],
        capabilities: vec![
            AgentPluginCapability::SessionPrompt,
            AgentPluginCapability::SessionLoad,
            AgentPluginCapability::SessionCancel,
            AgentPluginCapability::SessionSetMode,
            AgentPluginCapability::SessionSetModel,
            AgentPluginCapability::StreamingUpdates,
            AgentPluginCapability::ToolCalls,
            AgentPluginCapability::ImageInput,
            AgentPluginCapability::SlashCommands,
            AgentPluginCapability::McpEvents,
        ],
        permission_profile: AgentPermissionProfile::RuntimeGated,
        experimental_methods: vec![
            "_kiro.dev/commands/available".to_string(),
            "_kiro.dev/commands/options".to_string(),
            "_kiro.dev/commands/execute".to_string(),
            "_kiro.dev/mcp/oauth_request".to_string(),
            "_kiro.dev/mcp/server_initialized".to_string(),
            "_kiro.dev/compaction/status".to_string(),
            "_kiro.dev/clear/status".to_string(),
            "_session/terminate".to_string(),
        ],
        config_schema_version: 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use viden_plugin_api::{
        AgentPluginCapability, AgentSource, AgentTransport, ContextReducerCanonicalRef,
        ContextReducerCircuitBreakerConfig, ContextReducerLimits, ContextReducerOmission,
        ContextReducerPolicy, ContextReducerQualityFacts, ContextReducerScope, PluginCapability,
        PluginPermission,
    };

    #[test]
    fn registry_filters_static_plugins_by_kind() {
        let mut registry = StaticPluginRegistry::new();
        registry.register(PluginManifest {
            id: "deepseek".to_string(),
            name: "DeepSeek".to_string(),
            version: "1".to_string(),
            kind: PluginKind::Provider,
            capabilities: vec![PluginCapability::Provider],
            permissions: vec![PluginPermission::Network],
            config_schema_version: 1,
        });

        assert_eq!(registry.by_kind(PluginKind::Provider).count(), 1);
        assert_eq!(registry.by_kind(PluginKind::Tool).count(), 0);
    }

    #[test]
    fn registry_resolves_static_agents_by_id() {
        let mut registry = StaticPluginRegistry::new();
        let kiro = builtin_agent_descriptors()
            .into_iter()
            .find(|agent| agent.agent_id == "kiro-cli")
            .expect("kiro descriptor exists");

        registry.register_agent(kiro);

        let descriptor = registry
            .agent_by_id("kiro-cli")
            .expect("registered descriptor resolves by id");
        assert_eq!(descriptor.transport, AgentTransport::Acp);
        assert_eq!(descriptor.source, AgentSource::LocalCommand);
        assert_eq!(descriptor.command.command, "kiro-cli");
        assert_eq!(descriptor.command.args, vec!["acp"]);
        let env_names = descriptor
            .command
            .env
            .iter()
            .map(|env| env.name.as_str())
            .collect::<Vec<_>>();
        assert!(env_names.contains(&"VIDEN_KIRO_AGENT"));
        assert!(env_names.contains(&"VIDEN_KIRO_MODEL"));
        assert!(env_names.contains(&"VIDEN_KIRO_EFFORT"));
        assert!(env_names.contains(&"VIDEN_KIRO_TRUST_ALL_TOOLS"));
        assert!(env_names.contains(&"VIDEN_KIRO_TRUST_TOOLS"));
        assert!(env_names.contains(&"VIDEN_KIRO_AGENT_ENGINE"));
        assert!(
            descriptor
                .capabilities
                .contains(&AgentPluginCapability::SessionSetModel)
        );
    }

    #[test]
    fn builtin_acp_agents_cover_claude_codex_and_kiro() {
        let agents = builtin_agent_descriptors();
        let ids = agents
            .iter()
            .map(|agent| agent.agent_id.as_str())
            .collect::<Vec<_>>();

        assert!(ids.contains(&"claude-acp"));
        assert!(ids.contains(&"codex-acp"));
        assert!(ids.contains(&"kiro-cli"));
        assert!(
            agents
                .iter()
                .all(|agent| agent.transport == AgentTransport::Acp)
        );
        let claude = agents
            .iter()
            .find(|agent| agent.agent_id == "claude-acp")
            .expect("claude descriptor");
        assert_eq!(claude.version, "0.60.0");
        assert_eq!(
            claude.command.args,
            vec!["-y", "@agentclientprotocol/claude-agent-acp@0.60.0"]
        );
        let codex = agents
            .iter()
            .find(|agent| agent.agent_id == "codex-acp")
            .expect("codex descriptor");
        assert_eq!(codex.version, "1.1.7");
        assert_eq!(
            codex.command.args,
            vec!["-y", "@agentclientprotocol/codex-acp@1.1.7"]
        );
    }

    fn context_reducer_descriptor(id: &str) -> ContextReducerDescriptor {
        ContextReducerDescriptor {
            reducer_id: id.to_string(),
            display_name: "Neutral Reducer".to_string(),
            version: "0.1.0".to_string(),
            supported_schema_versions: vec![CONTEXT_REDUCER_SCHEMA_VERSION],
            content_kinds: vec![ContextReducerContentKind::Log],
            limits: ContextReducerLimits {
                max_input_bytes: 4096,
                max_output_bytes: 1024,
                max_output_items: 32,
                max_depth: 8,
            },
            default_enabled: false,
            config_schema_version: 1,
            process: None,
        }
    }

    fn context_reducer_request() -> ContextReducerRequest {
        ContextReducerRequest {
            schema_version: CONTEXT_REDUCER_SCHEMA_VERSION,
            request_id: "ctxred-1".to_string(),
            content_kind: ContextReducerContentKind::Log,
            canonical: ContextReducerCanonicalRef {
                item_id: "ctxi-1".to_string(),
                content_sha256: "ab".repeat(32),
                evidence_id: None,
                reference: "context-item:ctxi-1".to_string(),
            },
            policy: ContextReducerPolicy {
                max_output_bytes: 128,
                max_output_tokens: 64,
                max_input_bytes: 4096,
                max_depth: 8,
                max_output_items: 16,
                required_markers: vec!["first_failure".to_string()],
            },
            scope: ContextReducerScope {
                role: "executor".to_string(),
                task_id: "task-1".to_string(),
                dag_id: None,
                workflow_id: Some("wf-1".to_string()),
            },
            permission_snapshot_ref: "perm-snap-1".to_string(),
            native_baseline_quality: Some(ContextReducerQualityFacts {
                passed: true,
                score_microunits: 950_000,
                evidence_recall_microunits: 950_000,
                checks: vec!["native".to_string()],
                deterministic_fingerprint: "native-fp".to_string(),
            }),
        }
    }

    fn context_reducer_response(request: &ContextReducerRequest) -> ContextReducerResponse {
        ContextReducerResponse {
            schema_version: request.schema_version,
            request_id: request.request_id.clone(),
            canonical_hash: request.canonical.content_sha256.clone(),
            permission_snapshot_ref: request.permission_snapshot_ref.clone(),
            scope: request.scope.clone(),
            content_kind: request.content_kind,
            reduced_content: "ERROR src/a.rs:1 boom".to_string(),
            omissions: vec![ContextReducerOmission {
                reason: "deduplicated".to_string(),
                omitted_count: 1,
            }],
            reducer_id: "adapter".to_string(),
            reducer_version: "0.1.0".to_string(),
            quality: ContextReducerQualityFacts {
                passed: true,
                score_microunits: 990_000,
                evidence_recall_microunits: 990_000,
                checks: vec!["first_failure_retained".to_string()],
                deterministic_fingerprint: "adapter-fp".to_string(),
            },
            health: ContextReducerHealthMetadata {
                latency_ms: 4,
                status: ContextReducerHealthStatus::Ok,
                message: None,
            },
        }
    }

    fn native_response(request: &ContextReducerRequest) -> ContextReducerResponse {
        let mut response = context_reducer_response(request);
        response.reducer_id = "viden-context-native".to_string();
        response.reducer_version = "native-v1".to_string();
        response.reduced_content = "native fallback".to_string();
        response
    }

    fn in_process(
        executor: impl FnOnce(
            ContextReducerRequest,
        ) -> Result<ContextReducerResponse, ContextReducerHostError>
        + Send
        + 'static,
    ) -> ContextReducerExecutor {
        ContextReducerExecutor::trusted_in_process_for_test(executor)
    }

    /// Source of the out-of-process context-reducer adapter these tests spawn.
    const CONTEXT_REDUCER_HELPER_SOURCE: &str = r###"
use std::env;
use std::fs;
use std::io::{self, Read};
use std::thread;
use std::time::Duration;

fn success_json() -> String {
    format!(
        r#"{{"schema_version":1,"request_id":"ctxred-1","canonical_hash":"{}","permission_snapshot_ref":"perm-snap-1","scope":{{"role":"executor","task_id":"task-1","dag_id":null,"workflow_id":"wf-1"}},"content_kind":"log","reduced_content":"ERROR src/a.rs:1 boom","omissions":[{{"reason":"deduplicated","omitted_count":1}}],"reducer_id":"adapter","reducer_version":"0.1.0","quality":{{"passed":true,"score_microunits":990000,"evidence_recall_microunits":990000,"checks":["first_failure_retained"],"deterministic_fingerprint":"adapter-fp"}},"health":{{"latency_ms":99999,"status":"ok","message":null}}}}"#,
        "ab".repeat(32)
    )
}

fn main() {
    let mode = env::args().nth(1).unwrap_or_else(|| "success".to_string());
    if matches!(mode.as_str(), "sleep" | "stream_oversize" | "timeout_descendant") {
        if let Some(pid_path) = env::args().nth(3) {
            fs::write(pid_path, std::process::id().to_string()).unwrap();
        }
    }
    let mut stdin = String::new();
    let _ = io::stdin().read_to_string(&mut stdin);
    match mode.as_str() {
        "success" => print!("{}", success_json()),
        "mark_success" => {
            let marker = env::args().nth(2).expect("marker path");
            fs::write(marker, "spawned").unwrap();
            print!("{}", success_json());
        }
        "stdin_file" => {
            #[cfg(unix)]
            {
                let meta = fs::metadata("/dev/fd/0").unwrap();
                if !meta.file_type().is_file() {
                    std::process::exit(8);
                }
            }
            if !stdin.contains("\"request_id\":\"ctxred-1\"") {
                std::process::exit(9);
            }
            print!("{}", success_json());
        }
        "sleep" => {
            let marker = env::args().nth(2).expect("marker path");
            if let Some(pid_path) = env::args().nth(3) {
                fs::write(pid_path, std::process::id().to_string()).unwrap();
            }
            // 20s, not 5s: the marker below is the witness that the host killed
            // this child instead of waiting for it, so its lifetime must dwarf
            // every host deadline under test even on a fully loaded machine.
            thread::sleep(Duration::from_millis(20000));
            fs::write(marker, "late mutation").unwrap();
            print!("{}", success_json());
        }
        "nonzero" => {
            eprintln!("failed at /Users/wiki/private with sk-test-secret");
            std::process::exit(7);
        }
        "malformed" => print!("not-json"),
        "oversize" => print!("{}", "x".repeat(20_000)),
        "stream_oversize" => {
            let marker = env::args().nth(2).expect("marker path");
            if let Some(pid_path) = env::args().nth(3) {
                fs::write(pid_path, std::process::id().to_string()).unwrap();
            }
            print!("{}", "x".repeat(2_000_000));
            thread::sleep(Duration::from_millis(20000));
            fs::write(marker, "late mutation").unwrap();
        }
        "descendant_stdout" => {
            let marker = env::args().nth(2).expect("marker path");
            print!("{}", success_json());
            let child = std::process::Command::new(env::current_exe().unwrap())
                .arg("hold_stdout")
                .arg(marker)
                .stdout(std::process::Stdio::inherit())
                .spawn()
                .unwrap();
            std::mem::forget(child);
        }
        "hold_stdout" => {
            let marker = env::args().nth(2).expect("marker path");
            thread::sleep(Duration::from_millis(20000));
            fs::write(marker, "descendant retained stdout").unwrap();
        }
        "timeout_descendant" => {
            let marker = env::args().nth(2).expect("marker path");
            if let Some(pid_path) = env::args().nth(3) {
                fs::write(pid_path, std::process::id().to_string()).unwrap();
            }
            let child = std::process::Command::new(env::current_exe().unwrap())
                .arg("late_marker")
                .arg(marker)
                .stdout(std::process::Stdio::inherit())
                .spawn()
                .unwrap();
            std::mem::forget(child);
            thread::sleep(Duration::from_millis(8000));
        }
        "late_marker" => {
            let marker = env::args().nth(2).expect("marker path");
            thread::sleep(Duration::from_millis(6500));
            fs::write(marker, "descendant late mutation").unwrap();
        }
        "stderr_large_nonzero" => {
            eprintln!("{}", "failed /Users/wiki/private sk-test-secret ".repeat(1000));
            std::process::exit(9);
        }
        _ => std::process::exit(2),
    }
}
"###;

    /// Compiles the helper once per test process and hands each caller its own
    /// directory holding a copy of it.
    ///
    /// This used to run `rustc` once per test. A dozen concurrent compilations
    /// is the load that starved the children the deadline-sensitive tests below
    /// spawn, which is what made this module flake under a parallel run — the
    /// tests themselves were sound. The per-caller directory is still unique, so
    /// each test keeps its own marker and pid files and its own `trusted_root`.
    fn compile_context_reducer_helper() -> std::path::PathBuf {
        static SHARED_HELPER: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();
        static HELPER_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

        let shared = SHARED_HELPER.get_or_init(|| {
            let dir = std::env::temp_dir().join(format!(
                "viden-context-reducer-helper-src-{}",
                std::process::id()
            ));
            std::fs::create_dir_all(&dir).unwrap();
            let src = dir.join("helper.rs");
            let bin = dir.join("helper");
            std::fs::write(&src, CONTEXT_REDUCER_HELPER_SOURCE).unwrap();
            let status = std::process::Command::new("rustc")
                .arg(&src)
                .arg("-o")
                .arg(&bin)
                .status()
                .unwrap();
            assert!(status.success(), "helper should compile");
            bin
        });

        let unique = format!(
            "viden-context-reducer-helper-{}-{}",
            std::process::id(),
            HELPER_COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
        );
        let dir = std::env::temp_dir().join(unique);
        std::fs::create_dir_all(&dir).unwrap();
        let bin = dir.join("helper");
        // `fs::copy` carries the executable bit on unix, so the copy is spawnable.
        std::fs::copy(shared, &bin).unwrap();
        // Execute the copy once, here, outside any host deadline. macOS validates
        // a Mach-O's signature on the first exec of a new inode, and paying that
        // cost inside a timeout window — while other tests are doing the same —
        // is what left the timeout test's child unscheduled and its pid file
        // unwritten. "warmup" is an unknown mode, so the helper exits at once.
        let _ = std::process::Command::new(&bin)
            .arg("warmup")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
        bin
    }

    fn process_descriptor(helper: &std::path::Path, mode: &str) -> ContextReducerProcessDescriptor {
        let executable = helper.canonicalize().unwrap().display().to_string();
        let trusted_root = helper
            .parent()
            .unwrap()
            .canonicalize()
            .unwrap()
            .display()
            .to_string();
        ContextReducerProcessDescriptor {
            executable: executable.clone(),
            args: vec![mode.to_string()],
            cwd: None,
            trusted_root,
            env_allowlist: vec!["PATH".to_string(), "HOME".to_string()],
            max_stderr_bytes: 128,
            authorization: Some(ContextReducerProcessAuthorization {
                adapter_id: "adapter".to_string(),
                adapter_version: "0.1.0".to_string(),
                executable_identity: executable,
                permission_snapshot_ref: "plugin-install:adapter:approved".to_string(),
            }),
        }
    }

    fn process_request_with_limit(max_output_bytes: usize) -> ContextReducerRequest {
        let mut request = context_reducer_request();
        request.policy.max_output_bytes = max_output_bytes;
        request
    }

    #[test]
    fn context_reducer_registration_rejects_duplicates_and_negotiates_only_when_enabled() {
        let mut registry = StaticPluginRegistry::new();
        registry
            .register_context_reducer(context_reducer_descriptor("adapter"))
            .unwrap();

        assert!(
            registry
                .register_context_reducer(context_reducer_descriptor("adapter"))
                .is_err()
        );
        assert!(
            registry
                .negotiate_context_reducer(
                    &ContextReducerAdapterConfig::default(),
                    ContextReducerContentKind::Log,
                    CONTEXT_REDUCER_SCHEMA_VERSION,
                )
                .is_none()
        );

        let config = ContextReducerAdapterConfig {
            enabled: true,
            preferred_reducer_id: Some("adapter".to_string()),
            ..ContextReducerAdapterConfig::default()
        };
        let selected = registry
            .negotiate_context_reducer(
                &config,
                ContextReducerContentKind::Log,
                CONTEXT_REDUCER_SCHEMA_VERSION,
            )
            .unwrap();

        assert_eq!(selected.reducer_id, "adapter");
    }

    #[test]
    fn context_reducer_registration_rejects_unsafe_process_descriptors() {
        let mut unsafe_id = context_reducer_descriptor("bad/id");
        unsafe_id.version = "0.1.0".to_string();
        assert!(
            StaticPluginRegistry::new()
                .register_context_reducer(unsafe_id)
                .is_err()
        );

        let mut unsafe_version = context_reducer_descriptor("safe-id");
        unsafe_version.version = "../0.1.0".to_string();
        assert!(
            StaticPluginRegistry::new()
                .register_context_reducer(unsafe_version)
                .is_err()
        );

        let helper = compile_context_reducer_helper();
        let trusted_root = helper.parent().unwrap().canonicalize().unwrap();
        let mut relative = context_reducer_descriptor("relative-adapter");
        relative.process = Some(ContextReducerProcessDescriptor {
            executable: "helper".to_string(),
            args: Vec::new(),
            cwd: None,
            trusted_root: trusted_root.display().to_string(),
            env_allowlist: Vec::new(),
            max_stderr_bytes: 128,
            authorization: Some(ContextReducerProcessAuthorization {
                adapter_id: "relative-adapter".to_string(),
                adapter_version: "0.1.0".to_string(),
                executable_identity: "helper".to_string(),
                permission_snapshot_ref: "plugin-install:relative-adapter:approved".to_string(),
            }),
        });
        assert!(
            StaticPluginRegistry::new()
                .register_context_reducer(relative)
                .is_err()
        );

        let mut outside_cwd = context_reducer_descriptor("cwd-adapter");
        let mut process = process_descriptor(&helper, "success");
        process.cwd = Some(std::env::temp_dir().display().to_string());
        outside_cwd.process = Some(process);
        assert!(
            StaticPluginRegistry::new()
                .register_context_reducer(outside_cwd)
                .is_err()
        );

        #[cfg(unix)]
        {
            let outside = trusted_root.with_file_name("outside-helper");
            std::fs::copy(&helper, &outside).unwrap();
            let link = trusted_root.join("link-helper");
            std::os::unix::fs::symlink(&outside, &link).unwrap();
            let mut symlink_escape = context_reducer_descriptor("symlink-adapter");
            symlink_escape.process = Some(ContextReducerProcessDescriptor {
                executable: link.display().to_string(),
                args: Vec::new(),
                cwd: None,
                trusted_root: trusted_root.display().to_string(),
                env_allowlist: Vec::new(),
                max_stderr_bytes: 128,
                authorization: Some(ContextReducerProcessAuthorization {
                    adapter_id: "symlink-adapter".to_string(),
                    adapter_version: "0.1.0".to_string(),
                    executable_identity: link.display().to_string(),
                    permission_snapshot_ref: "plugin-install:symlink-adapter:approved".to_string(),
                }),
            });
            assert!(
                StaticPluginRegistry::new()
                    .register_context_reducer(symlink_escape)
                    .is_err()
            );
        }
    }

    #[test]
    fn context_reducer_process_missing_authorization_falls_back_without_spawn() {
        let helper = compile_context_reducer_helper();
        let marker = helper.with_file_name("auth-missing-marker");
        let mut process = process_descriptor(&helper, "sleep");
        process.args.push(marker.display().to_string());
        process.authorization = None;
        let config = ContextReducerAdapterConfig {
            enabled: true,
            timeout_ms: 250,
            ..ContextReducerAdapterConfig::default()
        };

        let outcome = execute_context_reducer(
            &config,
            &context_reducer_descriptor("adapter"),
            context_reducer_request(),
            Some(ContextReducerExecutor::process(process)),
            native_response,
        );

        assert!(outcome.used_native_fallback);
        assert_eq!(
            outcome.health.status,
            ContextReducerHealthStatus::PolicyRejected
        );
        assert!(!marker.exists(), "unauthorized process must not spawn");
    }

    #[test]
    fn context_reducer_accepts_valid_external_response() {
        let request = context_reducer_request();
        let config = ContextReducerAdapterConfig {
            enabled: true,
            ..ContextReducerAdapterConfig::default()
        };
        let outcome = execute_context_reducer(
            &config,
            &context_reducer_descriptor("adapter"),
            request.clone(),
            Some(in_process(|request| Ok(context_reducer_response(&request)))),
            native_response,
        );

        assert_eq!(outcome.response.reducer_id, "adapter");
        assert!(
            !outcome.used_native_fallback,
            "unexpected fallback: {:?}",
            outcome.health
        );
        assert_eq!(outcome.health.status, ContextReducerHealthStatus::Ok);
    }

    #[test]
    fn context_reducer_falls_back_on_absent_timeout_crash_malformed_and_binding_failures() {
        let config = ContextReducerAdapterConfig {
            enabled: true,
            timeout_ms: 250,
            ..ContextReducerAdapterConfig::default()
        };
        let descriptor = context_reducer_descriptor("adapter");

        let cases: Vec<(&str, Option<ContextReducerExecutor>)> = vec![
            ("absent", None),
            (
                "crash",
                Some(in_process(|_| Err(ContextReducerHostError::AdapterCrash))),
            ),
            (
                "malformed",
                Some(in_process(|_| {
                    Err(ContextReducerHostError::MalformedResponse)
                })),
            ),
            (
                "wrong_version",
                Some(in_process(|request| {
                    let mut response = context_reducer_response(&request);
                    response.schema_version = 999;
                    Ok(response)
                })),
            ),
            (
                "wrong_hash",
                Some(in_process(|request| {
                    let mut response = context_reducer_response(&request);
                    response.canonical_hash = "cd".repeat(32);
                    Ok(response)
                })),
            ),
            (
                "wrong_scope",
                Some(in_process(|request| {
                    let mut response = context_reducer_response(&request);
                    response.scope.task_id = "task-2".to_string();
                    Ok(response)
                })),
            ),
            (
                "oversize",
                Some(in_process(|request| {
                    let mut response = context_reducer_response(&request);
                    response.reduced_content = "x".repeat(1024);
                    Ok(response)
                })),
            ),
            (
                "too_many_items",
                Some(in_process(|request| {
                    let mut response = context_reducer_response(&request);
                    response.reduced_content = (0..64)
                        .map(|index| format!("line {index}"))
                        .collect::<Vec<_>>()
                        .join("\n");
                    Ok(response)
                })),
            ),
            (
                "too_deep",
                Some(in_process(|request| {
                    let mut response = context_reducer_response(&request);
                    response.reduced_content = "[[[[[[[[[too deep]]]]]]]]]".to_string();
                    Ok(response)
                })),
            ),
            (
                "quality",
                Some(in_process(|request| {
                    let mut response = context_reducer_response(&request);
                    response.quality.passed = false;
                    response.quality.evidence_recall_microunits = 1;
                    Ok(response)
                })),
            ),
        ];

        for (name, executor) in cases {
            let outcome = execute_context_reducer(
                &config,
                &descriptor,
                context_reducer_request(),
                executor,
                native_response,
            );
            assert!(outcome.used_native_fallback, "{name}");
            assert_eq!(
                outcome.response.reducer_id, "viden-context-native",
                "{name}"
            );
            assert_ne!(
                outcome.health.status,
                ContextReducerHealthStatus::Ok,
                "{name}"
            );
        }
    }

    #[test]
    fn context_reducer_does_not_trust_response_reported_latency_for_timeout() {
        let config = ContextReducerAdapterConfig {
            enabled: true,
            timeout_ms: 25,
            ..ContextReducerAdapterConfig::default()
        };

        let outcome = execute_context_reducer(
            &config,
            &context_reducer_descriptor("adapter"),
            context_reducer_request(),
            Some(in_process(|request| {
                let mut response = context_reducer_response(&request);
                response.health.latency_ms = 99_999;
                Ok(response)
            })),
            native_response,
        );

        assert!(
            !outcome.used_native_fallback,
            "unexpected fallback: {:?}",
            outcome.health
        );
        assert_eq!(outcome.health.status, ContextReducerHealthStatus::Ok);
        assert!(outcome.health.latency_ms < 25);
    }

    #[test]
    fn context_reducer_rejects_wrong_permission_reducer_version_and_content_bindings() {
        let config = ContextReducerAdapterConfig {
            enabled: true,
            timeout_ms: 250,
            ..ContextReducerAdapterConfig::default()
        };
        let descriptor = context_reducer_descriptor("adapter");
        let cases: Vec<(&str, ContextReducerExecutor)> = vec![
            (
                "wrong_permission_snapshot",
                in_process(|request| {
                    let mut response = context_reducer_response(&request);
                    response.permission_snapshot_ref = "perm-snap-2".to_string();
                    Ok(response)
                }),
            ),
            (
                "wrong_reducer_id",
                in_process(|request| {
                    let mut response = context_reducer_response(&request);
                    response.reducer_id = "other-adapter".to_string();
                    Ok(response)
                }),
            ),
            (
                "wrong_reducer_version",
                in_process(|request| {
                    let mut response = context_reducer_response(&request);
                    response.reducer_version = "9.9.9".to_string();
                    Ok(response)
                }),
            ),
            (
                "wrong_content_kind",
                in_process(|request| {
                    let mut response = context_reducer_response(&request);
                    response.content_kind = ContextReducerContentKind::Text;
                    Ok(response)
                }),
            ),
        ];

        for (name, executor) in cases {
            let outcome = execute_context_reducer(
                &config,
                &descriptor,
                context_reducer_request(),
                Some(executor),
                native_response,
            );
            assert!(outcome.used_native_fallback, "{name}");
            assert_eq!(
                outcome.response.reducer_id, "viden-context-native",
                "{name}"
            );
            assert_eq!(
                outcome.health.status,
                ContextReducerHealthStatus::BindingMismatch,
                "{name}"
            );
        }
    }

    #[test]
    fn context_reducer_times_out_sleeping_executor_near_host_deadline() {
        let config = ContextReducerAdapterConfig {
            enabled: true,
            timeout_ms: 25,
            ..ContextReducerAdapterConfig::default()
        };
        // Deterministic by construction instead of by wall clock: the adapter
        // blocks on a channel this test owns, so it cannot return before
        // `execute_context_reducer` does, whatever the machine load. The old
        // `elapsed() < 125ms` bound raced the host's own 25ms deadline plus
        // thread spawn under a parallel workspace run.
        let (release, blocked) = std::sync::mpsc::channel::<()>();
        let blocked = std::sync::Mutex::new(blocked);

        let outcome = execute_context_reducer(
            &config,
            &context_reducer_descriptor("adapter"),
            context_reducer_request(),
            Some(in_process(move |request| {
                let _ = blocked.lock().expect("adapter mutex is uncontended").recv();
                Ok(context_reducer_response(&request))
            })),
            native_response,
        );

        // Releasing only now proves the host returned while the adapter was
        // still blocked; it also lets the isolated adapter thread exit.
        drop(release);
        assert!(outcome.used_native_fallback);
        assert_eq!(outcome.response.reducer_id, "viden-context-native");
        assert_eq!(outcome.health.status, ContextReducerHealthStatus::Timeout);
        assert!(outcome.health.message.as_deref().unwrap_or("").len() < 128);
    }

    #[test]
    fn context_reducer_catches_executor_panic_as_crash() {
        let config = ContextReducerAdapterConfig {
            enabled: true,
            timeout_ms: 100,
            ..ContextReducerAdapterConfig::default()
        };

        let outcome = execute_context_reducer(
            &config,
            &context_reducer_descriptor("adapter"),
            context_reducer_request(),
            Some(in_process(|_request| panic!("adapter panic"))),
            native_response,
        );

        assert!(outcome.used_native_fallback);
        assert_eq!(outcome.response.reducer_id, "viden-context-native");
        assert_eq!(outcome.health.status, ContextReducerHealthStatus::Crash);
    }

    #[test]
    fn context_reducer_process_executor_accepts_successful_json_response() {
        let helper = compile_context_reducer_helper();
        let config = ContextReducerAdapterConfig {
            enabled: true,
            timeout_ms: 10_000,
            ..ContextReducerAdapterConfig::default()
        };
        let descriptor = context_reducer_descriptor("adapter");
        let process = process_descriptor(&helper, "success");

        let outcome = execute_context_reducer(
            &config,
            &descriptor,
            process_request_with_limit(4096),
            Some(ContextReducerExecutor::process(process)),
            native_response,
        );

        assert!(
            !outcome.used_native_fallback,
            "unexpected fallback: {:?}",
            outcome.health
        );
        assert_eq!(outcome.health.status, ContextReducerHealthStatus::Ok);
        assert!(outcome.health.latency_ms < 10_000);
        assert_eq!(outcome.response.reducer_id, "adapter");
    }

    #[test]
    fn context_reducer_registered_process_descriptor_executes_when_approved() {
        let helper = compile_context_reducer_helper();
        let mut descriptor = context_reducer_descriptor("adapter");
        descriptor.process = Some(process_descriptor(&helper, "success"));
        let mut registry = StaticPluginRegistry::new();
        registry.register_context_reducer(descriptor).unwrap();
        let config = ContextReducerAdapterConfig {
            enabled: true,
            preferred_reducer_id: Some("adapter".to_string()),
            timeout_ms: 10_000,
            ..ContextReducerAdapterConfig::default()
        };
        let selected = registry
            .negotiate_context_reducer(
                &config,
                ContextReducerContentKind::Log,
                CONTEXT_REDUCER_SCHEMA_VERSION,
            )
            .unwrap();
        let process = selected.process.clone().expect("registered process");

        let outcome = execute_context_reducer(
            &config,
            selected,
            process_request_with_limit(4096),
            Some(ContextReducerExecutor::process(process)),
            native_response,
        );

        assert!(
            !outcome.used_native_fallback,
            "unexpected fallback: {:?}",
            outcome.health
        );
        assert_eq!(outcome.response.reducer_id, "adapter");
    }

    #[test]
    fn context_reducer_process_timeout_kills_and_reaps_child_before_fallback() {
        let helper = compile_context_reducer_helper();
        let marker = helper.with_file_name("late-marker");
        let pid_file = helper.with_file_name("sleep-pid");
        let config = ContextReducerAdapterConfig {
            enabled: true,
            timeout_ms: 4_000,
            ..ContextReducerAdapterConfig::default()
        };
        let descriptor = context_reducer_descriptor("adapter");
        let executable = helper.canonicalize().unwrap().display().to_string();
        let process = ContextReducerProcessDescriptor {
            executable: executable.clone(),
            args: vec![
                "sleep".to_string(),
                marker.display().to_string(),
                pid_file.display().to_string(),
            ],
            cwd: None,
            trusted_root: helper
                .parent()
                .unwrap()
                .canonicalize()
                .unwrap()
                .display()
                .to_string(),
            env_allowlist: Vec::new(),
            max_stderr_bytes: 128,
            authorization: Some(ContextReducerProcessAuthorization {
                adapter_id: "adapter".to_string(),
                adapter_version: "0.1.0".to_string(),
                executable_identity: executable,
                permission_snapshot_ref: "plugin-install:adapter:approved".to_string(),
            }),
        };
        let started = std::time::Instant::now();

        let outcome = execute_context_reducer(
            &config,
            &descriptor,
            context_reducer_request(),
            Some(ContextReducerExecutor::process(process)),
            native_response,
        );

        // The witness that the host did not wait for the child is the absent
        // marker below, which the helper writes only after a 20s sleep; this
        // ceiling is a generous liveness bound, not the assertion that matters,
        // so a loaded host cannot flake it the way `< 4_500ms` did.
        assert!(started.elapsed() < std::time::Duration::from_secs(15));
        assert!(outcome.used_native_fallback);
        assert_eq!(outcome.health.status, ContextReducerHealthStatus::Timeout);
        assert_eq!(outcome.response.reducer_id, "viden-context-native");
        std::thread::sleep(std::time::Duration::from_millis(100));
        assert!(
            !marker.exists(),
            "timed-out child must be killed before late mutation"
        );
        let pid = std::fs::read_to_string(&pid_file).expect("helper writes pid before sleep");
        let ps = std::process::Command::new("ps")
            .arg("-p")
            .arg(pid.trim())
            .output()
            .expect("ps is available on unix-like test host");
        assert!(
            !String::from_utf8_lossy(&ps.stdout).contains(pid.trim()),
            "timed-out direct child must be reaped before host returns"
        );
    }

    #[test]
    fn context_reducer_process_absent_nonzero_malformed_and_oversize_fallback() {
        let helper = compile_context_reducer_helper();
        let config = ContextReducerAdapterConfig {
            enabled: true,
            timeout_ms: 10_000,
            ..ContextReducerAdapterConfig::default()
        };
        let descriptor = context_reducer_descriptor("adapter");
        let cases = vec![
            (
                "absent",
                ContextReducerProcessDescriptor {
                    executable: helper
                        .with_file_name("missing-helper")
                        .display()
                        .to_string(),
                    args: Vec::new(),
                    cwd: None,
                    trusted_root: helper
                        .parent()
                        .unwrap()
                        .canonicalize()
                        .unwrap()
                        .display()
                        .to_string(),
                    env_allowlist: Vec::new(),
                    max_stderr_bytes: 128,
                    authorization: Some(ContextReducerProcessAuthorization {
                        adapter_id: "adapter".to_string(),
                        adapter_version: "0.1.0".to_string(),
                        executable_identity: helper
                            .with_file_name("missing-helper")
                            .display()
                            .to_string(),
                        permission_snapshot_ref: "plugin-install:adapter:approved".to_string(),
                    }),
                },
                ContextReducerHealthStatus::AdapterAbsent,
            ),
            (
                "nonzero",
                process_descriptor(&helper, "nonzero"),
                ContextReducerHealthStatus::Crash,
            ),
            (
                "malformed",
                process_descriptor(&helper, "malformed"),
                ContextReducerHealthStatus::Malformed,
            ),
            (
                "oversize",
                process_descriptor(&helper, "oversize"),
                ContextReducerHealthStatus::Oversize,
            ),
        ];

        for (name, process, expected_status) in cases {
            let outcome = execute_context_reducer(
                &config,
                &descriptor,
                process_request_with_limit(4096),
                Some(ContextReducerExecutor::process(process)),
                native_response,
            );
            assert!(outcome.used_native_fallback, "{name}");
            assert_eq!(outcome.health.status, expected_status, "{name}");
            assert_eq!(
                outcome.response.reducer_id, "viden-context-native",
                "{name}"
            );
            let message = outcome.health.message.unwrap_or_default();
            assert!(!message.contains("/Users/wiki"), "{name}");
            assert!(!message.contains("sk-test-secret"), "{name}");
            assert!(message.len() <= 200, "{name}");
        }
    }

    #[test]
    fn context_reducer_process_stdout_limit_uses_smallest_request_policy_bound() {
        let helper = compile_context_reducer_helper();
        let config = ContextReducerAdapterConfig {
            enabled: true,
            timeout_ms: 10_000,
            max_output_bytes: 4096,
            ..ContextReducerAdapterConfig::default()
        };

        let outcome = execute_context_reducer(
            &config,
            &context_reducer_descriptor("adapter"),
            process_request_with_limit(64),
            Some(ContextReducerExecutor::process(process_descriptor(
                &helper, "success",
            ))),
            native_response,
        );

        assert!(outcome.used_native_fallback);
        assert_eq!(outcome.health.status, ContextReducerHealthStatus::Oversize);
        assert_eq!(
            outcome.health.message.as_deref(),
            Some("context reducer response exceeds byte limit")
        );
    }

    #[test]
    fn context_reducer_process_rejects_oversize_request_envelope_before_spawn() {
        let helper = compile_context_reducer_helper();
        let marker = helper.with_file_name("oversize-request-spawned");
        let mut process = process_descriptor(&helper, "mark_success");
        process.args.push(marker.display().to_string());
        let mut request = process_request_with_limit(4096);
        request.policy.required_markers =
            vec!["x".repeat(CONTEXT_REDUCER_PROCESS_REQUEST_HARD_CAP_BYTES)];

        let outcome = execute_context_reducer(
            &ContextReducerAdapterConfig {
                enabled: true,
                timeout_ms: 10_000,
                ..ContextReducerAdapterConfig::default()
            },
            &context_reducer_descriptor("adapter"),
            request,
            Some(ContextReducerExecutor::process(process)),
            native_response,
        );

        assert!(outcome.used_native_fallback);
        assert_eq!(
            outcome.health.status,
            ContextReducerHealthStatus::PolicyRejected
        );
        assert!(
            !marker.exists(),
            "oversize request must be rejected before spawn"
        );
    }

    #[test]
    fn context_reducer_process_stdout_global_cap_kills_and_reaps_child_on_sentinel() {
        let helper = compile_context_reducer_helper();
        let marker = helper.with_file_name("oversize-marker");
        let pid_file = helper.with_file_name("oversize-pid");
        let mut descriptor = context_reducer_descriptor("adapter");
        descriptor.limits.max_output_bytes = usize::MAX;
        let mut process = process_descriptor(&helper, "stream_oversize");
        process.args.push(marker.display().to_string());
        process.args.push(pid_file.display().to_string());
        let config = ContextReducerAdapterConfig {
            enabled: true,
            timeout_ms: 10_000,
            max_output_bytes: usize::MAX,
            ..ContextReducerAdapterConfig::default()
        };
        let started = std::time::Instant::now();

        let outcome = execute_context_reducer(
            &config,
            &descriptor,
            process_request_with_limit(usize::MAX),
            Some(ContextReducerExecutor::process(process)),
            native_response,
        );

        // As above: `Oversize` plus the absent 20s marker prove the host stopped
        // on the sentinel rather than on its own 10s deadline, so the elapsed
        // bound only has to be generous.
        assert!(started.elapsed() < std::time::Duration::from_secs(15));
        assert!(outcome.used_native_fallback);
        assert_eq!(outcome.health.status, ContextReducerHealthStatus::Oversize);
        std::thread::sleep(std::time::Duration::from_millis(100));
        assert!(
            !marker.exists(),
            "oversize child must be killed before late mutation"
        );
        let pid = std::fs::read_to_string(&pid_file).expect("helper writes pid before stream");
        let ps = std::process::Command::new("ps")
            .arg("-p")
            .arg(pid.trim())
            .output()
            .expect("ps is available on unix-like test host");
        assert!(
            !String::from_utf8_lossy(&ps.stdout).contains(pid.trim()),
            "oversize direct child must be reaped before host returns"
        );
    }

    #[cfg(unix)]
    #[test]
    fn context_reducer_process_stdin_is_private_request_file_not_pipe() {
        let helper = compile_context_reducer_helper();
        let config = ContextReducerAdapterConfig {
            enabled: true,
            timeout_ms: 10_000,
            ..ContextReducerAdapterConfig::default()
        };

        let outcome = execute_context_reducer(
            &config,
            &context_reducer_descriptor("adapter"),
            process_request_with_limit(4096),
            Some(ContextReducerExecutor::process(process_descriptor(
                &helper,
                "stdin_file",
            ))),
            native_response,
        );

        assert!(
            !outcome.used_native_fallback,
            "adapter should read the request from a regular stdin file"
        );
        assert_eq!(outcome.response.reducer_id, "adapter");
    }

    #[test]
    fn context_reducer_process_does_not_wait_for_descendant_held_stdout_eof() {
        let helper = compile_context_reducer_helper();
        let marker = helper.with_file_name("descendant-stdout-marker");
        let mut process = process_descriptor(&helper, "descendant_stdout");
        process.args.push(marker.display().to_string());
        let config = ContextReducerAdapterConfig {
            enabled: true,
            timeout_ms: 10_000,
            ..ContextReducerAdapterConfig::default()
        };
        let started = std::time::Instant::now();

        let outcome = execute_context_reducer(
            &config,
            &context_reducer_descriptor("adapter"),
            process_request_with_limit(4096),
            Some(ContextReducerExecutor::process(process)),
            native_response,
        );

        // The descendant holds the inherited stdout for 20s, so returning inside
        // this window is itself the proof that the host waited for the direct
        // child's exit rather than for stdout EOF.
        assert!(started.elapsed() < std::time::Duration::from_secs(15));
        assert!(
            !outcome.used_native_fallback,
            "direct child emitted a valid response before descendant retained stdout"
        );
        assert_eq!(outcome.response.reducer_id, "adapter");
    }

    #[cfg(unix)]
    #[test]
    fn context_reducer_process_timeout_kills_descendant_process_group() {
        let helper = compile_context_reducer_helper();
        let marker = helper.with_file_name("timeout-descendant-marker");
        let mut process = process_descriptor(&helper, "timeout_descendant");
        process.args.push(marker.display().to_string());
        let config = ContextReducerAdapterConfig {
            enabled: true,
            timeout_ms: 4_000,
            ..ContextReducerAdapterConfig::default()
        };

        let outcome = execute_context_reducer(
            &config,
            &context_reducer_descriptor("adapter"),
            context_reducer_request(),
            Some(ContextReducerExecutor::process(process)),
            native_response,
        );

        assert!(outcome.used_native_fallback);
        assert_eq!(outcome.health.status, ContextReducerHealthStatus::Timeout);
        std::thread::sleep(std::time::Duration::from_millis(7_000));
        assert!(
            !marker.exists(),
            "timeout must kill adapter process group before descendant writes"
        );
    }

    #[test]
    fn context_reducer_process_temp_io_artifacts_are_cleaned_after_failure() {
        let io = ProcessAdapterIo::new(b"{}").expect("temp io can be created");
        let dir = io.dir_for_test().to_path_buf();
        assert!(dir.exists());
        drop(io);
        assert!(
            !dir.exists(),
            "private process io directory must be removed"
        );
    }

    #[test]
    fn context_reducer_process_stderr_health_is_separately_hard_capped() {
        let helper = compile_context_reducer_helper();
        let mut process = process_descriptor(&helper, "stderr_large_nonzero");
        process.max_stderr_bytes = 32;
        let config = ContextReducerAdapterConfig {
            enabled: true,
            timeout_ms: 10_000,
            ..ContextReducerAdapterConfig::default()
        };

        let outcome = execute_context_reducer(
            &config,
            &context_reducer_descriptor("adapter"),
            process_request_with_limit(4096),
            Some(ContextReducerExecutor::process(process)),
            native_response,
        );

        assert!(outcome.used_native_fallback);
        assert_eq!(outcome.health.status, ContextReducerHealthStatus::Crash);
        let message = outcome.health.message.unwrap_or_default();
        assert!(message.len() <= 96, "{message}");
        assert!(!message.contains("/Users/"));
        assert!(!message.contains("sk-test-secret"));
    }

    #[test]
    fn context_reducer_circuit_breaker_skips_repeated_failures() {
        let config = ContextReducerAdapterConfig {
            enabled: true,
            circuit_breaker: ContextReducerCircuitBreakerConfig {
                failure_threshold: 2,
                backoff_ms: 30_000,
            },
            ..ContextReducerAdapterConfig::default()
        };
        let descriptor = context_reducer_descriptor("adapter");
        let mut breaker = ContextReducerCircuitBreaker::default();

        for _ in 0..2 {
            let outcome = execute_context_reducer_with_breaker(
                &config,
                &descriptor,
                context_reducer_request(),
                Some(in_process(|_| Err(ContextReducerHostError::AdapterCrash))),
                native_response,
                &mut breaker,
            );
            assert!(outcome.used_native_fallback);
        }

        let outcome = execute_context_reducer_with_breaker(
            &config,
            &descriptor,
            context_reducer_request(),
            Some(in_process(|request| Ok(context_reducer_response(&request)))),
            native_response,
            &mut breaker,
        );

        assert!(outcome.used_native_fallback);
        assert_eq!(
            outcome.health.status,
            ContextReducerHealthStatus::CircuitOpen
        );
        assert_eq!(breaker.failure_count("adapter"), 2);
    }

    #[test]
    fn context_reducer_circuit_breaker_uses_backoff_and_half_open_probe() {
        let config = ContextReducerAdapterConfig {
            enabled: true,
            circuit_breaker: ContextReducerCircuitBreakerConfig {
                failure_threshold: 2,
                backoff_ms: 30,
            },
            ..ContextReducerAdapterConfig::default()
        };
        let descriptor = context_reducer_descriptor("adapter");
        let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let mut breaker = ContextReducerCircuitBreaker::default();

        for _ in 0..2 {
            let calls = std::sync::Arc::clone(&calls);
            let outcome = execute_context_reducer_with_breaker(
                &config,
                &descriptor,
                context_reducer_request(),
                Some(in_process(move |_request| {
                    calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    Err(ContextReducerHostError::AdapterCrash)
                })),
                native_response,
                &mut breaker,
            );
            assert!(outcome.used_native_fallback);
        }
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 2);

        let calls_for_skipped = std::sync::Arc::clone(&calls);
        let skipped = execute_context_reducer_with_breaker(
            &config,
            &descriptor,
            context_reducer_request(),
            Some(in_process(move |request| {
                calls_for_skipped.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Ok(context_reducer_response(&request))
            })),
            native_response,
            &mut breaker,
        );
        assert_eq!(
            skipped.health.status,
            ContextReducerHealthStatus::CircuitOpen
        );
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 2);

        std::thread::sleep(std::time::Duration::from_millis(40));
        let calls_for_probe = std::sync::Arc::clone(&calls);
        let probe = execute_context_reducer_with_breaker(
            &config,
            &descriptor,
            context_reducer_request(),
            Some(in_process(move |request| {
                calls_for_probe.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Ok(context_reducer_response(&request))
            })),
            native_response,
            &mut breaker,
        );
        assert!(!probe.used_native_fallback);
        assert_eq!(breaker.failure_count("adapter"), 0);
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 3);

        for _ in 0..2 {
            let outcome = execute_context_reducer_with_breaker(
                &config,
                &descriptor,
                context_reducer_request(),
                Some(in_process(|_| Err(ContextReducerHostError::AdapterCrash))),
                native_response,
                &mut breaker,
            );
            assert!(outcome.used_native_fallback);
        }
        std::thread::sleep(std::time::Duration::from_millis(40));
        let failed_probe = execute_context_reducer_with_breaker(
            &config,
            &descriptor,
            context_reducer_request(),
            Some(in_process(|_| Err(ContextReducerHostError::AdapterCrash))),
            native_response,
            &mut breaker,
        );
        assert!(failed_probe.used_native_fallback);
        let reopened = execute_context_reducer_with_breaker(
            &config,
            &descriptor,
            context_reducer_request(),
            Some(in_process(|request| Ok(context_reducer_response(&request)))),
            native_response,
            &mut breaker,
        );
        assert_eq!(
            reopened.health.status,
            ContextReducerHealthStatus::CircuitOpen
        );
    }

    #[test]
    fn context_reducer_request_validation_rejects_paths_and_secrets_before_adapter_call() {
        let config = ContextReducerAdapterConfig {
            enabled: true,
            ..ContextReducerAdapterConfig::default()
        };
        let descriptor = context_reducer_descriptor("adapter");
        let mut request = context_reducer_request();
        request.canonical.reference = "/Users/wiki/private/context".to_string();
        request.permission_snapshot_ref = "sk-test-secret".to_string();

        let outcome = execute_context_reducer(
            &config,
            &descriptor,
            request,
            Some(in_process(|request| Ok(context_reducer_response(&request)))),
            native_response,
        );

        assert!(outcome.used_native_fallback);
        assert_eq!(
            outcome.health.status,
            ContextReducerHealthStatus::PolicyRejected
        );
    }
}
