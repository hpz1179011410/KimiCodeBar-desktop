import { memo, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import type { LocalUsageReport, ModelDayUsage, ModelTrend } from "../types";
import { formatTokens, shortDate } from "../lib/format";

type TrendMetric = "tokens" | "cache";

const WIDTH = 300;
const HEIGHT = 132;
// 与图例行的模型文字、右侧用量文字对齐，避免绘图区视觉上向右缩进。
const LEFT = 18;
const RIGHT = 4;
const TOP = 10;
const BOTTOM = 22;
const PLOT_WIDTH = WIDTH - LEFT - RIGHT;
const PLOT_HEIGHT = HEIGHT - TOP - BOTTOM;
const SERIES_COLORS = [
    "#6ea2ff",
    "#9ece6a",
    "#e0af68",
    "#bb9af7",
    "#f7768e",
    "#2ac3de",
    "#ff9e64",
    "#73daca",
];

function pointX(index: number, count: number) {
    return LEFT + (count <= 1 ? 0 : (index / (count - 1)) * PLOT_WIDTH);
}

function pointY(value: number, maxValue: number) {
    return TOP + (1 - Math.max(0, Math.min(1, value / Math.max(1, maxValue)))) * PLOT_HEIGHT;
}

function linePath(points: Array<{ x: number; y: number }>) {
    return points
        .map((point, index) => `${index === 0 ? "M" : "L"}${point.x},${point.y}`)
        .join(" ");
}

function metricValue(day: ModelDayUsage, metric: TrendMetric) {
    return metric === "tokens" ? day.tokens : day.cache_hit_rate;
}

function seriesSegments(trend: ModelTrend, metric: TrendMetric, maxValue: number) {
    const segments: Array<Array<{ x: number; y: number }>> = [];
    let segment: Array<{ x: number; y: number }> = [];
    const flush = () => {
        if (segment.length > 1) segments.push(segment);
        segment = [];
    };

    trend.days.forEach((day, index) => {
        const value = metricValue(day, metric);
        if (value == null) {
            flush();
            return;
        }
        segment.push({
            x: pointX(index, trend.days.length),
            y: pointY(value, maxValue),
        });
    });
    flush();
    return segments;
}

function formatRate(rate: number | null, fallback: string) {
    return rate == null ? fallback : `${(rate * 100).toFixed(1)}%`;
}

/** 折线几何与悬停状态无关，拆出 memo 层，移动鼠标时不重复计算全部模型路径。 */
const TrendSeriesLayer = memo(function TrendSeriesLayer({
    trends,
    metric,
    maxValue,
}: {
    trends: ModelTrend[];
    metric: TrendMetric;
    maxValue: number;
}) {
    return trends.map((trend, trendIndex) => {
        const color = SERIES_COLORS[trendIndex % SERIES_COLORS.length];
        return (
            <g key={`${metric}:${trend.is_secondary}:${trend.model}`}>
                {seriesSegments(trend, metric, maxValue).map((segment, index) => (
                    <path
                        className="trend-line"
                        style={{ stroke: color }}
                        pathLength={1}
                        d={linePath(segment)}
                        key={index}
                    />
                ))}
                {trend.days.map((day, dayIndex) => {
                    const value = metricValue(day, metric);
                    return value == null ? null : (
                        <circle
                            className="trend-point"
                            style={{ fill: color }}
                            cx={pointX(dayIndex, trend.days.length)}
                            cy={pointY(value, maxValue)}
                            r="3"
                            key={day.date}
                        />
                    );
                })}
            </g>
        );
    });
});

function CombinedTrendChart({ trends, metric }: { trends: ModelTrend[]; metric: TrendMetric }) {
    const { t } = useTranslation();
    const [hoveredIndex, setHoveredIndex] = useState<number | null>(null);
    const days = trends[0]?.days ?? [];
    const maxValue = useMemo(
        () =>
            metric === "tokens"
                ? Math.max(1, ...trends.flatMap((trend) => trend.days.map((day) => day.tokens)))
                : 1,
        [metric, trends],
    );
    const topLabel = metric === "tokens" ? formatTokens(maxValue) : "100%";
    const bottomLabel = metric === "tokens" ? "0" : "0%";
    const hoveredDay = hoveredIndex == null ? null : days[hoveredIndex];
    const tooltipClass =
        hoveredIndex == null
            ? ""
            : hoveredIndex <= 1
              ? " edge-left"
              : hoveredIndex >= days.length - 2
                ? " edge-right"
                : "";

    return (
        <div className="combined-trend-chart-wrap" onMouseLeave={() => setHoveredIndex(null)}>
            <svg
                className="model-trend-chart"
                viewBox={`0 0 ${WIDTH} ${HEIGHT}`}
                role="img"
                aria-label={t("panel.modelTrend")}
            >
                {[0, 0.5, 1].map((ratio) => {
                    const y = TOP + ratio * PLOT_HEIGHT;
                    return (
                        <line
                            className="trend-grid"
                            x1={LEFT}
                            y1={y}
                            x2={WIDTH - RIGHT}
                            y2={y}
                            key={ratio}
                        />
                    );
                })}

                <text className="trend-axis-label" x={LEFT - 4} y={TOP + 3} textAnchor="end">
                    {topLabel}
                </text>
                <text
                    className="trend-axis-label"
                    x={LEFT - 4}
                    y={TOP + PLOT_HEIGHT + 3}
                    textAnchor="end"
                >
                    {bottomLabel}
                </text>

                <TrendSeriesLayer trends={trends} metric={metric} maxValue={maxValue} />

                {hoveredIndex != null && (
                    <line
                        className="trend-crosshair"
                        x1={pointX(hoveredIndex, days.length)}
                        y1={TOP}
                        x2={pointX(hoveredIndex, days.length)}
                        y2={TOP + PLOT_HEIGHT}
                    />
                )}

                {days.map((day, index) => {
                    const x = pointX(index, days.length);
                    const step = days.length <= 1 ? PLOT_WIDTH : PLOT_WIDTH / (days.length - 1);
                    const hitLeft = index === 0 ? LEFT : x - step / 2;
                    const hitRight = index === days.length - 1 ? WIDTH - RIGHT : x + step / 2;
                    return (
                        <g key={day.date}>
                            <text
                                className="trend-date-label"
                                x={x}
                                y={HEIGHT - 5}
                                textAnchor="middle"
                            >
                                {shortDate(day.date)}
                            </text>
                            <rect
                                className="trend-hit-zone"
                                x={hitLeft}
                                y={TOP}
                                width={hitRight - hitLeft}
                                height={PLOT_HEIGHT}
                                onMouseEnter={() => setHoveredIndex(index)}
                            />
                        </g>
                    );
                })}
            </svg>

            {hoveredDay && hoveredIndex != null && (
                <div
                    className={`combined-trend-tooltip${tooltipClass}`}
                    style={{ left: `${(pointX(hoveredIndex, days.length) / WIDTH) * 100}%` }}
                >
                    <div className="combined-trend-tooltip-date">{hoveredDay.date}</div>
                    {trends.map((trend, index) => {
                        const day = trend.days[hoveredIndex];
                        const value = day ? metricValue(day, metric) : null;
                        const formatted =
                            value == null
                                ? t("panel.noRate")
                                : metric === "tokens"
                                  ? formatTokens(value)
                                  : `${(value * 100).toFixed(1)}%`;
                        return (
                            <div
                                className="combined-trend-tooltip-row"
                                key={`${trend.is_secondary}:${trend.model}`}
                            >
                                <span
                                    className="trend-series-dot"
                                    style={{
                                        background: SERIES_COLORS[index % SERIES_COLORS.length],
                                    }}
                                />
                                <span className="combined-trend-tooltip-model">{trend.model}</span>
                                <span className="combined-trend-tooltip-value">{formatted}</span>
                            </div>
                        );
                    })}
                </div>
            )}
        </div>
    );
}

function ModelTrendCard({
    report,
    loading = false,
}: {
    report: LocalUsageReport | null;
    loading?: boolean;
}) {
    const { t } = useTranslation();
    const [metric, setMetric] = useState<TrendMetric>("tokens");
    const trends = report?.model_trends ?? [];

    return (
        <section className={`card${loading ? " loading" : ""}`}>
            <div className="model-trend-title-row">
                <div className="card-title">{t("panel.modelTrend")}</div>
                <div className="trend-metric-toggle" role="tablist">
                    <button
                        className={metric === "tokens" ? "active" : ""}
                        role="tab"
                        aria-selected={metric === "tokens"}
                        onClick={() => setMetric("tokens")}
                    >
                        {t("panel.trendTokens")}
                    </button>
                    <button
                        className={metric === "cache" ? "active" : ""}
                        role="tab"
                        aria-selected={metric === "cache"}
                        onClick={() => setMetric("cache")}
                    >
                        {t("panel.trendCache")}
                    </button>
                </div>
            </div>

            {trends.length === 0 ? (
                <div className="card-empty">{t("panel.modelTrendEmpty")}</div>
            ) : (
                <>
                    <div className="combined-trend-legend">
                        {trends.map((trend, index) => (
                            <div
                                className="combined-trend-legend-row"
                                key={`${trend.is_secondary}:${trend.model}`}
                            >
                                <span
                                    className="trend-series-dot"
                                    style={{
                                        background: SERIES_COLORS[index % SERIES_COLORS.length],
                                    }}
                                />
                                <span className="combined-trend-model" title={trend.model}>
                                    {trend.model}
                                </span>
                                {trend.is_secondary && (
                                    <span className="model-secondary">
                                        {t("panel.secondaryModel")}
                                    </span>
                                )}
                                <span className="combined-trend-value">
                                    {metric === "tokens"
                                        ? formatTokens(trend.seven_day_tokens)
                                        : formatRate(
                                              trend.seven_day_cache_hit_rate,
                                              t("panel.noRate"),
                                          )}
                                </span>
                            </div>
                        ))}
                    </div>
                    <CombinedTrendChart trends={trends} metric={metric} />
                </>
            )}
        </section>
    );
}

export default memo(ModelTrendCard);
