//! Tauri commands. Thin wrappers over [`AppState`]; all logic lives in state.
//!
//! Every command returns `Result<T, String>`; the error string is derived from
//! [`HostError`] and never contains secrets.

use crate::events::TauriEventSink;
use crate::events::{EV_QEMU_DOWNLOAD, TOOLCHAIN_DOWNLOAD};
use crate::preflight::PreflightView;
use crate::run_diff::FingerprintFieldDiff;
use crate::state::QemuView;
use crate::state::{
    AgentOutcomeView, AppState, AuditStatusView, LlmConfigStatus, LlmReadiness, LocalProbeResult,
    ProviderPresetView, QemuDownloadStatus, RunView, SessionDetailView, SnapshotMetaView,
    StoredEventView, ToolchainDownloadStatus, ToolchainView, VmStatusView,
};
use crate::SessionMeta;
use crate::{CandidatesView, SandboxRequestView, SandboxView};
use host_core::SandboxAction;
use host_core::UnpackReport;
use std::sync::Arc;
use tauri::{Manager, State};

/// Start the one-click RISC-V GCC download.
#[tauri::command]
pub async fn start_toolchain_download(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let spec = crate::toolchain_download::spec_for_current_platform().map_err(|e| e.to_string())?;
    let cancel = state
        .begin_toolchain_download(&spec)
        .map_err(|e| e.user_message())?;

    // The download is blocking (`reqwest::blocking`), so keep it off the async
    // runtime; the app handle gives the worker access to the managed state.
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let emitter: Arc<dyn crate::events::EventSink> =
            Arc::new(TauriEventSink::new(app.clone(), state.agent_id()));
        let mut on_event = |event: crate::toolchain_download::DownloadEvent| {
            if let Ok(payload) = serde_json::to_value(&event) {
                emitter.emit(TOOLCHAIN_DOWNLOAD, payload);
            }
        };
        // v0.8: the destination belongs to the instance, not to a process-wide
        // default, so two hosts in one process cannot overwrite each other.
        let dest_root = state.toolchain_dir();
        if let Err(e) = state.download_toolchain_now(&spec, &dest_root, cancel, &mut on_event) {
            eprintln!("toolchain download failed: {e}");
        }
    });
    Ok(())
}

/// Ask an in-flight toolchain download to stop.
#[tauri::command]
pub async fn cancel_toolchain_download(state: State<'_, AppState>) -> Result<(), String> {
    state
        .cancel_toolchain_download()
        .map_err(|e| e.user_message())
}

/// Whether a download is running, plus the last event seen.
#[tauri::command]
pub async fn toolchain_download_status(
    state: State<'_, AppState>,
) -> Result<ToolchainDownloadStatus, String> {
    Ok(state.toolchain_download_status())
}

/// Start the one-click QEMU download.
///
/// Today every platform answers `Err` with the install guidance: the project
/// guides users to a QEMU they install themselves (`docs/qemu-distribution.md` §5)
/// and pins no release, so the spec lookup is what refuses. The rest of this
/// command is the toolchain's shape, so pinning a release later needs no code here.
#[tauri::command]
pub async fn start_qemu_download(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let spec = crate::qemu_download::spec_for_current_platform().map_err(|e| e.to_string())?;
    let cancel = state
        .begin_qemu_download(&spec)
        .map_err(|e| e.user_message())?;

    // The download is blocking (`reqwest::blocking`), so keep it off the async
    // runtime; the app handle gives the worker access to the managed state.
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let emitter: Arc<dyn crate::events::EventSink> =
            Arc::new(TauriEventSink::new(app.clone(), state.agent_id()));
        let mut on_event = |event: crate::qemu_download::QemuDownloadEvent| {
            if let Ok(payload) = serde_json::to_value(&event) {
                emitter.emit(EV_QEMU_DOWNLOAD, payload);
            }
        };
        // The destination belongs to the instance, like the toolchain's.
        // A failure is carried by `host.qemu.download.failed` and by the status
        // the next caller reads; this closure has no caller left to tell.
        let dest_root = state.qemu_dir();
        let _ = state.download_qemu_now(&spec, &dest_root, cancel, &mut on_event);
    });
    Ok(())
}

/// Ask an in-flight QEMU download to stop.
#[tauri::command]
pub async fn cancel_qemu_download(state: State<'_, AppState>) -> Result<(), String> {
    state.cancel_qemu_download().map_err(|e| e.user_message())
}

/// Whether a QEMU download is running, plus the last event seen.
#[tauri::command]
pub async fn qemu_download_status(
    state: State<'_, AppState>,
) -> Result<QemuDownloadStatus, String> {
    Ok(state.qemu_download_status())
}

/// The merged sandbox registry: hand-written, then scanned, then the fallback.
///
/// Read-only (v0.9 sandbox F2a-2). The interface is not wired to these commands in
/// this batch.
#[tauri::command]
pub async fn list_sandboxes(state: State<'_, AppState>) -> Result<Vec<SandboxView>, String> {
    Ok(state.sandboxes())
}

/// The definition a run would use, and the fallback's name.
#[tauri::command]
pub async fn current_sandbox(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    Ok(serde_json::json!({
        "current": state.current_sandbox(),
        "default": state.sandbox_default_name(),
    }))
}

/// The raw scan: what is installed here, plus this machine's QEMU.
///
/// Not the registry: nothing in the answer is a definition, and nothing was
/// written to `settings.json`.
#[tauri::command]
pub async fn sandbox_candidates(state: State<'_, AppState>) -> Result<CandidatesView, String> {
    Ok(state.sandbox_candidates())
}

/// One definition by name, or `null` when no sandbox has that name.
#[tauri::command]
pub async fn get_sandbox(
    state: State<'_, AppState>,
    name: String,
) -> Result<Option<SandboxView>, String> {
    Ok(state.sandbox(&name))
}

/// Switch this node to another sandbox definition (v0.9 sandbox F2b-2).
///
/// Blocking, the way a snapshot restore is: validation, a stop and a start are
/// seconds of work and the outcome is what the caller asked for, so it is not
/// pushed onto a worker and answered with an acknowledgement. The
/// `sandbox:switch` event reaches the interface either way; the interface is not
/// wired to this command in this batch (that is the D line).
#[tauri::command]
pub async fn switch_sandbox(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    name: String,
) -> Result<(), String> {
    let emitter: Arc<dyn crate::events::EventSink> =
        Arc::new(TauriEventSink::new(app.clone(), state.agent_id()));
    state
        .switch_sandbox(&name, emitter)
        .map_err(|e| e.user_message())
}

/// The request queue (v0.9 sandbox F2c), newest first.
///
/// `status` filters by `pending` / `approved` / `rejected` (`expired` is reserved:
/// v0.9 sets no TTL); `none` is the whole queue. The interface is not wired to
/// these commands in this batch (that is the D line).
#[tauri::command]
pub async fn list_sandbox_requests(
    state: State<'_, AppState>,
    status: Option<String>,
) -> Result<Vec<SandboxRequestView>, String> {
    let status = match status.as_deref() {
        None => None,
        Some(raw) => Some(
            host_core::SandboxRequestStatus::parse(raw)
                .ok_or_else(|| format!("unknown status {raw:?}"))?,
        ),
    };
    Ok(state.list_sandbox_requests(status))
}

/// Leave a request for a sandbox change (v0.9 sandbox F2c).
///
/// Nothing is switched: the ask waits for an actor that holds the capability its
/// `action` implies. Answers the new id, and publishes `sandbox:request`.
#[tauri::command]
pub async fn request_sandbox(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    action: String,
    sandbox: Option<String>,
    reason: Option<String>,
) -> Result<String, String> {
    let action =
        SandboxAction::parse(&action).ok_or_else(|| format!("unknown action {action:?}"))?;
    let emitter: Arc<dyn crate::events::EventSink> =
        Arc::new(TauriEventSink::new(app.clone(), state.agent_id()));
    state
        .request_sandbox(
            state.agent_id(),
            action,
            sandbox,
            // The interface does not write definitions yet (the assemble endpoint
            // is a later batch), so an ask never carries one.
            None,
            reason,
            emitter,
        )
        .map(|view| view.id)
        .map_err(|e| e.user_message())
}

/// Approve a pending request. Changes the record and nothing else.
#[tauri::command]
pub async fn approve_sandbox_request(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<SandboxRequestView, String> {
    let emitter: Arc<dyn crate::events::EventSink> =
        Arc::new(TauriEventSink::new(app.clone(), state.agent_id()));
    state
        .approve_sandbox_request(&id, state.agent_id(), emitter)
        .map_err(|e| e.user_message())
}

/// Reject a pending request.
#[tauri::command]
pub async fn reject_sandbox_request(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<SandboxRequestView, String> {
    let emitter: Arc<dyn crate::events::EventSink> =
        Arc::new(TauriEventSink::new(app.clone(), state.agent_id()));
    state
        .reject_sandbox_request(&id, state.agent_id(), emitter)
        .map_err(|e| e.user_message())
}

/// Bring a project in: unpack an archive into the workspace (v0.9 project in/out).
///
/// The bytes are the archive itself (Tauri moves them as a byte array), and the
/// format is read from them rather than from a parameter — the desktop shell has no
/// `Content-Type` to pass along, and reading the magic bytes is what the HTTP
/// endpoint falls back to as well. `force` replaces files that are already there;
/// without it the host refuses and the answer names the first clash.
///
/// The interface is not wired to this command in this batch (that is the D line).
#[tauri::command]
pub async fn import_workspace(
    state: State<'_, AppState>,
    archive: Vec<u8>,
    force: Option<bool>,
) -> Result<UnpackReport, String> {
    let format = host_core::ArchiveFormat::from_magic(&archive)
        .ok_or_else(|| "the file is neither a zip nor a tar/tar.gz archive".to_string())?;
    state
        .import_workspace(&archive, format, force.unwrap_or(false))
        .map_err(|e| e.user_message())
}

/// Take the project out: the workspace as a `tar.gz` (v0.9 project in/out).
#[tauri::command]
pub async fn export_workspace(state: State<'_, AppState>) -> Result<Vec<u8>, String> {
    state.export_workspace().map_err(|e| e.user_message())
}

/// List snapshots on disk (real `.mig` and reboot-fallback `.json`).
#[tauri::command]
pub async fn list_snapshots(state: State<'_, AppState>) -> Result<Vec<SnapshotMetaView>, String> {
    state.list_snapshots().map_err(|e| e.user_message())
}

/// Delete a snapshot by name.
#[tauri::command]
pub async fn delete_snapshot(state: State<'_, AppState>, name: String) -> Result<bool, String> {
    state.delete_snapshot(&name).map_err(|e| e.user_message())
}

/// Stop the host-owned VM (no-op when none is running).
#[tauri::command]
pub async fn stop_current_vm(state: State<'_, AppState>) -> Result<(), String> {
    state.stop_current_vm().map_err(|e| e.user_message())
}

/// Where QEMU is (and the full search record).
#[tauri::command]
pub async fn probe_qemu(state: State<'_, AppState>) -> Result<QemuView, String> {
    Ok(state.probe_qemu())
}

/// Same as `probe_qemu`; the UI reads it on mount.
#[tauri::command]
pub async fn get_qemu_status(state: State<'_, AppState>) -> Result<QemuView, String> {
    Ok(state.probe_qemu())
}

/// Point the app at a specific QEMU binary (validated with `--version`).
#[tauri::command]
pub async fn set_qemu_path(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> Result<(), String> {
    state.set_qemu_path(&path).map_err(|e| e.user_message())?;
    // Configuration changed: check the new environment in the background.
    spawn_preflight(app);
    Ok(())
}

/// Forget the manual QEMU path and go back to auto-discovery.
#[tauri::command]
pub async fn clear_qemu_path(state: State<'_, AppState>) -> Result<(), String> {
    state.clear_qemu_path().map_err(|e| e.user_message())
}
/// Is a VM currently held by the host (i.e. kept alive across runs)?
#[tauri::command]
pub async fn vm_is_running(state: State<'_, AppState>) -> Result<bool, String> {
    Ok(state.vm_is_running())
}

/// VM status for the top-bar badge: running + when it started.
#[tauri::command]
pub async fn vm_status(state: State<'_, AppState>) -> Result<VmStatusView, String> {
    Ok(state.vm_status())
}

/// Where the RISC-V GCC toolchain is (and the full search record).
#[tauri::command]
pub async fn probe_toolchain(state: State<'_, AppState>) -> Result<ToolchainView, String> {
    Ok(state.probe_toolchain())
}

/// Point the app at a specific RISC-V GCC (validated with `--version`).
#[tauri::command]
pub async fn set_toolchain_path(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> Result<(), String> {
    state
        .set_toolchain_path(&path)
        .map_err(|e| e.user_message())?;
    // Configuration changed: check the new environment in the background.
    spawn_preflight(app);
    Ok(())
}

/// Forget the manual path and go back to auto-discovery.
#[tauri::command]
pub async fn clear_toolchain_path(state: State<'_, AppState>) -> Result<(), String> {
    state.clear_toolchain_path().map_err(|e| e.user_message())
}

/// Save a real (tcp-relay) snapshot of the host-owned VM. Returns bytes written.
#[tauri::command]
pub async fn save_snapshot_real(state: State<'_, AppState>, name: String) -> Result<u64, String> {
    state
        .save_snapshot_real(&name)
        .map_err(|e| e.user_message())
}

/// Restore the VM from a real snapshot (stops the current VM first).
#[tauri::command]
pub async fn resume_from_snapshot_real(
    state: State<'_, AppState>,
    name: String,
) -> Result<(), String> {
    state
        .resume_from_snapshot_real(&name)
        .map_err(|e| e.user_message())
}

/// Recent sessions, newest first.
#[tauri::command]
pub async fn list_sessions(
    state: State<'_, AppState>,
    limit: usize,
) -> Result<Vec<SessionMeta>, String> {
    state.list_sessions(limit).map_err(|e| e.user_message())
}

/// Create a session and make it current.
#[tauri::command]
pub async fn create_session(state: State<'_, AppState>, title: String) -> Result<String, String> {
    state.create_session(&title).map_err(|e| e.user_message())
}

/// Open a session: returns its metadata and messages, and makes it current.
#[tauri::command]
pub async fn open_session(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<SessionDetailView, String> {
    state
        .open_session(&session_id)
        .map_err(|e| e.user_message())
}

/// Rename a session.
#[tauri::command]
pub async fn rename_session(
    state: State<'_, AppState>,
    session_id: String,
    title: String,
) -> Result<(), String> {
    state
        .rename_session(&session_id, &title)
        .map_err(|e| e.user_message())
}

/// Delete a session (its messages cascade).
#[tauri::command]
pub async fn delete_session(state: State<'_, AppState>, session_id: String) -> Result<(), String> {
    state
        .delete_session(&session_id)
        .map_err(|e| e.user_message())
}

/// Delete every session. The UI must ask for confirmation first.
#[tauri::command]
pub async fn clear_all_sessions(state: State<'_, AppState>) -> Result<(), String> {
    state.clear_all_sessions().map_err(|e| e.user_message())
}

/// The session the next run appends to.
#[tauri::command]
pub async fn get_current_session_id(state: State<'_, AppState>) -> Result<Option<String>, String> {
    Ok(state.current_session_id())
}

/// Audit event count + chain status, plus the pending write failures (v0.8).
///
/// The failures are **taken** by this call: the panel is told about each one
/// once, and anything not yet announced is sent as an `audit:failed` event first.
#[tauri::command]
pub async fn get_audit_status(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<AuditStatusView, String> {
    let mut view = state.audit_status().map_err(|e| e.user_message())?;
    let emitter: Arc<dyn crate::events::EventSink> =
        Arc::new(TauriEventSink::new(app, state.agent_id()));
    state.emit_audit_failures(emitter.as_ref());
    view.failures = state.take_audit_failures();
    Ok(view)
}

/// Turn the audit-failure alert (banner + popup) on or off (v0.8).
#[tauri::command]
pub async fn set_audit_alert(state: State<'_, AppState>, enabled: bool) -> Result<(), String> {
    state
        .set_alert_on_audit_failure(enabled)
        .map_err(|e| e.user_message())
}

/// The stored UI theme preference: `light` / `dark` / `system` (v0.4 #11a).
#[tauri::command]
pub async fn get_theme(state: State<'_, AppState>) -> Result<String, String> {
    Ok(state.theme())
}

/// Store the UI theme preference.
#[tauri::command]
pub async fn set_theme(state: State<'_, AppState>, theme: String) -> Result<(), String> {
    state.set_theme(&theme).map_err(|e| e.user_message())
}

/// The stored UI language preference: `system` / `en` / `zh` (v0.7 batch 2).
#[tauri::command]
pub async fn get_language(state: State<'_, AppState>) -> Result<String, String> {
    Ok(state.language())
}

/// Store the UI language preference.
#[tauri::command]
pub async fn set_language(state: State<'_, AppState>, language: String) -> Result<(), String> {
    state.set_language(&language).map_err(|e| e.user_message())
}

/// The environment preflight result for the current configuration (v0.4 batch 3).
#[tauri::command]
pub async fn preflight_status(state: State<'_, AppState>) -> Result<PreflightView, String> {
    Ok(state.preflight_status())
}

/// Run the preflight now. Returns immediately; progress arrives as
/// `preflight:progress` and the result is readable through `preflight_status`.
#[tauri::command]
pub async fn run_preflight(app: tauri::AppHandle) -> Result<(), String> {
    spawn_preflight(app);
    Ok(())
}

/// The escape hatch: accept this configuration as it is (recorded in settings).
#[tauri::command]
pub async fn acknowledge_preflight(state: State<'_, AppState>) -> Result<PreflightView, String> {
    state.acknowledge_preflight().map_err(|e| e.user_message())
}

/// Kick off the preflight in the background: it compiles and boots a guest, so it
/// must never run on the UI thread.
fn spawn_preflight(app: tauri::AppHandle) {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let emitter: Arc<dyn crate::events::EventSink> =
            Arc::new(TauriEventSink::new(app.clone(), state.agent_id()));
        if let Err(e) = state.ensure_preflight(true, Some(emitter)) {
            eprintln!("preflight failed: {e}");
        }
    });
}

/// Recent audit events (newest first), optionally filtered.
#[tauri::command]
pub async fn list_audit_events(
    state: State<'_, AppState>,
    limit: usize,
    actor: Option<String>,
    action_prefix: Option<String>,
) -> Result<Vec<StoredEventView>, String> {
    state
        .list_events(limit, actor, action_prefix)
        .map_err(|e| e.user_message())
}

/// Recent runs from the derived index (read-only, v0.4 1d).
#[tauri::command]
pub async fn list_runs(
    state: State<'_, AppState>,
    limit: Option<usize>,
) -> Result<Vec<RunView>, String> {
    state
        .list_runs(limit.unwrap_or(20))
        .map_err(|e| e.user_message())
}

/// One run by id, or `null` when this log has never seen it.
#[tauri::command]
pub async fn get_run(
    state: State<'_, AppState>,
    run_id: String,
) -> Result<Option<RunView>, String> {
    state.get_run(&run_id).map_err(|e| e.user_message())
}

/// Probe localhost for local OpenAI-compatible LLM servers.
#[tauri::command]
pub async fn probe_local_llm(state: State<'_, AppState>) -> Result<LocalProbeResult, String> {
    Ok(state.probe_local_llm())
}

/// Whether the LLM is ready to run, and why not.
#[tauri::command]
pub async fn get_llm_readiness(state: State<'_, AppState>) -> Result<LlmReadiness, String> {
    Ok(state.llm_readiness())
}

/// The built-in provider presets (for the settings dropdown).
#[tauri::command]
pub async fn get_provider_presets(
    state: State<'_, AppState>,
) -> Result<Vec<ProviderPresetView>, String> {
    Ok(state.provider_presets())
}

/// Store LLM config for this session (in memory only).
#[tauri::command]
pub async fn set_llm_config(
    state: State<'_, AppState>,
    api_key: String,
    base_url: String,
    model: String,
    provider_id: Option<String>,
    remember: Option<bool>,
) -> Result<(), String> {
    state
        .set_llm_config_with(provider_id, api_key, base_url, model, remember)
        .map_err(|e| e.user_message())
}

/// Does a key for `provider_id` exist in the OS keyring? (never returns the key)
#[tauri::command]
pub async fn has_stored_key(
    state: State<'_, AppState>,
    provider_id: String,
) -> Result<bool, String> {
    Ok(state.has_stored_key(&provider_id))
}

/// Load a stored key from the keyring into memory (startup restore).
#[tauri::command]
pub async fn load_stored_key(
    state: State<'_, AppState>,
    provider_id: String,
) -> Result<(), String> {
    state.load_stored_key(&provider_id)
}

/// Forget LLM config.
#[tauri::command]
pub async fn clear_llm_config(state: State<'_, AppState>) -> Result<(), String> {
    state.clear_llm_config();
    Ok(())
}

/// Whether an LLM is configured (never returns the key).
#[tauri::command]
pub async fn get_llm_config_status(state: State<'_, AppState>) -> Result<LlmConfigStatus, String> {
    Ok(state.llm_config_status())
}

/// Run one agent turn (v0.9 sandbox F2d adds the optional `sandbox` declaration).
///
/// The interface is not wired to the parameter in this batch (that is the D line);
/// the command carries it so the surface is complete when it is.
#[tauri::command]
pub async fn run_agent(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    user_input: String,
    sandbox: Option<String>,
) -> Result<AgentOutcomeView, String> {
    let emitter: Arc<dyn crate::events::EventSink> =
        Arc::new(TauriEventSink::new(app, state.agent_id()));
    state
        .run_agent_for(emitter, &user_input, sandbox.as_deref())
        .map_err(|e| e.user_message())
}

/// Workspace files (relative paths).
#[tauri::command]
pub async fn get_workspace_files(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    state.workspace_files().map_err(|e| e.user_message())
}

/// Read a workspace file (policy-checked).
#[tauri::command]
pub async fn read_workspace_file(
    state: State<'_, AppState>,
    path: String,
) -> Result<String, String> {
    state
        .read_workspace_file(path)
        .map_err(|e| e.user_message())
}

/// The accumulated serial output so far.
#[tauri::command]
pub async fn get_serial_buffer(state: State<'_, AppState>) -> Result<String, String> {
    Ok(state.serial_buffer())
}

/// The AI workspace root, as an absolute path (v0.5 batch 2).
#[tauri::command]
pub async fn workspace_root(state: State<'_, AppState>) -> Result<String, String> {
    Ok(state.workspace_root_display())
}

/// Write the serial log into the workspace. Returns bytes written.
#[tauri::command]
pub async fn export_serial_log(state: State<'_, AppState>, path: String) -> Result<usize, String> {
    state.export_serial_log(path).map_err(|e| e.user_message())
}

/// Export the audit log as JSONL into the workspace.
#[tauri::command]
pub async fn export_audit_jsonl(state: State<'_, AppState>, path: String) -> Result<usize, String> {
    state.export_audit_jsonl(path).map_err(|e| e.user_message())
}

/// Export **one run's** audit interval as JSONL into the workspace (v0.5 batch 1).
#[tauri::command]
pub async fn export_run_audit(
    state: State<'_, AppState>,
    run_id: String,
    path: String,
) -> Result<usize, String> {
    state
        .export_run_audit(&run_id, path)
        .map_err(|e| e.user_message())
}

/// Two runs' configuration fingerprints, field by field (v0.6 batch 1).
///
/// The rows come back in the order the fingerprint declares its fields, and every
/// field the two documents carry is in the list — unchanged ones included.
#[tauri::command]
pub async fn compare_run_fingerprints(
    state: State<'_, AppState>,
    run_a: String,
    run_b: String,
) -> Result<Vec<FingerprintFieldDiff>, String> {
    state
        .compare_run_fingerprints(&run_a, &run_b)
        .map_err(|e| e.user_message())
}

/// The executors this node can dispatch a task to (v0.9 interface E0).
///
/// Read-only, and not wired to the interface in this batch: the control plane's
/// `GET /v0/executors` is the surface this mirrors.
#[tauri::command]
pub async fn list_executors(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    Ok(state.executors())
}

/// Dispatch one task to the executor its `target` names (v0.9 interface E0).
///
/// Synchronous, like `POST /v0/tasks` and `run_agent`: the answer is the outcome.
/// A target nobody owns is an error (`String`), and a run that merely failed comes
/// back as an outcome whose `outcome` is `failed`.
///
/// Not wired to the interface in this batch.
#[tauri::command]
pub async fn dispatch_task(
    state: State<'_, AppState>,
    target: String,
    input: String,
    sandbox: Option<String>,
    id: Option<String>,
) -> Result<host_core::TaskOutcome, String> {
    state
        .dispatch_task(&target, &input, sandbox.as_deref(), id.as_deref())
        .map_err(|e| e.user_message())
}
