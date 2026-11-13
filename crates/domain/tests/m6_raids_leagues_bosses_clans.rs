use campuslingo_domain::combat::InteractionSource;
use campuslingo_domain::raids_leagues::*;
use campuslingo_domain::shields::{NodeState, ProtectionKind};

#[test]
fn interaction_source_enum_and_mode_multipliers() {
    assert_eq!(InteractionSource::RevengeRaid.mode_bps(), 15_000);
    assert_eq!(InteractionSource::RevengeDefense.mode_bps(), 15_000);
    assert_eq!(InteractionSource::AsyncRaid.mode_bps(), 10_000);
    assert_eq!(InteractionSource::Spectator.mode_bps(), 0);
    assert!(!InteractionSource::Practice.is_competitive_reward_mode());
    assert!(!InteractionSource::TutorialSandbox.is_competitive_reward_mode());
    assert!(InteractionSource::LiveDuel.is_competitive_reward_mode());
    assert!(InteractionSource::CoopBoss.is_competitive_reward_mode());
    assert!(InteractionSource::League.is_competitive_reward_mode());
}

#[test]
fn raid_targetability_opt_in_immunity_caps_and_mutual_revenge_guard() {
    let base = RaidTargetabilityInput {
        attacker_user_id: "u1".into(),
        defender_user_id: "u2".into(),
        course_is_private: true,
        attacker_raid_opt_in: true,
        defender_raid_opt_in: true,
        node_state: NodeState::Active,
        node_tier: Some(2),
        active_protections: vec![],
        attacker_outbound_raids_today: 0,
        defender_inbound_raids_today: 0,
        unordered_pair_raids_today: 0,
        is_revenge: false,
        revenge_source_available: false,
        revenge_age_seconds: None,
    };
    assert!(evaluate_raid_targetability(&base).targetable);

    // Private course requires explicit opt-in from both sides
    let mut no_opt_in = base.clone();
    no_opt_in.defender_raid_opt_in = false;
    assert_eq!(
        evaluate_raid_targetability(&no_opt_in)
            .blocker_reason
            .as_deref(),
        Some("PRIVATE_COURSE_OPT_IN_REQUIRED")
    );

    // All raid_immune kinds + Tier 0 + INACTIVE + INVALID block targetability
    for kind in [
        ProtectionKind::NewNodeGrace,
        ProtectionKind::VersionGrace,
        ProtectionKind::NewPlayerBubble,
        ProtectionKind::PostRaidCooldown,
        ProtectionKind::ResumeGrace,
        ProtectionKind::Dormant,
    ] {
        let mut p = base.clone();
        p.active_protections = vec![kind];
        let res = evaluate_raid_targetability(&p);
        assert!(!res.targetable && res.raid_immune);
    }
    let mut tier0 = base.clone();
    tier0.node_tier = Some(0);
    assert!(evaluate_raid_targetability(&tier0).raid_immune);
    let mut inactive = base.clone();
    inactive.node_state = NodeState::Inactive;
    inactive.node_tier = None;
    assert!(evaluate_raid_targetability(&inactive).raid_immune);
    let mut invalid = base.clone();
    invalid.node_state = NodeState::Invalid;
    assert!(evaluate_raid_targetability(&invalid).raid_immune);

    // Unordered pair cap = 1 blocks mutual 1.5x revenge farming on same local day
    let mut revenge_same_day = base.clone();
    revenge_same_day.is_revenge = true;
    revenge_same_day.revenge_source_available = true;
    revenge_same_day.revenge_age_seconds = Some(3_600);
    revenge_same_day.unordered_pair_raids_today = 1;
    let rev_blocked = evaluate_raid_targetability(&revenge_same_day);
    assert!(!rev_blocked.targetable);
    assert_eq!(
        rev_blocked.blocker_reason.as_deref(),
        Some("UNORDERED_PAIR_DAILY_CAP_REACHED")
    );
    assert_eq!(rev_blocked.mode_bps, 15_000);

    // Outbound cap = 10 & inbound cap = 3
    let mut out_cap = base.clone();
    out_cap.attacker_outbound_raids_today = 10;
    assert_eq!(
        evaluate_raid_targetability(&out_cap)
            .blocker_reason
            .as_deref(),
        Some("OUTBOUND_DAILY_CAP_REACHED")
    );
    let mut in_cap = base;
    in_cap.defender_inbound_raids_today = 3;
    assert_eq!(
        evaluate_raid_targetability(&in_cap)
            .blocker_reason
            .as_deref(),
        Some("INBOUND_DAILY_CAP_REACHED")
    );
}

#[test]
fn raid_settlement_concurrent_cooldown_and_45_trophy_daily_cap() {
    // Normal win on defender already at 40 daily trophy loss -> capped at 5 trophies lost (total 45)
    let win = evaluate_raid_settlement(&RaidSettlementInput {
        is_revenge: false,
        sandbox_solved: true,
        drills_passed: 3,
        elapsed_from_stage2_ms: 42_000,
        defender_immune_at_settle: false,
        defender_daily_trophy_loss_so_far: 40,
        attacker_practice_xp_today: 0,
        reward_eligible: true,
        base_score_v2_points: 20,
    })
    .unwrap();
    assert_eq!(win.status, "SETTLED_WIN");
    assert_eq!(win.attacker_trophy_gain, 15);
    assert_eq!(win.defender_trophy_loss, 5);
    assert_eq!(win.clan_points_awarded, 25);
    assert!(win.creates_revenge_window);
    assert_eq!(win.loot_chest_tier.as_deref(), Some("APEX_RAID_CHEST"));

    // Concurrent settlement after first raid already applied POST_RAID_COOLDOWN
    let blocked = evaluate_raid_settlement(&RaidSettlementInput {
        is_revenge: false,
        sandbox_solved: true,
        drills_passed: 3,
        elapsed_from_stage2_ms: 50_000,
        defender_immune_at_settle: true,
        defender_daily_trophy_loss_so_far: 15,
        attacker_practice_xp_today: 90,
        reward_eligible: true,
        base_score_v2_points: 20,
    })
    .unwrap();
    assert_eq!(blocked.status, "SETTLED_COOLDOWN_BLOCKED");
    assert!(blocked.concurrent_cooldown_blocked);
    assert!(blocked.fsrs_transitions_commit);
    assert_eq!(blocked.practice_xp_awarded, 10);
    assert_eq!(blocked.attacker_trophy_gain, 0);
    assert_eq!(blocked.defender_trophy_loss, 0);
    assert!(blocked.consumes_attacker_outbound_cap);
    assert!(!blocked.consumes_defender_inbound_cap);
}

#[test]
fn clan_war_relative_stability_formula_and_normalization() {
    // delta_s = 12.0, s_before = 2.0 -> relative = 6.0 -> 10 * ln(7) = 19.459 -> raw = 19
    // active_members_snapshot = 9 -> denom = sqrt(9) = 3.0 -> norm = 6
    let out = compute_clan_stability_points(&ClanStabilityInput {
        s_before: 2.0,
        s_after: 14.0,
        active_members_snapshot: 9,
        reward_eligible: true,
        member_in_frozen_roster: true,
    })
    .unwrap();
    assert_eq!(out.raw_points, 19);
    assert_eq!(out.normalized_points, 6);

    // Huge relative jump clamps at 50 raw points; min denom sqrt(max(3, active)) = sqrt(3)
    let clamped = compute_clan_stability_points(&ClanStabilityInput {
        s_before: 0.5,
        s_after: 500.5,
        active_members_snapshot: 1,
        reward_eligible: true,
        member_in_frozen_roster: true,
    })
    .unwrap();
    assert_eq!(clamped.raw_points, 50);
    assert_eq!(clamped.normalized_points, 29);

    // Mid-week non-frozen member scores 0
    let zero = compute_clan_stability_points(&ClanStabilityInput {
        s_before: 2.0,
        s_after: 14.0,
        active_members_snapshot: 9,
        reward_eligible: true,
        member_in_frozen_roster: false,
    })
    .unwrap();
    assert_eq!(zero.raw_points, 0);
    assert_eq!(zero.normalized_points, 0);
}

#[test]
fn weekly_30_player_league_promo_floor_tiebreaks_and_dormancy() {
    let mut participants = Vec::new();
    for i in 0..29 {
        participants.push(LeagueParticipantInput {
            user_id: format!("u{:02}", i),
            trophies_earned: if i < 4 {
                180 - i
            } else if i == 4 {
                140 // rank 5 top slot, but < 150 floor -> must NOT promote
            } else {
                50 - i.min(40)
            },
            eligible_mastery_recovery_count: 5,
            last_trophy_earned_epoch_ms: 1_000 + (i as i64),
            is_dormant: false,
            returned_from_dormancy_this_week: false,
            qualifying_sessions_since_return: 0,
            consecutive_dormant_weeks: 1,
        });
    }
    // Returning dormant user with only 2 sessions (< 3) and prior consecutive_dormant_weeks = 1 -> DORMANT + demoted
    participants.push(LeagueParticipantInput {
        user_id: "z_dormant".into(),
        trophies_earned: 200,
        eligible_mastery_recovery_count: 10,
        last_trophy_earned_epoch_ms: 900,
        is_dormant: false,
        returned_from_dormancy_this_week: true,
        qualifying_sessions_since_return: 2,
        consecutive_dormant_weeks: 1,
    });

    let res = evaluate_league_rollover(&LeagueBracketRolloverInput {
        tier: 3,
        late_joiner_shielded: false,
        participants,
    })
    .unwrap();

    assert_eq!(res.ranked_count, 29);
    assert_eq!(res.max_promo_slots, 5);
    assert_eq!(res.demotion_slots, 5);

    let rank1 = res.standings.iter().find(|s| s.user_id == "u00").unwrap();
    assert_eq!(rank1.outcome, LeagueOutcomeKind::Promoted);
    assert_eq!(rank1.tier_after, 4);
    assert_eq!(rank1.next_consecutive_dormant_weeks, 0);

    let rank5_low = res.standings.iter().find(|s| s.user_id == "u04").unwrap();
    assert_eq!(rank5_low.rank, Some(5));
    assert_eq!(rank5_low.outcome, LeagueOutcomeKind::Held);
    assert_eq!(rank5_low.tier_after, 3);

    let dorm = res
        .standings
        .iter()
        .find(|s| s.user_id == "z_dormant")
        .unwrap();
    assert_eq!(dorm.league_status, "DORMANT");
    assert_eq!(dorm.outcome, LeagueOutcomeKind::DormantDemoted);
    assert_eq!(dorm.tier_after, 2);
    assert_eq!(dorm.next_consecutive_dormant_weeks, 2);
}

#[test]
fn boss_strike_overkill_clamp() {
    let strike = evaluate_boss_strike(&BossStrikeInput {
        boss_hp_before: 120,
        base_damage: 100,
        rating: 4,
        overcharged: true,
    })
    .unwrap();
    assert_eq!(strike.raw_damage, 200);
    assert_eq!(strike.applied_damage, 120);
    assert_eq!(strike.overkill_damage, 80);
    assert_eq!(strike.boss_hp_after, 0);
    assert!(strike.defeated);
}
