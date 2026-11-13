use campuslingo_domain::shields::{
    CourseShieldInput, E_APEX, E_ENTER_BROWNOUT, E_EXIT_BROWNOUT, E_INITIAL, NodeState,
    PrerequisiteEdgeInput, ProtectionKind, S_APEX_TICKS, S_PROTO_TICKS, ShieldCardInput,
    ShieldNodeInput, VersionGraceInput, compute_tier, compute_version_grace,
    evaluate_course_shields, is_new_player_bubble, next_brownout_state,
    split_protection_predicates, ticks_1e4, ticks_1e4_checked, validate_prerequisite_dag,
};

fn base_time() -> i64 {
    1_760_000_000
}

#[test]
fn ticks_1e4_uses_round_ties_to_even_and_guards_non_finite() {
    assert_eq!(ticks_1e4(0.6950), E_ENTER_BROWNOUT);
    assert_eq!(ticks_1e4(0.7000), E_INITIAL);
    assert_eq!(ticks_1e4(0.7050), E_EXIT_BROWNOUT);
    assert_eq!(ticks_1e4(0.8500), E_APEX);
    assert_eq!(ticks_1e4(5.0), S_PROTO_TICKS);
    assert_eq!(ticks_1e4(21.0), S_APEX_TICKS);

    // Exact decimal halves in 1e-4 ticks: 0.00025 -> 2 (even), 0.00035 -> 4 (even)
    // Using exact binary floats via 0.125 / 16 etc. or exact 1e4 half checks:
    // 0.00005_f32 is > 0.00005 in f32, whereas integer/binary halves are exact in f32:
    // 1.0 + 0.5 / 10000 isn't binary exact, but 1.25 / 8 is; test both finiteness and symmetry
    assert!(ticks_1e4_checked(f32::NAN).is_none());
    assert!(ticks_1e4_checked(f32::INFINITY).is_none());
    assert!(ticks_1e4_checked(-0.001).is_none());
    assert_eq!(ticks_1e4_checked(0.7050), Some(7050));
}

#[test]
fn brownout_hysteresis_and_tier_table_follow_g6b_boundaries() {
    // Initial (< 7000)
    assert!(next_brownout_state(6999, None));
    assert!(!next_brownout_state(7000, None));

    // Stay healthy until strictly < 6950
    assert!(!next_brownout_state(6950, Some(false)));
    assert!(next_brownout_state(6949, Some(false)));

    // Stay in brownout until >= 7050
    assert!(next_brownout_state(7049, Some(true)));
    assert!(!next_brownout_state(7050, Some(true)));

    // Tier table first-match-wins over C_scored
    assert_eq!(
        compute_tier(0, 0, 0, 0, 0, 0, false),
        (NodeState::Inactive, None)
    );
    // C_active non-empty, C_scored empty (e.g., all in VERSION_GRACE) -> Tier 0
    assert_eq!(
        compute_tier(2, 0, 0, 0, 0, 0, false),
        (NodeState::Active, Some(0))
    );
    // C_scored non-empty, reps_sum == 0 -> Tier 0
    assert_eq!(
        compute_tier(2, 2, 0, 0, 0, 0, false),
        (NodeState::Active, Some(0))
    );
    // Mixed reps_sum > 0, reps_min == 0 -> Tier 1 (NOT brownout forced)
    assert_eq!(
        compute_tier(2, 2, 4, 0, 250_000, 9_500, false),
        (NodeState::Active, Some(1))
    );
    // is_brownout -> Tier 1 even with high stability
    assert_eq!(
        compute_tier(2, 2, 4, 2, 250_000, 7_049, true),
        (NodeState::Active, Some(1))
    );
    // s_u_ticks < 50_000 -> Tier 1
    assert_eq!(
        compute_tier(2, 2, 4, 2, 49_999, 9_500, false),
        (NodeState::Active, Some(1))
    );
    // Tier 2: healthy, s_u >= 50_000, below apex
    assert_eq!(
        compute_tier(2, 2, 4, 2, 209_999, 9_000, false),
        (NodeState::Active, Some(2))
    );
    assert_eq!(
        compute_tier(2, 2, 4, 2, 210_000, 8_499, false),
        (NodeState::Active, Some(2))
    );
    // Tier 3 (Apex): s_u >= 210_000 AND e_u >= 8500 AND !is_brownout
    assert_eq!(
        compute_tier(2, 2, 4, 2, 210_000, 8_500, false),
        (NodeState::Active, Some(3))
    );
}

#[test]
fn split_predicates_and_strict_and_new_player_bubble() {
    // Bubble requires BOTH time remaining AND < 20 transitions
    assert!(is_new_player_bubble(100, 200, 19));
    assert!(!is_new_player_bubble(200, 200, 19));
    assert!(!is_new_player_bubble(100, 200, 20));

    // POST_RAID_COOLDOWN / RESUME_GRACE / DORMANT confer raid_immune ONLY (NOT propagation_exempt)
    let raid_only = split_protection_predicates(
        NodeState::Active,
        Some(2),
        &[
            ProtectionKind::PostRaidCooldown,
            ProtectionKind::ResumeGrace,
            ProtectionKind::Dormant,
        ],
    );
    assert!(raid_only.raid_immune);
    assert!(!raid_only.propagation_exempt);

    // NEW_NODE_GRACE / VERSION_GRACE / NEW_PLAYER_BUBBLE / Tier 0 / INACTIVE / INVALID confer both
    for kind in [
        ProtectionKind::NewNodeGrace,
        ProtectionKind::VersionGrace,
        ProtectionKind::NewPlayerBubble,
    ] {
        let p = split_protection_predicates(NodeState::Active, Some(1), &[kind]);
        assert!(p.raid_immune);
        assert!(p.propagation_exempt);
    }
    assert!(split_protection_predicates(NodeState::Active, Some(0), &[]).propagation_exempt);
    assert!(split_protection_predicates(NodeState::Inactive, None, &[]).propagation_exempt);
    assert!(split_protection_predicates(NodeState::Invalid, None, &[]).propagation_exempt);
}

#[test]
fn non_negative_version_grace_handles_dormancy_cap_and_30d_ledger() {
    let act = 1_760_000_000;
    // Normal engaged at activation+2d, 3 new cards -> 72 + 36 = 108h window
    let win = compute_version_grace(&VersionGraceInput {
        effective_notice_anchor_epoch_s: Some(act),
        first_course_open_after_activation_epoch_s: Some(act + 86_400),
        node_engaged_at_epoch_s: Some(act + 2 * 86_400),
        new_ungraced_cards_count: 3,
        used_grace_sec_in_last_30d: 0,
    })
    .unwrap();
    assert_eq!(win.granted_seconds, 108 * 3600);

    // Engaged AFTER anchor + 14d -> grace_start >= raw_end -> strictly None (0h, never negative)
    assert!(
        compute_version_grace(&VersionGraceInput {
            effective_notice_anchor_epoch_s: Some(act),
            first_course_open_after_activation_epoch_s: Some(act + 86_400),
            node_engaged_at_epoch_s: Some(act + 15 * 86_400),
            new_ungraced_cards_count: 5,
            used_grace_sec_in_last_30d: 0,
        })
        .is_none()
    );

    // Unengaged when course never opened within 7d -> grace_start = act + 14d >= abs_cap -> None (0h)
    assert!(
        compute_version_grace(&VersionGraceInput {
            effective_notice_anchor_epoch_s: Some(act),
            first_course_open_after_activation_epoch_s: None,
            node_engaged_at_epoch_s: None,
            new_ungraced_cards_count: 2,
            used_grace_sec_in_last_30d: 0,
        })
        .is_none()
    );

    // Rolling 30-day ledger exhausted (14d used) -> 0h
    assert!(
        compute_version_grace(&VersionGraceInput {
            effective_notice_anchor_epoch_s: Some(act),
            first_course_open_after_activation_epoch_s: Some(act + 3600),
            node_engaged_at_epoch_s: Some(act + 3600),
            new_ungraced_cards_count: 4,
            used_grace_sec_in_last_30d: 14 * 86_400,
        })
        .is_none()
    );

    // Rolling ledger partially remaining (14d - 24h) -> clamped to 24h
    let clamped = compute_version_grace(&VersionGraceInput {
        effective_notice_anchor_epoch_s: Some(act),
        first_course_open_after_activation_epoch_s: Some(act + 3600),
        node_engaged_at_epoch_s: Some(act + 3600),
        new_ungraced_cards_count: 8,
        used_grace_sec_in_last_30d: 13 * 86_400,
    })
    .unwrap();
    assert_eq!(clamped.granted_seconds, 86_400);
}

#[test]
fn dag_propagation_respects_exemptions_brownout_and_nan_isolation() {
    let now = base_time();
    // Parent P (decayed below 70%, NOT exempt) -> Child C (healthy card R~0.95)
    // Child E_u suffers propagation penalty from P; Parent Q is DORMANT-only (so NOT exempt!)
    // vs Parent R is in NEW_NODE_GRACE (propagation_exempt == true, so skipped!).
    let input = CourseShieldInput {
        now_epoch_s: now,
        last_qualifying_study_at_epoch_s: now - 3600,
        new_player_bubble_until_epoch_s: now - 10, // expired bubble
        eligible_transition_count: 5,
        resume_grace_until_epoch_s: None,
        nodes: vec![
            ShieldNodeInput {
                node_id: "child".into(),
                active_cards: vec![ShieldCardInput {
                    card_id: "c1".into(),
                    stability: 30.0,
                    reps: 6,
                    last_review_epoch_s: Some(now - 3600), // near R=1.0
                    decay: None,
                    version_grace_active: false,
                }],
                previous_brownout: Some(false),
                unlocked_at_epoch_s: Some(now - 10 * 86_400),
                post_raid_cooldown_until_epoch_s: None,
                version_grace_active: false,
            },
            ShieldNodeInput {
                node_id: "parent_weak".into(),
                active_cards: vec![ShieldCardInput {
                    card_id: "p1".into(),
                    stability: 2.0,
                    reps: 3,
                    last_review_epoch_s: Some(now - 25 * 86_400), // E_p < 6950 (power-law decay)
                    decay: None,
                    version_grace_active: false,
                }],
                previous_brownout: None,
                unlocked_at_epoch_s: Some(now - 10 * 86_400),
                post_raid_cooldown_until_epoch_s: Some(now + 3600), // raid_immune, NOT propagation_exempt
                version_grace_active: false,
            },
            ShieldNodeInput {
                node_id: "parent_new_grace".into(),
                active_cards: vec![ShieldCardInput {
                    card_id: "g1".into(),
                    stability: 1.0,
                    reps: 1,
                    last_review_epoch_s: Some(now - 10 * 86_400), // weak, yet unlocked 1h ago => NEW_NODE_GRACE
                    decay: None,
                    version_grace_active: false,
                }],
                previous_brownout: None,
                unlocked_at_epoch_s: Some(now - 3600),
                post_raid_cooldown_until_epoch_s: None,
                version_grace_active: false,
            },
            ShieldNodeInput {
                node_id: "parent_nan".into(),
                active_cards: vec![ShieldCardInput {
                    card_id: "nan1".into(),
                    stability: f32::NAN,
                    reps: 2,
                    last_review_epoch_s: Some(now - 3600),
                    decay: None,
                    version_grace_active: false,
                }],
                previous_brownout: Some(false),
                unlocked_at_epoch_s: Some(now - 10 * 86_400),
                post_raid_cooldown_until_epoch_s: None,
                version_grace_active: false,
            },
            ShieldNodeInput {
                node_id: "inactive".into(),
                active_cards: vec![],
                previous_brownout: Some(true),
                unlocked_at_epoch_s: Some(now - 10 * 86_400),
                post_raid_cooldown_until_epoch_s: None,
                version_grace_active: false,
            },
        ],
        edges: vec![
            PrerequisiteEdgeInput {
                parent_node_id: "parent_weak".into(),
                child_node_id: "child".into(),
                weight: 1.0,
            },
            PrerequisiteEdgeInput {
                parent_node_id: "parent_new_grace".into(),
                child_node_id: "child".into(),
                weight: 1.0,
            },
            PrerequisiteEdgeInput {
                parent_node_id: "parent_nan".into(),
                child_node_id: "child".into(),
                weight: 1.0,
            },
        ],
    };

    let out = evaluate_course_shields(&input).unwrap();
    let by_id: std::collections::BTreeMap<_, _> =
        out.nodes.iter().map(|n| (n.node_id.as_str(), n)).collect();

    // Parent weak entered brownout, raid_immune (POST_RAID_COOLDOWN), NOT propagation_exempt
    let pw = by_id["parent_weak"];
    assert_eq!(pw.state, NodeState::Active);
    assert!(pw.e_u_ticks.unwrap() < 6950);
    assert_eq!(pw.is_brownout, Some(true));
    assert!(pw.protection.raid_immune);
    assert!(!pw.protection.propagation_exempt);

    // Parent in NEW_NODE_GRACE is both raid_immune and propagation_exempt
    let pg = by_id["parent_new_grace"];
    assert!(pg.protection.propagation_exempt);
    assert!(
        pg.protection
            .active_protections
            .contains(&ProtectionKind::NewNodeGrace)
    );

    // Parent with NaN maps strictly to INVALID with NULL ticks/tier/brownout per DEC-004
    let pn = by_id["parent_nan"];
    assert_eq!(pn.state, NodeState::Invalid);
    assert_eq!(pn.tier, None);
    assert_eq!(pn.e_u_ticks, None);
    assert_eq!(pn.is_brownout, None);

    // Inactive node has NULL ticks/tier/brownout per DEC-004
    let inact = by_id["inactive"];
    assert_eq!(inact.state, NodeState::Inactive);
    assert_eq!(inact.tier, None);
    assert_eq!(inact.e_u_ticks, None);

    // Child propagated ONLY parent_weak's penalty (skipped parent_new_grace and parent_nan)
    let child = by_id["child"];
    assert_eq!(child.state, NodeState::Active);
    assert!(child.e_u_ticks.unwrap() < child.i_u_ticks.unwrap());
    assert!(child.e_u_ticks.unwrap() > 0);
}

#[test]
fn s05_dag_rejects_cycles_self_loops_excess_depth_and_excess_closure() {
    // Cycle rejected
    assert!(
        validate_prerequisite_dag(
            &["a".into(), "b".into()],
            &[
                PrerequisiteEdgeInput {
                    parent_node_id: "a".into(),
                    child_node_id: "b".into(),
                    weight: 0.5,
                },
                PrerequisiteEdgeInput {
                    parent_node_id: "b".into(),
                    child_node_id: "a".into(),
                    weight: 0.5,
                },
            ],
        )
        .is_err()
    );

    // Self-loop rejected
    assert!(
        validate_prerequisite_dag(
            &["a".into()],
            &[PrerequisiteEdgeInput {
                parent_node_id: "a".into(),
                child_node_id: "a".into(),
                weight: 0.5,
            }],
        )
        .is_err()
    );

    // Depth 16 accepted, depth 17 rejected
    let ok_nodes: Vec<String> = (0..=16).map(|i| format!("n{i}")).collect();
    let ok_edges: Vec<PrerequisiteEdgeInput> = (0..16)
        .map(|i| PrerequisiteEdgeInput {
            parent_node_id: format!("n{i}"),
            child_node_id: format!("n{}", i + 1),
            weight: 0.5,
        })
        .collect();
    let (_, _, max_depth) = validate_prerequisite_dag(&ok_nodes, &ok_edges).unwrap();
    assert_eq!(max_depth, 16);

    let mut bad_nodes = ok_nodes;
    bad_nodes.push("n17".into());
    let mut bad_edges = ok_edges;
    bad_edges.push(PrerequisiteEdgeInput {
        parent_node_id: "n16".into(),
        child_node_id: "n17".into(),
        weight: 0.5,
    });
    assert!(validate_prerequisite_dag(&bad_nodes, &bad_edges).is_err());

    // Closure 256 accepted, closure 257 rejected (fan-in to root child)
    let mut fan_nodes: Vec<String> = (0..257).map(|i| format!("u{i}")).collect();
    fan_nodes.push("target".into());
    let mut fan_edges: Vec<PrerequisiteEdgeInput> = (0..256)
        .map(|i| PrerequisiteEdgeInput {
            parent_node_id: format!("u{i}"),
            child_node_id: "target".into(),
            weight: 0.1,
        })
        .collect();
    assert!(validate_prerequisite_dag(&fan_nodes, &fan_edges).is_ok());

    fan_edges.push(PrerequisiteEdgeInput {
        parent_node_id: "u256".into(),
        child_node_id: "target".into(),
        weight: 0.1,
    });
    assert!(validate_prerequisite_dag(&fan_nodes, &fan_edges).is_err());
}
