import { Fragment, useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { enable, disable, isEnabled } from "@tauri-apps/plugin-autostart";
import "../i18n";
import { applyLanguage } from "../i18n";
import { applyTheme } from "../lib/theme";
import * as ipc from "../lib/ipc";
import { formatDateTime } from "../lib/format";
import ConfirmDialog from "../components/ConfirmDialog";
import type {
    AppSettings,
    AppUpdateInfo,
    ArchiveOverview,
    KimiSubscriptionRowKey,
    LoginMethod,
    LoginState,
    OpenCodeGoRowKey,
    PanelCardKey,
    SkillInfo,
    WidgetCardKey,
} from "../types";
import "../styles/styles.css";

const GITHUB_URL = "https://github.com/hpz1179011410/KimiCodeBar-desktop";
const GITEE_URL = "https://gitee.com/hpz1179011410/KimiCodeBar-desktop";
const ORIGINAL_PROJECT_URL = "https://github.com/xifandev/KimiCodeBar";
const ORIGINAL_AUTHOR_URL = "https://github.com/xifandev";
const WINDOWS_REFERENCE_URL = "https://github.com/JYH1878/KimiCodeBar-Windows";
const WINDOWS_REFERENCE_AUTHOR_URL = "https://github.com/JYH1878";
const KIMI_CODE_URL = "https://github.com/MoonshotAI/kimi-code";

type PageKey = "general" | "panel" | "archive" | "skills" | "about";

export function SettingsApp() {
    const { t } = useTranslation();
    const [page, setPage] = useState<PageKey>("general");
    const [settings, setSettings] = useState<AppSettings | null>(null);
    const [loginState, setLoginState] = useState<LoginState | null>(null);
    const [credentialsRevision, setCredentialsRevision] = useState(0);

    useEffect(() => {
        void (async () => {
            try {
                const s = await ipc.getSettings();
                setSettings(s);
                applyTheme(s.theme);
                applyLanguage(s.language);
            } catch {
                applyTheme("system");
                applyLanguage("system");
            }
            await reloadLoginState();
        })();

        // 订阅设置变更：widget 窗口被关闭等场景后端广播 settings-changed，
        // 用事件载荷（完整 AppSettings）刷新本地状态，开关即时同步。
        // 本页自身 patch 保存也会触发该事件（save_settings 广播），
        // 载荷即 normalized 后的设置，回环写入无害（本页主要是开关/选择器）。
        const unlisteners: Array<() => void> = [];
        // mounted 标志：listen 的 Promise 在组件卸载后才 resolve 时，立即调用返回的
        // unlisten 清理，避免 cleanup 已跑过导致漏取消订阅
        let mounted = true;
        void ipc
            .onSettingsChanged((s) => setSettings(s))
            .then((u) => {
                if (mounted) unlisteners.push(u);
                else u();
            });
        void ipc
            .onCredentialsCleared(() => {
                void reloadLoginState();
                setCredentialsRevision((revision) => revision + 1);
            })
            .then((u) => {
                if (mounted) unlisteners.push(u);
                else u();
            });
        return () => {
            mounted = false;
            unlisteners.forEach((u) => u());
        };
    }, []);

    const reloadLoginState = useCallback(async () => {
        try {
            setLoginState(await ipc.getLoginState());
        } catch {
            /* ignore */
        }
    }, []);

    /** 即改即存：合并补丁并持久化。 */
    const patch = useCallback((partial: Partial<AppSettings>) => {
        setSettings((prev) => {
            if (!prev) return prev;
            const next = { ...prev, ...partial };
            void ipc.saveSettings(next).catch(() => undefined);
            return next;
        });
    }, []);

    if (!settings) {
        return (
            <div className="settings-app center">
                <div className="spinner" />
            </div>
        );
    }

    const nav: Array<{ key: PageKey; label: string }> = [
        { key: "general", label: t("nav.general") },
        { key: "panel", label: t("nav.panel") },
        { key: "archive", label: t("nav.archive") },
        { key: "skills", label: t("nav.skills") },
        { key: "about", label: t("nav.about") },
    ];

    return (
        <div className="settings-app">
            <aside className="sidebar">
                {nav.map((n) => (
                    <button
                        key={n.key}
                        className={`nav-item${page === n.key ? " active" : ""}`}
                        onClick={() => setPage(n.key)}
                    >
                        {n.label}
                    </button>
                ))}
            </aside>
            <main className="content">
                {page === "general" && (
                    <GeneralPage
                        settings={settings}
                        loginState={loginState}
                        patch={patch}
                        reloadLoginState={reloadLoginState}
                        credentialsRevision={credentialsRevision}
                    />
                )}
                {page === "panel" && <PanelPage settings={settings} patch={patch} />}
                {page === "archive" && <ArchivePage settings={settings} patch={patch} />}
                {page === "skills" && <SkillsPage />}
                {page === "about" && <AboutPage />}
            </main>
        </div>
    );
}

// ---------- 通用小组件 ----------

function Toggle({ on, onChange }: { on: boolean; onChange: (v: boolean) => void }) {
    return (
        <button
            className={`switch${on ? " on" : ""}`}
            role="switch"
            aria-checked={on}
            onClick={() => onChange(!on)}
        >
            <span className="switch-knob" />
        </button>
    );
}

function Seg<T extends string | number>({
    options,
    value,
    onChange,
}: {
    options: Array<{ value: T; label: string }>;
    value: T;
    onChange: (v: T) => void;
}) {
    return (
        <div className="seg">
            {options.map((o) => (
                <button
                    key={String(o.value)}
                    className={`seg-item${value === o.value ? " active" : ""}`}
                    onClick={() => onChange(o.value)}
                >
                    {o.label}
                </button>
            ))}
        </div>
    );
}

// ---------- 基本设置 ----------

function GeneralPage({
    settings,
    loginState,
    patch,
    reloadLoginState,
    credentialsRevision,
}: {
    settings: AppSettings;
    loginState: LoginState | null;
    patch: (p: Partial<AppSettings>) => void;
    reloadLoginState: () => Promise<void>;
    credentialsRevision: number;
}) {
    const { t } = useTranslation();
    const [editingKey, setEditingKey] = useState(false);
    const [keyInput, setKeyInput] = useState("");
    const [keySaved, setKeySaved] = useState(false);
    const [autostart, setAutostart] = useState(false);
    const [logoutConfirmOpen, setLogoutConfirmOpen] = useState(false);
    const [loggingOut, setLoggingOut] = useState(false);
    const minutes = Math.max(1, Math.round(settings.refresh_interval_secs / 60));

    useEffect(() => {
        void isEnabled()
            .then(setAutostart)
            .catch(() => undefined);
    }, []);

    const chooseMethod = (method: LoginMethod) => {
        patch({ login_method: method });
        void ipc.setLoginMethod(method).catch(() => undefined);
        void reloadLoginState();
    };

    const saveKey = async () => {
        const key = keyInput.trim();
        if (!key) return;
        try {
            await ipc.setApiKey(key);
            setKeyInput("");
            setEditingKey(false);
            setKeySaved(true);
            setTimeout(() => setKeySaved(false), 2000);
            await reloadLoginState();
        } catch {
            /* 后端返回错误时保持编辑态 */
        }
    };

    const toggleAutostart = async (v: boolean) => {
        setAutostart(v);
        try {
            if (v) await enable();
            else await disable();
        } catch {
            setAutostart(!v);
        }
    };

    const logout = async () => {
        if (loggingOut) return;
        setLoggingOut(true);
        try {
            await ipc.logout();
            await reloadLoginState();
            setLogoutConfirmOpen(false);
        } catch {
            /* 退出失败时保留当前登录状态 */
        } finally {
            setLoggingOut(false);
        }
    };

    const maskedKey = loginState?.masked_key ?? null;

    return (
        <div className="page">
            <div className="section-title">{t("general.loginMethod")}</div>
            <div className="method-cards">
                <button
                    className={`method-card${settings.login_method === "oauth" ? " active" : ""}`}
                    onClick={() => chooseMethod("oauth")}
                >
                    <div className="method-name">{t("general.oauth")}</div>
                    <div className="method-desc">{t("general.oauthDesc")}</div>
                </button>
                <button
                    className={`method-card${settings.login_method === "api_key" ? " active" : ""}`}
                    onClick={() => chooseMethod("api_key")}
                >
                    <div className="method-name">{t("general.apiKey")}</div>
                    <div className="method-desc">{t("general.apiKeyDesc")}</div>
                </button>
            </div>

            {settings.login_method === "api_key" && (
                <div className="field-row">
                    <span className="field-label">{t("general.currentKey")}</span>
                    {editingKey ? (
                        <span className="field-edit">
                            <input
                                className="input"
                                type="password"
                                placeholder={t("general.keyPlaceholder")}
                                value={keyInput}
                                onChange={(e) => setKeyInput(e.target.value)}
                                onKeyDown={(e) => {
                                    if (e.key === "Enter") void saveKey();
                                }}
                            />
                            <button
                                className="btn primary sm"
                                onClick={saveKey}
                                disabled={!keyInput.trim()}
                            >
                                {t("common.save")}
                            </button>
                            <button className="btn ghost sm" onClick={() => setEditingKey(false)}>
                                {t("common.cancel")}
                            </button>
                        </span>
                    ) : (
                        <span className="field-edit">
                            <span className="masked-key">{maskedKey ?? "—"}</span>
                            <button className="btn ghost sm" onClick={() => setEditingKey(true)}>
                                {t("common.edit")}
                            </button>
                            {keySaved && (
                                <span className="saved-hint">{t("general.keySaved")}</span>
                            )}
                        </span>
                    )}
                </div>
            )}

            <div className="field-row">
                <span className="field-label">{t("general.loginStatus")}</span>
                <span className="field-edit">
                    <span className={`status-dot${loginState?.logged_in ? " ok" : ""}`} />
                    <span>
                        {loginState?.logged_in ? t("general.loggedIn") : t("general.notLoggedIn")}
                    </span>
                    {loginState?.logged_in && (
                        <button
                            className="btn danger sm"
                            onClick={() => setLogoutConfirmOpen(true)}
                        >
                            {t("general.logout")}
                        </button>
                    )}
                </span>
            </div>

            <div className="field-row">
                <span className="field-label">{t("general.theme")}</span>
                <Seg
                    value={settings.theme}
                    onChange={(v) => {
                        patch({ theme: v });
                        applyTheme(v);
                    }}
                    options={[
                        { value: "system", label: t("general.themeSystem") },
                        { value: "dark", label: t("general.themeDark") },
                        { value: "light", label: t("general.themeLight") },
                    ]}
                />
            </div>

            <div className="field-row">
                <span className="field-label">{t("general.language")}</span>
                <Seg
                    value={settings.language}
                    onChange={(v) => {
                        patch({ language: v });
                        applyLanguage(v);
                    }}
                    options={[
                        { value: "system", label: t("general.langSystem") },
                        { value: "zh", label: t("general.langZh") },
                        { value: "en", label: t("general.langEn") },
                    ]}
                />
            </div>

            <div className="field-row">
                <span className="field-label">{t("general.autostart")}</span>
                <Toggle on={autostart} onChange={toggleAutostart} />
            </div>

            <div className="field-row">
                <span className="field-label">{t("general.refreshInterval")}</span>
                <span className="field-edit">
                    <input
                        className="input number"
                        type="number"
                        min={1}
                        value={minutes}
                        onChange={(e) => {
                            const m = Math.max(1, Math.floor(Number(e.target.value) || 1));
                            patch({ refresh_interval_secs: m * 60 });
                        }}
                    />
                    <span className="field-unit">{t("general.minutes")}</span>
                    <span className="field-hint">{t("general.intervalHint")}</span>
                </span>
            </div>

            <WebTokenSection credentialsRevision={credentialsRevision} />
            <OpenCodeGoSection credentialsRevision={credentialsRevision} />
            <ConfirmDialog
                open={logoutConfirmOpen}
                title={t("general.logoutConfirmTitle")}
                message={t("general.logoutConfirm")}
                confirmLabel={t("common.confirm")}
                cancelLabel={t("common.cancel")}
                busy={loggingOut}
                onConfirm={() => void logout()}
                onCancel={() => setLogoutConfirmOpen(false)}
            />
        </div>
    );
}

// ---------- 月度用量（网页端令牌） ----------

function WebTokenSection({ credentialsRevision }: { credentialsRevision: number }) {
    const { t } = useTranslation();
    const [open, setOpen] = useState(false);
    const [configured, setConfigured] = useState(false);
    const [input, setInput] = useState("");
    const [saving, setSaving] = useState(false);
    const [result, setResult] = useState("");
    const [error, setError] = useState("");

    useEffect(() => {
        void ipc
            .getWebTokenConfigured()
            .then(setConfigured)
            .catch(() => undefined);
    }, [credentialsRevision]);

    const save = async () => {
        if (!input.trim() || saving) return;
        setSaving(true);
        setResult("");
        setError("");
        try {
            const info = await ipc.setWebToken(input);
            setConfigured(true);
            setInput("");
            const pct = Math.round(info.total_pct * 100) / 100;
            setResult(
                t("general.webTokenSaved", {
                    pct: Number.isInteger(pct) ? String(pct) : pct.toFixed(2),
                }),
            );
        } catch (e) {
            setError(String(e));
        } finally {
            setSaving(false);
        }
    };

    const clear = async () => {
        try {
            await ipc.clearWebToken();
            setConfigured(false);
            setResult("");
            setError("");
        } catch {
            /* ignore */
        }
    };

    return (
        <>
            <button
                className={`section-title web-token-toggle${open ? " open" : ""}`}
                onClick={() => setOpen((v) => !v)}
            >
                <span className="web-token-toggle-label">{t("general.webToken")}</span>
                {configured && <span className="badge ok">{t("general.webTokenConfigured")}</span>}
            </button>
            {open && (
                <div className="web-token-section">
                    <p className="page-desc">
                        {t("general.webTokenStep1")}
                        <br />
                        {t("general.webTokenStep2")}
                        <br />
                        {t("general.webTokenStep3")}
                    </p>
                    <div className="btn-row">
                        <button
                            className="btn ghost"
                            onClick={() =>
                                void ipc.openUrl("https://www.kimi.com").catch(() => undefined)
                            }
                        >
                            {t("general.openKimi")}
                        </button>
                    </div>
                    <textarea
                        className="input token-textarea"
                        rows={3}
                        placeholder={t("general.webTokenPlaceholder")}
                        value={input}
                        onChange={(e) => setInput(e.target.value)}
                    />
                    <div className="btn-row">
                        <button
                            className="btn primary"
                            onClick={() => void save()}
                            disabled={saving || !input.trim()}
                        >
                            {saving ? t("general.webTokenSaving") : t("common.save")}
                        </button>
                        {configured && (
                            <button className="btn danger" onClick={() => void clear()}>
                                {t("general.webTokenClear")}
                            </button>
                        )}
                    </div>
                    {result && <div className="banner ok">{result}</div>}
                    {error && <div className="banner error">{error}</div>}
                </div>
            )}
        </>
    );
}

// ---------- OpenCode Go 订阅配额 ----------

function OpenCodeGoSection({ credentialsRevision }: { credentialsRevision: number }) {
    const { t } = useTranslation();
    const [open, setOpen] = useState(false);
    const [configured, setConfigured] = useState(false);
    const [workspaceId, setWorkspaceId] = useState("");
    const [authCookie, setAuthCookie] = useState("");
    const [saving, setSaving] = useState(false);
    const [result, setResult] = useState("");
    const [error, setError] = useState("");

    useEffect(() => {
        void ipc
            .getOpenCodeGoConfigured()
            .then(setConfigured)
            .catch(() => undefined);
    }, [credentialsRevision]);

    const save = async () => {
        if (!workspaceId.trim() || !authCookie.trim() || saving) return;
        setSaving(true);
        setResult("");
        setError("");
        try {
            const usage = await ipc.setOpenCodeGoCredentials(workspaceId, authCookie);
            setConfigured(true);
            setWorkspaceId("");
            setAuthCookie("");
            const remaining = usage.five_hour
                ? Math.round(usage.five_hour.remaining_percent * 10_000) / 100
                : null;
            setResult(
                remaining == null
                    ? t("general.openCodeGoSaved")
                    : t("general.openCodeGoSavedWithQuota", {
                          pct: Number.isInteger(remaining)
                              ? String(remaining)
                              : remaining.toFixed(2),
                      }),
            );
        } catch (e) {
            setError(String(e));
        } finally {
            setSaving(false);
        }
    };

    const clear = async () => {
        try {
            await ipc.clearOpenCodeGoCredentials();
            setConfigured(false);
            setWorkspaceId("");
            setAuthCookie("");
            setResult("");
            setError("");
        } catch {
            /* ignore */
        }
    };

    return (
        <>
            <button
                className={`section-title web-token-toggle${open ? " open" : ""}`}
                onClick={() => setOpen((value) => !value)}
            >
                <span className="web-token-toggle-label">{t("general.openCodeGo")}</span>
                {configured && <span className="badge ok">{t("general.webTokenConfigured")}</span>}
            </button>
            {open && (
                <div className="web-token-section">
                    <p className="page-desc">
                        {t("general.openCodeGoStep1")}
                        <br />
                        {t("general.openCodeGoStep2")}
                        <br />
                        {t("general.openCodeGoStep3")}
                    </p>
                    <div className="btn-row">
                        <button
                            className="btn ghost"
                            onClick={() =>
                                void ipc.openUrl("https://opencode.ai").catch(() => undefined)
                            }
                        >
                            {t("general.openOpenCode")}
                        </button>
                    </div>
                    <input
                        className="input opencode-workspace-input"
                        type="text"
                        spellCheck={false}
                        placeholder={t("general.openCodeGoWorkspacePlaceholder")}
                        value={workspaceId}
                        onChange={(e) => setWorkspaceId(e.target.value)}
                    />
                    <textarea
                        className="input token-textarea"
                        rows={3}
                        placeholder={t("general.openCodeGoCookiePlaceholder")}
                        value={authCookie}
                        onChange={(e) => setAuthCookie(e.target.value)}
                    />
                    <div className="btn-row">
                        <button
                            className="btn primary"
                            onClick={() => void save()}
                            disabled={saving || !workspaceId.trim() || !authCookie.trim()}
                        >
                            {saving ? t("general.webTokenSaving") : t("common.save")}
                        </button>
                        {configured && (
                            <button className="btn danger" onClick={() => void clear()}>
                                {t("general.webTokenClear")}
                            </button>
                        )}
                    </div>
                    {result && <div className="banner ok">{result}</div>}
                    {error && <div className="banner error">{error}</div>}
                </div>
            )}
        </>
    );
}

// ---------- 面板自定义 ----------

/** 数组内相邻交换（dir = -1 上移 / 1 下移）；越界或不存在时原样返回 */
function moveCard<T extends string>(cards: T[], key: T, dir: -1 | 1): T[] {
    const i = cards.indexOf(key);
    const j = i + dir;
    if (i < 0 || j < 0 || j >= cards.length) return cards;
    const next = [...cards];
    [next[i], next[j]] = [next[j], next[i]];
    return next;
}

/** 卡片显隐：开 → 追加到末尾；关 → 从数组中移除（全部取消时后端 normalized() 回退默认两张） */
function toggleCard(cards: WidgetCardKey[], key: WidgetCardKey, on: boolean): WidgetCardKey[] {
    if (on) return cards.includes(key) ? cards : [...cards, key];
    return cards.filter((k) => k !== key);
}

function ArrowUpIcon() {
    return (
        <svg width="12" height="12" viewBox="0 0 12 12" fill="currentColor" aria-hidden>
            <path d="M6 2.5 10 7H8.2L6 4.7 3.8 7H2l4-4.5Z" />
        </svg>
    );
}

function ArrowDownIcon() {
    return (
        <svg width="12" height="12" viewBox="0 0 12 12" fill="currentColor" aria-hidden>
            <path d="M6 9.5 2 5h1.8L6 7.3 8.2 5H10l-4 4.5Z" />
        </svg>
    );
}

function PanelPage({
    settings,
    patch,
}: {
    settings: AppSettings;
    patch: (p: Partial<AppSettings>) => void;
}) {
    const { t } = useTranslation();
    const panelRows: Record<PanelCardKey, { visibility: keyof AppSettings | null; label: string }> =
        {
            kimi_subscription: {
                visibility: null,
                label: t("panelPage.showKimiSubscription"),
            },
            open_code_go: {
                visibility: null,
                label: t("panelPage.showOpenCodeGo"),
            },
            local_usage: {
                visibility: "show_local_usage_card",
                label: t("panelPage.showLocalUsage"),
            },
            model_trend: {
                visibility: "show_model_trend_card",
                label: t("panelPage.showModelTrend"),
            },
        };
    const widgetRows: Array<{ key: WidgetCardKey; label: string }> = [
        { key: "kimi_subscription", label: t("panelPage.showKimiSubscription") },
        { key: "open_code_go", label: t("panelPage.showOpenCodeGo") },
    ];
    const widgetCards = settings.widget_cards;
    type SubscriptionVisibility =
        | "show_weekly_card"
        | "show_five_hour_card"
        | "show_monthly_card"
        | "show_booster_card"
        | "show_opencode_go_five_hour_card"
        | "show_opencode_go_weekly_card"
        | "show_opencode_go_monthly_card";
    type SubscriptionRowKey = KimiSubscriptionRowKey | OpenCodeGoRowKey;
    const kimiSubscriptionRowMap: Record<
        KimiSubscriptionRowKey,
        { visibility: SubscriptionVisibility; label: string }
    > = {
        weekly: { visibility: "show_weekly_card", label: t("panel.weekly") },
        five_hour: { visibility: "show_five_hour_card", label: t("panel.fiveHour") },
        monthly: { visibility: "show_monthly_card", label: t("panel.monthly") },
        booster: { visibility: "show_booster_card", label: t("panel.booster") },
    };
    const openCodeGoRowMap: Record<
        OpenCodeGoRowKey,
        { visibility: SubscriptionVisibility; label: string }
    > = {
        five_hour: {
            visibility: "show_opencode_go_five_hour_card",
            label: t("panel.openCodeGoFiveHour"),
        },
        weekly: {
            visibility: "show_opencode_go_weekly_card",
            label: t("panel.openCodeGoWeekly"),
        },
        monthly: {
            visibility: "show_opencode_go_monthly_card",
            label: t("panel.openCodeGoMonthly"),
        },
    };
    const kimiSubscriptionRows = settings.kimi_subscription_rows.map((key) => ({
        key: key as SubscriptionRowKey,
        ...kimiSubscriptionRowMap[key],
    }));
    const openCodeGoRows = settings.opencode_go_rows.map((key) => ({
        key: key as SubscriptionRowKey,
        ...openCodeGoRowMap[key],
    }));
    return (
        <div className="page">
            <p className="page-desc">{t("panelPage.desc")}</p>
            {settings.panel_cards.map((key, index) => {
                const row = panelRows[key];
                const isKimiSubscription = key === "kimi_subscription";
                const isOpenCodeGo = key === "open_code_go";
                const subscriptionRows = isKimiSubscription
                    ? kimiSubscriptionRows
                    : isOpenCodeGo
                      ? openCodeGoRows
                      : [];
                const visible = isKimiSubscription
                    ? settings.show_weekly_card ||
                      settings.show_five_hour_card ||
                      settings.show_monthly_card ||
                      settings.show_booster_card
                    : isOpenCodeGo
                      ? settings.show_opencode_go_card &&
                        (settings.show_opencode_go_five_hour_card ||
                            settings.show_opencode_go_weekly_card ||
                            settings.show_opencode_go_monthly_card)
                      : Boolean(settings[row.visibility!]);
                return (
                    <Fragment key={key}>
                        <div className="field-row panel-card-row">
                            <span className="field-label">{row.label}</span>
                            <span className="card-sort-btns">
                                <button
                                    className="icon-btn sm"
                                    title={t("panelPage.moveUp")}
                                    disabled={index === 0}
                                    onClick={() =>
                                        patch({
                                            panel_cards: moveCard(settings.panel_cards, key, -1),
                                        })
                                    }
                                >
                                    <ArrowUpIcon />
                                </button>
                                <button
                                    className="icon-btn sm"
                                    title={t("panelPage.moveDown")}
                                    disabled={index === settings.panel_cards.length - 1}
                                    onClick={() =>
                                        patch({
                                            panel_cards: moveCard(settings.panel_cards, key, 1),
                                        })
                                    }
                                >
                                    <ArrowDownIcon />
                                </button>
                            </span>
                            <Toggle
                                on={visible}
                                onChange={(nextVisible) => {
                                    if (isKimiSubscription) {
                                        patch({
                                            show_weekly_card: nextVisible,
                                            show_five_hour_card: nextVisible,
                                            show_monthly_card: nextVisible,
                                            show_booster_card: nextVisible,
                                        });
                                    } else if (isOpenCodeGo) {
                                        patch({
                                            show_opencode_go_card: nextVisible,
                                            show_opencode_go_five_hour_card: nextVisible,
                                            show_opencode_go_weekly_card: nextVisible,
                                            show_opencode_go_monthly_card: nextVisible,
                                        });
                                    } else {
                                        patch({
                                            [row.visibility!]: nextVisible,
                                        } as Partial<AppSettings>);
                                    }
                                }}
                            />
                        </div>
                        {subscriptionRows.length > 0 && (
                            <div className="subscription-options">
                                {subscriptionRows.map((option, optionIndex) => (
                                    <div className="subscription-option" key={option.key}>
                                        <span className="subscription-option-label">
                                            {option.label}
                                        </span>
                                        <span className="card-sort-btns">
                                            <button
                                                className="icon-btn sm"
                                                title={t("panelPage.moveUp")}
                                                disabled={optionIndex === 0}
                                                onClick={() => {
                                                    if (isKimiSubscription) {
                                                        patch({
                                                            kimi_subscription_rows: moveCard(
                                                                settings.kimi_subscription_rows,
                                                                option.key as KimiSubscriptionRowKey,
                                                                -1,
                                                            ),
                                                        });
                                                    } else {
                                                        patch({
                                                            opencode_go_rows: moveCard(
                                                                settings.opencode_go_rows,
                                                                option.key as OpenCodeGoRowKey,
                                                                -1,
                                                            ),
                                                        });
                                                    }
                                                }}
                                            >
                                                <ArrowUpIcon />
                                            </button>
                                            <button
                                                className="icon-btn sm"
                                                title={t("panelPage.moveDown")}
                                                disabled={
                                                    optionIndex === subscriptionRows.length - 1
                                                }
                                                onClick={() => {
                                                    if (isKimiSubscription) {
                                                        patch({
                                                            kimi_subscription_rows: moveCard(
                                                                settings.kimi_subscription_rows,
                                                                option.key as KimiSubscriptionRowKey,
                                                                1,
                                                            ),
                                                        });
                                                    } else {
                                                        patch({
                                                            opencode_go_rows: moveCard(
                                                                settings.opencode_go_rows,
                                                                option.key as OpenCodeGoRowKey,
                                                                1,
                                                            ),
                                                        });
                                                    }
                                                }}
                                            >
                                                <ArrowDownIcon />
                                            </button>
                                        </span>
                                        <Toggle
                                            on={settings[option.visibility]}
                                            onChange={(nextVisible) => {
                                                const next: Partial<AppSettings> = {
                                                    [option.visibility]: nextVisible,
                                                };
                                                if (isOpenCodeGo && nextVisible) {
                                                    next.show_opencode_go_card = true;
                                                }
                                                patch(next);
                                            }}
                                        />
                                    </div>
                                ))}
                            </div>
                        )}
                    </Fragment>
                );
            })}

            <div className="section-title">{t("panelPage.widget")}</div>
            <div className="field-row panel-card-row">
                <span className="field-label">{t("panelPage.widgetEnabled")}</span>
                <Toggle
                    on={settings.widget_enabled}
                    onChange={(v) => patch({ widget_enabled: v })}
                />
            </div>
            <p className="page-desc">{t("panelPage.widgetDesc")}</p>
            {widgetRows.map((r) => {
                const index = widgetCards.indexOf(r.key);
                return (
                    <div className="field-row panel-card-row" key={r.key}>
                        <span className="field-label">{r.label}</span>
                        <span className="card-sort-btns">
                            <button
                                className="icon-btn sm"
                                title={t("panelPage.moveUp")}
                                disabled={index <= 0}
                                onClick={() =>
                                    patch({ widget_cards: moveCard(widgetCards, r.key, -1) })
                                }
                            >
                                <ArrowUpIcon />
                            </button>
                            <button
                                className="icon-btn sm"
                                title={t("panelPage.moveDown")}
                                disabled={index < 0 || index >= widgetCards.length - 1}
                                onClick={() =>
                                    patch({ widget_cards: moveCard(widgetCards, r.key, 1) })
                                }
                            >
                                <ArrowDownIcon />
                            </button>
                        </span>
                        <Toggle
                            on={index >= 0}
                            onChange={(v) =>
                                patch({ widget_cards: toggleCard(widgetCards, r.key, v) })
                            }
                        />
                    </div>
                );
            })}
        </div>
    );
}

// ---------- 自动归档 ----------

function ArchivePage({
    settings,
    patch,
}: {
    settings: AppSettings;
    patch: (p: Partial<AppSettings>) => void;
}) {
    const { t } = useTranslation();
    const [overview, setOverview] = useState<ArchiveOverview | null>(null);
    const [busy, setBusy] = useState(false);
    const [notice, setNotice] = useState("");

    const reload = useCallback(async () => {
        try {
            setOverview(await ipc.getArchiveOverview());
        } catch {
            /* ignore */
        }
    }, []);

    useEffect(() => {
        void reload();
    }, [reload]);

    const archiveNow = async () => {
        if (!window.confirm(t("archive.confirmArchive"))) return;
        setBusy(true);
        try {
            const count = await ipc.runAutoArchiveNow();
            setNotice(t("archive.archivedCount", { count }));
            await reload();
        } catch {
            /* ignore */
        } finally {
            setBusy(false);
        }
    };

    const unarchive = async (id: string) => {
        try {
            await ipc.unarchiveSession(id);
            await reload();
        } catch {
            /* ignore */
        }
    };

    return (
        <div className="page">
            {overview && (
                <div className="badges">
                    <span className="badge">
                        {t("archive.total")} {overview.total}
                    </span>
                    <span className="badge ok">
                        {t("archive.archived")} {overview.archived}
                    </span>
                    <span className="badge warn">
                        {t("archive.pending")} {overview.total - overview.archived}
                    </span>
                </div>
            )}

            <div className="field-row">
                <span className="field-label">{t("archive.auto")}</span>
                <Toggle
                    on={settings.auto_archive_enabled}
                    onChange={(v) => patch({ auto_archive_enabled: v })}
                />
            </div>
            <p className="page-desc">{t("archive.autoDesc")}</p>

            <div className="field-row">
                <span className="field-label">{t("archive.threshold")}</span>
                <Seg
                    value={settings.auto_archive_threshold_days}
                    onChange={(v) => patch({ auto_archive_threshold_days: v })}
                    options={[
                        { value: 1, label: t("archive.day1") },
                        { value: 7, label: t("archive.week1") },
                        { value: 30, label: t("archive.month1") },
                    ]}
                />
            </div>

            <div className="btn-row">
                <button className="btn ghost" onClick={() => void reload()} disabled={busy}>
                    {t("archive.scanNow")}
                </button>
                <button className="btn primary" onClick={archiveNow} disabled={busy}>
                    {t("archive.archiveNow")}
                </button>
            </div>
            {notice && <div className="banner ok">{notice}</div>}

            {!overview ? (
                <div className="spinner" />
            ) : overview.groups.length === 0 ? (
                <div className="card-empty">{t("archive.empty")}</div>
            ) : (
                overview.groups.map((g) => (
                    <div className="session-group" key={g.work_dir}>
                        <div className="session-group-title" title={g.work_dir}>
                            {g.work_dir}
                        </div>
                        {g.sessions.map((s) => (
                            <div className="session-row" key={s.id}>
                                <div className="session-main">
                                    <span className="session-title">
                                        {s.title || t("archive.untitled")}
                                    </span>
                                    {s.archived && (
                                        <span className="session-tag">
                                            {t("archive.archivedTag")}
                                        </span>
                                    )}
                                    {s.updated_at && (
                                        <span className="session-time">
                                            {formatDateTime(s.updated_at_ms ?? s.updated_at)}
                                        </span>
                                    )}
                                </div>
                                {s.archived && (
                                    <button
                                        className="btn ghost sm"
                                        onClick={() => void unarchive(s.id)}
                                    >
                                        {t("archive.unarchive")}
                                    </button>
                                )}
                            </div>
                        ))}
                    </div>
                ))
            )}
        </div>
    );
}

// ---------- 技能管理 ----------

function SkillsPage() {
    const { t } = useTranslation();
    const [skills, setSkills] = useState<SkillInfo[] | null>(null);
    const [expanded, setExpanded] = useState<string | null>(null);
    const [content, setContent] = useState("");

    useEffect(() => {
        void ipc
            .getSkills()
            .then(setSkills)
            .catch(() => setSkills([]));
    }, []);

    const toggle = async (skill: SkillInfo) => {
        if (expanded === skill.dir) {
            setExpanded(null);
            setContent("");
            return;
        }
        setExpanded(skill.dir);
        setContent("");
        try {
            setContent(await ipc.readSkill(skill.dir));
        } catch (e) {
            setContent(String(e));
        }
    };

    if (!skills) {
        return (
            <div className="page">
                <div className="spinner" />
            </div>
        );
    }
    if (skills.length === 0) {
        return (
            <div className="page">
                <div className="card-empty">{t("skills.empty")}</div>
            </div>
        );
    }

    return (
        <div className="page">
            {skills.map((s) => (
                <div className="skill-card" key={s.dir}>
                    <button className="skill-head" onClick={() => void toggle(s)}>
                        <span className="skill-name">{s.name}</span>
                        <span className="skill-toggle">
                            {expanded === s.dir ? t("skills.collapse") : t("skills.expand")}
                        </span>
                    </button>
                    {s.description && <div className="skill-desc">{s.description}</div>}
                    <div className="skill-actions">
                        <button
                            className="link-btn"
                            onClick={() => void ipc.revealInExplorer(s.path).catch(() => undefined)}
                        >
                            {t("skills.reveal")}
                        </button>
                    </div>
                    {expanded === s.dir && content && (
                        <pre className="skill-preview">{content}</pre>
                    )}
                </div>
            ))}
        </div>
    );
}

// ---------- 关于 ----------

function AboutPage() {
    const { t } = useTranslation();
    const [info, setInfo] = useState<AppUpdateInfo | null>(null);
    const [checking, setChecking] = useState(false);
    const [checked, setChecked] = useState(false);

    useEffect(() => {
        void ipc
            .checkAppUpdate(false)
            .then(setInfo)
            .catch(() => undefined);
    }, []);

    useEffect(() => {
        if (!checked || !info || info.update_available) return;
        const timer = window.setTimeout(() => setChecked(false), 3500);
        return () => window.clearTimeout(timer);
    }, [checked, info]);

    const check = async () => {
        setChecking(true);
        try {
            setInfo(await ipc.checkAppUpdate(true));
            setChecked(true);
        } catch {
            /* ignore */
        } finally {
            setChecking(false);
        }
    };

    return (
        <div className="page">
            <div className="about-logo">
                <div className="logo-badge large">K</div>
                <div className="about-name">{t("app.name")}</div>
                <div className="about-version">
                    {t("about.currentVersion", { version: info?.current ?? "—" })}
                </div>
            </div>
            <div className="about-repositories">
                <button className="about-repository" onClick={() => void ipc.openUrl(GITHUB_URL)}>
                    <span className="about-repository-brand">GitHub</span>
                    <span className="about-repository-copy">
                        <strong>{t("about.primaryRepository")}</strong>
                        <span>hpz1179011410/KimiCodeBar-desktop</span>
                    </span>
                    <span className="about-external" aria-hidden>
                        ↗
                    </span>
                </button>
                <button className="about-repository" onClick={() => void ipc.openUrl(GITEE_URL)}>
                    <span className="about-repository-brand gitee">Gitee</span>
                    <span className="about-repository-copy">
                        <strong>{t("about.backupRepository")}</strong>
                        <span>hpz1179011410/KimiCodeBar-desktop</span>
                    </span>
                    <span className="about-external" aria-hidden>
                        ↗
                    </span>
                </button>
            </div>
            <div className="about-update-row">
                <button className="btn primary" onClick={check} disabled={checking}>
                    {checking ? t("about.checking") : t("about.checkUpdate")}
                </button>
            </div>
            {checked && info && (
                <>
                    {info.update_available && info.latest ? (
                        <button
                            className="about-update-toast ok clickable"
                            onClick={() => void ipc.openUrl(info.release_url)}
                        >
                            <span className="about-update-toast-icon">↗</span>
                            <span>{t("about.newVersion", { version: info.latest })}</span>
                        </button>
                    ) : info.error ? (
                        <div className="about-update-toast error" role="alert">
                            <span className="about-update-toast-icon">!</span>
                            <span>{t("about.checkFailed", { msg: info.error })}</span>
                        </div>
                    ) : (
                        <div className="about-update-toast ok" role="status">
                            <span className="about-update-toast-icon">✓</span>
                            <span>{t("about.upToDate")}</span>
                        </div>
                    )}
                </>
            )}
            <div className="about-acknowledgments">
                <div className="section-title">{t("about.acknowledgments")}</div>
                <div className="about-credit-list">
                    <div className="about-credit">
                        <button
                            className="link-btn about-credit-repo"
                            onClick={() => void ipc.openUrl(ORIGINAL_PROJECT_URL)}
                        >
                            xifandev/KimiCodeBar ↗
                        </button>
                        <p>
                            {t("about.originalProject")}
                            <button
                                className="link-btn about-credit-author"
                                onClick={() => void ipc.openUrl(ORIGINAL_AUTHOR_URL)}
                            >
                                @xifandev
                            </button>
                            {t("about.openSourceThanks")}
                        </p>
                    </div>
                    <div className="about-credit">
                        <button
                            className="link-btn about-credit-repo"
                            onClick={() => void ipc.openUrl(WINDOWS_REFERENCE_URL)}
                        >
                            JYH1878/KimiCodeBar-Windows ↗
                        </button>
                        <p>
                            {t("about.windowsReferenceBefore")}
                            <button
                                className="link-btn about-credit-author"
                                onClick={() => void ipc.openUrl(WINDOWS_REFERENCE_AUTHOR_URL)}
                            >
                                @JYH1878
                            </button>
                            {t("about.windowsReferenceAfter")}
                        </p>
                    </div>
                    <div className="about-credit">
                        <button
                            className="link-btn about-credit-repo"
                            onClick={() => void ipc.openUrl(KIMI_CODE_URL)}
                        >
                            MoonshotAI/kimi-code ↗
                        </button>
                        <p>{t("about.officialProject")}</p>
                    </div>
                </div>
            </div>
        </div>
    );
}
