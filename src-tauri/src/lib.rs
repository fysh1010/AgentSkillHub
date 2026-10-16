mod commands;
mod models;

use commands::AppState;
use std::sync::Mutex;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let cfg = commands::load_config(app.handle());
            let handle = app.handle().clone();
            app.manage(AppState {
                config: Mutex::new(cfg),
                handle,
                inventory: Mutex::new(None),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_sync_data,
            commands::scan_skills_cmd,
            commands::set_skills_root,
            commands::toggle_skill,
            commands::compute_diff_cmd,
            commands::deploy_cmd,
            commands::rollback_cmd,
            commands::delete_skill,
            commands::clear_audit,
            commands::refresh_status,
            commands::import_existing_links,
            commands::detect_agents,
            commands::refresh_discovery,
            commands::set_agent_management,
            commands::save_custom_agent,
            commands::market_sources,
            commands::market_add_source,
            commands::market_remove_source,
            commands::market_list_ex,
            commands::market_install,
            commands::list_external_skills,
            commands::adopt_skill,
            commands::detect_builtin_sources,
            commands::builtin_source_skills,
            commands::adopt_builtin_skill,
            commands::translate_text,
            commands::ensure_skill_icon,
            commands::read_skill_detail,
            commands::list_trash,
            commands::restore_trash,
            commands::purge_trash,
            commands::set_skill_meta,
            commands::git_backup,
            commands::scan_skill_security,
            commands::scan_security_summary,
            commands::token_stats,
            commands::set_token_budget,
            commands::export_manifest,
            commands::import_manifest,
            commands::latest_manifest,
            commands::reveal_path
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
