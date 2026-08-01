//! 本地用量统计：增量扫描 `%USERPROFILE%\.kimi-code\sessions\**\wire.jsonl`。
//!
//! 规则：
//! - `usage.record` 的 inputOther+output+inputCacheRead+inputCacheCreation 求和
//! - `SECONDARY_MODEL` 的用量模型名固定为内部别名 `__secondary__`；扫描同文件前序
//!   `llm.request`，并结合 config.toml 的完整模型别名归属供应商与模型
//! - time 为 epoch 毫秒（缺失丢弃）；按本地时区 YYYY-MM-DD 分桶
//! - scan-state.json 记录每文件偏移 + by_date/model_by_date（保留 30 天）/by_model 累计
//! - 残尾不消费（偏移停在最后一个 \n 之后）；文件变短→撤销该文件旧贡献后从头重读；
//!   已消失文件→撤销贡献并清理偏移
//! - 永不因扫描失败崩溃（错误向上返回，由调用方记录）

use chrono::{DateTime, Duration as ChronoDuration, Local, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use crate::storage;

/// 进程内节流：180s 内不重复扫描（在命令层执行）。
pub const SCAN_THROTTLE_SECS: u64 = 180;
const RETENTION_DAYS: i64 = 30;
const TOP_MODELS: usize = 5;
/// scan-state.json 结构版本：5 = 增加按日期、模型交叉聚合的趋势数据。
const SCAN_STATE_VERSION: u32 = 5;
const SECONDARY_MODEL_ALIAS: &str = "__secondary__";
/// 状态文件内部 key 前缀，防止二级模型与同名主模型的聚合互相污染。
const SECONDARY_MODEL_KEY_PREFIX: &str = "secondary::";

/// 缓存命中统计：hit_rate = cache_read / input。
/// input = inputOther + inputCacheRead + inputCacheCreation（输入侧总量）。
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct CacheStat {
    pub input: i64,
    pub cache_read: i64,
}

impl CacheStat {
    /// 命中率（input 为 0 时无数据 → None）
    pub fn hit_rate(&self) -> Option<f64> {
        if self.input > 0 {
            Some(self.cache_read as f64 / self.input as f64)
        } else {
            None
        }
    }

    fn add(&mut self, other: &CacheStat) {
        self.input += other.input;
        self.cache_read += other.cache_read;
    }

    fn sub(&mut self, other: &CacheStat) {
        self.input -= other.input;
        self.cache_read -= other.cache_read;
    }

    fn is_empty(&self) -> bool {
        self.input <= 0 && self.cache_read <= 0
    }
}

/// 单文件扫描状态：偏移 + 该文件贡献的聚合（用于截断/消失时撤销）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct FileScanState {
    pub offset: u64,
    pub by_date: BTreeMap<String, i64>,
    pub by_model: HashMap<String, i64>,
    pub cache_by_date: BTreeMap<String, CacheStat>,
    pub cache_by_model: HashMap<String, CacheStat>,
    pub model_by_date: BTreeMap<String, HashMap<String, i64>>,
    pub model_cache_by_date: BTreeMap<String, HashMap<String, CacheStat>>,
    /// 最近一次二级模型请求发给供应商接口的模型 ID。
    pub secondary_model: Option<String>,
    /// 最近一次二级模型请求对应的完整配置别名（含供应商），例如 opencode-go/deepseek-v4-flash。
    pub secondary_model_alias: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ScanState {
    /// 结构版本，低于 SCAN_STATE_VERSION 视为过期（load 时重置全扫）
    pub version: u32,
    /// key = 文件路径字符串
    pub files: HashMap<String, FileScanState>,
    pub by_date: BTreeMap<String, i64>,
    pub by_model: HashMap<String, i64>,
    pub cache_by_date: BTreeMap<String, CacheStat>,
    pub cache_by_model: HashMap<String, CacheStat>,
    pub model_by_date: BTreeMap<String, HashMap<String, i64>>,
    pub model_cache_by_date: BTreeMap<String, HashMap<String, CacheStat>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DayUsage {
    pub date: String,
    pub tokens: i64,
    /// 当日缓存命中率（无输入数据 → null）
    pub cache_hit_rate: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ModelUsage {
    pub model: String,
    pub tokens: i64,
    /// 该模型累计缓存命中率（无输入数据 → null）
    pub cache_hit_rate: Option<f64>,
    /// 是否来自 Kimi Code 的 SECONDARY_MODEL。
    pub is_secondary: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ModelDayUsage {
    pub date: String,
    pub tokens: i64,
    /// 该模型当日缓存命中率（无输入数据 → null）
    pub cache_hit_rate: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ModelTrend {
    pub model: String,
    pub is_secondary: bool,
    pub seven_day_tokens: i64,
    /// 最近 7 天加权缓存命中率（无输入数据 → null）
    pub seven_day_cache_hit_rate: Option<f64>,
    /// 最近 7 天（含今天，日期升序；无用量的日期补零）
    pub days: Vec<ModelDayUsage>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LocalUsageReport {
    pub today_tokens: i64,
    /// 今日缓存命中率（无输入数据 → null）
    pub today_cache_hit_rate: Option<f64>,
    /// 最近 7 天合计缓存命中率（无输入数据 → null）
    pub week_cache_hit_rate: Option<f64>,
    /// 最近 7 天（含今天，日期升序）
    pub last_7_days: Vec<DayUsage>,
    /// 保留 30 天的完整分桶
    pub by_date: BTreeMap<String, i64>,
    /// 按模型 top5
    pub top_models: Vec<ModelUsage>,
    /// 最近 7 天有用量的全部模型趋势，按 7 天总量降序。
    pub model_trends: Vec<ModelTrend>,
    pub scanned_at: String,
}

pub fn sessions_dir(kimi_home: &Path) -> PathBuf {
    kimi_home.join("sessions")
}

pub fn scan_state_path(config_dir: &Path) -> PathBuf {
    config_dir.join("scan-state.json")
}

/// 单条 usage.record 的解析结果。
#[derive(Debug, Clone, PartialEq)]
pub struct UsageSample {
    pub time_ms: i64,
    pub tokens: i64,
    /// 输入侧总量：inputOther + inputCacheRead + inputCacheCreation
    pub input: i64,
    /// 命中缓存的输入 token
    pub cache_read: i64,
    pub model: Option<String>,
}

/// 解析单行：非 usage.record / time 缺失 → None。
pub fn parse_usage_record(line: &str) -> Option<UsageSample> {
    let v: serde_json::Value = serde_json::from_str(line).ok()?;
    parse_usage_value(&v)
}

fn parse_usage_value(v: &serde_json::Value) -> Option<UsageSample> {
    if v.get("type").and_then(|t| t.as_str()) != Some("usage.record") {
        return None;
    }
    let time_ms = v.get("time").and_then(|t| t.as_i64())?;
    let usage = v.get("usage")?;
    let field = |key: &str| usage.get(key).and_then(|x| x.as_i64()).unwrap_or(0);
    let input_other = field("inputOther");
    let output = field("output");
    let cache_read = field("inputCacheRead");
    let cache_creation = field("inputCacheCreation");
    // model 位置不确定，兼容顶层 / usage 内两种（待真实样本验证）
    let model = v
        .get("model")
        .and_then(|m| m.as_str())
        .or_else(|| usage.get("model").and_then(|m| m.as_str()))
        .map(|s| s.to_string());
    Some(UsageSample {
        time_ms,
        tokens: input_other + output + cache_read + cache_creation,
        input: input_other + cache_read + cache_creation,
        cache_read,
        model,
    })
}

/// 从 llm.request 取出二级模型发给供应商接口的模型 ID。
///
/// Kimi Code 会把 `usage.record.model` 固定写成 `__secondary__`，只有紧邻请求事件
/// 的 `model` 字段保留接口模型 ID；供应商身份仍需读取 config.toml 的模型别名。
fn parse_secondary_request_model(v: &serde_json::Value) -> Option<String> {
    if v.get("type").and_then(|t| t.as_str()) != Some("llm.request")
        || v.get("modelAlias").and_then(|m| m.as_str()) != Some(SECONDARY_MODEL_ALIAS)
    {
        return None;
    }
    v.get("model")
        .and_then(|m| m.as_str())
        .map(str::trim)
        .filter(|m| !m.is_empty())
        .map(str::to_string)
}

#[derive(Debug, Clone)]
struct ConfiguredSecondaryModel {
    /// Kimi Code 配置使用的完整别名，包含供应商身份。
    alias: String,
    /// 该别名最终发给供应商接口的模型 ID；缺失时不做一致性校验。
    wire_model: Option<String>,
}

/// 读取本次扫描时生效的 SECONDARY_MODEL 配置。
///
/// 环境变量与 Kimi Code 一致，优先于 config.toml。模型别名必须原样保留，不能仅使用
/// llm.request.provider（它是 openai/anthropic 等协议类型，不是 deepseek/opencode-go 等供应商）。
fn configured_secondary_model(sessions_root: &Path) -> Option<ConfiguredSecondaryModel> {
    let text = sessions_root
        .parent()
        .map(|kimi_home| kimi_home.join("config.toml"))
        .and_then(|path| fs::read_to_string(path).ok());
    let config = text.as_deref().and_then(|text| {
        toml::from_str::<toml::Value>(text.strip_prefix('\u{feff}').unwrap_or(text)).ok()
    });

    let env_alias = std::env::var("KIMI_SECONDARY_MODEL")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let alias = env_alias.or_else(|| {
        config
            .as_ref()?
            .get("secondary_model")?
            .get("model")?
            .as_str()
            .map(str::to_string)
    })?;
    let wire_model = config
        .as_ref()
        .and_then(|config| config.get("models"))
        .and_then(|models| models.get(&alias))
        .and_then(|model| model.get("model"))
        .and_then(toml::Value::as_str)
        .map(str::to_string);

    Some(ConfiguredSecondaryModel { alias, wire_model })
}

fn model_key(model: &str, is_secondary: bool) -> String {
    if is_secondary {
        format!("{SECONDARY_MODEL_KEY_PREFIX}{model}")
    } else {
        model.to_string()
    }
}

fn model_from_key(key: &str) -> (&str, bool) {
    match key.strip_prefix(SECONDARY_MODEL_KEY_PREFIX) {
        Some(model) => (model, true),
        None => (key, false),
    }
}

/// epoch 毫秒 → 本地时区 YYYY-MM-DD。
pub fn bucket_day(time_ms: i64) -> Option<String> {
    DateTime::from_timestamp_millis(time_ms)
        .map(|dt| dt.with_timezone(&Local).format("%Y-%m-%d").to_string())
}

fn collect_wire_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_wire_files(&path, out);
        } else if entry.file_name() == "wire.jsonl" {
            out.push(path);
        }
    }
}

fn load_state(state_path: &Path) -> ScanState {
    let state = fs::read_to_string(state_path)
        .ok()
        .and_then(|text| serde_json::from_str::<ScanState>(&text).ok())
        .unwrap_or_default();
    // 结构版本过期：旧数据不含缓存统计，整体作废触发全量重扫
    if state.version < SCAN_STATE_VERSION {
        return ScanState::default();
    }
    state
}

fn save_state(state_path: &Path, state: &ScanState) -> Result<(), String> {
    let text = serde_json::to_string_pretty(state).map_err(|e| e.to_string())?;
    storage::atomic_write(state_path, text.as_bytes()).map_err(|e| e.to_string())
}

/// 从全局聚合中撤销某文件的贡献。
fn subtract_file(state: &mut ScanState, entry: &FileScanState) {
    for (day, v) in &entry.by_date {
        let remove = match state.by_date.get_mut(day) {
            Some(total) => {
                *total -= v;
                *total <= 0
            }
            None => false,
        };
        if remove {
            state.by_date.remove(day);
        }
    }
    for (model, v) in &entry.by_model {
        let remove = match state.by_model.get_mut(model) {
            Some(total) => {
                *total -= v;
                *total <= 0
            }
            None => false,
        };
        if remove {
            state.by_model.remove(model);
        }
    }
    for (day, v) in &entry.cache_by_date {
        let remove = match state.cache_by_date.get_mut(day) {
            Some(total) => {
                total.sub(v);
                total.is_empty()
            }
            None => false,
        };
        if remove {
            state.cache_by_date.remove(day);
        }
    }
    for (model, v) in &entry.cache_by_model {
        let remove = match state.cache_by_model.get_mut(model) {
            Some(total) => {
                total.sub(v);
                total.is_empty()
            }
            None => false,
        };
        if remove {
            state.cache_by_model.remove(model);
        }
    }
    for (day, models) in &entry.model_by_date {
        let mut remove_day = false;
        if let Some(total_models) = state.model_by_date.get_mut(day) {
            for (model, value) in models {
                let remove_model = match total_models.get_mut(model) {
                    Some(total) => {
                        *total -= value;
                        *total <= 0
                    }
                    None => false,
                };
                if remove_model {
                    total_models.remove(model);
                }
            }
            remove_day = total_models.is_empty();
        }
        if remove_day {
            state.model_by_date.remove(day);
        }
    }
    for (day, models) in &entry.model_cache_by_date {
        let mut remove_day = false;
        if let Some(total_models) = state.model_cache_by_date.get_mut(day) {
            for (model, value) in models {
                let remove_model = match total_models.get_mut(model) {
                    Some(total) => {
                        total.sub(value);
                        total.is_empty()
                    }
                    None => false,
                };
                if remove_model {
                    total_models.remove(model);
                }
            }
            remove_day = total_models.is_empty();
        }
        if remove_day {
            state.model_cache_by_date.remove(day);
        }
    }
}

/// 只保留最近 keep_days 天的分桶（含今天）。
fn prune_by_date(state: &mut ScanState, keep_days: i64) {
    let cutoff = (Local::now() - ChronoDuration::days(keep_days - 1))
        .format("%Y-%m-%d")
        .to_string();
    state
        .by_date
        .retain(|day, _| day.as_str() >= cutoff.as_str());
    state
        .cache_by_date
        .retain(|day, _| day.as_str() >= cutoff.as_str());
    state
        .model_by_date
        .retain(|day, _| day.as_str() >= cutoff.as_str());
    state
        .model_cache_by_date
        .retain(|day, _| day.as_str() >= cutoff.as_str());
}

/// 增量扫描 sessions 目录并更新 scan-state.json。返回最新聚合状态。
pub fn scan_and_update(sessions_root: &Path, state_path: &Path) -> Result<ScanState, String> {
    let mut state = load_state(state_path);
    let configured_secondary = configured_secondary_model(sessions_root);

    let mut files = Vec::new();
    if sessions_root.is_dir() {
        collect_wire_files(sessions_root, &mut files);
    }

    let mut seen: HashSet<String> = HashSet::new();
    for file in files {
        let key = file.to_string_lossy().into_owned();
        seen.insert(key.clone());

        let len = fs::metadata(&file).map(|m| m.len()).unwrap_or(0);
        let mut entry = state.files.remove(&key).unwrap_or_default();
        let mut offset = entry.offset;
        if offset > len {
            // 文件变短（截断/重写）：撤销旧贡献，从头重读
            subtract_file(&mut state, &entry);
            entry = FileScanState::default();
            offset = 0;
        }

        let Ok(mut f) = fs::File::open(&file) else {
            state.files.insert(key, entry);
            continue;
        };
        if f.seek(SeekFrom::Start(offset)).is_err() {
            state.files.insert(key, entry);
            continue;
        }
        let mut buf = Vec::new();
        if f.read_to_end(&mut buf).is_err() {
            state.files.insert(key, entry);
            continue;
        }

        // 残尾不消费：只处理到最后一个 \n
        let consumed = match buf.iter().rposition(|b| *b == b'\n') {
            Some(idx) => idx + 1,
            None => 0,
        };
        if consumed > 0 {
            let text = String::from_utf8_lossy(&buf[..consumed]);
            for line in text.lines() {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
                    continue;
                };
                if let Some(model) = parse_secondary_request_model(&value) {
                    entry.secondary_model_alias = configured_secondary
                        .as_ref()
                        .filter(|configured| match configured.wire_model.as_deref() {
                            Some(name) => name == model,
                            None => true,
                        })
                        .map(|configured| configured.alias.clone());
                    entry.secondary_model = Some(model);
                    continue;
                }
                if let Some(sample) = parse_usage_value(&value) {
                    let stat = CacheStat {
                        input: sample.input,
                        cache_read: sample.cache_read,
                    };
                    let model_key = sample.model.as_deref().map(|model| {
                        let is_secondary = model == SECONDARY_MODEL_ALIAS;
                        let resolved_model = if is_secondary {
                            entry
                                .secondary_model_alias
                                .as_deref()
                                .or(entry.secondary_model.as_deref())
                                .unwrap_or(SECONDARY_MODEL_ALIAS)
                        } else {
                            model
                        };
                        model_key(resolved_model, is_secondary)
                    });
                    if let Some(day) = bucket_day(sample.time_ms) {
                        *state.by_date.entry(day.clone()).or_insert(0) += sample.tokens;
                        *entry.by_date.entry(day.clone()).or_insert(0) += sample.tokens;
                        state
                            .cache_by_date
                            .entry(day.clone())
                            .or_default()
                            .add(&stat);
                        entry
                            .cache_by_date
                            .entry(day.clone())
                            .or_default()
                            .add(&stat);
                        if let Some(key) = &model_key {
                            *state
                                .model_by_date
                                .entry(day.clone())
                                .or_default()
                                .entry(key.clone())
                                .or_insert(0) += sample.tokens;
                            *entry
                                .model_by_date
                                .entry(day.clone())
                                .or_default()
                                .entry(key.clone())
                                .or_insert(0) += sample.tokens;
                            state
                                .model_cache_by_date
                                .entry(day.clone())
                                .or_default()
                                .entry(key.clone())
                                .or_default()
                                .add(&stat);
                            entry
                                .model_cache_by_date
                                .entry(day)
                                .or_default()
                                .entry(key.clone())
                                .or_default()
                                .add(&stat);
                        }
                    }
                    if let Some(key) = model_key {
                        *state.by_model.entry(key.clone()).or_insert(0) += sample.tokens;
                        *entry.by_model.entry(key.clone()).or_insert(0) += sample.tokens;
                        state
                            .cache_by_model
                            .entry(key.clone())
                            .or_default()
                            .add(&stat);
                        entry.cache_by_model.entry(key).or_default().add(&stat);
                    }
                }
            }
            entry.offset = offset + consumed as u64;
        }
        state.files.insert(key, entry);
    }

    // 已消失文件：撤销贡献并清理偏移
    let gone: Vec<String> = state
        .files
        .keys()
        .filter(|k| !seen.contains(*k))
        .cloned()
        .collect();
    for key in gone {
        if let Some(entry) = state.files.remove(&key) {
            subtract_file(&mut state, &entry);
        }
    }

    prune_by_date(&mut state, RETENTION_DAYS);
    state.version = SCAN_STATE_VERSION;
    save_state(state_path, &state)?;
    Ok(state)
}

/// 由聚合状态生成给前端的报告。
pub fn build_report(state: &ScanState) -> LocalUsageReport {
    let today = Local::now().format("%Y-%m-%d").to_string();
    let today_tokens = state.by_date.get(&today).copied().unwrap_or(0);
    let today_cache_hit_rate = state.cache_by_date.get(&today).and_then(|s| s.hit_rate());

    let mut last_7_days = Vec::with_capacity(7);
    let mut week_stat = CacheStat::default();
    for i in (0..7).rev() {
        let date = (Local::now() - ChronoDuration::days(i))
            .format("%Y-%m-%d")
            .to_string();
        let tokens = state.by_date.get(&date).copied().unwrap_or(0);
        let stat = state.cache_by_date.get(&date).copied().unwrap_or_default();
        week_stat.add(&stat);
        last_7_days.push(DayUsage {
            date,
            tokens,
            cache_hit_rate: stat.hit_rate(),
        });
    }

    let mut models: Vec<ModelUsage> = state
        .by_model
        .iter()
        .map(|(key, tokens)| {
            let (model, is_secondary) = model_from_key(key);
            ModelUsage {
                model: model.to_string(),
                tokens: *tokens,
                cache_hit_rate: state.cache_by_model.get(key).and_then(CacheStat::hit_rate),
                is_secondary,
            }
        })
        .collect();
    models.sort_by(|a, b| b.tokens.cmp(&a.tokens).then_with(|| a.model.cmp(&b.model)));
    models.truncate(TOP_MODELS);

    let trend_dates: Vec<String> = last_7_days.iter().map(|day| day.date.clone()).collect();
    let mut trend_totals: HashMap<String, i64> = HashMap::new();
    for date in &trend_dates {
        if let Some(day_models) = state.model_by_date.get(date) {
            for (key, tokens) in day_models {
                *trend_totals.entry(key.clone()).or_insert(0) += tokens;
            }
        }
    }
    let mut trend_models: Vec<(String, i64)> = trend_totals.into_iter().collect();
    trend_models.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    let model_trends = trend_models
        .into_iter()
        .map(|(key, seven_day_tokens)| {
            let (model, is_secondary) = model_from_key(&key);
            let mut seven_day_cache = CacheStat::default();
            let days = trend_dates
                .iter()
                .map(|date| {
                    let tokens = state
                        .model_by_date
                        .get(date)
                        .and_then(|models| models.get(&key))
                        .copied()
                        .unwrap_or(0);
                    let cache_hit_rate = state
                        .model_cache_by_date
                        .get(date)
                        .and_then(|models| models.get(&key))
                        .and_then(CacheStat::hit_rate);
                    if let Some(stat) = state
                        .model_cache_by_date
                        .get(date)
                        .and_then(|models| models.get(&key))
                    {
                        seven_day_cache.add(stat);
                    }
                    ModelDayUsage {
                        date: date.clone(),
                        tokens,
                        cache_hit_rate,
                    }
                })
                .collect();
            ModelTrend {
                model: model.to_string(),
                is_secondary,
                seven_day_tokens,
                seven_day_cache_hit_rate: seven_day_cache.hit_rate(),
                days,
            }
        })
        .collect();

    LocalUsageReport {
        today_tokens,
        today_cache_hit_rate,
        week_cache_hit_rate: week_stat.hit_rate(),
        last_7_days,
        by_date: state.by_date.clone(),
        top_models: models,
        model_trends,
        scanned_at: Utc::now().to_rfc3339(),
    }
}
