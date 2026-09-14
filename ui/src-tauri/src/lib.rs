//! RiscDom Tauri shell library entry point.
//!
//! Registers the `host` crate's commands. No business logic lives here.

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            // Audit DB + AI workspace live under the app's data dir.
            let base = app
                .path()
                .app_data_dir()
                .unwrap_or_else(|_| std::env::temp_dir());
            let workspace = base.join("workspace");
            let state = host::AppState::new(&workspace)
                .map_err(|e| format!("failed to init host state: {e}"))?;
            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            host::commands::get_audit_status,
            host::commands::list_audit_events,
            host::commands::set_llm_config,
            host::commands::get_provider_presets,
            host::commands::clear_llm_config,
            host::commands::get_llm_config_status,
            host::commands::run_agent,
            host::commands::get_workspace_files,
            host::commands::read_workspace_file,
            host::commands::get_serial_buffer,
            host::commands::export_serial_log,
            host::commands::export_audit_jsonl,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
