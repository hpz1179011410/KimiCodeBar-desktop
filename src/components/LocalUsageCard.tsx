import { useTranslation } from "react-i18next";
import type { LocalUsageReport } from "../types";
import { formatTokens, localDate, shortDate } from "../lib/format";

export default function LocalUsageCard({
    report,
    loading = false,
}: {
    report: LocalUsageReport | null;
    loading?: boolean;
}) {
    const { t } = useTranslation();

    if (!report) {
        return (
            <section className={`card${loading ? " loading" : ""}`}>
                <div className="card-title">{t("panel.localUsage")}</div>
                <div className="card-empty">{t("panel.noLocalData")}</div>
            </section>
        );
    }

    const yesterdayTokens = report.by_date[localDate(-1)] ?? 0;
    const weekTotalTokens = report.last_7_days.reduce((sum, d) => sum + d.tokens, 0);
    const maxTokens = Math.max(1, ...report.last_7_days.map((d) => d.tokens));
    const fmtRate = (r: number | null) =>
        r == null ? t("panel.noRate") : `${(r * 100).toFixed(1)}%`;

    return (
        <section className={`card${loading ? " loading" : ""}`}>
            <div className="card-title">{t("panel.localUsage")}</div>
            <div className="local-stats">
                <div className="local-stat">
                    <div className="local-stat-value">{formatTokens(report.today_tokens)}</div>
                    <div className="local-stat-label">{t("panel.today")}</div>
                </div>
                <div className="local-stat">
                    <div className="local-stat-value">{formatTokens(yesterdayTokens)}</div>
                    <div className="local-stat-label">{t("panel.yesterday")}</div>
                </div>
                <div className="local-stat">
                    <div className="local-stat-value">{fmtRate(report.today_cache_hit_rate)}</div>
                    <div className="local-stat-label">{t("panel.cacheHitToday")}</div>
                </div>
            </div>

            <div className="chart-title">
                {t("panel.last7days")}
                <span className="chart-title-extra">{formatTokens(weekTotalTokens)}</span>
                {report.week_cache_hit_rate != null && (
                    <span className="chart-title-extra">
                        {t("panel.cacheHitWeek")} {fmtRate(report.week_cache_hit_rate)}
                    </span>
                )}
            </div>
            <div className="chart">
                {report.last_7_days.map((d) => (
                    <div className="bar-col" key={d.date}>
                        <div className="bar-track">
                            <div
                                className="bar"
                                style={{ height: `${Math.round((d.tokens / maxTokens) * 100)}%` }}
                            />
                        </div>
                        <div className="bar-tooltip">
                            <span>{d.date}</span>
                            <span>
                                {t("panel.tipTotalLabel")}
                                <span className="tt-num">{formatTokens(d.tokens)}</span> tokens
                            </span>
                            {d.cache_hit_rate != null && (
                                <span>
                                    {t("panel.tipRateLabel")}
                                    <span className="tt-num">{fmtRate(d.cache_hit_rate)}</span>
                                </span>
                            )}
                        </div>
                        <div className="bar-label">{shortDate(d.date)}</div>
                    </div>
                ))}
            </div>

            {report.top_models.length > 0 && (
                <>
                    <div className="chart-title">{t("panel.topModels")}</div>
                    <div className="model-list">
                        {report.top_models.map((m) => (
                            <div className="model-row" key={`${m.is_secondary}:${m.model}`}>
                                <span className="model-name" title={m.model}>
                                    {m.model}
                                    {m.is_secondary && (
                                        <span className="model-secondary">
                                            {t("panel.secondaryModel")}
                                        </span>
                                    )}
                                </span>
                                <span className="model-metrics">
                                    {m.cache_hit_rate != null && (
                                        <span className="model-cache">
                                            {t("panel.cacheHit")} {fmtRate(m.cache_hit_rate)}
                                        </span>
                                    )}
                                    <span className="model-tokens">{formatTokens(m.tokens)}</span>
                                </span>
                            </div>
                        ))}
                    </div>
                </>
            )}
        </section>
    );
}
