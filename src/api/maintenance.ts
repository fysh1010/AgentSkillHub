import { invoke } from "@tauri-apps/api/core";

export interface SkillSyncSettings { repository: string; branch: string }
export interface UpdateSettings { check_on_start: boolean }
export interface UpdateInfo {
  current_version: string; latest_version: string; available: boolean;
  release_url: string; download_url: string; notes: string; checked_at: number;
}
export const skillSyncSettings = () => invoke<SkillSyncSettings>("skill_sync_settings");
export const saveSkillSyncSettings = (settings: SkillSyncSettings) => invoke<void>("save_skill_sync_settings", { settings });
export const skillSyncRun = (operation: "connect" | "push" | "pull") => invoke<string>("skill_sync_run", { operation });
export const officialReleases = "https://github.com/fysh1010/AgentSkillHub/releases/latest";
export const updateSettings = () => invoke<UpdateSettings>("update_settings");
export const saveUpdateSettings = (settings: UpdateSettings) => invoke<void>("save_update_settings", { settings });
export const checkAppUpdate = (force = false) => invoke<UpdateInfo>("check_app_update", { force });
export const openUpdateLink = (url: string) => invoke<void>("open_update_link", { url });
