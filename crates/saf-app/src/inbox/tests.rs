use super::*;
use saf_core::ports::{
    AccountScheduleRequest, LogReader, MinecraftAction, Notification, ScheduledAccountAction,
};
use saf_core::{
    AccountId, BlacklistAction, BlacklistField, BlacklistRequest, BlacklistScope, BlacklistUpdate,
    BotState, RuntimeOutcome, SafConfig,
};

#[tokio::test]
async fn process_inbox_executes_session_backed_ports() {
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        ..SafConfig::default()
    };
    let session = RecordedRuntimeSession::from_config(&config, Vec::new());
    let results = process_inbox(
        session.runtime(),
        r#"
{"type":"terminal","line":"/cofl s minProfit 30m"}
{"type":"terminal","line":"Main chat hello"}
{"type":"terminal","line":"Main bank 50m withdraw"}
{"type":"terminal","line":"Main queue"}
{"type":"terminal","line":"start Main"}
{"type":"terminal","line":"test"}
bad-json
"#,
    )
    .await;

    assert!(matches!(
        results.first(),
        Some(InboxCommandResult::Processed { .. })
    ));
    assert!(matches!(
        results.get(1),
        Some(InboxCommandResult::Processed { .. })
    ));
    assert!(matches!(
        results.get(2),
        Some(InboxCommandResult::Processed { .. })
    ));
    assert!(matches!(
        results.get(3),
        Some(InboxCommandResult::Processed { .. })
    ));
    assert!(matches!(
        results.get(4),
        Some(InboxCommandResult::Processed { .. })
    ));
    assert!(matches!(
        results.get(5),
        Some(InboxCommandResult::Processed { .. })
    ));
    assert!(matches!(
        results.get(6),
        Some(InboxCommandResult::Invalid { .. })
    ));
    assert_eq!(
        session.cofl_commands(),
        vec![CoflCommandRecord {
            account: AccountId::new("Main").unwrap(),
            command: "/cofl s minProfit 30m".to_string(),
        }]
    );
    assert_eq!(
        session.minecraft_actions(),
        vec![MinecraftActionRecord {
            account: AccountId::new("Main").unwrap(),
            actions: vec![MinecraftAction::Chat("hello".to_string())],
        }]
    );
    assert_eq!(
        session.queue_entries(),
        vec![QueueRecord {
            account: AccountId::new("Main").unwrap(),
            action: serde_json::json!({
                "amount": 50_000_000.0,
                "withdraw": true,
                "personal": false
            }),
            state: BotState::Custom("bank".to_string()),
            priority: 5,
        }]
    );
    assert_eq!(
        session.lifecycle_actions(),
        vec![LifecycleRecord {
            action: LifecycleAction::Start,
            account: Some(AccountId::new("Main").unwrap()),
        }]
    );
    assert_eq!(
        session.notifications(),
        vec![Notification {
            title: "SAF test".to_string(),
            body: "Webhook notifier path is connected.".to_string(),
            account: Some(AccountId::new("Main").unwrap()),
        }]
    );
}

#[tokio::test]
async fn process_inbox_can_persist_queue_actions() {
    let temp = tempfile::tempdir().unwrap();
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        ..SafConfig::default()
    };
    let session =
        RecordedRuntimeSession::from_config_with_file_queue(&config, Vec::new(), temp.path());

    let report = session
        .process_inbox_report(r#"{"type":"terminal","line":"Main list_item item-uuid 25m 12h"}"#)
        .await;

    assert!(matches!(
        report.results.first(),
        Some(InboxCommandResult::Processed { .. })
    ));
    assert_eq!(report.queue_entries.len(), 1);
    let saved = crate::state_cli::snapshot(temp.path(), "Main").unwrap();
    assert_eq!(saved.queue.len(), 1);
    assert_eq!(saved.queue[0].state, BotState::ListingNoName);
}

#[tokio::test]
async fn process_inbox_can_clear_saved_data() {
    let temp = tempfile::tempdir().unwrap();
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        ..SafConfig::default()
    };
    let session =
        RecordedRuntimeSession::from_config_with_file_queue(&config, Vec::new(), temp.path());

    let report = session
        .process_inbox_report(
            r#"
{"type":"terminal","line":"Main list_item item-uuid 25m 12h"}
{"type":"terminal","line":"Main clear_data"}
"#,
        )
        .await;

    assert_eq!(report.saved_data_clears.len(), 1);
    assert_eq!(
        report.saved_data_clears[0],
        SavedDataClearRecord {
            account: AccountId::new("Main").unwrap(),
            queue_removed: 1,
            bid_data_cleared: false,
        }
    );
    assert!(session.queue_entries().is_empty());
    assert!(
        crate::state_cli::snapshot(temp.path(), "Main")
            .unwrap()
            .queue
            .is_empty()
    );
}

#[tokio::test]
async fn process_inbox_records_blacklist_requests() {
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        ..SafConfig::default()
    };
    let session = RecordedRuntimeSession::from_config(&config, Vec::new());

    let report = session
        .process_inbox_report(
            r#"{"type":"terminal","line":"Main blacklist add buy tag SPEED_RELIC --duration 7d"}"#,
        )
        .await;

    assert!(matches!(
        report.results.first(),
        Some(InboxCommandResult::Processed {
            outcome: RuntimeOutcome::BlacklistApplied { .. },
            ..
        })
    ));
    assert_eq!(
        report.blacklist_requests,
        vec![BlacklistRecord {
            account: AccountId::new("Main").unwrap(),
            request: BlacklistRequest::Update(BlacklistUpdate {
                action: BlacklistAction::Add,
                scope: BlacklistScope::Buy,
                field: BlacklistField::Tag,
                value: "SPEED_RELIC".to_string(),
                duration: Some("7d".to_string()),
                until: None,
            }),
            changed: true,
            summary: "Add Buy.Tag SPEED_RELIC".to_string(),
        }]
    );
}

#[tokio::test]
async fn process_inbox_records_stats_requests() {
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        ..SafConfig::default()
    };
    let session = RecordedRuntimeSession::from_config(&config, Vec::new());

    let report = session
        .process_inbox_report(
            r#"
{"type":"terminal","line":"Main stats"}
{"type":"terminal","line":"Main ping"}
"#,
        )
        .await;

    assert!(matches!(
        report.results.first(),
        Some(InboxCommandResult::Processed {
            outcome: RuntimeOutcome::StatsSnapshot { .. },
            ..
        })
    ));
    assert!(matches!(
        report.results.get(1),
        Some(InboxCommandResult::Processed {
            outcome: RuntimeOutcome::PingSnapshot { .. },
            ..
        })
    ));
    assert_eq!(
        report.stats_requests,
        vec![
            StatsRequestRecord {
                account: AccountId::new("Main").unwrap(),
                kind: StatsRequestKind::Stats,
            },
            StatsRequestRecord {
                account: AccountId::new("Main").unwrap(),
                kind: StatsRequestKind::Ping,
            },
        ]
    );
}

#[tokio::test]
async fn process_inbox_records_scheduled_account_actions() {
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        ..SafConfig::default()
    };
    let session = RecordedRuntimeSession::from_config(&config, Vec::new());

    let report = session
        .process_inbox_report(r#"{"type":"terminal","line":"timeout 30m Main"}"#)
        .await;

    assert!(matches!(
        report.results.first(),
        Some(InboxCommandResult::Processed {
            outcome: RuntimeOutcome::AccountScheduled { .. },
            ..
        })
    ));
    assert_eq!(
        report.scheduled_accounts,
        vec![AccountScheduleRecord {
            request: AccountScheduleRequest {
                account: AccountId::new("Main").unwrap(),
                action: ScheduledAccountAction::Stop,
                delay_ms: 1_800_000,
            },
        }]
    );
}

#[tokio::test]
async fn file_log_reader_returns_recent_lines() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("latest.log");
    tokio::fs::write(&path, "one\n\n two \nthree\n")
        .await
        .unwrap();
    let reader = FileLogReader::new(&path);

    let snapshot = reader.latest(2).await.unwrap();

    assert_eq!(snapshot.path, path.display().to_string());
    assert!(snapshot.exists);
    assert_eq!(snapshot.lines, vec!["two".to_string(), "three".to_string()]);
}
