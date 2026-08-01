// 桌面小部件窗口入口：无边框悬浮窗，直接复用主面板的
// KimiSubscriptionCard / OpenCodeGoCard，展示口径、行级显隐与排序完全一致。
// 窗口尺寸由内容撑开（ResizeObserver 测量 .widget-root → setSize，右下角锚定）。
// z 序：窗口保持普通层（后端维护 WS_EX_NOACTIVATE，点击不激活不排顶），
// 普通窗口打开时自然盖住小部件。拖拽为显式实现：mousedown → startDragging；
// 单击（非拖拽，位移 ≤5px）唤起主面板。
import React, { useCallback, useEffect, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import { getCurrentWindow, PhysicalPosition, PhysicalSize } from "@tauri-apps/api/window";
import { WebviewWindow } from "@tauri-apps/api/webviewWindow";
import "../i18n";
import { applyLanguage } from "../i18n";
import { applyTheme } from "../lib/theme";
import * as ipc from "../lib/ipc";
import KimiSubscriptionCard from "../components/KimiSubscriptionCard";
import OpenCodeGoCard from "../components/OpenCodeGoCard";
import type {
    AppSettings,
    KimiSubscriptionRowKey,
    MonthlyInfo,
    OpenCodeGoRowKey,
    OpenCodeGoUsage,
    QuotaInfo,
    WidgetCardKey,
} from "../types";
import "../styles/styles.css";

/** 默认卡片顺序（与后端 normalized() 回退一致） */
const DEFAULT_CARDS: WidgetCardKey[] = ["kimi_subscription", "open_code_go"];
const DEFAULT_KIMI_SUBSCRIPTION_ROWS: KimiSubscriptionRowKey[] = [
    "weekly",
    "five_hour",
    "monthly",
    "booster",
];
const DEFAULT_OPENCODE_GO_ROWS: OpenCodeGoRowKey[] = ["five_hour", "weekly", "monthly"];

function Widget() {
    const [quota, setQuota] = useState<QuotaInfo | null>(null);
    const [monthly, setMonthly] = useState<MonthlyInfo | null>(null);
    const [monthlyConfigured, setMonthlyConfigured] = useState(false);
    const [monthlyError, setMonthlyError] = useState<string | null>(null);
    const [openCodeGo, setOpenCodeGo] = useState<OpenCodeGoUsage | null>(null);
    const [openCodeGoConfigured, setOpenCodeGoConfigured] = useState(false);
    const [openCodeGoError, setOpenCodeGoError] = useState<string | null>(null);
    const [settings, setSettings] = useState<AppSettings | null>(null);
    // mousedown 位置：用于区分单击与拖拽（位移 ≤5px 视为单击）
    const downPos = useRef<{ x: number; y: number } | null>(null);
    const rootRef = useRef<HTMLDivElement>(null);
    // 上次请求的窗口尺寸（去重，打断 ResizeObserver ↔ setSize 循环）
    const winSizeRef = useRef({ width: 0, height: 0 });
    // 锚定流程串行链：几何读取 → setSize → setPosition 依次执行。连续快速变化时，
    // 后回调不再并发读取过期坐标，而是排队等前一次落地后再读，避免最终位置偏移
    const resizeChainRef = useRef<Promise<void>>(Promise.resolve());

    // 内容自适应：测量 .widget-root（fit-content，尺寸由内容决定，不随窗口变化），
    // 变化时 setSize 到对应尺寸。窗口已显示时以右下角为锚点重定位——保持窗口自身
    // 右下角不动，窗口向左/向上伸缩（未拖动过则始终贴屏幕右下角；拖动后从自定义
    // 位置的右下角原地伸缩）。窗口未显示（首帧/隐藏态）只调整尺寸不做锚定。
    // 循环防护：窗口尺寸变化不改内容尺寸（fit-content），加上尺寸去重，循环自然中断。
    useEffect(() => {
        const el = rootRef.current;
        if (!el) return;
        // mounted 防护：卸载后链上未执行的回调安全跳过
        let mounted = true;
        const observer = new ResizeObserver(() => {
            const rect = el.getBoundingClientRect();
            const width = Math.ceil(rect.width);
            const height = Math.ceil(rect.height);
            const prev = winSizeRef.current;
            // 去重保持在同步段（首个 await 前），链上执行无需重复判断
            if (Math.abs(width - prev.width) <= 1 && Math.abs(height - prev.height) <= 1) {
                return;
            }
            winSizeRef.current = { width, height };
            // 串行化锚定流程；前后 catch 兜底，单次失败不阻塞后续
            resizeChainRef.current = resizeChainRef.current
                .catch(() => undefined)
                .then(async () => {
                    if (!mounted) return;
                    const win = getCurrentWindow();
                    // CSS 像素（逻辑）→ 物理像素（outer* 系列 API 返回物理值）
                    const scale = await win.scaleFactor();
                    const newW = Math.round(width * scale);
                    const newH = Math.round(height * scale);
                    if (!(await win.isVisible())) {
                        // 首帧/隐藏态：窗口位置未定，只调整尺寸
                        await win.setSize(new PhysicalSize(newW, newH));
                        return;
                    }
                    // 锚定右下角：新位置 = 旧位置 + (旧内尺寸 - 新内尺寸)。
                    // 用 innerSize 而非 outerSize：setSize 是内尺寸语义，而 tao 在
                    // 无边框+阴影窗口上 set_inner_size 会补偿 hidden offsets（实际
                    // 窗口矩形 = 内尺寸 + 阴影边距 d），用 outerSize 会让每轮锚定
                    // 注入 +d 偏移，窗口逐次右漂。
                    const [pos, inner] = await Promise.all([win.outerPosition(), win.innerSize()]);
                    await win.setSize(new PhysicalSize(newW, newH));
                    await win.setPosition(
                        new PhysicalPosition(
                            pos.x + inner.width - newW,
                            pos.y + inner.height - newH,
                        ),
                    );
                })
                .catch(() => undefined);
        });
        observer.observe(el);
        return () => {
            mounted = false;
            observer.disconnect();
        };
    }, []);

    const applySettings = useCallback((next: AppSettings) => {
        setSettings(next);
        applyTheme(next.theme);
        applyLanguage(next.language);
    }, []);

    // 本月用量：与主面板一致，未配置 web token 时不发请求。
    const refreshMonthly = useCallback(async () => {
        try {
            const configured = await ipc.getWebTokenConfigured();
            setMonthlyConfigured(configured);
            if (!configured) {
                setMonthly(null);
                setMonthlyError(null);
                return;
            }
            try {
                setMonthly(await ipc.getMonthly());
                setMonthlyError(null);
            } catch (error) {
                setMonthlyError(String(error));
            }
        } catch {
            /* 查询配置失败不打扰其他额度 */
        }
    }, []);

    // OpenCode Go：配置存在时直接读取与主面板相同的三档订阅额度。
    const refreshOpenCodeGo = useCallback(async () => {
        try {
            const configured = await ipc.getOpenCodeGoConfigured();
            setOpenCodeGoConfigured(configured);
            if (!configured) {
                setOpenCodeGo(null);
                setOpenCodeGoError(null);
                return;
            }
            try {
                setOpenCodeGo(await ipc.getOpenCodeGoUsage());
                setOpenCodeGoError(null);
            } catch (error) {
                setOpenCodeGoError(String(error));
            }
        } catch {
            /* 查询配置失败不打扰 Kimi 额度 */
        }
    }, []);

    // 初始化：主题 / 语言 / 缓存配额 / 两类订阅数据，并订阅数据事件。
    useEffect(() => {
        void (async () => {
            try {
                applySettings(await ipc.getSettings());
            } catch {
                applyTheme("system");
                applyLanguage("system");
            }
            try {
                setQuota(await ipc.getQuota());
            } catch {
                /* 无缓存 */
            }
        })();
        void Promise.all([refreshMonthly(), refreshOpenCodeGo()]);

        const unlisteners: Array<() => void> = [];
        // mounted 标志：listen 的 Promise 在组件卸载后才 resolve 时，立即调用返回的
        // unlisten 清理，避免 cleanup 已跑过导致漏取消订阅
        let mounted = true;
        void ipc
            .onQuotaUpdated((q) => {
                setQuota(q);
                // 配额刷新时顺带刷新两类订阅扩展数据（无新增定时器）。
                void Promise.all([refreshMonthly(), refreshOpenCodeGo()]);
            })
            .then((u) => {
                if (mounted) unlisteners.push(u);
                else u();
            });
        void ipc
            .onSettingsChanged((next: AppSettings) => {
                applySettings(next);
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
            })
            .then((u) => {
                if (mounted) unlisteners.push(u);
                else u();
            });
        return () => {
            mounted = false;
            unlisteners.forEach((u) => u());
        };
    }, [applySettings, refreshMonthly, refreshOpenCodeGo]);

    // 左键按下：记录位置用于单击判定，并启动系统拖拽（SC_MOVE 模态循环，
    // 结束后 Promise resolve）。窗口保持普通层，拖拽无需与贴底机制交互。
    const onMouseDown = (e: React.MouseEvent) => {
        if (e.button !== 0) return;
        downPos.current = { x: e.screenX, y: e.screenY };
        void getCurrentWindow()
            .startDragging()
            .catch(() => undefined);
    };

    // 单击（非拖拽）唤起主面板（锦上添花，失败不打扰）
    const onClick = (e: React.MouseEvent) => {
        const d = downPos.current;
        if (!d || Math.hypot(e.screenX - d.x, e.screenY - d.y) > 5) return;
        void (async () => {
            try {
                const win = await WebviewWindow.getByLabel("main");
                if (win) {
                    await win.show();
                    await win.unminimize();
                    await win.setFocus();
                }
            } catch {
                /* 忽略 */
            }
        })();
    };

    const cards = settings?.widget_cards ?? DEFAULT_CARDS;
    const showWeekly = settings?.show_weekly_card ?? true;
    const showFiveHour = settings?.show_five_hour_card ?? true;
    const showMonthly = (settings?.show_monthly_card ?? true) && monthlyConfigured;
    const showBooster = (settings?.show_booster_card ?? true) && quota?.booster != null;
    const showKimiSubscription = showWeekly || showFiveHour || showMonthly || showBooster;
    const showOpenCodeGoFiveHour = settings?.show_opencode_go_five_hour_card ?? true;
    const showOpenCodeGoWeekly = settings?.show_opencode_go_weekly_card ?? true;
    const showOpenCodeGoMonthly = settings?.show_opencode_go_monthly_card ?? true;
    const showOpenCodeGo =
        (settings?.show_opencode_go_card ?? true) &&
        openCodeGoConfigured &&
        (showOpenCodeGoFiveHour || showOpenCodeGoWeekly || showOpenCodeGoMonthly);

    // 按 widget_cards 顺序直接渲染主面板的两张组合卡片。
    return (
        <div ref={rootRef} className="widget-root" onMouseDown={onMouseDown} onClick={onClick}>
            {cards.map((key) => {
                if (key === "kimi_subscription") {
                    return showKimiSubscription ? (
                        <KimiSubscriptionCard
                            key={key}
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
                        />
                    ) : null;
                }
                return showOpenCodeGo ? (
                    <OpenCodeGoCard
                        key={key}
                        usage={openCodeGo}
                        error={openCodeGoError}
                        showFiveHour={showOpenCodeGoFiveHour}
                        showWeekly={showOpenCodeGoWeekly}
                        showMonthly={showOpenCodeGoMonthly}
                        rowOrder={settings?.opencode_go_rows ?? DEFAULT_OPENCODE_GO_ROWS}
                    />
                ) : null;
            })}
        </div>
    );
}

createRoot(document.getElementById("root")!).render(
    <React.StrictMode>
        <Widget />
    </React.StrictMode>,
);
