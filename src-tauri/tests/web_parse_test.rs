//! 网页端月度用量解析 / token 规范化 / JWT 条件头测试。

use kimicodebar_lib::kimi::web::{
    jwt_identity_headers, normalize_web_token, parse_subscription_stats, MonthlyInfo, WebError,
};

fn load_fixture(name: &str) -> String {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("读取 {path} 失败: {e}"))
}

/// 测试用手写 base64url 编码（不带 `=` 填充，顺带验证缺填充容忍）。
fn b64url_encode(data: &[u8]) -> String {
    const CHARS: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::new();
    for chunk in data.chunks(3) {
        let b0 = u32::from(chunk[0]);
        let b1 = u32::from(*chunk.get(1).unwrap_or(&0));
        let b2 = u32::from(*chunk.get(2).unwrap_or(&0));
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(CHARS[(n >> 18) as usize & 63] as char);
        out.push(CHARS[(n >> 12) as usize & 63] as char);
        if chunk.len() > 1 {
            out.push(CHARS[(n >> 6) as usize & 63] as char);
        }
        if chunk.len() > 2 {
            out.push(CHARS[n as usize & 63] as char);
        }
    }
    out
}

fn make_jwt(payload: &str) -> String {
    format!(
        "{}.{}.sig",
        b64url_encode(br#"{"alg":"HS256"}"#),
        b64url_encode(payload.as_bytes())
    )
}

// ---- 解析 ----

#[test]
fn camel_case_fixture_parses_and_splits() {
    let info = parse_subscription_stats(&load_fixture("web_stats_full.json")).expect("应解析成功");
    // 0.1612 ≤ 1 → ×100
    assert!((info.total_pct - 16.12).abs() < 1e-9);
    assert!((info.code_pct - 5.0).abs() < 1e-9);
    assert!((info.kimi_pct - 11.12).abs() < 1e-9);
    assert_eq!(info.reset_time.as_deref(), Some("2026-08-01T00:00:00Z"));
}

#[test]
fn snake_case_aliases_hit() {
    let info = parse_subscription_stats(&load_fixture("web_stats_snake.json")).expect("应解析成功");
    assert!((info.total_pct - 40.0).abs() < 1e-9);
    assert!((info.code_pct - 10.0).abs() < 1e-9);
    assert!((info.kimi_pct - 30.0).abs() < 1e-9);
    assert_eq!(info.reset_time.as_deref(), Some("2026-08-01T00:00:00Z"));
}

#[test]
fn data_wrapper_drills_down_one_level() {
    let body = r#"{"data": {"subscriptionBalance": {"amountUsedRatio": 0.5}}}"#;
    let info = parse_subscription_stats(body).expect("data 包裹应下钻");
    assert!((info.total_pct - 50.0).abs() < 1e-9);
    assert_eq!(info.code_pct, 0.0);
    assert!(info.reset_time.is_none());
}

#[test]
fn used_ratio_aliases_hit() {
    for body in [
        r#"{"subscriptionBalance": {"usedRatio": 0.4}}"#,
        r#"{"subscriptionBalance": {"used_ratio": 0.4}}"#,
    ] {
        let info = parse_subscription_stats(body).expect("别名应命中");
        assert!((info.total_pct - 40.0).abs() < 1e-9, "body: {body}");
    }
}

#[test]
fn untrusted_feature_or_type_is_parse_error() {
    let bad_feature =
        r#"{"subscriptionBalance": {"feature": "FEATURE_LITE", "amountUsedRatio": 0.1}}"#;
    assert!(matches!(
        parse_subscription_stats(bad_feature),
        Err(WebError::Parse(_))
    ));
    let bad_type = r#"{"subscriptionBalance": {"type": "CREDIT_PACK", "amountUsedRatio": 0.1}}"#;
    assert!(matches!(
        parse_subscription_stats(bad_type),
        Err(WebError::Parse(_))
    ));
    // feature/type 缺失放行
    let missing = r#"{"subscriptionBalance": {"amountUsedRatio": 0.1}}"#;
    assert!(parse_subscription_stats(missing).is_ok());
}

#[test]
fn missing_fields_are_parse_errors() {
    assert!(matches!(
        parse_subscription_stats(r#"{"ratelimitCode5h": {"ratio": 0.32}}"#),
        Err(WebError::Parse(_))
    ));
    assert!(matches!(
        parse_subscription_stats(r#"{"subscriptionBalance": {"kimiCodeUsedRatio": 0.1}}"#),
        Err(WebError::Parse(_))
    ));
    // 负数视为缺失 → 缺 amountUsedRatio 报错
    assert!(matches!(
        parse_subscription_stats(r#"{"subscriptionBalance": {"amountUsedRatio": -0.5}}"#),
        Err(WebError::Parse(_))
    ));
}

#[test]
fn string_numbers_and_percent_form() {
    // 数字字符串
    let info = parse_subscription_stats(r#"{"subscriptionBalance": {"amountUsedRatio": "0.4"}}"#)
        .expect("字符串数字应解析");
    assert!((info.total_pct - 40.0).abs() < 1e-9);
    // >1 视为百分数原样
    let info = parse_subscription_stats(r#"{"subscriptionBalance": {"amountUsedRatio": 32.5}}"#)
        .expect("百分数原样");
    assert!((info.total_pct - 32.5).abs() < 1e-9);
}

#[test]
fn kimi_pct_clamps_to_zero() {
    let info = parse_subscription_stats(
        r#"{"subscriptionBalance": {"amountUsedRatio": 0.05, "kimiCodeUsedRatio": 0.1}}"#,
    )
    .expect("应解析成功");
    assert!((info.total_pct - 5.0).abs() < 1e-9);
    assert!((info.code_pct - 10.0).abs() < 1e-9);
    assert_eq!(info.kimi_pct, 0.0);
}

#[test]
fn reset_time_passthrough_and_serde_skip() {
    let info = parse_subscription_stats(r#"{"subscriptionBalance": {"amountUsedRatio": 0.1}}"#)
        .expect("应解析成功");
    assert!(info.reset_time.is_none());
    // reset_time 为 None 时不输出该键
    let json = serde_json::to_string(&info).unwrap();
    assert!(!json.contains("reset_time"), "json: {json}");

    let with_reset = MonthlyInfo {
        total_pct: 1.0,
        kimi_pct: 1.0,
        code_pct: 0.0,
        reset_time: Some("2026-08-01T00:00:00Z".into()),
    };
    let json = serde_json::to_string(&with_reset).unwrap();
    assert!(json.contains("\"reset_time\":\"2026-08-01T00:00:00Z\""));
}

// ---- token 规范化 ----

#[test]
fn normalize_plain_and_quoted() {
    assert_eq!(normalize_web_token("abc123").unwrap(), "abc123");
    assert_eq!(normalize_web_token("  abc123  ").unwrap(), "abc123");
    assert_eq!(normalize_web_token("  \"abc123\"  ").unwrap(), "abc123");
    assert_eq!(normalize_web_token("'abc123'").unwrap(), "abc123");
}

#[test]
fn normalize_strips_authorization_and_bearer() {
    assert_eq!(
        normalize_web_token("Authorization: Bearer abc").unwrap(),
        "abc"
    );
    assert_eq!(normalize_web_token("authorization: abc").unwrap(), "abc");
    // Bearer 可叠加
    assert_eq!(normalize_web_token("Bearer Bearer abc").unwrap(), "abc");
    // Bearer 后不跟空白不剥
    assert_eq!(normalize_web_token("Bearerabc").unwrap(), "Bearerabc");
}

#[test]
fn normalize_extracts_kimi_auth_cookie() {
    assert_eq!(
        normalize_web_token("kimi-auth=abc123; other=x").unwrap(),
        "abc123"
    );
    assert_eq!(normalize_web_token("KIMI-AUTH=\"abc\";").unwrap(), "abc");
    assert_eq!(normalize_web_token("foo=1; kimi-auth=zzz").unwrap(), "zzz");
}

#[test]
fn normalize_rejects_invalid_input() {
    for bad in [
        "",
        "   ",
        "foo=1; bar=2", // 含 ; 但找不到 kimi-auth
        "kimi-auth=;",  // kimi-auth 值为空
        "kimi-auth=  ", // 值仅空白
        "abc def",      // 内部空白
        "abc\ndef",     // 换行
        "\"\"",         // 引号内为空
    ] {
        assert!(normalize_web_token(bad).is_err(), "应拒绝: {bad:?}");
        assert_eq!(
            normalize_web_token(bad).unwrap_err(),
            "无法识别的 token 格式，请直接粘贴 kimi-auth 的值"
        );
    }
}

// ---- JWT 条件头 ----

#[test]
fn jwt_with_full_identity_yields_headers() {
    let token = make_jwt(r#"{"device_id":"d1","ssid":"s1","sub":"u1"}"#);
    let (device_id, ssid, sub) = jwt_identity_headers(&token).expect("应提取成功");
    assert_eq!(device_id, "d1");
    assert_eq!(ssid, "s1");
    assert_eq!(sub, "u1");
}

#[test]
fn jwt_missing_any_field_yields_none() {
    // 缺 ssid
    assert!(jwt_identity_headers(&make_jwt(r#"{"device_id":"d1","sub":"u1"}"#)).is_none());
    // sub 为空字符串
    assert!(
        jwt_identity_headers(&make_jwt(r#"{"device_id":"d1","ssid":"s1","sub":""}"#)).is_none()
    );
}

#[test]
fn non_jwt_or_malformed_tokens_yield_none() {
    assert!(jwt_identity_headers("plain-token").is_none());
    assert!(jwt_identity_headers("a.b").is_none());
    // 非法 base64 payload
    assert!(jwt_identity_headers("a.%%%.b").is_none());
    // payload 不是 JSON
    assert!(jwt_identity_headers(&make_jwt("not-json")).is_none());
    // 四段
    assert!(jwt_identity_headers("a.b.c.d").is_none());
}
