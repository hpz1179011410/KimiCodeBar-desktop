import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import type { QuotaWindow } from "../types";

/** 每 intervalMs 触发一次重渲染，用于倒计时类文案。 */
export function useNow(intervalMs: number): number {
    const [now, setNow] = useState(() => Date.now());
    useEffect(() => {
        const id = setInterval(() => setNow(Date.now()), intervalMs);
        return () => clearInterval(id);
    }, [intervalMs]);
    return now;
}

export function useResetText(resetTime: string | null): string {
    const { t } = useTranslation();
    useNow(30_000);
    if (!resetTime) return "";
    const ms = Date.parse(resetTime) - Date.now();
    if (Number.isNaN(ms) || ms <= 60_000) return t("reset.soon");
    const minutes = Math.floor(ms / 60_000);
    const hours = Math.floor(minutes / 60);
    const days = Math.floor(hours / 24);
    if (days > 0) return t("reset.daysHours", { days, hours: hours % 24 });
    if (hours > 0) return t("reset.hoursMinutes", { hours, minutes: minutes % 60 });
    return t("reset.minutes", { minutes });
}

export default function UsageCard({
    title,
    win,
    loading = false,
}: {
    title: string;
    win: QuotaWindow | null;
    loading?: boolean;
}) {
    const { t } = useTranslation();
    const resetText = useResetText(win?.reset_time ?? null);

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
