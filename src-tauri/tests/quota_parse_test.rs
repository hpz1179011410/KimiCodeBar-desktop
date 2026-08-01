//! 配额解析正确性测试：字符串数字、5h 窗口筛选（duration==300）、
//! remaining 反推、booster amountLeft/1e8 与未启用置零。

use kimicodebar_lib::kimi::models::UsagesResponse;
use kimicodebar_lib::quota::parse_usage;

fn load_fixture(name: &str) -> UsagesResponse {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("读取 {path} 失败: {e}"));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("解析 {path} 失败: {e}"))
}

#[test]
fn full_response_parses_all_sections() {
    let info = parse_usage(&load_fixture("full.json"));

    // 周窗口：字符串数字，remaining = limit - used 反推
    let weekly = info.weekly.as_ref().expect("weekly 应存在");
    assert_eq!(weekly.limit, 1000);
    assert_eq!(weekly.used, 200);
    assert_eq!(weekly.remaining, 800);
    assert!((weekly.remaining_percent - 0.8).abs() < 1e-9);
    assert_eq!(
        weekly.reset_time.as_deref(),
        Some("2026-08-06T00:00:00+00:00")
    );

    // 5h 窗口：limits[] 中 duration == 300 的那条（不是 10080 的那条）
    let five_hour = info.five_hour.as_ref().expect("five_hour 应存在");
    assert_eq!(five_hour.limit, 100);
    assert_eq!(five_hour.used, 90);
    assert_eq!(five_hour.remaining, 10);
    // 带 +08:00 偏移的 resetTime 归一化为 UTC
    assert_eq!(
        five_hour.reset_time.as_deref(),
        Some("2026-07-30T07:00:00+00:00")
    );

    // 总额度：used = limit - remaining 反推
    let total = info.total.as_ref().expect("total 应存在");
    assert_eq!(total.limit, 2000);
    assert_eq!(total.used, 500);
    assert_eq!(total.remaining, 1500);

    // booster：amountLeft / 1e8 元，priceInCents / 100 元
    let booster = info.booster.as_ref().expect("booster 应存在");
    assert!(booster.enabled);
    assert!((booster.amount_left_yuan - 1.23456789).abs() < 1e-9);
    assert!((booster.price_yuan.unwrap() - 5.0).abs() < 1e-9);

    assert_eq!(info.membership_level.as_deref(), Some("pro"));

    // 5h 剩余 10% < 20% → 低额告警
    assert!(info.low_warning);
    assert!(info.needs_low_warning());
}

#[test]
fn missing_booster_yields_none_and_no_warning() {
    let info = parse_usage(&load_fixture("no_booster.json"));
    assert!(info.booster.is_none());
    assert!(info.membership_level.is_none());
    // 各窗口剩余均 >= 20%
    assert!(!info.low_warning);
    let five_hour = info.five_hour.expect("five_hour 应存在");
    assert_eq!(five_hour.remaining, 60);
}

#[test]
fn proto3_omitted_fields_and_string_duration() {
    let info = parse_usage(&load_fixture("proto3_omit.json"));
    // 无 resetTime / totalQuota / boosterWallet（proto3 省略空值）
    let weekly = info.weekly.expect("weekly 应存在");
    assert_eq!(weekly.remaining, 700);
    assert!(weekly.reset_time.is_none());
    assert!(info.total.is_none());
    assert!(info.booster.is_none());
    // duration 以字符串形式出现时也能识别 5h 窗口
    let five_hour = info.five_hour.expect("duration 字符串也应匹配 300");
    assert_eq!(five_hour.limit, 50);
    assert!(!info.low_warning);
}

#[test]
fn sparse_fields_and_malformed_numbers() {
    let info = parse_usage(&load_fixture("sparse.json"));
    assert!(info.weekly.is_none());
    assert!(info.five_hour.is_none());

    // limit 带空白可解析；remaining 非法 → 按 0 处理，used 反推为满额
    let total = info.total.expect("total 应存在");
    assert_eq!(total.limit, 3000);
    assert_eq!(total.used, 3000);
    assert_eq!(total.remaining, 0);
    // 剩余 0% → 低额告警
    assert!(info.low_warning);

    // booster 未启用：余额显示 0，价格仍可展示
    let booster = info.booster.expect("booster 应存在");
    assert!(!booster.enabled);
    assert_eq!(booster.amount_left_yuan, 0.0);
    assert!((booster.price_yuan.unwrap() - 1.99).abs() < 1e-9);

    // membership 为纯字符串形态
    assert_eq!(info.membership_level.as_deref(), Some("free"));
}

/// 基于真实 API 响应（脱敏）：明细嵌套在 limits[].detail，
/// membership 在 user.membership.level，booster 金额字段嵌套在 balance/月度对象里。
#[test]
fn real_response_nested_detail_and_membership() {
    let text = std::fs::read_to_string(format!(
        "{}/tests/fixtures/real_sanitized.json",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap();
    let wire: UsagesResponse = serde_json::from_str(&text).unwrap();
    let info = parse_usage(&wire);

    // 周窗口
    let weekly = info.weekly.as_ref().expect("weekly 应存在");
    assert_eq!(weekly.limit, 100);
    assert_eq!(weekly.used, 35);
    assert_eq!(weekly.remaining, 65);

    // 5h 窗口：明细在 detail 里（微秒级 resetTime 也应解析成功）
    let five_hour = info.five_hour.as_ref().expect("five_hour 应从 detail 解析");
    assert_eq!(five_hour.limit, 100);
    assert_eq!(five_hour.used, 56);
    assert_eq!(five_hour.remaining, 44);
    assert_eq!(
        five_hour.reset_time.as_deref(),
        Some("2026-07-30T14:40:34.344771+00:00")
    );

    // totalQuota 为空对象 → None
    assert!(info.total.is_none());

    // booster 未启用：余额显示 0，月度上限 0 分 → 0 元
    let booster = info.booster.as_ref().expect("booster 应存在");
    assert!(!booster.enabled);
    assert_eq!(booster.amount_left_yuan, 0.0);
    assert!((booster.price_yuan.unwrap() - 0.0).abs() < 1e-9);

    // membership 在 user.membership.level
    assert_eq!(info.membership_level.as_deref(), Some("LEVEL_INTERMEDIATE"));

    // 启用态 + 嵌套 balance.amountLeft：1e-8 元换算（315250700 → 3.152507 元）
    let mut value: serde_json::Value = serde_json::from_str(&text).unwrap();
    let booster_value = value.get_mut("boosterWallet").unwrap();
    booster_value["status"] = serde_json::json!("STATUS_ACTIVE");
    booster_value["balance"]["amountLeft"] = serde_json::json!("315250700");
    booster_value["monthlyChargeLimit"]["priceInCents"] = serde_json::json!("7500");
    let wire2: UsagesResponse = serde_json::from_value(value).unwrap();
    let info2 = parse_usage(&wire2);
    let booster2 = info2.booster.as_ref().unwrap();
    assert!(booster2.enabled);
    assert!((booster2.amount_left_yuan - 3.152507).abs() < 1e-9);
    assert!((booster2.price_yuan.unwrap() - 75.0).abs() < 1e-9);
}
