//! 本地用量增量扫描测试：临时目录构造 wire.jsonl，覆盖
//! 增量扫描、残尾处理、截断重读、消失文件清理、分桶聚合。
//! 改环境变量的测试用全局互斥锁串行。

use kimicodebar_lib::local_usage::{bucket_day, build_report, parse_usage_record, scan_and_update};
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

static ENV_LOCK: Mutex<()> = Mutex::new(());

/// 当前时间（epoch 毫秒）。必须落在 30 天保留窗口内，否则会被 prune 掉。
fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

fn test_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("kcb-test-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn record(time_ms: i64, model: &str, a: i64, b: i64, c: i64, d: i64) -> String {
    format!(
        r#"{{"type":"usage.record","time":{time_ms},"model":"{model}","usage":{{"inputOther":{a},"output":{b},"inputCacheRead":{c},"inputCacheCreation":{d}}}}}"#
    )
}

fn secondary_request(model: &str) -> String {
    format!(
        r#"{{"type":"llm.request","provider":"openai","model":"{model}","modelAlias":"__secondary__"}}"#
    )
}

fn secondary_config(alias: &str, wire_model: &str) -> String {
    format!(
        r#"[models."{alias}"]
provider = "test-provider"
model = "{wire_model}"

[secondary_model]
model = "{alias}"
"#
    )
}

#[test]
fn parse_record_rules() {
    // 正常行：四字段求和；input = inputOther+CacheRead+CacheCreation
    let t = now_ms();
    let s = parse_usage_record(&record(t, "kimi-k2", 10, 20, 30, 40)).unwrap();
    assert_eq!(s.time_ms, t);
    assert_eq!(s.tokens, 100);
    assert_eq!(s.input, 80);
    assert_eq!(s.cache_read, 30);
    assert_eq!(s.model.as_deref(), Some("kimi-k2"));

    // 非 usage.record → 丢弃
    assert!(parse_usage_record(r#"{"type":"message","time":1}"#).is_none());
    // time 缺失 → 丢弃
    assert!(parse_usage_record(
        r#"{"type":"usage.record","usage":{"inputOther":1,"output":1,"inputCacheRead":1,"inputCacheCreation":1}}"#
    )
    .is_none());
    // 坏 JSON → 丢弃
    assert!(parse_usage_record("{not json").is_none());
}

#[test]
fn incremental_scan_and_tail_handling() {
    let root = test_dir("scan");
    let sessions = root.join("sessions");
    let sess = sessions.join("ws1").join("s1");
    fs::create_dir_all(&sess).unwrap();
    let wire = sess.join("wire.jsonl");
    let state_path = root.join("scan-state.json");
    let day = bucket_day(now_ms()).unwrap();

    let line1 = record(now_ms(), "kimi-k2", 10, 20, 30, 40); // 100
    let line2 = record(now_ms(), "kimi-k1", 1, 2, 3, 4); // 10
                                                         // 残尾：不完整 JSON 且无换行
    fs::write(
        &wire,
        format!("{line1}\n{line2}\n{{\"type\":\"usage.record\",\"time\":175"),
    )
    .unwrap();

    let state = scan_and_update(&sessions, &state_path).unwrap();
    assert_eq!(state.by_date.get(&day), Some(&110));
    assert_eq!(state.by_model.get("kimi-k2"), Some(&100));
    assert_eq!(state.by_model.get("kimi-k1"), Some(&10));
    // 缓存命中：input = 80+8 = 88，cache_read = 30+3 = 33 → 33/88 = 0.375
    let day_stat = state.cache_by_date.get(&day).unwrap();
    assert_eq!(day_stat.input, 88);
    assert_eq!(day_stat.cache_read, 33);
    assert!((day_stat.hit_rate().unwrap() - 0.375).abs() < 1e-9);
    assert_eq!(
        state
            .model_by_date
            .get(&day)
            .and_then(|models| models.get("kimi-k2")),
        Some(&100)
    );
    // 偏移停在最后一个 \n 之后（残尾不消费）
    let offset = state.files.values().next().unwrap().offset;
    assert_eq!(offset as usize, line1.len() + 1 + line2.len() + 1);

    // 二次扫描幂等（无新增数据）
    let state = scan_and_update(&sessions, &state_path).unwrap();
    assert_eq!(state.by_date.get(&day), Some(&110));

    // 补全残尾为新行后，增量被消费
    let line3 = record(now_ms(), "kimi-k2", 5, 5, 5, 5); // 20
    fs::write(&wire, format!("{line1}\n{line2}\n{line3}\n")).unwrap();
    let state = scan_and_update(&sessions, &state_path).unwrap();
    assert_eq!(state.by_date.get(&day), Some(&130));
    assert_eq!(state.by_model.get("kimi-k2"), Some(&120));

    let report = build_report(&state);
    assert_eq!(
        report.top_models.first().map(|m| m.model.as_str()),
        Some("kimi-k2")
    );
    // 今日命中率：input 103，cache_read 38 → 38/103
    let rate = report.today_cache_hit_rate.expect("今日应有命中率");
    assert!((rate - 38.0 / 103.0).abs() < 1e-9);
    // 近 7 天合计命中率（仅今天有数据，与今日相同）
    let week = report.week_cache_hit_rate.expect("7 天应有命中率");
    assert!((week - rate).abs() < 1e-9);
    // 每天的命中率随 last_7_days 输出；今天以外的日期无数据 → None
    let today_entry = report.last_7_days.last().unwrap();
    assert_eq!(
        today_entry.cache_hit_rate.map(|r| (r * 1e9).round() as i64),
        Some((rate * 1e9).round() as i64)
    );
    assert!(report.last_7_days[0].cache_hit_rate.is_none());

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn secondary_model_uses_request_identity_and_separate_cache_stat() {
    let root = test_dir("secondary-model");
    let sessions = root.join("sessions");
    let sess = sessions.join("ws1").join("s1");
    fs::create_dir_all(&sess).unwrap();
    let wire = sess.join("wire.jsonl");
    let state_path = root.join("scan-state.json");
    let config_path = root.join("config.toml");

    let alias_a = "supplier-a/deepseek-v4-flash";
    let alias_b = "supplier-b/deepseek-v4-flash";
    fs::write(&config_path, secondary_config(alias_a, "deepseek-v4-flash")).unwrap();
    let usage_a_time = now_ms() - 2 * 24 * 60 * 60 * 1000;
    let usage_a_day = bucket_day(usage_a_time).unwrap();
    let usage_a = record(usage_a_time, "__secondary__", 10, 20, 30, 40); // 100
    fs::write(
        &wire,
        format!("{}\n", secondary_request("deepseek-v4-flash")),
    )
    .unwrap();

    // 先只扫描请求，验证配置身份能跨增量边界保存。
    let state = scan_and_update(&sessions, &state_path).unwrap();
    assert!(state.by_model.is_empty());

    // 即使配置随后切到同 ID 的另一供应商，上一条请求仍应归到 supplier-a。
    fs::write(&config_path, secondary_config(alias_b, "deepseek-v4-flash")).unwrap();
    let mut file = fs::OpenOptions::new().append(true).open(&wire).unwrap();
    writeln!(file, "{usage_a}").unwrap();
    drop(file);
    let state = scan_and_update(&sessions, &state_path).unwrap();
    assert_eq!(
        state.by_model.get(&format!("secondary::{alias_a}")),
        Some(&100)
    );
    assert_eq!(
        state
            .model_by_date
            .get(&usage_a_day)
            .and_then(|models| models.get(&format!("secondary::{alias_a}"))),
        Some(&100)
    );
    assert!(!state.by_model.contains_key("__secondary__"));

    // 新请求使用当前配置，同名模型必须按供应商拆成另一项。
    let usage_b = record(now_ms(), "__secondary__", 1, 2, 3, 4); // 10
    let mut file = fs::OpenOptions::new().append(true).open(&wire).unwrap();
    writeln!(file, "{}", secondary_request("deepseek-v4-flash")).unwrap();
    writeln!(file, "{usage_b}").unwrap();
    drop(file);

    let state = scan_and_update(&sessions, &state_path).unwrap();
    assert_eq!(
        state.by_model.get(&format!("secondary::{alias_b}")),
        Some(&10)
    );
    let report = build_report(&state);
    let supplier_a = report
        .top_models
        .iter()
        .find(|m| m.model == alias_a)
        .unwrap();
    assert!(supplier_a.is_secondary);
    assert_eq!(supplier_a.tokens, 100);
    assert!((supplier_a.cache_hit_rate.unwrap() - 30.0 / 80.0).abs() < 1e-9);
    let supplier_b = report
        .top_models
        .iter()
        .find(|m| m.model == alias_b)
        .unwrap();
    assert!(supplier_b.is_secondary);
    assert!((supplier_b.cache_hit_rate.unwrap() - 3.0 / 8.0).abs() < 1e-9);
    let trend_a = report
        .model_trends
        .iter()
        .find(|trend| trend.model == alias_a)
        .unwrap();
    assert_eq!(trend_a.seven_day_tokens, 100);
    assert!((trend_a.seven_day_cache_hit_rate.unwrap() - 30.0 / 80.0).abs() < 1e-9);
    assert_eq!(trend_a.days.len(), 7);
    let trend_a_day = trend_a
        .days
        .iter()
        .find(|day| day.date == usage_a_day)
        .unwrap();
    assert_eq!(trend_a_day.tokens, 100);
    assert!((trend_a_day.cache_hit_rate.unwrap() - 30.0 / 80.0).abs() < 1e-9);
    let trend_b = report
        .model_trends
        .iter()
        .find(|trend| trend.model == alias_b)
        .unwrap();
    assert_eq!(trend_b.seven_day_tokens, 10);
    assert!((trend_b.seven_day_cache_hit_rate.unwrap() - 3.0 / 8.0).abs() < 1e-9);
    assert_eq!(trend_b.days.last().unwrap().tokens, 10);

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn truncated_file_rereads_from_scratch() {
    let root = test_dir("truncate");
    let sessions = root.join("sessions");
    let sess = sessions.join("ws1").join("s1");
    fs::create_dir_all(&sess).unwrap();
    let wire = sess.join("wire.jsonl");
    let state_path = root.join("scan-state.json");
    let day = bucket_day(now_ms()).unwrap();

    fs::write(
        &wire,
        format!(
            "{}\n{}\n",
            record(now_ms(), "m", 10, 20, 30, 40),
            record(now_ms(), "m", 1, 2, 3, 4)
        ),
    )
    .unwrap();
    let state = scan_and_update(&sessions, &state_path).unwrap();
    assert_eq!(state.by_date.get(&day), Some(&110));

    // 文件被截断重写（更短）：撤销旧贡献，从头重读
    fs::write(&wire, format!("{}\n", record(now_ms(), "m", 1, 1, 1, 4))).unwrap(); // 7
    let state = scan_and_update(&sessions, &state_path).unwrap();
    assert_eq!(state.by_date.get(&day), Some(&7));
    // 缓存统计同步撤销：仅剩新内容 input=1+1+4=6, cache_read=1
    let day_stat = state.cache_by_date.get(&day).unwrap();
    assert_eq!(day_stat.input, 6);
    assert_eq!(day_stat.cache_read, 1);

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn disappeared_file_cleans_up_contribution() {
    let root = test_dir("gone");
    let sessions = root.join("sessions");
    let sess = sessions.join("ws1").join("s1");
    fs::create_dir_all(&sess).unwrap();
    let wire = sess.join("wire.jsonl");
    let state_path = root.join("scan-state.json");
    let day = bucket_day(now_ms()).unwrap();

    fs::write(
        &wire,
        format!("{}\n", record(now_ms(), "m", 10, 20, 30, 40)),
    )
    .unwrap();
    let state = scan_and_update(&sessions, &state_path).unwrap();
    assert_eq!(state.by_date.get(&day), Some(&100));

    // 文件消失：偏移清理、贡献撤销
    fs::remove_file(&wire).unwrap();
    let state = scan_and_update(&sessions, &state_path).unwrap();
    assert!(state.files.is_empty());
    assert!(!state.by_date.contains_key(&day));
    assert!(state.by_model.is_empty());
    assert!(state.cache_by_date.is_empty());
    assert!(state.cache_by_model.is_empty());
    assert!(state.model_by_date.is_empty());
    assert!(state.model_cache_by_date.is_empty());

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn env_vars_override_paths() {
    let _guard = ENV_LOCK.lock().unwrap();

    std::env::set_var("KIMICODEBAR_CONFIG_DIR", r"D:\kcb-test-config");
    assert_eq!(
        kimicodebar_lib::config::config_dir(),
        PathBuf::from(r"D:\kcb-test-config")
    );
    std::env::remove_var("KIMICODEBAR_CONFIG_DIR");

    std::env::set_var("KIMI_CODE_HOME", r"D:\kcb-test-home");
    assert_eq!(
        kimicodebar_lib::config::kimi_code_home(),
        PathBuf::from(r"D:\kcb-test-home")
    );
    std::env::remove_var("KIMI_CODE_HOME");
}
