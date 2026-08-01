//! 设置持久化（settings.json）与通用原子写工具。

use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub const DEFAULT_REFRESH_INTERVAL_SECS: u64 = 180;
pub const MIN_REFRESH_INTERVAL_SECS: u64 = 60;
pub const DEFAULT_ARCHIVE_THRESHOLD_DAYS: u64 = 7;

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LoginMethod {
    #[default]
    Oauth,
    ApiKey,
}

/// 桌面小部件卡片标识（数组顺序即显示顺序）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WidgetCard {
    KimiSubscription,
    OpenCodeGo,
    /// 以下三项只用于兼容旧 settings.json，normalized() 会合并为 KimiSubscription。
    /// 本月用量（旧设置中的 "total" 已存盘，反序列化兼容）
    #[serde(alias = "total")]
    Monthly,
    Weekly,
    FiveHour,
}

impl WidgetCard {
    const ALL: [Self; 2] = [Self::KimiSubscription, Self::OpenCodeGo];
}

/// 主面板业务卡片标识（数组顺序即显示顺序，版本信息卡固定在末尾）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PanelCard {
    KimiSubscription,
    /// 以下四项只用于兼容旧 settings.json，normalized() 会合并为 KimiSubscription。
    Weekly,
    FiveHour,
    Monthly,
    OpenCodeGo,
    Booster,
    LocalUsage,
    ModelTrend,
}

impl PanelCard {
    const ALL: [Self; 4] = [
        Self::KimiSubscription,
        Self::OpenCodeGo,
        Self::LocalUsage,
        Self::ModelTrend,
    ];
}

/// Kimi 订阅组合卡片内部行顺序。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KimiSubscriptionRow {
    Weekly,
    FiveHour,
    Monthly,
    Booster,
}

impl KimiSubscriptionRow {
    const ALL: [Self; 4] = [Self::Weekly, Self::FiveHour, Self::Monthly, Self::Booster];
}

/// OpenCode Go 组合卡片内部行顺序。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OpenCodeGoRow {
    FiveHour,
    Weekly,
    Monthly,
}

impl OpenCodeGoRow {
    const ALL: [Self; 3] = [Self::FiveHour, Self::Weekly, Self::Monthly];
}

fn default_kimi_subscription_rows() -> Vec<KimiSubscriptionRow> {
    KimiSubscriptionRow::ALL.to_vec()
}

fn default_opencode_go_rows() -> Vec<OpenCodeGoRow> {
    OpenCodeGoRow::ALL.to_vec()
}

/// 行顺序去重并补齐新增项，保留用户已有顺序。
fn normalize_order<T: Copy + PartialEq, const N: usize>(items: &mut Vec<T>, all: [T; N]) {
    let mut seen = Vec::new();
    items.retain(|item| {
        if seen.contains(item) {
            false
        } else {
            seen.push(*item);
            true
        }
    });
    for item in all {
        if !seen.contains(&item) {
            items.push(item);
            seen.push(item);
        }
    }
}

/// 小部件窗口坐标（物理像素，与 WindowEvent::Moved 一致）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WidgetPosition {
    pub x: i32,
    pub y: i32,
}

/// App 更新检查缓存（写入 settings.json）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct UpdateCheckCache {
    /// 上次检查时间（epoch 秒）
    pub last_checked_at: Option<i64>,
    /// 上次检查到的最新版本（成功时写入）
    pub latest_version: Option<String>,
    /// 上次检查的错误信息（失败时写入）
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    pub login_method: LoginMethod,
    /// 配额轮询间隔（秒），默认 180，下限 60
    pub refresh_interval_secs: u64,
    /// system / dark / light
    pub theme: String,
    /// system / zh / en
    pub language: String,
    // ---- 面板卡片显隐开关 ----
    pub show_weekly_card: bool,
    pub show_five_hour_card: bool,
    pub show_booster_card: bool,
    pub show_local_usage_card: bool,
    /// 独立模型趋势卡片；字段新增时，已有设置文件也默认开启。
    #[serde(default = "default_true")]
    pub show_model_trend_card: bool,
    pub show_monthly_card: bool,
    /// OpenCode Go 订阅卡片；新增字段对已有设置默认开启。
    #[serde(default = "default_true")]
    pub show_opencode_go_card: bool,
    /// OpenCode Go 组合卡片的行级显隐；旧设置缺少字段时默认全部开启。
    #[serde(default = "default_true")]
    pub show_opencode_go_five_hour_card: bool,
    #[serde(default = "default_true")]
    pub show_opencode_go_weekly_card: bool,
    #[serde(default = "default_true")]
    pub show_opencode_go_monthly_card: bool,
    /// 两张订阅组合卡片的内部行顺序。
    #[serde(default = "default_kimi_subscription_rows")]
    pub kimi_subscription_rows: Vec<KimiSubscriptionRow>,
    #[serde(default = "default_opencode_go_rows")]
    pub opencode_go_rows: Vec<OpenCodeGoRow>,
    /// 主面板卡片顺序；显隐仍由上面的独立开关控制。
    pub panel_cards: Vec<PanelCard>,
    // ---- 自动归档 ----
    pub auto_archive_enabled: bool,
    /// 归档阈值（天），常用 1 / 7 / 30
    pub auto_archive_threshold_days: u64,
    // ---- 桌面小部件 ----
    pub widget_enabled: bool,
    /// 卡片显隐 + 顺序（数组顺序即显示顺序）
    pub widget_cards: Vec<WidgetCard>,
    /// 拖拽记忆位置（无记忆时显示在工作区右下角）
    pub widget_position: Option<WidgetPosition>,
    // ---- 更新检查缓存 ----
    pub app_update_check: UpdateCheckCache,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            login_method: LoginMethod::Oauth,
            refresh_interval_secs: DEFAULT_REFRESH_INTERVAL_SECS,
            theme: "system".into(),
            language: "system".into(),
            show_weekly_card: true,
            show_five_hour_card: true,
            show_booster_card: true,
            show_local_usage_card: true,
            show_model_trend_card: true,
            show_monthly_card: true,
            show_opencode_go_card: true,
            show_opencode_go_five_hour_card: true,
            show_opencode_go_weekly_card: true,
            show_opencode_go_monthly_card: true,
            kimi_subscription_rows: default_kimi_subscription_rows(),
            opencode_go_rows: default_opencode_go_rows(),
            panel_cards: PanelCard::ALL.to_vec(),
            auto_archive_enabled: false,
            auto_archive_threshold_days: DEFAULT_ARCHIVE_THRESHOLD_DAYS,
            widget_enabled: false,
            widget_cards: WidgetCard::ALL.to_vec(),
            widget_position: None,
            app_update_check: UpdateCheckCache::default(),
        }
    }
}

impl AppSettings {
    /// 修正非法值：间隔下限 60s，阈值为 0 时回到默认 7 天。
    pub fn normalized(mut self) -> Self {
        if self.refresh_interval_secs == 0 {
            self.refresh_interval_secs = DEFAULT_REFRESH_INTERVAL_SECS;
        }
        self.refresh_interval_secs = self.refresh_interval_secs.max(MIN_REFRESH_INTERVAL_SECS);
        if self.auto_archive_threshold_days == 0 {
            self.auto_archive_threshold_days = DEFAULT_ARCHIVE_THRESHOLD_DAYS;
        }
        // 兼容只有整卡开关的旧设置：旧值为关闭时，新增的三个行级开关也应关闭。
        if !self.show_opencode_go_card {
            self.show_opencode_go_five_hour_card = false;
            self.show_opencode_go_weekly_card = false;
            self.show_opencode_go_monthly_card = false;
        }
        normalize_order(&mut self.kimi_subscription_rows, KimiSubscriptionRow::ALL);
        normalize_order(&mut self.opencode_go_rows, OpenCodeGoRow::ALL);
        // 主面板卡片迁移 + 去重：旧周/5h/月度/加油包在首次出现的位置合并为 Kimi 订阅卡。
        self.panel_cards = self
            .panel_cards
            .into_iter()
            .map(|card| match card {
                PanelCard::Weekly
                | PanelCard::FiveHour
                | PanelCard::Monthly
                | PanelCard::Booster => PanelCard::KimiSubscription,
                other => other,
            })
            .collect();
        // 已有设置保留原顺序，新卡片追加到末尾。
        let mut seen_panel: Vec<PanelCard> = Vec::new();
        self.panel_cards.retain(|card| {
            if seen_panel.contains(card) {
                false
            } else {
                seen_panel.push(*card);
                true
            }
        });
        for card in PanelCard::ALL {
            if !seen_panel.contains(&card) {
                self.panel_cards.push(card);
                seen_panel.push(card);
            }
        }
        // 小部件迁移：旧本月/周/5h 合并为 Kimi 订阅卡，并为旧配置补入 OpenCode Go。
        // 新格式中用户主动关闭 OpenCode Go 后不再自动补回。
        let had_legacy_widget_card = self.widget_cards.iter().any(|card| {
            matches!(
                card,
                WidgetCard::Monthly | WidgetCard::Weekly | WidgetCard::FiveHour
            )
        });
        self.widget_cards = self
            .widget_cards
            .into_iter()
            .map(|card| match card {
                WidgetCard::Monthly | WidgetCard::Weekly | WidgetCard::FiveHour => {
                    WidgetCard::KimiSubscription
                }
                other => other,
            })
            .collect();
        // 小部件卡片去重（保序）；全部取消时回退默认两张，防止小部件空白。
        let mut seen: Vec<WidgetCard> = Vec::new();
        self.widget_cards.retain(|c| {
            if seen.contains(c) {
                false
            } else {
                seen.push(*c);
                true
            }
        });
        if had_legacy_widget_card && !seen.contains(&WidgetCard::OpenCodeGo) {
            self.widget_cards.push(WidgetCard::OpenCodeGo);
        }
        if self.widget_cards.is_empty() {
            self.widget_cards = AppSettings::default().widget_cards;
        }
        self
    }
}

pub fn settings_path(config_dir: &Path) -> PathBuf {
    config_dir.join("settings.json")
}

pub fn load_settings(config_dir: &Path) -> AppSettings {
    let path = settings_path(config_dir);
    let Ok(text) = fs::read_to_string(&path) else {
        return AppSettings::default();
    };
    // 容错 UTF-8 BOM（部分编辑器/脚本会写入），否则 serde_json 解析失败回退默认设置
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    serde_json::from_str::<AppSettings>(text)
        .unwrap_or_default()
        .normalized()
}

pub fn save_settings(config_dir: &Path, settings: &AppSettings) -> io::Result<()> {
    let settings = settings.clone().normalized();
    let text = serde_json::to_string_pretty(&settings)?;
    atomic_write(&settings_path(config_dir), text.as_bytes())
}

/// 通用原子写：写 tmp → 目标存在先删 → rename（Windows 上 rename 不能覆盖已存在文件）。
pub fn atomic_write(path: &Path, data: &[u8]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, data)?;
    if path.exists() {
        fs::remove_file(path)?;
    }
    fs::rename(&tmp, path)?;
    Ok(())
}
