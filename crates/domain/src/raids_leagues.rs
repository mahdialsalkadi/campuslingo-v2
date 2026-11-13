use std::cmp::Ordering;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::shields::{NodeState, ProtectionKind, split_protection_predicates};

pub const MAX_OUTBOUND_RAIDS_PER_DAY: i32 = 10;
pub const MAX_INBOUND_RAIDS_PER_DAY: i32 = 3;
pub const MAX_UNORDERED_PAIR_RAIDS_DAY: i32 = 1;
pub const MAX_DAILY_TROPHY_LOSS_FROM_RAIDS: i32 = 45;
pub const REVENGE_WINDOW_HOURS: i64 = 48;
pub const RAID_SESSION_CEILING_MS: u32 = 120_000;
pub const BASE_RAID_TROPHY_STAKE: i32 = 15;
pub const REVENGE_RAID_TROPHY_STAKE: i32 = 22;
pub const DAILY_PRACTICE_XP_CAP: i32 = 100;
pub const MAX_PUSH_NOTIFICATIONS_PER_DAY: i32 = 2;

pub const LEAGUE_MIN_PROMOTION_TROPHIES: i32 = 150;
pub const LEAGUE_MAX_BRACKET_SIZE: usize = 30;
pub const LEAGUE_MIN_TIER: u8 = 1;
pub const LEAGUE_MAX_TIER: u8 = 10;
pub const DORMANT_RETURN_MIN_SESSIONS: u32 = 3;

pub const CLAN_RAID_WIN_POINTS: i64 = 25;
pub const CLAN_REVENGE_WIN_POINTS: i64 = 38;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RaidTargetabilityInput {
    pub attacker_user_id: String,
    pub defender_user_id: String,
    pub course_is_private: bool,
    pub attacker_raid_opt_in: bool,
    pub defender_raid_opt_in: bool,
    pub node_state: NodeState,
    pub node_tier: Option<u8>,
    pub active_protections: Vec<ProtectionKind>,
    pub attacker_outbound_raids_today: i32,
    pub defender_inbound_raids_today: i32,
    pub unordered_pair_raids_today: i32,
    pub is_revenge: bool,
    pub revenge_source_available: bool,
    pub revenge_age_seconds: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RaidTargetabilityOutcome {
    pub targetable: bool,
    pub blocker_reason: Option<String>,
    pub raid_immune: bool,
    pub mode_bps: i64,
}

/// Evaluates F-01 / F-02 raid eligibility, daily user-local caps, mutual-revenge anti-farming,
/// opt-in requirements for private courses, and node `raid_immune(u, t)` protections.
pub fn evaluate_raid_targetability(input: &RaidTargetabilityInput) -> RaidTargetabilityOutcome {
    let mode_bps = if input.is_revenge { 15_000 } else { 10_000 };
    let protection =
        split_protection_predicates(input.node_state, input.node_tier, &input.active_protections);

    let deny = |reason: &str, immune: bool| RaidTargetabilityOutcome {
        targetable: false,
        blocker_reason: Some(reason.to_string()),
        raid_immune: immune,
        mode_bps,
    };

    if input.attacker_user_id == input.defender_user_id {
        return deny("SELF_RAID_FORBIDDEN", protection.raid_immune);
    }
    if input.course_is_private && (!input.attacker_raid_opt_in || !input.defender_raid_opt_in) {
        return deny("PRIVATE_COURSE_OPT_IN_REQUIRED", protection.raid_immune);
    }
    if input.attacker_outbound_raids_today >= MAX_OUTBOUND_RAIDS_PER_DAY {
        return deny("OUTBOUND_DAILY_CAP_REACHED", protection.raid_immune);
    }
    // Revenge raids share the unordered-pair daily cap to block mutual 1.5x revenge farming.
    if input.unordered_pair_raids_today >= MAX_UNORDERED_PAIR_RAIDS_DAY {
        return deny("UNORDERED_PAIR_DAILY_CAP_REACHED", protection.raid_immune);
    }
    if input.is_revenge {
        let valid_window = input.revenge_source_available
            && input
                .revenge_age_seconds
                .is_some_and(|secs| (0..=REVENGE_WINDOW_HOURS * 3_600).contains(&secs));
        if !valid_window {
            return deny("REVENGE_WINDOW_INVALID", protection.raid_immune);
        }
    }
    if input.defender_inbound_raids_today >= MAX_INBOUND_RAIDS_PER_DAY {
        return deny("INBOUND_DAILY_CAP_REACHED", protection.raid_immune);
    }
    if protection.raid_immune {
        return deny("DEFENDER_NODE_RAID_IMMUNE", true);
    }

    RaidTargetabilityOutcome {
        targetable: true,
        blocker_reason: None,
        raid_immune: false,
        mode_bps,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RaidSettlementInput {
    pub is_revenge: bool,
    pub sandbox_solved: bool,
    pub drills_passed: u8,
    pub elapsed_from_stage2_ms: u32,
    pub defender_immune_at_settle: bool,
    pub defender_daily_trophy_loss_so_far: i32,
    pub attacker_practice_xp_today: i32,
    pub reward_eligible: bool,
    pub base_score_v2_points: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RaidSettlementOutcome {
    pub status: String,
    pub raid_succeeded: bool,
    pub concurrent_cooldown_blocked: bool,
    pub fsrs_transitions_commit: bool,
    pub attacker_trophy_gain: i32,
    pub defender_trophy_loss: i32,
    pub practice_xp_awarded: i32,
    pub gold_awarded: i32,
    pub league_trophies_awarded: i32,
    pub clan_points_awarded: i64,
    pub consumes_attacker_outbound_cap: bool,
    pub consumes_defender_inbound_cap: bool,
    pub grants_defender_post_raid_cooldown: bool,
    pub creates_revenge_window: bool,
    pub loot_chest_tier: Option<String>,
}

/// Evaluates §11.2 / D-03 snapshot raid settlement, including the concurrent cooldown rule
/// when another attacker settled on the same vulnerable node first.
pub fn evaluate_raid_settlement(
    input: &RaidSettlementInput,
) -> Result<RaidSettlementOutcome, String> {
    if input.drills_passed > 3 {
        return Err("drills_passed must be in 0..=3".into());
    }
    if input.defender_daily_trophy_loss_so_far < 0 || input.attacker_practice_xp_today < 0 {
        return Err("daily accumulators must be non-negative".into());
    }

    if input.elapsed_from_stage2_ms > RAID_SESSION_CEILING_MS {
        return Ok(RaidSettlementOutcome {
            status: "EXPIRED".into(),
            raid_succeeded: false,
            concurrent_cooldown_blocked: false,
            fsrs_transitions_commit: false,
            attacker_trophy_gain: 0,
            defender_trophy_loss: 0,
            practice_xp_awarded: 0,
            gold_awarded: 0,
            league_trophies_awarded: 0,
            clan_points_awarded: 0,
            consumes_attacker_outbound_cap: true,
            consumes_defender_inbound_cap: false,
            grants_defender_post_raid_cooldown: false,
            creates_revenge_window: false,
            loot_chest_tier: None,
        });
    }

    let remaining_practice_xp = (DAILY_PRACTICE_XP_CAP - input.attacker_practice_xp_today).max(0);
    if input.defender_immune_at_settle {
        return Ok(RaidSettlementOutcome {
            status: "SETTLED_COOLDOWN_BLOCKED".into(),
            raid_succeeded: false,
            concurrent_cooldown_blocked: true,
            fsrs_transitions_commit: true,
            attacker_trophy_gain: 0,
            defender_trophy_loss: 0,
            practice_xp_awarded: 15.min(remaining_practice_xp),
            gold_awarded: 0,
            league_trophies_awarded: 0,
            clan_points_awarded: 0,
            consumes_attacker_outbound_cap: true,
            consumes_defender_inbound_cap: false,
            grants_defender_post_raid_cooldown: false,
            creates_revenge_window: false,
            loot_chest_tier: None,
        });
    }

    let raid_succeeded = input.sandbox_solved && input.drills_passed >= 2;
    if raid_succeeded {
        let stake = if input.is_revenge {
            REVENGE_RAID_TROPHY_STAKE
        } else {
            BASE_RAID_TROPHY_STAKE
        };
        let remaining_loss_cap =
            (MAX_DAILY_TROPHY_LOSS_FROM_RAIDS - input.defender_daily_trophy_loss_so_far).max(0);
        let defender_trophy_loss = stake.min(remaining_loss_cap);
        let attacker_trophy_gain = if input.reward_eligible { stake } else { 0 };
        let gold_awarded = if input.reward_eligible {
            i32::try_from(input.base_score_v2_points.clamp(10, 5_000) * 2).unwrap_or(10_000)
        } else {
            0
        };
        let clan_points_awarded = if input.reward_eligible {
            if input.is_revenge {
                CLAN_REVENGE_WIN_POINTS
            } else {
                CLAN_RAID_WIN_POINTS
            }
        } else {
            0
        };
        let loot_chest_tier = if input.reward_eligible {
            Some(if input.drills_passed == 3 {
                "APEX_RAID_CHEST".into()
            } else {
                "STANDARD_RAID_CHEST".into()
            })
        } else {
            None
        };

        return Ok(RaidSettlementOutcome {
            status: "SETTLED_WIN".into(),
            raid_succeeded: true,
            concurrent_cooldown_blocked: false,
            fsrs_transitions_commit: true,
            attacker_trophy_gain,
            defender_trophy_loss,
            practice_xp_awarded: 0,
            gold_awarded,
            league_trophies_awarded: attacker_trophy_gain,
            clan_points_awarded,
            consumes_attacker_outbound_cap: true,
            consumes_defender_inbound_cap: true,
            grants_defender_post_raid_cooldown: true,
            creates_revenge_window: !input.is_revenge,
            loot_chest_tier,
        });
    }

    Ok(RaidSettlementOutcome {
        status: "SETTLED_LOSS".into(),
        raid_succeeded: false,
        concurrent_cooldown_blocked: false,
        fsrs_transitions_commit: true,
        attacker_trophy_gain: 0,
        defender_trophy_loss: 0,
        practice_xp_awarded: 5.min(remaining_practice_xp),
        gold_awarded: 0,
        league_trophies_awarded: 0,
        clan_points_awarded: 0,
        consumes_attacker_outbound_cap: true,
        consumes_defender_inbound_cap: true,
        grants_defender_post_raid_cooldown: false,
        creates_revenge_window: false,
        loot_chest_tier: None,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ClanStabilityInput {
    pub s_before: f64,
    pub s_after: f64,
    pub active_members_snapshot: u32,
    pub reward_eligible: bool,
    pub member_in_frozen_roster: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ClanStabilityOutput {
    pub raw_points: i64,
    pub normalized_points: i64,
    pub relative_gain_bps: i64,
}

/// Server-only `f64` clan stability scoring (§11.4), normalized against the frozen
/// week-start `active_members_snapshot` (`sqrt(max(3, active_members_snapshot))`).
pub fn compute_clan_stability_points(
    input: &ClanStabilityInput,
) -> Result<ClanStabilityOutput, String> {
    if !input.s_before.is_finite()
        || !input.s_after.is_finite()
        || input.s_before < 0.0
        || input.s_after < 0.0
    {
        return Err("stability inputs must be finite and non-negative".into());
    }
    if input.active_members_snapshot == 0 {
        return Err("active_members_snapshot must be positive".into());
    }
    if !input.reward_eligible || !input.member_in_frozen_roster {
        return Ok(ClanStabilityOutput {
            raw_points: 0,
            normalized_points: 0,
            relative_gain_bps: 0,
        });
    }

    let delta_s = (input.s_after - input.s_before).max(0.0);
    let relative = delta_s / input.s_before.max(1.0);
    let raw_pts_f = (10.0 * (1.0 + relative).ln()).round_ties_even();
    let raw_points = (raw_pts_f as i64).clamp(0, 50);
    let denom = (input.active_members_snapshot.max(3) as f64).sqrt();
    let norm_pts_f = ((raw_points as f64) / denom).round_ties_even();
    let normalized_points = (norm_pts_f as i64).clamp(0, 50);
    let relative_gain_bps = ((relative * 10_000.0).round_ties_even() as i64).clamp(0, 1_000_000);

    Ok(ClanStabilityOutput {
        raw_points,
        normalized_points,
        relative_gain_bps,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LeagueParticipantInput {
    pub user_id: String,
    pub trophies_earned: i32,
    pub eligible_mastery_recovery_count: i32,
    pub last_trophy_earned_epoch_ms: i64,
    pub is_dormant: bool,
    pub returned_from_dormancy_this_week: bool,
    pub qualifying_sessions_since_return: u32,
    pub consecutive_dormant_weeks: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LeagueBracketRolloverInput {
    pub tier: u8,
    pub late_joiner_shielded: bool,
    pub participants: Vec<LeagueParticipantInput>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum LeagueOutcomeKind {
    Promoted,
    Held,
    Demoted,
    DormantPreserved,
    DormantDemoted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct LeagueStandingResult {
    pub user_id: String,
    pub rank: Option<usize>,
    pub outcome: LeagueOutcomeKind,
    pub league_status: String,
    pub tier_before: u8,
    pub tier_after: u8,
    pub next_consecutive_dormant_weeks: u16,
    pub trophies_earned: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct LeagueBracketRolloverOutput {
    pub tier_before: u8,
    pub ranked_count: usize,
    pub max_promo_slots: usize,
    pub demotion_slots: usize,
    pub standings: Vec<LeagueStandingResult>,
}

fn compare_league_participants(a: &LeagueParticipantInput, b: &LeagueParticipantInput) -> Ordering {
    b.trophies_earned
        .cmp(&a.trophies_earned)
        .then_with(|| {
            b.eligible_mastery_recovery_count
                .cmp(&a.eligible_mastery_recovery_count)
        })
        .then_with(|| {
            a.last_trophy_earned_epoch_ms
                .cmp(&b.last_trophy_earned_epoch_ms)
        })
        .then_with(|| a.user_id.cmp(&b.user_id))
}

/// Evaluates 10-tier, <= 30-player weekly league rollover (§11.3):
/// unified `>= 150` trophy promotion floor across all bracket sizes,
/// 4-key deterministic tiebreaks, late-joiner shield, and DORMANT anti-camp demotion.
pub fn evaluate_league_rollover(
    input: &LeagueBracketRolloverInput,
) -> Result<LeagueBracketRolloverOutput, String> {
    if !(LEAGUE_MIN_TIER..=LEAGUE_MAX_TIER).contains(&input.tier) {
        return Err("league tier must be in 1..=10".into());
    }
    if input.participants.len() > LEAGUE_MAX_BRACKET_SIZE {
        return Err("bracket exceeds 30 players".into());
    }

    let mut ranked: Vec<&LeagueParticipantInput> = Vec::new();
    let mut dormant: Vec<&LeagueParticipantInput> = Vec::new();

    for p in &input.participants {
        let dormant_this_week = p.is_dormant
            || (p.returned_from_dormancy_this_week
                && p.qualifying_sessions_since_return < DORMANT_RETURN_MIN_SESSIONS);
        if dormant_this_week {
            dormant.push(p);
        } else {
            ranked.push(p);
        }
    }

    ranked.sort_by(|a, b| compare_league_participants(a, b));
    dormant.sort_by(|a, b| a.user_id.cmp(&b.user_id));

    let ranked_count = ranked.len();
    let max_promo_slots = if ranked_count == 0 {
        0
    } else {
        (5 * ranked_count).div_ceil(30).max(1)
    };
    let demotion_slots = if input.late_joiner_shielded || ranked_count < 10 {
        0
    } else {
        (5 * ranked_count).div_ceil(30).max(1)
    };

    let mut standings = Vec::with_capacity(input.participants.len());

    for (idx, p) in ranked.iter().enumerate() {
        let rank = idx + 1;
        let can_promote = idx < max_promo_slots
            && input.tier < LEAGUE_MAX_TIER
            && p.trophies_earned >= LEAGUE_MIN_PROMOTION_TROPHIES;
        let can_demote = demotion_slots > 0
            && idx >= ranked_count.saturating_sub(demotion_slots)
            && input.tier > LEAGUE_MIN_TIER;

        let (outcome, tier_after) = if can_promote {
            (LeagueOutcomeKind::Promoted, input.tier + 1)
        } else if can_demote {
            (LeagueOutcomeKind::Demoted, input.tier - 1)
        } else {
            (LeagueOutcomeKind::Held, input.tier)
        };

        standings.push(LeagueStandingResult {
            user_id: p.user_id.clone(),
            rank: Some(rank),
            outcome,
            league_status: "ACTIVE".into(),
            tier_before: input.tier,
            tier_after,
            next_consecutive_dormant_weeks: 0,
            trophies_earned: p.trophies_earned,
        });
    }

    for p in dormant {
        let next_weeks = p.consecutive_dormant_weeks.saturating_add(1);
        let (outcome, tier_after) = if next_weeks >= 2 && input.tier > LEAGUE_MIN_TIER {
            (LeagueOutcomeKind::DormantDemoted, input.tier - 1)
        } else {
            (LeagueOutcomeKind::DormantPreserved, input.tier)
        };
        standings.push(LeagueStandingResult {
            user_id: p.user_id.clone(),
            rank: None,
            outcome,
            league_status: "DORMANT".into(),
            tier_before: input.tier,
            tier_after,
            next_consecutive_dormant_weeks: next_weeks,
            trophies_earned: p.trophies_earned,
        });
    }

    Ok(LeagueBracketRolloverOutput {
        tier_before: input.tier,
        ranked_count,
        max_promo_slots,
        demotion_slots,
        standings,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BossStrikeInput {
    pub boss_hp_before: i32,
    pub base_damage: i32,
    pub rating: u8,
    pub overcharged: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct BossStrikeOutcome {
    pub raw_damage: i32,
    pub applied_damage: i32,
    pub overkill_damage: i32,
    pub boss_hp_after: i32,
    pub defeated: bool,
}

/// Evaluates authoritative shared-HP boss strike damage clamping overkill cleanly to `0 HP` (`S-17`).
pub fn evaluate_boss_strike(input: &BossStrikeInput) -> Result<BossStrikeOutcome, String> {
    if input.boss_hp_before < 0 || !(1..=10_000).contains(&input.base_damage) {
        return Err("boss_hp_before must be >= 0 and base_damage in 1..=10000".into());
    }
    let scale_num = match input.rating {
        1 => 0,
        2 => 75,
        3 => 100,
        4 => {
            if input.overcharged {
                200
            } else {
                125
            }
        }
        _ => return Err("rating must be in 1..=4".into()),
    };
    let raw_damage = (input.base_damage * scale_num) / 100;
    let applied_damage = raw_damage.min(input.boss_hp_before);
    let overkill_damage = raw_damage - applied_damage;
    let boss_hp_after = input.boss_hp_before - applied_damage;

    Ok(BossStrikeOutcome {
        raw_damage,
        applied_damage,
        overkill_damage,
        boss_hp_after,
        defeated: input.boss_hp_before > 0 && boss_hp_after == 0,
    })
}
