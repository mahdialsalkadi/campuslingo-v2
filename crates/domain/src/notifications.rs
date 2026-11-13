//! M7 Notification Aggregation, Quiet-Hours Deferral (`S-18`, `F-05`), and Quota Rules.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const MAX_PUSH_NOTIFICATIONS_PER_DAY: u8 = 2;
pub const QUIET_HOURS_START_LOCAL_HOUR: u8 = 22;
pub const QUIET_HOURS_END_LOCAL_HOUR: u8 = 8;
pub const QUIET_HOURS_START_MINUTE: u16 = 22 * 60;
pub const QUIET_HOURS_END_MINUTE: u16 = 8 * 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NotificationSlotKind {
    RaidAlert,
    ShieldDigest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NotificationPreferencesInput {
    pub push_enabled: bool,
    pub raid_alerts_enabled: bool,
    pub shield_digest_enabled: bool,
    #[serde(default = "default_quiet_start_minute")]
    pub quiet_hours_start_minute: u16,
    #[serde(default = "default_quiet_end_minute")]
    pub quiet_hours_end_minute: u16,
}

fn default_quiet_start_minute() -> u16 {
    QUIET_HOURS_START_MINUTE
}

fn default_quiet_end_minute() -> u16 {
    QUIET_HOURS_END_MINUTE
}

impl Default for NotificationPreferencesInput {
    fn default() -> Self {
        Self {
            push_enabled: true,
            raid_alerts_enabled: true,
            shield_digest_enabled: true,
            quiet_hours_start_minute: QUIET_HOURS_START_MINUTE,
            quiet_hours_end_minute: QUIET_HOURS_END_MINUTE,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuietHoursWindowEvaluation {
    pub in_quiet_hours: bool,
    pub deferred_to_local_hour: u8,
    pub defer_seconds: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InboundRaidNotificationEvent {
    pub raid_id: String,
    pub occurred_local_hour: u8,
    pub occurred_local_minute: u8,
    pub occurred_local_second: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreExpiryShieldDigestInput {
    pub expiring_node_ids: Vec<String>,
    pub occurred_local_hour: u8,
    pub occurred_local_minute: u8,
    pub occurred_local_second: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QueuedNotificationItem {
    pub kind: NotificationSlotKind,
    pub event_count: u32,
    pub summary_text: String,
    pub raid_ids: Vec<String>,
    pub shield_node_ids: Vec<String>,
    pub quiet_hours_deferred: bool,
    pub defer_seconds: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AggregatedDailyNotificationPlan {
    pub max_push_per_day: u8,
    pub total_queued: u8,
    pub suppressed_due_to_opt_out: bool,
    pub items: Vec<QueuedNotificationItem>,
}

/// Evaluate whether a user-local wall-clock time falls inside `22:00–08:00` quiet hours
/// and return the exact deferral duration in seconds until `08:00:00` local time.
pub fn evaluate_quiet_hours(
    local_hour: u8,
    local_minute: u8,
    local_second: u8,
) -> Result<QuietHoursWindowEvaluation, String> {
    if local_hour > 23 || local_minute > 59 || local_second > 59 {
        return Err("invalid local wall-clock time for quiet hours evaluation".into());
    }
    let secs_since_midnight =
        u32::from(local_hour) * 3600 + u32::from(local_minute) * 60 + u32::from(local_second);
    let quiet_start_secs = u32::from(QUIET_HOURS_START_LOCAL_HOUR) * 3600;
    let quiet_end_secs = u32::from(QUIET_HOURS_END_LOCAL_HOUR) * 3600;
    let day_secs = 24 * 3600;

    if local_hour >= QUIET_HOURS_START_LOCAL_HOUR {
        Ok(QuietHoursWindowEvaluation {
            in_quiet_hours: true,
            deferred_to_local_hour: QUIET_HOURS_END_LOCAL_HOUR,
            defer_seconds: (day_secs - secs_since_midnight) + quiet_end_secs,
        })
    } else if local_hour < QUIET_HOURS_END_LOCAL_HOUR {
        Ok(QuietHoursWindowEvaluation {
            in_quiet_hours: true,
            deferred_to_local_hour: QUIET_HOURS_END_LOCAL_HOUR,
            defer_seconds: quiet_end_secs - secs_since_midnight,
        })
    } else {
        let _ = quiet_start_secs;
        Ok(QuietHoursWindowEvaluation {
            in_quiet_hours: false,
            deferred_to_local_hour: QUIET_HOURS_END_LOCAL_HOUR,
            defer_seconds: 0,
        })
    }
}

/// Aggregate inbound raids and pre-expiry shields into at most `MAX_PUSH_NOTIFICATIONS_PER_DAY = 2`
/// notifications for a user-local day, enforcing opt-outs and `22:00–08:00` quiet-hours deferral.
pub fn plan_daily_notifications(
    prefs: &NotificationPreferencesInput,
    raids: &[InboundRaidNotificationEvent],
    shield_digest: Option<&PreExpiryShieldDigestInput>,
) -> Result<AggregatedDailyNotificationPlan, String> {
    if prefs.quiet_hours_start_minute != QUIET_HOURS_START_MINUTE
        || prefs.quiet_hours_end_minute != QUIET_HOURS_END_MINUTE
    {
        return Err("quiet hours must remain anchored to 22:00–08:00 user-local".into());
    }

    if !prefs.push_enabled {
        return Ok(AggregatedDailyNotificationPlan {
            max_push_per_day: MAX_PUSH_NOTIFICATIONS_PER_DAY,
            total_queued: 0,
            suppressed_due_to_opt_out: !raids.is_empty()
                || shield_digest.is_some_and(|s| !s.expiring_node_ids.is_empty()),
            items: Vec::new(),
        });
    }

    let mut items = Vec::new();
    let mut suppressed = false;

    if !raids.is_empty() {
        if prefs.raid_alerts_enabled {
            let mut seen_ids = BTreeSet::new();
            let mut ordered_raid_ids = Vec::new();
            let mut deferred = false;
            let mut max_defer_seconds = 0_u32;

            for event in raids {
                let id = event.raid_id.trim();
                if id.is_empty() {
                    return Err("inbound raid notification raid_id must not be empty".into());
                }
                let qh = evaluate_quiet_hours(
                    event.occurred_local_hour,
                    event.occurred_local_minute,
                    event.occurred_local_second,
                )?;
                if qh.in_quiet_hours {
                    deferred = true;
                    max_defer_seconds = max_defer_seconds.max(qh.defer_seconds);
                }
                if seen_ids.insert(id.to_owned()) {
                    ordered_raid_ids.push(id.to_owned());
                }
            }

            let count = ordered_raid_ids.len() as u32;
            let summary_text = if count == 1 {
                "Raided 1 time today".to_owned()
            } else {
                format!("Raided {count} times today")
            };
            items.push(QueuedNotificationItem {
                kind: NotificationSlotKind::RaidAlert,
                event_count: count,
                summary_text,
                raid_ids: ordered_raid_ids,
                shield_node_ids: Vec::new(),
                quiet_hours_deferred: deferred,
                defer_seconds: max_defer_seconds,
            });
        } else {
            suppressed = true;
        }
    }

    if let Some(digest) = shield_digest {
        let mut seen_nodes = BTreeSet::new();
        let mut ordered_nodes = Vec::new();
        for node in &digest.expiring_node_ids {
            let clean = node.trim();
            if clean.is_empty() {
                return Err("expiring node_id must not be empty".into());
            }
            if seen_nodes.insert(clean.to_owned()) {
                ordered_nodes.push(clean.to_owned());
            }
        }
        if !ordered_nodes.is_empty() {
            if prefs.shield_digest_enabled {
                let qh = evaluate_quiet_hours(
                    digest.occurred_local_hour,
                    digest.occurred_local_minute,
                    digest.occurred_local_second,
                )?;
                let count = ordered_nodes.len() as u32;
                let summary_text = if count == 1 {
                    "1 shield nearing expiry".to_owned()
                } else {
                    format!("{count} shields nearing expiry")
                };
                items.push(QueuedNotificationItem {
                    kind: NotificationSlotKind::ShieldDigest,
                    event_count: count,
                    summary_text,
                    raid_ids: Vec::new(),
                    shield_node_ids: ordered_nodes,
                    quiet_hours_deferred: qh.in_quiet_hours,
                    defer_seconds: qh.defer_seconds,
                });
            } else {
                suppressed = true;
            }
        }
    }

    debug_assert!(items.len() <= usize::from(MAX_PUSH_NOTIFICATIONS_PER_DAY));
    Ok(AggregatedDailyNotificationPlan {
        max_push_per_day: MAX_PUSH_NOTIFICATIONS_PER_DAY,
        total_queued: items.len() as u8,
        suppressed_due_to_opt_out: suppressed,
        items,
    })
}
