//! OpenCode Go Workspace Dashboard 解析与用户输入规范化测试。

use chrono::{TimeZone, Utc};
use kimicodebar_lib::opencode_go::{
    normalize_auth_cookie, normalize_workspace_id, parse_dashboard_at, parse_ecb_exchange_rate,
};

#[test]
fn dashboard_parses_all_windows_with_field_order_variants() {
    let html = r#"
        <script>const labels = "rollingUsage weeklyUsage monthlyUsage";</script>
        <script>
            rollingUsage:$R[31]={status:"ok",usagePercent:25.5,resetInSec:3600},
            weeklyUsage:$R[32]={status:"ok",resetInSec:172800,usagePercent:70},
            monthlyUsage:$R[33]={usagePercent:95,resetInSec:864000,status:"ok"}
        </script>
    "#;
    let now = Utc.with_ymd_and_hms(2026, 8, 1, 0, 0, 0).unwrap();
    let usage = parse_dashboard_at(html, now).unwrap();

    let five_hour = usage.five_hour.unwrap();
    assert_eq!(five_hour.limit_usd, 12.0);
    assert!((five_hour.used_usd - 3.06).abs() < 1e-9);
    assert!((five_hour.remaining_percent - 0.745).abs() < 1e-9);
    assert_eq!(five_hour.reset_time, "2026-08-01T01:00:00+00:00");

    let weekly = usage.weekly.unwrap();
    assert_eq!(weekly.limit_usd, 30.0);
    assert_eq!(weekly.used_usd, 21.0);
    assert_eq!(weekly.reset_time, "2026-08-03T00:00:00+00:00");

    let monthly = usage.monthly.unwrap();
    assert_eq!(monthly.limit_usd, 60.0);
    assert_eq!(monthly.used_usd, 57.0);
    assert!(usage.low_warning);
}

#[test]
fn dashboard_allows_missing_window_but_rejects_unrelated_html() {
    let now = Utc.with_ymd_and_hms(2026, 8, 1, 0, 0, 0).unwrap();
    let usage =
        parse_dashboard_at(r#"weeklyUsage:$R[1]={usagePercent:12,resetInSec:60}"#, now).unwrap();
    assert!(usage.five_hour.is_none());
    assert!(usage.weekly.is_some());
    assert!(usage.monthly.is_none());
    assert!(parse_dashboard_at("<html>login</html>", now).is_err());
}

#[test]
fn workspace_and_cookie_inputs_are_normalized_without_leaking_other_cookies() {
    assert_eq!(normalize_workspace_id("wrk_ABC123").unwrap(), "wrk_ABC123");
    assert_eq!(
        normalize_workspace_id("https://opencode.ai/workspace/wrk_ABC123/go").unwrap(),
        "wrk_ABC123"
    );
    assert!(normalize_workspace_id("../billing").is_err());

    assert_eq!(
        normalize_auth_cookie("Fe26.2**value").unwrap(),
        "Fe26.2**value"
    );
    assert_eq!(
        normalize_auth_cookie("Cookie: theme=dark; auth=Fe26.2**value; other=1").unwrap(),
        "Fe26.2**value"
    );
    assert!(normalize_auth_cookie("auth=").is_err());
}

#[test]
fn ecb_rates_are_cross_calculated_for_usd_cny() {
    let xml = r#"
        <Cube>
            <Cube time='2026-07-27'>
                <Cube currency='USD' rate='1.1389'/>
                <Cube rate="7.7059" currency="CNY"/>
            </Cube>
        </Cube>
    "#;
    let rate = parse_ecb_exchange_rate(xml).unwrap();
    assert_eq!(rate.reference_date, "2026-07-27");
    assert!((rate.usd_cny - 7.7059 / 1.1389).abs() < 1e-9);

    assert!(parse_ecb_exchange_rate("<Cube time='2026-07-27'/>").is_err());
    assert!(parse_ecb_exchange_rate(
        "<Cube time='2026-07-27'><Cube currency='USD' rate='0'/><Cube currency='CNY' rate='7'/></Cube>"
    )
    .is_err());
}
