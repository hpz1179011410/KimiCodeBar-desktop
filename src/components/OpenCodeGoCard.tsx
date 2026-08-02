import { memo } from "react";
import { useTranslation } from "react-i18next";
import type {
    OpenCodeGoExchangeRate,
    OpenCodeGoRowKey,
    OpenCodeGoUsage,
    OpenCodeGoWindow,
} from "../types";
import { formatResetText, useNow } from "./UsageCard";

function formatNumber(value: number, digits = 2): string {
    const rounded = Math.round(value * 10 ** digits) / 10 ** digits;
    return Number.isInteger(rounded) ? String(rounded) : rounded.toFixed(digits);
}

function QuotaRow({
    title,
    window,
    exchangeRate,
    now,
}: {
    title: string;
    window: OpenCodeGoWindow | null;
    exchangeRate: OpenCodeGoExchangeRate | null;
    now: number;
}) {
    const { t } = useTranslation();
    const resetText = formatResetText(window?.reset_time ?? null, now, t);

    if (!window) {
        return (
            <div className="subscription-window">
                <div className="subscription-window-head">
                    <span className="subscription-window-name">{title}</span>
                    <span className="subscription-window-empty">{t("panel.noData")}</span>
                </div>
            </div>
        );
    }

    const remaining = Math.max(0, Math.min(100, window.remaining_percent * 100));
    const low = window.limit_usd > 0 && window.remaining_percent < 0.2;

    return (
        <div className="subscription-window">
            <div className="subscription-window-head">
                <span className="subscription-window-name">{title}</span>
                <span className={`subscription-window-percent${low ? " low" : ""}`}>
                    {formatNumber(remaining)}%<span>{t("panel.remaining")}</span>
                </span>
            </div>
            <div className="progress subscription-progress">
                <div
                    className={`progress-fill${low ? " low" : ""}`}
                    style={{ width: `${remaining}%` }}
                />
            </div>
            <div className="subscription-window-meta">
                <span>
                    {t("panel.openCodeGoUsed", {
                        used: formatNumber(window.used_usd),
                        limit: formatNumber(window.limit_usd),
                    })}
                    {exchangeRate && (
                        <span
                            className="subscription-cny"
                            title={t("panel.openCodeGoCnyHint", {
                                rate: formatNumber(exchangeRate.usd_cny, 4),
                                date: exchangeRate.reference_date,
                            })}
                        >
                            {t("panel.openCodeGoUsedCny", {
                                used: formatNumber(window.used_usd * exchangeRate.usd_cny, 1),
                                limit: formatNumber(window.limit_usd * exchangeRate.usd_cny, 1),
                            })}
                        </span>
                    )}
                </span>
                {resetText && <span>{resetText}</span>}
            </div>
        </div>
    );
}

function OpenCodeGoCard({
    usage,
    error,
    showFiveHour,
    showWeekly,
    showMonthly,
    rowOrder,
    loading = false,
    onOpenSettings,
}: {
    usage: OpenCodeGoUsage | null;
    error: string | null;
    showFiveHour: boolean;
    showWeekly: boolean;
    showMonthly: boolean;
    rowOrder: OpenCodeGoRowKey[];
    loading?: boolean;
    onOpenSettings?: () => void;
}) {
    const { t } = useTranslation();
    const now = useNow();

    if (error) {
        return (
            <section className={`card${loading ? " loading" : ""}`}>
                <div className="card-title">{t("panel.openCodeGo")}</div>
                <button className="card-empty clickable" onClick={onOpenSettings}>
                    {error.startsWith("OpenCode Go 登录态无效或已过期")
                        ? t("panel.openCodeGoExpired")
                        : error}
                </button>
            </section>
        );
    }

    if (!usage) {
        return (
            <section className={`card${loading ? " loading" : ""}`}>
                <div className="card-title">{t("panel.openCodeGo")}</div>
                <div className="card-empty">{t("panel.noData")}</div>
            </section>
        );
    }

    return (
        <section className={`card subscription-card${loading ? " loading" : ""}`}>
            <div className="card-title">{t("panel.openCodeGo")}</div>
            {rowOrder.map((row) => {
                switch (row) {
                    case "five_hour":
                        return showFiveHour ? (
                            <QuotaRow
                                key={row}
                                title={t("panel.openCodeGoFiveHour")}
                                window={usage.five_hour}
                                exchangeRate={usage.exchange_rate}
                                now={now}
                            />
                        ) : null;
                    case "weekly":
                        return showWeekly ? (
                            <QuotaRow
                                key={row}
                                title={t("panel.openCodeGoWeekly")}
                                window={usage.weekly}
                                exchangeRate={usage.exchange_rate}
                                now={now}
                            />
                        ) : null;
                    case "monthly":
                        return showMonthly ? (
                            <QuotaRow
                                key={row}
                                title={t("panel.openCodeGoMonthly")}
                                window={usage.monthly}
                                exchangeRate={usage.exchange_rate}
                                now={now}
                            />
                        ) : null;
                }
            })}
        </section>
    );
}

export default memo(OpenCodeGoCard);
