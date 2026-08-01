//! 设置兼容性测试：新增字段不能让已有 settings.json 静默关闭新功能。

use kimicodebar_lib::storage::{
    self, AppSettings, KimiSubscriptionRow, OpenCodeGoRow, PanelCard, WidgetCard,
};
use std::fs;

#[test]
fn existing_settings_default_model_trend_card_to_visible() {
    let dir = std::env::temp_dir().join(format!(
        "kcb-test-settings-model-trend-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("settings.json"), r#"{"theme":"dark"}"#).unwrap();

    let settings = storage::load_settings(&dir);
    assert_eq!(settings.theme, "dark");
    assert!(settings.show_model_trend_card);
    assert!(settings.show_opencode_go_card);
    assert!(settings.show_opencode_go_five_hour_card);
    assert!(settings.show_opencode_go_weekly_card);
    assert!(settings.show_opencode_go_monthly_card);
    assert_eq!(settings.panel_cards.len(), 4);
    assert_eq!(
        settings.widget_cards,
        vec![WidgetCard::KimiSubscription, WidgetCard::OpenCodeGo]
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn legacy_widget_cards_collapse_and_add_opencode_go_once() {
    let settings = AppSettings {
        widget_cards: vec![
            WidgetCard::Monthly,
            WidgetCard::Weekly,
            WidgetCard::FiveHour,
            WidgetCard::Monthly,
        ],
        ..AppSettings::default()
    }
    .normalized();

    assert_eq!(
        settings.widget_cards,
        vec![WidgetCard::KimiSubscription, WidgetCard::OpenCodeGo]
    );

    let new_settings = AppSettings {
        widget_cards: vec![WidgetCard::KimiSubscription],
        ..AppSettings::default()
    }
    .normalized();
    assert_eq!(
        new_settings.widget_cards,
        vec![WidgetCard::KimiSubscription]
    );
}

#[test]
fn legacy_disabled_opencode_go_card_disables_new_quota_rows() {
    let dir = std::env::temp_dir().join(format!(
        "kcb-test-settings-opencode-go-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("settings.json"),
        r#"{"show_opencode_go_card":false}"#,
    )
    .unwrap();

    let settings = storage::load_settings(&dir);
    assert!(!settings.show_opencode_go_card);
    assert!(!settings.show_opencode_go_five_hour_card);
    assert!(!settings.show_opencode_go_weekly_card);
    assert!(!settings.show_opencode_go_monthly_card);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn subscription_row_orders_are_deduplicated_and_completed() {
    let settings = AppSettings {
        kimi_subscription_rows: vec![
            KimiSubscriptionRow::Booster,
            KimiSubscriptionRow::Weekly,
            KimiSubscriptionRow::Booster,
        ],
        opencode_go_rows: vec![OpenCodeGoRow::Monthly],
        ..AppSettings::default()
    }
    .normalized();

    assert_eq!(
        settings.kimi_subscription_rows,
        vec![
            KimiSubscriptionRow::Booster,
            KimiSubscriptionRow::Weekly,
            KimiSubscriptionRow::FiveHour,
            KimiSubscriptionRow::Monthly,
        ]
    );
    assert_eq!(
        settings.opencode_go_rows,
        vec![
            OpenCodeGoRow::Monthly,
            OpenCodeGoRow::FiveHour,
            OpenCodeGoRow::Weekly,
        ]
    );
}

#[test]
fn panel_card_order_is_deduplicated_and_new_cards_are_appended() {
    let settings = AppSettings {
        panel_cards: vec![
            PanelCard::ModelTrend,
            PanelCard::Weekly,
            PanelCard::ModelTrend,
        ],
        ..AppSettings::default()
    }
    .normalized();

    assert_eq!(
        settings.panel_cards,
        vec![
            PanelCard::ModelTrend,
            PanelCard::KimiSubscription,
            PanelCard::OpenCodeGo,
            PanelCard::LocalUsage,
        ]
    );

    assert_eq!(
        serde_json::to_value(PanelCard::OpenCodeGo).unwrap(),
        "open_code_go"
    );
    assert_eq!(
        serde_json::to_value(PanelCard::KimiSubscription).unwrap(),
        "kimi_subscription"
    );
}

#[test]
fn legacy_kimi_cards_collapse_at_the_first_legacy_position() {
    let settings = AppSettings {
        panel_cards: vec![
            PanelCard::LocalUsage,
            PanelCard::Monthly,
            PanelCard::OpenCodeGo,
            PanelCard::Weekly,
            PanelCard::Booster,
            PanelCard::FiveHour,
        ],
        ..AppSettings::default()
    }
    .normalized();

    assert_eq!(
        settings.panel_cards,
        vec![
            PanelCard::LocalUsage,
            PanelCard::KimiSubscription,
            PanelCard::OpenCodeGo,
            PanelCard::ModelTrend,
        ]
    );
}
