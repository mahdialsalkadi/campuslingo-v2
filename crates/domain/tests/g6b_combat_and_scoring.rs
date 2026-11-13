use campuslingo_domain::combat::{
    AttemptTermination, CombatStrikeInput, DOUBLE_ENERGY_AT_MS, DuelArenaState, DuelCardSlot,
    DuelMatchStatus, DuelSide, EligibilityInput, FighterArenaState, GhostReplayFrame,
    MATCH_DURATION_MS, RatingMapV2Input, RatingReason, SELECTION_DEADLINE_MS, ScoreV2Input,
    adapt_ghost_action, compute_duel_contribution, compute_l_net_ms, compute_score_v2,
    decide_bot_action, evaluate_eligibility, evaluate_rating_map_v2, round_comp_bps,
    round_ties_even_points,
};
use campuslingo_domain::windows::{
    w_base_for_live_duel_ms, w_personal_cut_from_base_ms, w_personal_eff_ms,
    w_quick_blitz_chain_cut_ms, w_quick_blitz_chain_eff_ms, w_round_cut_and_m1_ms,
    w_sudden_death_cut_ms,
};

fn sample_hand(prefix: &str) -> Vec<DuelCardSlot> {
    vec![
        DuelCardSlot {
            card_id: format!("{prefix}-c3"),
            archetype: "AXIOM_PACK".into(),
            modifier: "STANDARD".into(),
            card_cost: 3,
            base_window_ms: 9_000,
        },
        DuelCardSlot {
            card_id: format!("{prefix}-c1"),
            archetype: "AXIOM_PACK".into(),
            modifier: "SPEED_BLITZ".into(),
            card_cost: 1,
            base_window_ms: 6_000,
        },
        DuelCardSlot {
            card_id: format!("{prefix}-c4"),
            archetype: "PROOF_BUILDER".into(),
            modifier: "STANDARD".into(),
            card_cost: 4,
            base_window_ms: 10_000,
        },
        DuelCardSlot {
            card_id: format!("{prefix}-c2"),
            archetype: "DEBUG_DETECTOR".into(),
            modifier: "STANDARD".into(),
            card_cost: 2,
            base_window_ms: 8_000,
        },
    ]
}

#[test]
fn d02a_rating_map_v2_ordered_precedence_and_void_caps() {
    // Case 0: VOID_DELIVERY
    let out = evaluate_rating_map_v2(&RatingMapV2Input {
        termination: AttemptTermination::VoidDelivery,
        is_correct: true,
        partial_credit: false,
        hint_used: false,
        server_sandbox_submits: 1,
        raw_elapsed_ms: 1_200,
        l_net_ms: 1_200,
        w_personal_eff_ms: 8_000,
        w_round_cut_ms: 9_600,
        prior_void_transport_count: 5,
        prior_too_fast_count: 0,
        is_live_duel: true,
    })
    .unwrap();
    assert_eq!(out.fsrs_rating, None);
    assert!(out.is_void);
    assert!(out.voids_combat_effect);
    assert_eq!(out.reason, RatingReason::VoidDelivery);

    // Case 0: VOID_TRANSPORT_LOSS (< 2) voids without FSRS rating, >= 2 escalates to Again=1
    for prior in [0_u8, 1, 2] {
        let out = evaluate_rating_map_v2(&RatingMapV2Input {
            termination: AttemptTermination::VoidTransportLoss,
            is_correct: false,
            partial_credit: false,
            hint_used: false,
            server_sandbox_submits: 1,
            raw_elapsed_ms: 500,
            l_net_ms: 500,
            w_personal_eff_ms: 8_000,
            w_round_cut_ms: 9_600,
            prior_void_transport_count: prior,
            prior_too_fast_count: 0,
            is_live_duel: true,
        })
        .unwrap();
        if prior < 2 {
            assert_eq!(out.fsrs_rating, None);
            assert!(out.is_void);
            assert!(!out.triggers_counter_strike);
            assert_eq!(out.reason, RatingReason::VoidTransportLoss);
        } else {
            assert_eq!(out.fsrs_rating, Some(1));
            assert!(!out.is_void);
            assert!(out.triggers_counter_strike);
            assert!(out.voids_combat_effect);
            assert_eq!(out.reason, RatingReason::VoidTransportLossCapExceeded);
        }
    }

    // Case 0: TOO_FAST (< 150ms) checks raw_elapsed_ms even if l_net_ms is smaller or equal
    let too_fast = evaluate_rating_map_v2(&RatingMapV2Input {
        termination: AttemptTermination::Submitted,
        is_correct: true,
        partial_credit: false,
        hint_used: false,
        server_sandbox_submits: 1,
        raw_elapsed_ms: 149,
        l_net_ms: 100,
        w_personal_eff_ms: 8_000,
        w_round_cut_ms: 9_600,
        prior_void_transport_count: 0,
        prior_too_fast_count: 1,
        is_live_duel: true,
    })
    .unwrap();
    assert_eq!(too_fast.fsrs_rating, None);
    assert_eq!(too_fast.reason, RatingReason::TooFast);

    let too_fast_3rd = evaluate_rating_map_v2(&RatingMapV2Input {
        termination: AttemptTermination::Submitted,
        is_correct: true,
        partial_credit: false,
        hint_used: false,
        server_sandbox_submits: 1,
        raw_elapsed_ms: 149,
        l_net_ms: 100,
        w_personal_eff_ms: 8_000,
        w_round_cut_ms: 9_600,
        prior_void_transport_count: 0,
        prior_too_fast_count: 2,
        is_live_duel: true,
    })
    .unwrap();
    assert_eq!(too_fast_3rd.fsrs_rating, Some(1));
    assert!(too_fast_3rd.triggers_counter_strike);
    assert_eq!(too_fast_3rd.reason, RatingReason::TooFastCapExceeded);

    // Case 1: Timeout via raw_elapsed_ms > W_round_cut even if L_net <= W_round_cut (DEC-006)
    let raw_timeout = evaluate_rating_map_v2(&RatingMapV2Input {
        termination: AttemptTermination::Submitted,
        is_correct: true,
        partial_credit: false,
        hint_used: false,
        server_sandbox_submits: 1,
        raw_elapsed_ms: 9_601,
        l_net_ms: compute_l_net_ms(9_601, 300),
        w_personal_eff_ms: 8_000,
        w_round_cut_ms: 9_600,
        prior_void_transport_count: 0,
        prior_too_fast_count: 0,
        is_live_duel: true,
    })
    .unwrap();
    assert_eq!(raw_timeout.fsrs_rating, Some(1));
    assert_eq!(raw_timeout.reason, RatingReason::Timeout);
    assert!(raw_timeout.triggers_counter_strike);

    // Case 2: Hard = 2 beats Easy when hint_used, >1 submit, partial_credit, or L_net > W_personal_eff
    let hard_hint = evaluate_rating_map_v2(&RatingMapV2Input {
        termination: AttemptTermination::Submitted,
        is_correct: true,
        partial_credit: false,
        hint_used: true,
        server_sandbox_submits: 1,
        raw_elapsed_ms: 1_000,
        l_net_ms: 900,
        w_personal_eff_ms: 8_000,
        w_round_cut_ms: 10_000,
        prior_void_transport_count: 0,
        prior_too_fast_count: 0,
        is_live_duel: true,
    })
    .unwrap();
    assert_eq!(hard_hint.fsrs_rating, Some(2));
    assert_eq!(hard_hint.reason, RatingReason::HintUsed);
    assert!(!hard_hint.triggers_overcharge);

    let hard_slow = evaluate_rating_map_v2(&RatingMapV2Input {
        termination: AttemptTermination::Submitted,
        is_correct: true,
        partial_credit: false,
        hint_used: false,
        server_sandbox_submits: 1,
        raw_elapsed_ms: 8_200,
        l_net_ms: 8_001,
        w_personal_eff_ms: 8_000,
        w_round_cut_ms: 10_000,
        prior_void_transport_count: 0,
        prior_too_fast_count: 0,
        is_live_duel: true,
    })
    .unwrap();
    assert_eq!(hard_slow.fsrs_rating, Some(2));
    assert_eq!(hard_slow.reason, RatingReason::SlowAbovePersonalEff);

    // Case 3: Easy = 4 with L_net <= 0.50 * W_personal_eff triggers 2.0x Overcharge
    let easy = evaluate_rating_map_v2(&RatingMapV2Input {
        termination: AttemptTermination::Submitted,
        is_correct: true,
        partial_credit: false,
        hint_used: false,
        server_sandbox_submits: 1,
        raw_elapsed_ms: 4_150,
        l_net_ms: 4_000,
        w_personal_eff_ms: 8_000,
        w_round_cut_ms: 10_000,
        prior_void_transport_count: 0,
        prior_too_fast_count: 0,
        is_live_duel: true,
    })
    .unwrap();
    assert_eq!(easy.fsrs_rating, Some(4));
    assert!(easy.triggers_overcharge);

    // Case 4: Good = 3
    let good = evaluate_rating_map_v2(&RatingMapV2Input {
        termination: AttemptTermination::Submitted,
        is_correct: true,
        partial_credit: false,
        hint_used: false,
        server_sandbox_submits: 1,
        raw_elapsed_ms: 4_200,
        l_net_ms: 4_001,
        w_personal_eff_ms: 8_000,
        w_round_cut_ms: 10_000,
        prior_void_transport_count: 0,
        prior_too_fast_count: 0,
        is_live_duel: true,
    })
    .unwrap();
    assert_eq!(good.fsrs_rating, Some(3));
    assert!(!good.triggers_overcharge);
}

#[test]
fn windows_dec007_quick_blitz_and_sudden_death() {
    let base_a = w_base_for_live_duel_ms("PIPELINE", "FAULT_INJECTION").unwrap();
    let base_b = w_base_for_live_duel_ms("TRAP", "FORENSIC_AUDIT").unwrap();
    assert_eq!(base_a, 9_000);
    assert_eq!(base_b, 10_000); // 12_000ms pod falls back to 10_000ms live variant
    let eff_a = w_personal_eff_ms(base_a, 3, 2).unwrap();
    assert_eq!(eff_a, 13_500); // 9000 * 1.5
    let cut_a = w_personal_cut_from_base_ms(base_a, 3, 2).unwrap();
    assert_eq!(cut_a, 15_500); // 13500 + min(2000, 2700)
    let (round_cut, m1_cut) = w_round_cut_and_m1_ms(base_a, 3, 2, base_b, 1, 1).unwrap();
    assert_eq!(round_cut, 15_500);
    assert_eq!(m1_cut, 12_000); // 10000 + min(2000, 2000) at M=1.0
    assert_eq!(w_quick_blitz_chain_eff_ms(5, 4).unwrap(), 5_000);
    assert_eq!(w_quick_blitz_chain_cut_ms(5, 4).unwrap(), 6_000);
    assert_eq!(w_sudden_death_cut_ms(1, 1, 3_500).unwrap(), 3_500);
}

#[test]
fn score_v2_i128_ties_even_and_3x_clamp_and_dec008() {
    // Banker's rounding checks
    assert_eq!(round_ties_even_points(5, 2).unwrap(), 2);
    assert_eq!(round_ties_even_points(7, 2).unwrap(), 4);
    assert_eq!(round_comp_bps(1, 4).unwrap(), 2_500);

    // DEC-008: pure score_v2 accepts rating 1..=4
    let r1 = compute_score_v2(&ScoreV2Input {
        base_points: 10,
        rating: 1,
        recall_bps: 10_000,
        mastery_bps: 10_000,
        mode_bps: 10_000,
        recovery_x1e4: 10_000,
    })
    .unwrap();
    assert_eq!(r1.points, 2); // 10 * 0.25 = 2.5 -> ties-to-even = 2
    assert!(!r1.clamped_at_3x);

    // Check 3.0x clamp when product of factors exceeds 3.0x
    let clamped = compute_score_v2(&ScoreV2Input {
        base_points: 10,
        rating: 4,             // 1.25x
        recall_bps: 15_000,    // 1.5x
        mastery_bps: 15_000,   // 1.5x
        mode_bps: 12_500,      // 1.25x
        recovery_x1e4: 15_000, // 1.5x => product > 5.27x -> clamped at 3.0x = 30
    })
    .unwrap();
    assert!(clamped.clamped_at_3x);
    assert_eq!(clamped.effective_multiplier_bps, 30_000);
    assert_eq!(clamped.points, 30);

    let contrib = compute_duel_contribution(&campuslingo_domain::combat::DuelContributionInput {
        card_points: 20,
        player_won: true,
        is_draw: false,
        tower_margin_bps: 4_000,
    })
    .unwrap();
    assert_eq!(contrib, 27); // 20 * 1.35 = 27
}

#[test]
fn eligibility_exposure_and_caps() {
    let blocked_gap = evaluate_eligibility(&EligibilityInput {
        is_void: false,
        same_scope_duplicate: false,
        same_day_gap_secs: Some(1_799),
        card_daily_rewarded_count: 0,
        course_daily_rewarded_count: 0,
    });
    assert_eq!(
        blocked_gap.blocker_reason.as_deref(),
        Some("SAME_DAY_MIN_GAP")
    );
    assert!(!blocked_gap.fsrs_step_eligible);
    assert!(!blocked_gap.reward_eligible);

    let capped_card = evaluate_eligibility(&EligibilityInput {
        is_void: false,
        same_scope_duplicate: false,
        same_day_gap_secs: Some(2_000),
        card_daily_rewarded_count: 3,
        course_daily_rewarded_count: 12,
    });
    assert_eq!(
        capped_card.blocker_reason.as_deref(),
        Some("CARD_DAILY_CAP")
    );
    assert!(capped_card.fsrs_step_eligible);
    assert!(!capped_card.reward_eligible);
}

#[test]
fn arena_2lane_energy_overcharge_counter_strike_fizzle_and_f03_ghost() {
    let pa = FighterArenaState::new(DuelSide::PlayerA, 1, 1, sample_hand("a"));
    let pb = FighterArenaState::new(DuelSide::PlayerB, 1, 1, sample_hand("b"));
    let mut arena = DuelArenaState::new(pa, pb);

    // Test DEC-001 1500ms selection deadline picks lexicographically smallest card (`a-c1`)
    arena.player_a.selection_started_at_ms = Some(0);
    arena.advance_clock(SELECTION_DEADLINE_MS).unwrap();
    assert_eq!(arena.player_a.selected_card_id.as_deref(), Some("a-c1"));
    assert_eq!(arena.player_a.energy_milli, 5_600); // 5000 + 400*1.5

    // Verify 2x energy regen in final 30s
    arena
        .advance_clock(DOUBLE_ENERGY_AT_MS - SELECTION_DEADLINE_MS)
        .unwrap();
    assert!(arena.is_double_energy());
    let before = arena.player_a.energy_milli;
    arena.player_a.energy_milli = 2_000;
    arena.advance_clock(2_000).unwrap();
    assert_eq!(arena.player_a.energy_milli, 3_600); // 2000 + 800*2
    arena.player_a.energy_milli = before;

    // Wrong answer by Player B opens Counter-Strike for Player A
    let miss = arena
        .resolve_strike(&CombatStrikeInput {
            attacker: DuelSide::PlayerB,
            sequence: 1,
            lane: 0,
            card_id: "b-c2".into(),
            use_quick_blitz_chain: false,
            termination: AttemptTermination::Submitted,
            is_correct: false,
            partial_credit: false,
            hint_used: false,
            server_sandbox_submits: 1,
            raw_elapsed_ms: 2_000,
            rtt_compensation_ms: 0,
            opponent_card_base_ms: None,
        })
        .unwrap();
    assert_eq!(miss.rating.fsrs_rating, Some(1));
    assert!(arena.player_a.counter_strike_until_ms.is_some());

    // Player A hits Overcharge + Counter-Strike -> 2.0x * 1.5x = 3.0x base damage, bypassing shield!
    let hit = arena
        .resolve_strike(&CombatStrikeInput {
            attacker: DuelSide::PlayerA,
            sequence: 1,
            lane: 0,
            card_id: "a-c2".into(), // cost 2 => base 240; Overcharge 480 * CounterStrike 3/2 = 720
            use_quick_blitz_chain: false,
            termination: AttemptTermination::Submitted,
            is_correct: true,
            partial_credit: false,
            hint_used: false,
            server_sandbox_submits: 1,
            raw_elapsed_ms: 2_000,
            rtt_compensation_ms: 0,
            opponent_card_base_ms: None,
        })
        .unwrap();
    assert!(hit.overcharged);
    assert!(hit.counter_strike_consumed);
    assert_eq!(hit.damage_to_shield, 0);
    assert_eq!(hit.damage_to_tower, 720);
    assert_eq!(arena.player_b.tower_shield[0], 300); // shield untouched by Overcharge pierce!
    assert_eq!(arena.player_b.tower_hp[0], 1_080);

    // F-03 Honest Ghost adaptation: never applies blind `historical_damage_ignored = 9999`,
    // instead retargets living lane & respects current Ghost energy!
    let mut ghost_probe = arena.clone();
    ghost_probe.player_a.tower_hp[0] = 0; // destroy Player A lane 0 in probe
    ghost_probe.player_b.energy_milli = 2_000; // can only afford cost 1..=2 even if ghost preferred cost 4!
    let ghost_decision = adapt_ghost_action(
        &ghost_probe,
        DuelSide::PlayerB,
        &GhostReplayFrame {
            recorded_elapsed_ms: 63_000,
            preferred_lane: 0, // destroyed in current match!
            preferred_card_cost: 4,
            historical_is_correct: true,
            historical_response_ms: 100, // stale/too fast in replay
            historical_hint_used: false,
            historical_damage_ignored: 9_999,
        },
    )
    .unwrap();
    assert_eq!(ghost_decision.chosen_lane, 1); // retargeted to living lane 1!
    assert_eq!(ghost_decision.chosen_card_id, "b-c2"); // bounded to current energy!
    assert!(ghost_decision.simulated_raw_elapsed_ms >= 400);

    // Bot fallback controller picks valid affordable card cleanly
    let bot_pick = decide_bot_action(&arena, DuelSide::PlayerB, 7).unwrap();
    assert!(bot_pick.adapted_from_state);

    // Clock reaches 90s with unequal HP -> match ends deterministically
    arena.advance_clock(MATCH_DURATION_MS).unwrap();
    assert_eq!(arena.status, DuelMatchStatus::Completed);
    assert_eq!(arena.winner, Some(DuelSide::PlayerA));
}
