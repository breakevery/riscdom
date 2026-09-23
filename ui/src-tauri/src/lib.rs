//! RiscDom Tauri shell library entry point.
//!
//! Registers `host-tauri`'s commands. No business logic lives here.

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // Native file pickers (v0.4 batch 2). Only `dialog:allow-open` is granted
        // in `capabilities/default.json`: the app picks files, it never asks the
        // dialog plugin to write one.
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            // Audit DB + AI workspace live under the app's data dir.
            let base = app
                .path()
                .app_data_dir()
                .unwrap_or_else(|_| std::env::temp_dir());
            let workspace = base.join("workspace");
            // Session data lives next to it, in app data (never in the repo).
            host_tauri::paths::set_app_data_dir(base.clone());
            // v0.8: pass the data directory in, so this instance owns it rather
            // than sharing one process-wide default.
            let state = host_tauri::AppState::with_data_dir(&workspace, base.clone())
                .map_err(|e| format!("failed to init host state: {e}"))?;
            // Long-lived serial forwarder: outlives individual runs so the UI
            // keeps receiving serial output across runs.
            let emitter: std::sync::Arc<dyn host_tauri::EventSink> = std::sync::Arc::new(
                host_tauri::events::TauriEventSink::new(app.handle().clone(), state.agent_id()),
            );
            state
                .start_serial_forwarder(emitter)
                .map_err(|e| format!("failed to start serial forwarder: {e}"))?;
            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            host_tauri::commands::get_audit_status,
            host_tauri::commands::set_audit_alert,
            host_tauri::commands::list_audit_events,
            host_tauri::commands::list_runs,
            host_tauri::commands::get_run,
            host_tauri::commands::set_llm_config,
            host_tauri::commands::get_provider_presets,
            host_tauri::commands::clear_llm_config,
            host_tauri::commands::get_llm_config_status,
            host_tauri::commands::get_llm_readiness,
            host_tauri::commands::probe_local_llm,
            host_tauri::commands::has_stored_key,
            host_tauri::commands::load_stored_key,
            host_tauri::commands::list_sessions,
            host_tauri::commands::create_session,
            host_tauri::commands::open_session,
            host_tauri::commands::rename_session,
            host_tauri::commands::delete_session,
            host_tauri::commands::clear_all_sessions,
            host_tauri::commands::get_current_session_id,
            host_tauri::commands::list_snapshots,
            host_tauri::commands::delete_snapshot,
            host_tauri::commands::stop_current_vm,
            host_tauri::commands::vm_is_running,
            host_tauri::commands::vm_status,
            host_tauri::commands::save_snapshot_real,
            host_tauri::commands::resume_from_snapshot_real,
            host_tauri::commands::probe_toolchain,
            host_tauri::commands::set_toolchain_path,
            host_tauri::commands::clear_toolchain_path,
            host_tauri::commands::preflight_status,
            host_tauri::commands::run_preflight,
            host_tauri::commands::acknowledge_preflight,
            host_tauri::commands::get_theme,
            host_tauri::commands::set_theme,
            host_tauri::commands::get_language,
            host_tauri::commands::set_language,
            host_tauri::commands::probe_qemu,
            host_tauri::commands::get_qemu_status,
            host_tauri::commands::set_qemu_path,
            host_tauri::commands::clear_qemu_path,
            host_tauri::commands::start_toolchain_download,
            host_tauri::commands::cancel_toolchain_download,
            host_tauri::commands::toolchain_download_status,
            host_tauri::commands::start_qemu_download,
            host_tauri::commands::cancel_qemu_download,
            host_tauri::commands::qemu_download_status,
            host_tauri::commands::list_sandboxes,
            host_tauri::commands::current_sandbox,
            host_tauri::commands::sandbox_candidates,
            host_tauri::commands::get_sandbox,
            host_tauri::commands::switch_sandbox,
            host_tauri::commands::list_sandbox_requests,
            host_tauri::commands::request_sandbox,
            host_tauri::commands::approve_sandbox_request,
            host_tauri::commands::reject_sandbox_request,
            host_tauri::commands::import_workspace,
            host_tauri::commands::export_workspace,
            host_tauri::commands::run_agent,
            host_tauri::commands::get_workspace_files,
            host_tauri::commands::read_workspace_file,
            host_tauri::commands::workspace_root,
            host_tauri::commands::get_serial_buffer,
            host_tauri::commands::export_serial_log,
            host_tauri::commands::export_audit_jsonl,
            host_tauri::commands::export_run_audit,
            host_tauri::commands::compare_run_fingerprints,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
