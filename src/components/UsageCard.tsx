import { memo, useMemo, useSyncExternalStore } from "react";
import { useTranslation } from "react-i18next";
import type { TFunction } from "i18next";
import type { QuotaWindow } from "../types";

const CLOCK_INTERVAL_MS = 30_000;
const clocks = new Map<number, ReturnType<typeof createClock>>();

function createClock(intervalMs: number) {
    const listeners = new Set<() => void>();
    let now = Date.now();
    let timer: ReturnType<typeof setInterval> | null = null;
    const tick = () => {
        now = Date.now();
        listeners.forEach((listener) => listener());
    };
    return {
        subscribe(listener: () => void) {
            const wasIdle = listeners.size === 0;
            listeners.add(listener);
            if (wasIdle) {
                now = Date.now();
                timer = setInterval(tick, intervalMs);
            }
            return () => {
                listeners.delete(listener);
                if (listeners.size === 0 && timer) {
                    clearInterval(timer);
                    timer = null;
                }
            };
        },
        getSnapshot: () => now,
    };
}

function getClock(intervalMs: number) {
    let clock = clocks.get(intervalMs);
    if (!clock) {
        clock = createClock(intervalMs);
        clocks.set(intervalMs, clock);
    }
    return clock;
}

/** 同一 Webview 内相同刷新间隔的倒计时共享一个时钟，避免每个额度行各建定时器。 */
export function useNow(intervalMs = CLOCK_INTERVAL_MS): number {
    const clock = useMemo(() => getClock(intervalMs), [intervalMs]);
    return useSyncExternalStore(clock.subscribe, clock.getSnapshot, clock.getSnapshot);
}

export function formatResetText(resetTime: string | null, now: number, t: TFunction): string {
    if (!resetTime) return "";
    const ms = Date.parse(resetTime) - now;
    if (Number.isNaN(ms) || ms <= 60_000) return t("reset.soon");
    const minutes = Math.floor(ms / 60_000);
    const hours = Math.floor(minutes / 60);
    const days = Math.floor(hours / 24);
    if (days > 0) return t("reset.daysHours", { days, hours: hours % 24 });
    if (hours > 0) return t("reset.hoursMinutes", { hours, minutes: minutes % 60 });
    return t("reset.minutes", { minutes });
}

function UsageCard({
    title,
    win,
    loading = false,
}: {
    title: string;
    win: QuotaWindow | null;
    loading?: boolean;
}) {
    const { t } = useTranslation();
    const now = useNow();
    const resetText = formatResetText(win?.reset_time ?? null, now, t);

    if (!win) {
        return (
            <section className={`card${loading ? " loading" : ""}`}>
                <div className="card-title">{title}</div>
                <div className="card-empty">{t("panel.noData")}</div>
            </section>
        );
    }

    const pctNum = win.remaining_percent * 100;
    // CLI 接口返回整数口径：整数显示 62%，若未来出现小数值则保留两位小数
    const rounded = Math.round(pctNum * 100) / 100;
    const pct = Number.isInteger(rounded) ? String(rounded) : rounded.toFixed(2);
    const low = win.limit > 0 && win.remaining_percent < 0.2;

    return (
        <section className={`card${loading ? " loading" : ""}`}>
            <div className="card-title">{title}</div>
            <div className="usage-main">
                <span className={`usage-percent${low ? " low" : ""}`}>{pct}%</span>
                <span className="usage-remaining-label">{t("panel.remaining")}</span>
            </div>
            <div className="progress">
                <div
                    className={`progress-fill${low ? " low" : ""}`}
                    style={{ width: `${Math.max(0, Math.min(100, pctNum))}%` }}
                />
            </div>
            <div className="usage-nums">
                {t("panel.usedOfLimit", {
                    pct: (() => {
                        const p = win.limit > 0 ? (win.used / win.limit) * 100 : 0;
                        const r = Math.round(p * 100) / 100;
                        return Number.isInteger(r) ? String(r) : r.toFixed(2);
                    })(),
                })}
            </div>
            {resetText && <div className="reset-text">{resetText}</div>}
        </section>
    );
}

export default memo(UsageCard);
