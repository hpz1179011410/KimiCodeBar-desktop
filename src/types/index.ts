// 与 Rust 侧 serde 输出严格对齐的 TS 类型（snake_case 字段）。

export type LoginMethod = "oauth" | "api_key";

/** 桌面小部件卡片标识（与 Rust 侧 WidgetCard 枚举 snake_case 对齐） */
export type WidgetCardKey = "kimi_subscription" | "open_code_go";

/** 主面板业务卡片标识（数组顺序即显示顺序） */
export type PanelCardKey = "kimi_subscription" | "open_code_go" | "local_usage" | "model_trend";

/** 两张订阅组合卡片的内部行标识（数组顺序即显示顺序） */
export type KimiSubscriptionRowKey = "weekly" | "five_hour" | "monthly" | "booster";
export type OpenCodeGoRowKey = "five_hour" | "weekly" | "monthly";

/** 小部件窗口坐标（物理像素） */
export interface WidgetPosition {
    x: number;
    y: number;
}

export interface UpdateCheckCache {
    last_checked_at: number | null;
    latest_version: string | null;
    last_error: string | null;
}

export interface AppSettings {
    login_method: LoginMethod;
    /** 配额轮询间隔（秒），下限 60 */
    refresh_interval_secs: number;
    /** system / dark / light */
    theme: string;
    /** system / zh / en */
    language: string;
    show_weekly_card: boolean;
    show_five_hour_card: boolean;
    show_booster_card: boolean;
    show_local_usage_card: boolean;
    show_model_trend_card: boolean;
    show_monthly_card: boolean;
    show_opencode_go_card: boolean;
    show_opencode_go_five_hour_card: boolean;
    show_opencode_go_weekly_card: boolean;
    show_opencode_go_monthly_card: boolean;
    kimi_subscription_rows: KimiSubscriptionRowKey[];
    opencode_go_rows: OpenCodeGoRowKey[];
    /** 主面板卡片顺序；显隐由各 show_* 开关控制 */
    panel_cards: PanelCardKey[];
    auto_archive_enabled: boolean;
    auto_archive_threshold_days: number;
    /** 桌面小部件开关 */
    widget_enabled: boolean;
    /** 卡片显隐 + 顺序（数组顺序即显示顺序） */
    widget_cards: WidgetCardKey[];
    /** 拖拽记忆位置（无记忆时显示在工作区右下角） */
    widget_position: WidgetPosition | null;
    app_update_check: UpdateCheckCache;
}

export interface LoginState {
    method: LoginMethod;
    logged_in: boolean;
    masked_key: string | null;
}

export interface DeviceLoginStart {
    user_code: string;
    verification_uri_complete: string | null;
}

export interface QuotaWindow {
    limit: number;
    used: number;
    remaining: number;
    /** remaining / limit（0~1） */
    remaining_percent: number;
    /** RFC3339 */
    reset_time: string | null;
}

export interface BoosterInfo {
    enabled: boolean;
    /** 余额（元），未启用时为 0 */
    amount_left_yuan: number;
    /** 月度上限（元） */
    price_yuan: number | null;
}

export interface QuotaInfo {
    weekly: QuotaWindow | null;
    five_hour: QuotaWindow | null;
    total: QuotaWindow | null;
    booster: BoosterInfo | null;
    membership_level: string | null;
    /** RFC3339 */
    fetched_at: string;
    low_warning: boolean;
}

export interface DayUsage {
    date: string;
    tokens: number;
    /** 当日缓存命中率 0-1（无输入数据 → null） */
    cache_hit_rate: number | null;
}

export interface ModelUsage {
    model: string;
    tokens: number;
    /** 该模型累计缓存命中率 0-1（无输入数据 → null） */
    cache_hit_rate: number | null;
    /** 是否来自 Kimi Code 的 SECONDARY_MODEL */
    is_secondary: boolean;
}

export interface ModelDayUsage {
    date: string;
    tokens: number;
    /** 该模型当日缓存命中率 0-1（无输入数据 → null） */
    cache_hit_rate: number | null;
}

export interface ModelTrend {
    model: string;
    is_secondary: boolean;
    seven_day_tokens: number;
    /** 最近 7 天加权缓存命中率 0-1（无输入数据 → null） */
    seven_day_cache_hit_rate: number | null;
    /** 最近 7 天（含今天，日期升序；无用量的日期补零） */
    days: ModelDayUsage[];
}

export interface LocalUsageReport {
    today_tokens: number;
    /** 今日缓存命中率 0-1（无输入数据 → null） */
    today_cache_hit_rate: number | null;
    /** 最近 7 天合计缓存命中率 0-1（无输入数据 → null） */
    week_cache_hit_rate: number | null;
    /** 最近 7 天（含今天，日期升序） */
    last_7_days: DayUsage[];
    by_date: Record<string, number>;
    top_models: ModelUsage[];
    model_trends: ModelTrend[];
    /** RFC3339 */
    scanned_at: string;
}

export interface SessionInfo {
    /** "<工作区目录名>/<会话目录名>" */
    id: string;
    title: string | null;
    updated_at: string | null;
    updated_at_ms: number | null;
    archived: boolean;
    work_dir: string | null;
    /** state.json 绝对路径 */
    path: string;
}

export interface WorkdirGroup {
    work_dir: string;
    sessions: SessionInfo[];
}

export interface ArchiveOverview {
    total: number;
    archived: number;
    groups: WorkdirGroup[];
}

export interface SkillInfo {
    /** 目录名（read_skill 的入参） */
    dir: string;
    name: string;
    description: string | null;
    /** SKILL.md 绝对路径 */
    path: string;
}

export interface CliUpdateInfo {
    current: string | null;
    latest: string | null;
    update_available: boolean;
}

export interface AppUpdateInfo {
    current: string;
    latest: string | null;
    update_available: boolean;
    release_url: string;
    error: string | null;
}

export interface MonthlyInfo {
    /** 月度总额度已用百分比 */
    total_pct: number;
    /** 其中 Kimi 占用百分比 */
    kimi_pct: number;
    /** 其中 Kimi Code 占用百分比 */
    code_pct: number;
    /** expireTime 原样透传（可能缺省） */
    reset_time?: string;
}

export interface OpenCodeGoWindow {
    /** 套餐窗口额度（美元计价） */
    limit_usd: number;
    /** 按控制台百分比折算的已用美元值 */
    used_usd: number;
    /** 控制台报告的已用百分比（0-100） */
    used_percent: number;
    /** 剩余百分比（0-1） */
    remaining_percent: number;
    /** RFC3339 */
    reset_time: string;
}

export interface OpenCodeGoExchangeRate {
    /** 1 美元对应的人民币参考值 */
    usd_cny: number;
    /** 欧洲央行参考汇率日期（YYYY-MM-DD） */
    reference_date: string;
}

export interface OpenCodeGoUsage {
    five_hour: OpenCodeGoWindow | null;
    weekly: OpenCodeGoWindow | null;
    monthly: OpenCodeGoWindow | null;
    /** RFC3339 */
    fetched_at: string;
    low_warning: boolean;
    exchange_rate: OpenCodeGoExchangeRate | null;
}
