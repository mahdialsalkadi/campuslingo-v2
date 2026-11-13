use campuslingo_domain::notifications::{
    InboundRaidNotificationEvent, MAX_PUSH_NOTIFICATIONS_PER_DAY, NotificationPreferencesInput,
    NotificationSlotKind, PreExpiryShieldDigestInput, evaluate_quiet_hours,
    plan_daily_notifications,
};

#[test]
fn quiet_hours_deferral_spans_22_00_to_08_00_local() {
    let before = evaluate_quiet_hours(21, 59, 59).unwrap();
    assert!(!before.in_quiet_hours);
    assert_eq!(before.defer_seconds, 0);

    let late_evening = evaluate_quiet_hours(22, 30, 0).unwrap();
    assert!(late_evening.in_quiet_hours);
    assert_eq!(late_evening.deferred_to_local_hour, 8);
    assert_eq!(late_evening.defer_seconds, 9 * 3600 + 30 * 60);

    let early_morning = evaluate_quiet_hours(7, 15, 0).unwrap();
    assert!(early_morning.in_quiet_hours);
    assert_eq!(early_morning.defer_seconds, 45 * 60);

    let active_morning = evaluate_quiet_hours(8, 0, 0).unwrap();
    assert!(!active_morning.in_quiet_hours);
    assert_eq!(active_morning.defer_seconds, 0);
}

#[test]
fn aggregates_raids_and_shields_within_daily_cap_2_and_deduplicates() {
    let prefs = NotificationPreferencesInput::default();
    let raids = vec![
        InboundRaidNotificationEvent {
            raid_id: "raid-1".into(),
            occurred_local_hour: 23,
            occurred_local_minute: 10,
            occurred_local_second: 0,
        },
        InboundRaidNotificationEvent {
            raid_id: "raid-2".into(),
            occurred_local_hour: 1,
            occurred_local_minute: 20,
            occurred_local_second: 0,
        },
        // Duplicate raid ID should be deduplicated.
        InboundRaidNotificationEvent {
            raid_id: "raid-1".into(),
            occurred_local_hour: 2,
            occurred_local_minute: 0,
            occurred_local_second: 0,
        },
        InboundRaidNotificationEvent {
            raid_id: "raid-3".into(),
            occurred_local_hour: 10,
            occurred_local_minute: 5,
            occurred_local_second: 0,
        },
    ];
    let digest = PreExpiryShieldDigestInput {
        expiring_node_ids: vec!["node-a".into(), "node-b".into(), "node-a".into()],
        occurred_local_hour: 6,
        occurred_local_minute: 30,
        occurred_local_second: 0,
    };

    let plan = plan_daily_notifications(&prefs, &raids, Some(&digest)).unwrap();
    assert_eq!(plan.max_push_per_day, MAX_PUSH_NOTIFICATIONS_PER_DAY);
    assert_eq!(plan.total_queued, 2);
    assert!(!plan.suppressed_due_to_opt_out);
    assert_eq!(plan.items.len(), 2);

    let raid_item = &plan.items[0];
    assert_eq!(raid_item.kind, NotificationSlotKind::RaidAlert);
    assert_eq!(raid_item.event_count, 3);
    assert_eq!(raid_item.summary_text, "Raided 3 times today");
    assert_eq!(raid_item.raid_ids, vec!["raid-1", "raid-2", "raid-3"]);
    assert!(raid_item.quiet_hours_deferred);
    assert_eq!(raid_item.defer_seconds, 8 * 3600 + 50 * 60);

    let shield_item = &plan.items[1];
    assert_eq!(shield_item.kind, NotificationSlotKind::ShieldDigest);
    assert_eq!(shield_item.event_count, 2);
    assert_eq!(shield_item.summary_text, "2 shields nearing expiry");
    assert_eq!(shield_item.shield_node_ids, vec!["node-a", "node-b"]);
    assert!(shield_item.quiet_hours_deferred);
    assert_eq!(shield_item.defer_seconds, 90 * 60);
}

#[test]
fn respects_notification_opt_outs() {
    let mut prefs = NotificationPreferencesInput {
        push_enabled: false,
        raid_alerts_enabled: true,
        shield_digest_enabled: true,
        ..Default::default()
    };
    let raids = vec![InboundRaidNotificationEvent {
        raid_id: "raid-9".into(),
        occurred_local_hour: 12,
        occurred_local_minute: 0,
        occurred_local_second: 0,
    }];
    let all_off = plan_daily_notifications(&prefs, &raids, None).unwrap();
    assert_eq!(all_off.total_queued, 0);
    assert!(all_off.suppressed_due_to_opt_out);

    prefs.push_enabled = true;
    prefs.raid_alerts_enabled = false;
    let digest = PreExpiryShieldDigestInput {
        expiring_node_ids: vec!["node-z".into()],
        occurred_local_hour: 14,
        occurred_local_minute: 0,
        occurred_local_second: 0,
    };
    let raids_off = plan_daily_notifications(&prefs, &raids, Some(&digest)).unwrap();
    assert!(raids_off.suppressed_due_to_opt_out);
    assert_eq!(raids_off.total_queued, 1);
    assert_eq!(raids_off.items[0].kind, NotificationSlotKind::ShieldDigest);
    assert_eq!(raids_off.items[0].summary_text, "1 shield nearing expiry");
}
