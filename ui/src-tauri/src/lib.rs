//! RiscDom Tauri shell library entry point.
//!
//! Registers `host-tauri`'s commands. No business logic lives here.

use std::sync::Arc;
use tauri::Manager;

mod lan;

use host_tauri::settings::NetworkSettings;

/// The node's network wiring, as this instance holds it (v0.9.9 内网接入).
///
/// `None` means nothing has ever been configured — the embedded host and nobody
/// served, which is what every release before this one did. The network face is
/// the desktop shell's own (the browser has no network to wire), which is why
/// these commands live here rather than in `host-tauri`.
#[tauri::command]
fn get_network(
    state: tauri::State<'_, Arc<host_tauri::AppState>>,
) -> Result<Option<NetworkSettings>, String> {
    Ok(state.network())
}

/// Store the node's network wiring, then make the board match it.
///
/// The settings decide and the wiring acts on them: `lan::apply` stops whatever is
/// running and starts what the new settings ask for, so this one command is both
/// "remember this" and "do this".
#[tauri::command]
fn set_network(
    app: tauri::AppHandle,
    state: tauri::State<'_, Arc<host_tauri::AppState>>,
    network: NetworkSettings,
) -> Result<(), String> {
    state
        .set_network(network.clone())
        .map_err(|e| e.user_message())?;
    lan::apply(&app, &network)
}

/// What the board is doing right now: running, where it bound, and the address a
/// phone has to type (v0.9.9).
#[tauri::command]
fn lan_status(
    app: tauri::AppHandle,
    state: tauri::State<'_, Arc<host_tauri::AppState>>,
) -> Result<lan::LanStatus, String> {
    let settings = state.network();
    Ok(lan::status(&app, settings.as_ref()))
}

/// Read the token a started server requires, or say why there is none yet.
///
/// **Read-only on purpose.** The file is read and never created: minting a token
/// is the server's job when it first starts, and a settings screen must not bring
/// a credential into existence merely by being opened. The file name is the one
/// `server::token::TOKEN_FILE` declares — `probe-ui-network-tab.mjs` asserts the
/// two agree, so a rename there cannot make this read the wrong file in silence.
/// Nothing is logged, cached or sent anywhere: the value goes to the caller and
/// stops there.
#[tauri::command]
fn read_lan_token(state: tauri::State<'_, Arc<host_tauri::AppState>>) -> Result<String, String> {
    let path = state.data_dir().join("token");
    match std::fs::read_to_string(&path) {
        Ok(raw) => {
            let value = raw.trim().to_string();
            if value.is_empty() {
                return Err(format!("the token file is empty: {}", path.display()));
            }
            Ok(value)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(format!(
            "no token yet: {} does not exist. One is created when a server first \
             starts (switch the LAN board on, then it will be there).",
            path.display()
        )),
        Err(e) => Err(format!("cannot read {}: {e}", path.display())),
    }
}

/// Where a remote server's token lives: the OS keyring, keyed by the address the
/// operator typed (v0.9.9 内网接入 4/N `"out"`).
///
/// The token is a **credential**, so it is not written to `settings.json` — that
/// file's own rule is that no secret lives in it. The account name is
/// `host_tauri::keyring::user_for_remote`, so the shape is declared once, in the
/// crate that owns the keyring, and a probe can hold these commands to it.
///
/// **Silent-failure caveat, inherited from `keyring.rs`**: a keyring that will not
/// answer degrades to in-memory storage, so a "remembered" token can be forgotten
/// by the next restart. The caller is told what the keyring said, not what it
/// hoped for.
#[tauri::command]
fn save_remote_token(
    state: tauri::State<'_, Arc<host_tauri::AppState>>,
    host: String,
    token: String,
) -> Result<(), String> {
    let host = host.trim();
    if host.is_empty() {
        return Err("no server address to file the token under".to_string());
    }
    state
        .keyring
        .set(host_tauri::keyring::SERVICE, &host_tauri::keyring::user_for_remote(host), &token)
        .map_err(|e| format!("the OS keyring refused to store it: {e}"))
}

/// The token filed for `host`, or `None`.
///
/// Never logged, never cached, never put anywhere but the caller's hands — the
/// same rule `read_lan_token` above keeps for the local token.
#[tauri::command]
fn read_remote_token(
    state: tauri::State<'_, Arc<host_tauri::AppState>>,
    host: String,
) -> Result<Option<String>, String> {
    let host = host.trim();
    if host.is_empty() {
        return Ok(None);
    }
    state
        .keyring
        .get(host_tauri::keyring::SERVICE, &host_tauri::keyring::user_for_remote(host))
        .map_err(|e| format!("the OS keyring could not be read: {e}"))
}

/// Forget the token filed for `host`. Deleting what is not there is not an error.
#[tauri::command]
fn clear_remote_token(
    state: tauri::State<'_, Arc<host_tauri::AppState>>,
    host: String,
) -> Result<(), String> {
    let host = host.trim();
    if host.is_empty() {
        return Ok(());
    }
    state
        .keyring
        .delete(host_tauri::keyring::SERVICE, &host_tauri::keyring::user_for_remote(host))
        .map_err(|e| format!("the OS keyring refused to forget it: {e}"))
}

/// Start the application again from scratch.
///
/// The one way out of a mode that takes effect at startup: leaving the remote
/// node, or entering it, is a change to *which host this window talks to*, so the
/// honest way to apply it is the same way the process began. `AppHandle::restart`
/// is Tauri's own (no plugin, no extra dependency): it closes this instance and
/// starts a new one.
#[tauri::command]
fn restart_app(app: tauri::AppHandle) -> Result<(), String> {
    app.restart()
}

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
            // v0.9.9 内网接入: the shell hands **the same node** to the embedded
            // server, so the app's state is an `Arc<AppState>` from here on — and
            // `host-tauri`'s commands take it that way too. One instance, never a
            // copy: a copy would have its own VM slot and its own settings, and a
            // board that can start a second QEMU is worse than no board.
            let shared = Arc::new(state);
            app.manage(Arc::clone(&shared));
            app.manage(lan::LanServer::default());
            // A node left serving its board keeps serving it after a restart.
            if let Some(settings) = shared.network() {
                if let Err(e) = lan::apply(app.handle(), &settings) {
                    eprintln!("riscdom: the node's board was not started: {e}");
                }
            }
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
            host_tauri::commands::list_executors,
            host_tauri::commands::dispatch_task,
            get_network,
            set_network,
            lan_status,
            read_lan_token,
            save_remote_token,
            read_remote_token,
            clear_remote_token,
            restart_app,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|handle, event| {
            // The board's life is the app's: no socket outlives the window it was
            // opened for.
            if let tauri::RunEvent::ExitRequested { .. } = event {
                if let Some(lan) = handle.try_state::<lan::LanServer>() {
                    lan.stop();
                }
            }
        });
}
