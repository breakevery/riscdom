//! Tauri commands. Thin wrappers over [`AppState`]; all logic lives in state.
//!
//! Every command returns `Result<T, String>`; the error string is derived from
//! [`HostError`] and never contains secrets.

use crate::events::TauriEventSink;
use crate::state::{
    AgentOutcomeView, AppState, AuditStatusView, LlmConfigStatus, LlmReadiness, LocalProbeResult,
    ProviderPresetView, StoredEventView,
};
use std::sync::Arc;
use tauri::State;

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
