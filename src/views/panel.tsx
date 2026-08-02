import React, { useCallback, useEffect, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useTranslation } from "react-i18next";
import "../i18n";
import { applyLanguage } from "../i18n";
import { applyTheme } from "../lib/theme";
import * as ipc from "../lib/ipc";
import type {
    AppSettings,
    AppUpdateInfo,
    CliUpdateInfo,
    KimiSubscriptionRowKey,
    LocalUsageReport,
    MonthlyInfo,
    OpenCodeGoRowKey,
    OpenCodeGoUsage,
    PanelCardKey,
    QuotaInfo,
} from "../types";
import KimiSubscriptionCard from "../components/KimiSubscriptionCard";
import OpenCodeGoCard from "../components/OpenCodeGoCard";
import LocalUsageCard from "../components/LocalUsageCard";
import ModelTrendCard from "../components/ModelTrendCard";
import LoginOverlay from "../components/LoginOverlay";
import ConfirmDialog from "../components/ConfirmDialog";
import AppUpdateDialog from "../components/AppUpdateDialog";
import "../styles/styles.css";

const GITHUB_URL = "https://github.com/hpz1179011410/KimiCodeBar-desktop";
const CONSOLE_URL = "https://kimi.com/code/console";
const CLI_CHANGELOG_URL = "https://moonshotai.github.io/kimi-code/changelog.md";
const DEFAULT_PANEL_CARDS: PanelCardKey[] = [
    "kimi_subscription",
    "open_code_go",
    "local_usage",
    "model_trend",
];
const DEFAULT_KIMI_SUBSCRIPTION_ROWS: KimiSubscriptionRowKey[] = [
    "weekly",
    "five_hour",
    "monthly",
    "booster",
];
const DEFAULT_OPENCODE_GO_ROWS: OpenCodeGoRowKey[] = ["five_hour", "weekly", "monthly"];
const AUTO_REFRESH_DEDUP_MS = 30_000;

function Panel() {
    const { t } = useTranslation();
    const [settings, setSettings] = useState<AppSettings | null>(null);
    const [loggedIn, setLoggedIn] = useState<boolean | null>(null);
    const [quota, setQuota] = useState<QuotaInfo | null>(null);
    const [localUsage, setLocalUsage] = useState<LocalUsageReport | null>(null);
    const [monthly, setMonthly] = useState<MonthlyInfo | null>(null);
    const [monthlyError, setMonthlyError] = useState<string | null>(null);
    const [monthlyConfigured, setMonthlyConfigured] = useState(false);
    const [openCodeGo, setOpenCodeGo] = useState<OpenCodeGoUsage | null>(null);
    const [openCodeGoError, setOpenCodeGoError] = useState<string | null>(null);
    const [openCodeGoConfigured, setOpenCodeGoConfigured] = useState(false);
    const [cliUpdate, setCliUpdate] = useState<CliUpdateInfo | null>(null);
    const [appUpdate, setAppUpdate] = useState<AppUpdateInfo | null>(null);
    const [error, setError] = useState("");
    const [expired, setExpired] = useState(false);
    const [refreshing, setRefreshing] = useState(false);
    const [menuOpen, setMenuOpen] = useState(false);
    const [logoutConfirmOpen, setLogoutConfirmOpen] = useState(false);
    const [updateDialogOpen, setUpdateDialogOpen] = useState(false);
    const [loggingOut, setLoggingOut] = useState(false);
    const [animKey, setAnimKey] = useState(0);
    const [panelActive, setPanelActive] = useState(true);
    const scrollRef = useRef<HTMLDivElement>(null);
    const refreshInFlightRef = useRef<Promise<void> | null>(null);
    const lastAutoRefreshAtRef = useRef(0);

    // 滚动条平时隐藏：滚动时加 .scrolling 临时显示，停止 800ms 后移除
    useEffect(() => {
        const el = scrollRef.current;
        if (!el) return;
        let timer: ReturnType<typeof setTimeout> | undefined;
        const onScroll = () => {
            el.classList.add("scrolling");
            clearTimeout(timer);
            timer = setTimeout(() => el.classList.remove("scrolling"), 800);
        };
        el.addEventListener("scroll", onScroll, { passive: true });
        return () => {
            el.removeEventListener("scroll", onScroll);
            clearTimeout(timer);
        };
    }, [loggedIn, animKey]);

    // 菜单打开时：点击任意处关闭
    useEffect(() => {
        if (!menuOpen) return;
        const close = () => setMenuOpen(false);
        document.addEventListener("mousedown", close);
        return () => document.removeEventListener("mousedown", close);
    }, [menuOpen]);

    const applySettings = useCallback((s: AppSettings) => {
        setSettings(s);
        applyTheme(s.theme);
        applyLanguage(s.language);
    }, []);

    // 月度用量：先确认是否已配置 web token，配置了才拉取；失败不打扰其他卡片
    const refreshMonthly = useCallback(async (force = false) => {
        try {
            const configured = await ipc.getWebTokenConfigured();
            setMonthlyConfigured(configured);
            if (!configured) {
                setMonthly(null);
                setMonthlyError(null);
                return;
            }
            try {
                setMonthly(await ipc.getMonthly(force));
                setMonthlyError(null);
            } catch (e) {
                setMonthlyError(String(e));
            }
        } catch {
            /* 查询配置失败不打扰 */
        }
    }, []);

    // OpenCode Go：配置存在时从 Workspace Dashboard 读取三档订阅额度
    const refreshOpenCodeGo = useCallback(async (force = false) => {
        try {
            const configured = await ipc.getOpenCodeGoConfigured();
            setOpenCodeGoConfigured(configured);
            if (!configured) {
                setOpenCodeGo(null);
                setOpenCodeGoError(null);
                return;
            }
            try {
                setOpenCodeGo(await ipc.getOpenCodeGoUsage(force));
                setOpenCodeGoError(null);
            } catch (e) {
                setOpenCodeGoError(String(e));
            }
        } catch {
            /* 查询配置失败不打扰其他卡片 */
        }
    }, []);

    const silentRefresh = useCallback(
        async (force = false) => {
            if (refreshInFlightRef.current) return refreshInFlightRef.current;
            if (!force && Date.now() - lastAutoRefreshAtRef.current < AUTO_REFRESH_DEDUP_MS) return;

            // 四类数据彼此独立，并行刷新；同一 Webview 内的聚焦、登录和手动刷新复用
            // 同一个进行中任务，避免窗口快速开合时重复请求。
            const task = Promise.all([
                (async () => {
                    try {
                        const q = await ipc.refreshQuota();
                        setQuota(q);
                        setError("");
                        setExpired(false);
                    } catch (e) {
                        // 静默刷新失败只记录横幅，不清空已有数据
                        setError(String(e));
                    }
                })(),
                (async () => {
                    try {
                        setLocalUsage(await ipc.refreshLocalUsage());
                    } catch {
                        // 本地用量扫描失败不打扰用户
                    }
                })(),
                refreshMonthly(force),
                refreshOpenCodeGo(force),
            ]).then(() => {
                lastAutoRefreshAtRef.current = Date.now();
            });
            refreshInFlightRef.current = task;
            try {
                await task;
            } finally {
                if (refreshInFlightRef.current === task) refreshInFlightRef.current = null;
            }
        },
        [refreshMonthly, refreshOpenCodeGo],
    );

    // 挂载：读设置 / 登录态 / 缓存配额 / 缓存本地用量，并订阅事件
    useEffect(() => {
        void (async () => {
            const [settingsResult, loginResult, quotaResult, usageResult] =
                await Promise.allSettled([
                    ipc.getSettings(),
                    ipc.getLoginState(),
                    ipc.getQuota(),
                    ipc.getLocalUsage(),
                ]);
            if (settingsResult.status === "fulfilled") {
                applySettings(settingsResult.value);
            } else {
                applyTheme("system");
                applyLanguage("system");
            }
            setLoggedIn(loginResult.status === "fulfilled" && loginResult.value.logged_in);
            if (quotaResult.status === "fulfilled") setQuota(quotaResult.value);
            if (usageResult.status === "fulfilled") setLocalUsage(usageResult.value);
        })();

        const unlisteners: Array<() => void> = [];
        // mounted 标志：listen 的 Promise 在组件卸载后才 resolve 时，立即调用返回的
        // unlisten 清理，避免 cleanup 已跑过导致漏取消订阅
        let mounted = true;
        void ipc
            .onQuotaUpdated((q) => {
                setQuota(q);
                setError("");
                setExpired(false);
            })
            .then((u) => {
                if (mounted) unlisteners.push(u);
                else u();
            });
        void ipc
            .onLoginExpired(() => {
                setExpired(true);
                setLoggedIn(false);
            })
            .then((u) => {
                if (mounted) unlisteners.push(u);
                else u();
            });
        void ipc
            .onCredentialsCleared(() => {
                setQuota(null);
                setMonthly(null);
                setMonthlyError(null);
                setMonthlyConfigured(false);
                setOpenCodeGo(null);
                setOpenCodeGoError(null);
                setOpenCodeGoConfigured(false);
                setMenuOpen(false);
                setLoggedIn(false);
            })
            .then((u) => {
                if (mounted) unlisteners.push(u);
                else u();
            });

        // 窗口每次获得焦点（托盘弹出）时：重读设置 + 静默刷新 + 重播入场动画
        void getCurrentWindow()
            .onFocusChanged(({ payload: focused }) => {
                // 失焦/隐藏时暂停持续动画，聚焦恢复
                setPanelActive(focused);
                if (!focused) return;
                void ipc
                    .getSettings()
                    .then(applySettings)
                    .catch(() => undefined);
                void silentRefresh();
                // 换 key 重挂载滚动区，卡片入场/进度条/柱状图动画重新播放
                setAnimKey((k) => k + 1);
            })
            .then((u) => {
                if (mounted) unlisteners.push(u);
                else u();
            });

        return () => {
            mounted = false;
            unlisteners.forEach((u) => u());
        };
    }, [applySettings, silentRefresh]);

    // 登录后：查更新 + 主动刷新一次本地用量
    useEffect(() => {
        if (loggedIn !== true) return;
        void ipc
            .checkCliUpdate()
            .then(setCliUpdate)
            .catch(() => undefined);
        void ipc
            .checkAppUpdate(false)
            .then((info) => {
                setAppUpdate(info);
                if (info.update_available) setUpdateDialogOpen(true);
            })
            .catch(() => undefined);
        void silentRefresh();
    }, [loggedIn, silentRefresh]);

    const onRefresh = async () => {
        if (refreshing) return;
        setRefreshing(true);
        const started = Date.now();
        await silentRefresh(true);
        // 加载效果至少显示 600ms，避免闪烁
        const elapsed = Date.now() - started;
        if (elapsed < 600) {
            await new Promise((r) => setTimeout(r, 600 - elapsed));
        }
        setRefreshing(false);
    };

    const openSettings = useCallback(async () => {
        setMenuOpen(false);
        // 打开独立设置窗口：每次打开都居中显示（居中失败不阻塞打开）
        const { WebviewWindow } = await import("@tauri-apps/api/webviewWindow");
        const win = await WebviewWindow.getByLabel("settings");
        if (win) {
            try {
                await win.center();
            } catch {
                /* 无 center 权限等异常时忽略，仍正常打开 */
            }
            await win.show();
            await win.unminimize();
            await win.setFocus();
        }
    }, []);

    const logout = async () => {
        if (loggingOut) return;
        setLoggingOut(true);
        try {
            await ipc.logout();
        } catch {
            /* 清理失败也回到登录页 */
        } finally {
            setQuota(null);
            setLoggedIn(false);
            setLogoutConfirmOpen(false);
            setLoggingOut(false);
        }
    };

    if (loggedIn === null) {
        return (
            <div className="app center">
                <div className="spinner" />
            </div>
        );
    }

    if (!loggedIn) {
        return (
            <div className="app">
                {expired && <div className="banner error floating">{t("panel.loginExpired")}</div>}
                <LoginOverlay
                    onSuccess={() => {
                        setLoggedIn(true);
                        setExpired(false);
                    }}
                />
            </div>
        );
    }

    const showWeekly = settings?.show_weekly_card ?? true;
    const showFiveHour = settings?.show_five_hour_card ?? true;
    const showBooster = (settings?.show_booster_card ?? true) && quota?.booster != null;
    const showLocal = settings?.show_local_usage_card ?? true;
    const showModelTrend = settings?.show_model_trend_card ?? true;
    const showMonthly = (settings?.show_monthly_card ?? true) && monthlyConfigured;
    const showKimiSubscription = showWeekly || showFiveHour || showMonthly || showBooster;
    const showOpenCodeGoFiveHour = settings?.show_opencode_go_five_hour_card ?? true;
    const showOpenCodeGoWeekly = settings?.show_opencode_go_weekly_card ?? true;
    const showOpenCodeGoMonthly = settings?.show_opencode_go_monthly_card ?? true;
    const showOpenCodeGo =
        (settings?.show_opencode_go_card ?? true) &&
        openCodeGoConfigured &&
        (showOpenCodeGoFiveHour || showOpenCodeGoWeekly || showOpenCodeGoMonthly);
    const panelCards = settings?.panel_cards ?? DEFAULT_PANEL_CARDS;

    const renderPanelCard = (card: PanelCardKey): React.ReactNode => {
        switch (card) {
            case "kimi_subscription":
                return showKimiSubscription ? (
                    <KimiSubscriptionCard
                        key={card}
                        weekly={quota?.weekly ?? null}
                        fiveHour={quota?.five_hour ?? null}
                        monthly={monthly}
                        monthlyError={monthlyError}
                        booster={quota?.booster ?? null}
                        showWeekly={showWeekly}
                        showFiveHour={showFiveHour}
                        showMonthly={showMonthly}
                        showBooster={showBooster}
                        rowOrder={
                            settings?.kimi_subscription_rows ?? DEFAULT_KIMI_SUBSCRIPTION_ROWS
                        }
                        loading={refreshing}
                        onOpenSettings={openSettings}
                    />
                ) : null;
            case "open_code_go":
                return showOpenCodeGo ? (
                    <OpenCodeGoCard
                        key={card}
                        usage={openCodeGo}
                        error={openCodeGoError}
                        showFiveHour={showOpenCodeGoFiveHour}
                        showWeekly={showOpenCodeGoWeekly}
                        showMonthly={showOpenCodeGoMonthly}
                        rowOrder={settings?.opencode_go_rows ?? DEFAULT_OPENCODE_GO_ROWS}
                        loading={refreshing}
                        onOpenSettings={openSettings}
                    />
                ) : null;
            case "local_usage":
                return showLocal ? (
                    <LocalUsageCard key={card} report={localUsage} loading={refreshing} />
                ) : null;
            case "model_trend":
                return showModelTrend ? (
                    <ModelTrendCard key={card} report={localUsage} loading={refreshing} />
                ) : null;
        }
    };

    return (
        <div className={`app${panelActive ? "" : " paused"}`}>
            <header className="panel-header">
                <div className="logo-badge">K</div>
                <span className="header-title">{t("app.name")}</span>
                <button
                    className={`icon-btn refresh-btn${refreshing ? " spinning" : ""}`}
                    title={t("common.refresh")}
                    disabled={refreshing}
                    onClick={() => void onRefresh()}
                >
                    <RefreshIcon />
                </button>
                <button
                    className="icon-btn"
                    title={t("panel.github")}
                    onClick={() => ipc.openUrl(GITHUB_URL).catch(() => undefined)}
                >
                    <GithubIcon />
                </button>
                <div className="menu-wrap" onMouseDown={(e) => e.stopPropagation()}>
                    <button
                        className={`icon-btn${menuOpen ? " active" : ""}`}
                        title={t("panel.menu")}
                        onClick={() => setMenuOpen((v) => !v)}
                    >
                        <DotsIcon />
                    </button>
                    {menuOpen && (
                        <div className="menu-pop">
                            <button
                                className="menu-item"
                                onClick={() => {
                                    setMenuOpen(false);
                                    ipc.openUrl(CONSOLE_URL).catch(() => undefined);
                                }}
                            >
                                {t("panel.console")}
                            </button>
                            <button className="menu-item" onClick={() => void openSettings()}>
                                {t("panel.settings")}
                            </button>
                            <button
                                className="menu-item"
                                onClick={() => {
                                    setMenuOpen(false);
                                    setLogoutConfirmOpen(true);
                                }}
                            >
                                {t("general.logout")}
                            </button>
                            <button className="menu-item" onClick={() => void ipc.quitApp()}>
                                {t("panel.quit")}
                            </button>
                        </div>
                    )}
                </div>
            </header>

            <div className="app-scroll" ref={scrollRef} key={animKey}>
                {expired && <div className="banner error">{t("panel.loginExpired")}</div>}
                {error && !expired && (
                    <div className="banner error">{t("panel.refreshFailed", { msg: error })}</div>
                )}

                {panelCards.map(renderPanelCard)}

                <section className="card version-card">
                    <div
                        className={`version-row${cliUpdate?.update_available ? " clickable" : ""}`}
                        onClick={() => {
                            if (cliUpdate?.update_available)
                                void ipc.openUrl(CLI_CHANGELOG_URL).catch(() => undefined);
                        }}
                    >
                        <span className="version-name">{t("panel.cliVersion")}</span>
                        <span className="version-value">
                            {cliUpdate?.current ?? t("panel.notInstalled")}
                        </span>
                        {cliUpdate?.update_available && cliUpdate.latest && (
                            <span className="version-badge">
                                {t("panel.newVersion", { version: cliUpdate.latest })}
                            </span>
                        )}
                    </div>
                    <div
                        className={`version-row${appUpdate?.update_available ? " clickable" : ""}`}
                        onClick={() => {
                            if (appUpdate?.update_available) setUpdateDialogOpen(true);
                        }}
                    >
                        <span className="version-name">{t("panel.appVersion")}</span>
                        <span className="version-value">
                            {appUpdate?.current ?? t("panel.unknown")}
                        </span>
                        {appUpdate?.update_available && appUpdate.latest && (
                            <span className="version-badge">
                                {t("panel.newVersion", { version: appUpdate.latest })}
                            </span>
                        )}
                    </div>
                </section>
            </div>
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
            <AppUpdateDialog
                open={updateDialogOpen}
                info={appUpdate}
                onClose={() => setUpdateDialogOpen(false)}
            />
        </div>
    );
}

function RefreshIcon() {
    return (
        <svg
            width="15"
            height="15"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth="2.4"
            strokeLinecap="round"
            strokeLinejoin="round"
            aria-hidden
        >
            <path d="M21 12a9 9 0 1 1-2.64-6.36" />
            <polyline points="21 3 21 9 15 9" />
        </svg>
    );
}

function DotsIcon() {
    return (
        <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor" aria-hidden>
            <circle cx="8" cy="3" r="1.5" />
            <circle cx="8" cy="8" r="1.5" />
            <circle cx="8" cy="13" r="1.5" />
        </svg>
    );
}

function GithubIcon() {
    return (
        <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor" aria-hidden>
            <path d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27s1.36.09 2 .27c1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.01 8.01 0 0 0 16 8c0-4.42-3.58-8-8-8Z" />
        </svg>
    );
}

createRoot(document.getElementById("root")!).render(
    <React.StrictMode>
        <Panel />
    </React.StrictMode>,
);
