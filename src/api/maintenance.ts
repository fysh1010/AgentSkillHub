import { Channel, invoke } from "@tauri-apps/api/core";

export interface SkillSyncSettings { repository: string; branch: string; whole_repository: boolean }
export interface SkillSyncProgress { step: number; total: number; label: string }
export interface UpdateSettings { check_on_start: boolean }
export interface UpdateInfo {
  current_version: string; latest_version: string; available: boolean;
  release_url: string; download_url: string; notes: string; checked_at: number;
}
export const skillSyncSettings = () => invoke<SkillSyncSettings>("skill_sync_settings");
export const saveSkillSyncSettings = (settings: SkillSyncSettings) => invoke<void>("save_skill_sync_settings", { settings });
export function skillSyncRun(operation: "connect" | "push" | "pull", report: (progress: SkillSyncProgress) => void) {
  const onProgress = new Channel<SkillSyncProgress>(report);
  return invoke<string>("skill_sync_run", { operation, onProgress });
}
export const detectSkillGit = (repository?: string) => invoke<SkillSyncSettings>("detect_skill_git", { repository: repository || null });
export interface SkillBackupTarget { location: string; username: string; webdav: boolean }
export interface SkillBackupItem { name: string; created: number }
export interface SkillBackupResult { name: string; path: string; files: number }
export const skillBackupTarget = () => invoke<SkillBackupTarget>("skill_backup_target");
export const pickBackupFolder = () => invoke<string>("pick_backup_folder");
export const skillBackupList = (target: SkillBackupTarget, password: string) =>
  invoke<SkillBackupItem[]>("skill_backup_list", { target, password });
export const skillBackupCreate = (target: SkillBackupTarget, password: string, passphrase: string) =>
  invoke<SkillBackupResult>("skill_backup_create", { target, password, passphrase });
export const skillBackupRestore = (target: SkillBackupTarget, password: string, passphrase: string, name: string) =>
  invoke<SkillBackupResult>("skill_backup_restore", { target, password, passphrase, name });
export const officialReleases = "https://github.com/fysh1010/AgentSkillHub/releases/latest";
export const updateSettings = () => invoke<UpdateSettings>("update_settings");
export const saveUpdateSettings = (settings: UpdateSettings) => invoke<void>("save_update_settings", { settings });
export const checkAppUpdate = (force = false) => invoke<UpdateInfo>("check_app_update", { force });
export const openUpdateLink = (url: string) => invoke<void>("open_update_link", { url });
