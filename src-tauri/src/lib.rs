mod commands;
mod models;

#[cfg(all(not(debug_assertions), not(feature = "custom-protocol")))]
compile_error!("Release 构建必须启用 custom-protocol，否则窗口会连接开发服务器");

#[cfg(test)]
#[test]
fn default_build_uses_embedded_ui() {
    assert!(!tauri::is_dev());
}

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
            commands::skill_sync::skill_sync_settings,
            commands::skill_sync::save_skill_sync_settings,
            commands::skill_sync::skill_sync_run,
            commands::skill_sync::detect_skill_git,
            commands::skill_backup::skill_backup_target,
            commands::skill_backup::pick_backup_folder,
            commands::skill_backup::skill_backup_list,
            commands::skill_backup::skill_backup_create,
            commands::skill_backup::skill_backup_restore,
            commands::updates::update_settings,
            commands::updates::save_update_settings,
            commands::updates::check_app_update,
            commands::updates::open_update_link,
            commands::get_sync_data,
            commands::workspace_stamp,
            commands::apply_skill_assignments,
            commands::adopt_agent_skill,
            commands::preview_skill_link,
            commands::convert_skill_link,
            commands::restore_skill_link,
            commands::set_skills_root,
            commands::toggle_skill,
            commands::compute_diff_cmd,
            commands::deploy_cmd,
            commands::rollback_cmd,
            commands::delete_skill,
            commands::clear_audit,
            commands::refresh_status,
            commands::import_existing_links,
            commands::refresh_discovery,
            commands::set_agent_management,
            commands::save_custom_agent,
            commands::market_sources,
            commands::market_add_source,
            commands::market_remove_source,
            commands::market_list_ex,
            commands::market_skill_detail,
            commands::market_install,
            commands::builtin_source_skills,
            commands::adopt_builtin_skill,
            commands::save_builtin_source,
            commands::remove_builtin_source,
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
            commands::reveal_path,
            commands::mcp::mcp_inventory,
            commands::mcp::mcp_market_search,
            commands::mcp::mcp_market_sources,
            commands::mcp::mcp_save_market_sources,
            commands::mcp::mcp_save_server,
            commands::mcp::mcp_remove_server,
            commands::mcp::mcp_save_binding,
            commands::mcp::mcp_adopt_source,
            commands::mcp::mcp_preview,
            commands::mcp::mcp_preview_change,
            commands::mcp_probe::mcp_probe_check,
            commands::mcp_probe::mcp_probe_cancel,
            commands::mcp::mcp_apply,
            commands::mcp::mcp_restore
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
