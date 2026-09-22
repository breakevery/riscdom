//! RiscDom Tauri shell library entry point.
//!
//! Registers the `host` crate's commands. No business logic lives here.

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
            host::paths::set_app_data_dir(base.clone());
            // v0.8: pass the data directory in, so this instance owns it rather
            // than sharing one process-wide default.
            let state = host::AppState::with_data_dir(&workspace, base.clone())
                .map_err(|e| format!("failed to init host state: {e}"))?;
            // Long-lived serial forwarder: outlives individual runs so the UI
            // keeps receiving serial output across runs.
            let emitter: std::sync::Arc<dyn host::EventSink> = std::sync::Arc::new(
                host::events::TauriEventSink::new(app.handle().clone()),
            );
            state
                .start_serial_forwarder(emitter)
                .map_err(|e| format!("failed to start serial forwarder: {e}"))?;
            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            host::commands::get_audit_status,
            host::commands::set_audit_alert,
            host::commands::list_audit_events,
            host::commands::list_runs,
            host::commands::get_run,
            host::commands::set_llm_config,
            host::commands::get_provider_presets,
            host::commands::clear_llm_config,
            host::commands::get_llm_config_status,
            host::commands::get_llm_readiness,
            host::commands::probe_local_llm,
            host::commands::has_stored_key,
            host::commands::load_stored_key,
            host::commands::list_sessions,
            host::commands::create_session,
            host::commands::open_session,
            host::commands::rename_session,
            host::commands::delete_session,
            host::commands::clear_all_sessions,
            host::commands::get_current_session_id,
            host::commands::list_snapshots,
            host::commands::delete_snapshot,
            host::commands::stop_current_vm,
            host::commands::vm_is_running,
            host::commands::vm_status,
            host::commands::save_snapshot_real,
            host::commands::resume_from_snapshot_real,
            host::commands::probe_toolchain,
            host::commands::set_toolchain_path,
            host::commands::clear_toolchain_path,
            host::commands::preflight_status,
            host::commands::run_preflight,
            host::commands::acknowledge_preflight,
            host::commands::get_theme,
            host::commands::set_theme,
            host::commands::get_language,
            host::commands::set_language,
            host::commands::probe_qemu,
            host::commands::get_qemu_status,
            host::commands::set_qemu_path,
            host::commands::clear_qemu_path,
            host::commands::start_toolchain_download,
            host::commands::cancel_toolchain_download,
            host::commands::toolchain_download_status,
            host::commands::run_agent,
            host::commands::get_workspace_files,
            host::commands::read_workspace_file,
            host::commands::workspace_root,
            host::commands::get_serial_buffer,
            host::commands::export_serial_log,
            host::commands::export_audit_jsonl,
            host::commands::export_run_audit,
            host::commands::compare_run_fingerprints,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
