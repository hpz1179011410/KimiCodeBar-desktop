// 全部 Tauri invoke 封装 + 事件 listen 封装。
// 命令名 / 参数名 / 事件名与 src-tauri/src/commands.rs、lib.rs、polling.rs 严格对齐。

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
    AppSettings,
    AppUpdateInfo,
    ArchiveOverview,
    CliUpdateInfo,
    DeviceLoginStart,
    LocalUsageReport,
    LoginMethod,
    LoginState,
    MonthlyInfo,
    OpenCodeGoUsage,
    QuotaInfo,
    SkillInfo,
} from "../types";

// ---- 设置 ----
export const getSettings = () => invoke<AppSettings>("get_settings");
export const saveSettings = (settings: AppSettings) => invoke<void>("save_settings", { settings });

// ---- 登录 ----
export const getLoginState = () => invoke<LoginState>("get_login_state");
export const startDeviceLogin = () => invoke<DeviceLoginStart>("start_device_login");
export const cancelLogin = () => invoke<void>("cancel_login");
export const logout = () => invoke<void>("logout");
export const setApiKey = (key: string) => invoke<void>("set_api_key", { key });
export const getMaskedApiKey = () => invoke<string | null>("get_masked_api_key");
export const setLoginMethod = (method: LoginMethod) => invoke<void>("set_login_method", { method });

// ---- 配额 ----
export const refreshQuota = () => invoke<QuotaInfo>("refresh_quota");
export const getQuota = () => invoke<QuotaInfo | null>("get_quota");

// ---- 月度用量（网页端令牌） ----
export const setWebToken = (token: string) => invoke<MonthlyInfo>("set_web_token", { token });
export const clearWebToken = () => invoke<void>("clear_web_token");
export const getWebTokenConfigured = () => invoke<boolean>("get_web_token_configured");
export const getMonthly = () => invoke<MonthlyInfo>("get_monthly");

// ---- OpenCode Go 订阅配额（Workspace Dashboard） ----
export const setOpenCodeGoCredentials = (workspaceId: string, authCookie: string) =>
    invoke<OpenCodeGoUsage>("set_opencode_go_credentials", { workspaceId, authCookie });
export const clearOpenCodeGoCredentials = () => invoke<void>("clear_opencode_go_credentials");
export const getOpenCodeGoConfigured = () => invoke<boolean>("get_opencode_go_configured");
export const getOpenCodeGoUsage = () => invoke<OpenCodeGoUsage>("get_opencode_go_usage");

// ---- 本地用量 ----
export const getLocalUsage = () => invoke<LocalUsageReport | null>("get_local_usage");
export const refreshLocalUsage = () => invoke<LocalUsageReport>("refresh_local_usage");

// ---- 归档 ----
export const getArchiveOverview = () => invoke<ArchiveOverview>("get_archive_overview");
export const archiveSessions = (ids: string[]) => invoke<number>("archive_sessions", { ids });
export const unarchiveSession = (id: string) => invoke<void>("unarchive_session", { id });
export const runAutoArchiveNow = () => invoke<number>("run_auto_archive_now");

// ---- 技能 ----
export const getSkills = () => invoke<SkillInfo[]>("get_skills");
export const readSkill = (name: string) => invoke<string>("read_skill", { name });
export const revealInExplorer = (path: string) => invoke<void>("reveal_in_explorer", { path });

// ---- 更新检查 ----
export const checkCliUpdate = () => invoke<CliUpdateInfo>("check_cli_update");
export const checkAppUpdate = (force = false) =>
    invoke<AppUpdateInfo>("check_app_update", { force });

// ---- 其他 ----
export const openUrl = (url: string) => invoke<void>("open_url", { url });
export const quitApp = () => invoke<void>("quit_app");

// ---- 事件（事件名见 polling.rs / commands.rs）----
export function onQuotaUpdated(cb: (q: QuotaInfo) => void): Promise<UnlistenFn> {
    return listen<QuotaInfo>("quota-updated", (e) => cb(e.payload));
}
export function onSettingsChanged(cb: (s: AppSettings) => void): Promise<UnlistenFn> {
    return listen<AppSettings>("settings-changed", (e) => cb(e.payload));
}
export function onCredentialsCleared(cb: () => void): Promise<UnlistenFn> {
    return listen<void>("credentials-cleared", () => cb());
}
export function onLoginSuccess(cb: () => void): Promise<UnlistenFn> {
    return listen("login-success", () => cb());
}
export function onLoginError(cb: (msg: string) => void): Promise<UnlistenFn> {
    return listen<string>("login-error", (e) => cb(e.payload));
}
export function onLoginExpired(cb: () => void): Promise<UnlistenFn> {
    return listen("login-expired", () => cb());
}
