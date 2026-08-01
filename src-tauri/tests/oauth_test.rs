//! OAuth 错误分类与临期判断测试（纯函数，不发网络请求）。

use kimicodebar_lib::kimi::oauth::{
    classify_poll_error, classify_refresh_error, is_expiring_soon, OAuthError, PollErrorClass,
};

#[test]
fn poll_error_classification() {
    assert!(matches!(
        classify_poll_error("authorization_pending"),
        PollErrorClass::Pending
    ));
    assert!(matches!(
        classify_poll_error("slow_down"),
        PollErrorClass::SlowDown
    ));
    assert!(matches!(
        classify_poll_error("expired_token"),
        PollErrorClass::Fatal(OAuthError::Expired)
    ));
    assert!(matches!(
        classify_poll_error("access_denied"),
        PollErrorClass::Fatal(OAuthError::Denied)
    ));
    match classify_poll_error("server_error") {
        PollErrorClass::Fatal(OAuthError::Api(msg)) => assert!(msg.contains("server_error")),
        _ => panic!("未知错误码应归类为 Api"),
    }
}

#[test]
fn refresh_error_classification() {
    // 401 / 403 → 需重新登录
    assert!(matches!(
        classify_refresh_error(401, None),
        OAuthError::NotAuthorized
    ));
    assert!(matches!(
        classify_refresh_error(403, Some("whatever")),
        OAuthError::NotAuthorized
    ));
    // invalid_grant → 需重新登录
    assert!(matches!(
        classify_refresh_error(400, Some("invalid_grant")),
        OAuthError::NotAuthorized
    ));
    // 其他 → API 错误
    assert!(matches!(
        classify_refresh_error(400, Some("bad_request")),
        OAuthError::Api(_)
    ));
    assert!(matches!(
        classify_refresh_error(500, None),
        OAuthError::Api(_)
    ));
}

#[test]
fn expiring_soon_threshold() {
    let now = 1_000_000i64;
    // 剩余 299s < 300s → 临期
    assert!(is_expiring_soon(Some(now + 299), now));
    // 已过期 → 临期
    assert!(is_expiring_soon(Some(now - 1), now));
    // 剩余 301s → 不临期
    assert!(!is_expiring_soon(Some(now + 301), now));
    // 无过期时间（长期 token）→ 不临期
    assert!(!is_expiring_soon(None, now));
}
