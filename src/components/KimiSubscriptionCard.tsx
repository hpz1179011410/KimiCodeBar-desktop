import { memo } from "react";
import { useTranslation } from "react-i18next";
import { formatYuan } from "../lib/format";
import type { BoosterInfo, KimiSubscriptionRowKey, MonthlyInfo, QuotaWindow } from "../types";
import { formatResetText, useNow } from "./UsageCard";

function formatPercent(value: number): string {
    const rounded = Math.round(value * 100) / 100;
    return Number.isInteger(rounded) ? String(rounded) : rounded.toFixed(2);
}

function RemainingQuotaRow({
    title,
    window,
    now,
}: {
    title: string;
    window: QuotaWindow | null;
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
    const used = window.limit > 0 ? (window.used / window.limit) * 100 : 0;
    const low = window.limit > 0 && window.remaining_percent < 0.2;

    return (
        <div className="subscription-window">
            <div className="subscription-window-head">
                <span className="subscription-window-name">{title}</span>
                <span className={`subscription-window-percent${low ? " low" : ""}`}>
                    {formatPercent(remaining)}%<span>{t("panel.remaining")}</span>
                </span>
            </div>
            <div className="progress subscription-progress">
                <div
                    className={`progress-fill${low ? " low" : ""}`}
                    style={{ width: `${remaining}%` }}
                />
            </div>
            <div className="subscription-window-meta">
                <span>{t("panel.usedOfLimit", { pct: formatPercent(used) })}</span>
                {resetText && <span>{resetText}</span>}
            </div>
        </div>
    );
}

function MonthlyQuotaRow({
    info,
    error,
    now,
    onOpenSettings,
}: {
    info: MonthlyInfo | null;
    error: string | null;
    now: number;
    onOpenSettings?: () => void;
}) {
    const { t } = useTranslation();
    const resetText = formatResetText(info?.reset_time ?? null, now, t);

    if (error) {
        const expired = error.startsWith("网页登录态无效或已过期");
        return (
            <div className="subscription-window">
                <div className="subscription-window-head">
                    <span className="subscription-window-name">{t("panel.monthly")}</span>
                    {expired ? (
                        <button className="subscription-window-message" onClick={onOpenSettings}>
                            {t("panel.monthlyExpired")}
                        </button>
                    ) : (
                        <span className="subscription-window-message">{error}</span>
                    )}
                </div>
            </div>
        );
    }

    if (!info) {
        return (
            <div className="subscription-window">
                <div className="subscription-window-head">
                    <span className="subscription-window-name">{t("panel.monthly")}</span>
                    <span className="subscription-window-empty">{t("panel.noData")}</span>
                </div>
            </div>
        );
    }

    const used = Math.max(0, Math.min(100, info.total_pct));
    const low = used > 80;
    return (
        <div className="subscription-window">
            <div className="subscription-window-head">
                <span className="subscription-window-name">{t("panel.monthly")}</span>
                <span className={`subscription-window-percent${low ? " low" : ""}`}>
                    {formatPercent(used)}%<span>{t("panel.monthlyUsed")}</span>
                </span>
            </div>
            <div className="progress subscription-progress">
                <div
                    className={`progress-fill${low ? " low" : ""}`}
                    style={{ width: `${used}%` }}
                />
            </div>
            <div className="subscription-window-meta">
                <span>
                    {info.code_pct > 0
                        ? t("panel.monthlyCode", { pct: formatPercent(info.code_pct) })
                        : t("panel.usedOfLimit", { pct: formatPercent(used) })}
                </span>
                {resetText && <span>{resetText}</span>}
            </div>
        </div>
    );
}

function BoosterQuotaRow({ booster }: { booster: BoosterInfo | null }) {
    const { t } = useTranslation();

    if (!booster) {
        return (
            <div className="subscription-window">
                <div className="subscription-window-head">
                    <span className="subscription-window-name">{t("panel.booster")}</span>
                    <span className="subscription-window-empty">{t("panel.noData")}</span>
                </div>
            </div>
        );
    }

    if (!booster.enabled) {
        return (
            <div className="subscription-window">
                <div className="subscription-window-head">
                    <span className="subscription-window-name">{t("panel.booster")}</span>
                    <span className="subscription-window-empty">{t("panel.boosterDisabled")}</span>
                </div>
            </div>
        );
    }

    return (
        <div className="subscription-window">
            <div className="subscription-window-head">
                <span className="subscription-window-name">{t("panel.booster")}</span>
                <span className="subscription-window-percent">
                    {formatYuan(booster.amount_left_yuan)}
                    <span>{t("panel.boosterBalance")}</span>
                </span>
            </div>
            {booster.price_yuan != null && (
                <div className="subscription-window-meta subscription-booster-meta">
                    <span>
                        {t("panel.boosterLimit")} {formatYuan(booster.price_yuan)}
                    </span>
                </div>
            )}
        </div>
    );
}

function KimiSubscriptionCard({
    weekly,
    fiveHour,
    monthly,
    monthlyError,
    booster,
    showWeekly,
    showFiveHour,
    showMonthly,
    showBooster,
    rowOrder,
    loading = false,
    onOpenSettings,
}: {
    weekly: QuotaWindow | null;
    fiveHour: QuotaWindow | null;
    monthly: MonthlyInfo | null;
    monthlyError: string | null;
    booster: BoosterInfo | null;
    showWeekly: boolean;
    showFiveHour: boolean;
    showMonthly: boolean;
    showBooster: boolean;
    rowOrder: KimiSubscriptionRowKey[];
    loading?: boolean;
    onOpenSettings?: () => void;
}) {
    const { t } = useTranslation();
    const now = useNow();

    return (
        <section className={`card subscription-card${loading ? " loading" : ""}`}>
            <div className="card-title">{t("panel.kimiSubscription")}</div>
            {rowOrder.map((row) => {
                switch (row) {
                    case "weekly":
                        return showWeekly ? (
                            <RemainingQuotaRow
                                key={row}
                                title={t("panel.weekly")}
                                window={weekly}
                                now={now}
                            />
                        ) : null;
                    case "five_hour":
                        return showFiveHour ? (
                            <RemainingQuotaRow
                                key={row}
                                title={t("panel.fiveHour")}
                                window={fiveHour}
                                now={now}
                            />
                        ) : null;
                    case "monthly":
                        return showMonthly ? (
                            <MonthlyQuotaRow
                                key={row}
                                info={monthly}
                                error={monthlyError}
                                now={now}
                                onOpenSettings={onOpenSettings}
                            />
                        ) : null;
                    case "booster":
                        return showBooster ? <BoosterQuotaRow key={row} booster={booster} /> : null;
                }
            })}
        </section>
    );
}

export default memo(KimiSubscriptionCard);
