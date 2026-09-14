//! Tauri commands. Thin wrappers over [`AppState`]; all logic lives in state.
//!
//! Every command returns `Result<T, String>`; the error string is derived from
//! [`HostError`] and never contains secrets.

use crate::events::TauriEventSink;
use crate::state::{
    AgentOutcomeView, AppState, AuditStatusView, LlmConfigStatus, LlmReadiness, LocalProbeResult,
    ProviderPresetView, SessionDetailView, SnapshotMetaView, StoredEventView, ToolchainView,
};
use crate::SessionMeta;
use std::sync::Arc;
use tauri::State;

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

/// Is a VM currently held by the host (i.e. kept alive across runs)?
#[tauri::command]
pub async fn vm_is_running(state: State<'_, AppState>) -> Result<bool, String> {
    Ok(state.vm_is_running())
}

/// Where the RISC-V GCC toolchain is (and the full search record).
#[tauri::command]
pub async fn probe_toolchain(state: State<'_, AppState>) -> Result<ToolchainView, String> {
    Ok(state.probe_toolchain())
}

/// Point the app at a specific RISC-V GCC (validated with `--version`).
#[tauri::command]
pub async fn set_toolchain_path(state: State<'_, AppState>, path: String) -> Result<(), String> {
    state
        .set_toolchain_path(&path)
        .map_err(|e| e.user_message())
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

/// Audit event count + chain status.
#[tauri::command]
pub async fn get_audit_status(state: State<'_, AppState>) -> Result<AuditStatusView, String> {
    state.audit_status().map_err(|e| e.user_message())
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

/// Run one agent turn, streaming events to the webview.
#[tauri::command]
pub async fn run_agent(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    user_input: String,
) -> Result<AgentOutcomeView, String> {
    let emitter: Arc<dyn crate::events::EventSink> = Arc::new(TauriEventSink::new(app));
    state
        .run_agent(emitter, &user_input)
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
