use std::collections::{BTreeMap, BTreeSet, VecDeque};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::memory::{ShieldInput, shield};

pub const E_ENTER_BROWNOUT: i64 = 6950;
pub const E_INITIAL: i64 = 7000;
pub const E_EXIT_BROWNOUT: i64 = 7050;
pub const E_APEX: i64 = 8500;
pub const S_PROTO_TICKS: i64 = 50_000;
pub const S_APEX_TICKS: i64 = 210_000;

pub const MAX_DAG_DEPTH: usize = 16;
pub const MAX_DAG_CLOSURE: usize = 256;

pub const NEW_PLAYER_BUBBLE_HOURS: i64 = 48;
pub const NEW_PLAYER_BUBBLE_TRANSITION_LIMIT: i32 = 20;
pub const NEW_NODE_GRACE_HOURS: i64 = 72;
pub const POST_RAID_COOLDOWN_HOURS: i64 = 6;
pub const RESUME_GRACE_HOURS: i64 = 72;
pub const MIN_DORMANCY_CYCLE_DAYS: i64 = 14;
pub const DORMANCY_THRESHOLD_DAYS: i64 = 7;

pub const VERSION_NOTICE_DAYS: i64 = 7;
pub const VERSION_ABSOLUTE_CAP_DAYS: i64 = 14;
pub const VERSION_GRACE_BASE_HOURS: i64 = 72;
pub const VERSION_GRACE_PER_CARD_HOURS: i64 = 12;
pub const VERSION_GRACE_MAX_HOURS: i64 = 168;
pub const VERSION_GRACE_ROLLING_WINDOW_DAYS: i64 = 30;
pub const VERSION_GRACE_ROLLING_CAP_HOURS: i64 = 336;
pub const STALE_RECOMPUTE_SECS: i64 = 15 * 60;

/// Quantizes a finite floating-point metric into `10^-4` integer ticks using
/// IEEE-754 round-ties-to-even. Non-finite inputs must be guarded by callers.
pub fn ticks_1e4(value: f32) -> i64 {
    ((value as f64) * 10_000.0).round_ties_even() as i64
}

/// Checked `10^-4` quantization returning `None` for `NaN`/`Inf` or out-of-range
/// negative values so quantization never feeds invalid numbers into callers.
pub fn ticks_1e4_checked(value: f32) -> Option<i64> {
    if !value.is_finite() || !(0.0..=36_500.0).contains(&value) {
        return None;
    }
    Some(ticks_1e4(value))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NodeState {
    Inactive,
    Active,
    Invalid,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ProtectionKind {
    NewNodeGrace,
    VersionGrace,
    NewPlayerBubble,
    PostRaidCooldown,
    ResumeGrace,
    Dormant,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ProtectionStatus {
    pub active_protections: Vec<ProtectionKind>,
    pub raid_immune: bool,
    pub propagation_exempt: bool,
}

/// Evaluates §9.5 split protection predicates from node state, tier, and active shields.
pub fn split_protection_predicates(
    state: NodeState,
    tier: Option<u8>,
    protections: &[ProtectionKind],
) -> ProtectionStatus {
    let mut active_protections: Vec<ProtectionKind> = protections.to_vec();
    active_protections.sort();
    active_protections.dedup();

    let tier0_or_non_active = state != NodeState::Active || tier == Some(0) || tier.is_none();
    let has_propagation_grace = active_protections.iter().any(|p| {
        matches!(
            p,
            ProtectionKind::NewNodeGrace
                | ProtectionKind::VersionGrace
                | ProtectionKind::NewPlayerBubble
        )
    });
    let has_raid_only_protection = active_protections.iter().any(|p| {
        matches!(
            p,
            ProtectionKind::PostRaidCooldown
                | ProtectionKind::ResumeGrace
                | ProtectionKind::Dormant
        )
    });

    let propagation_exempt = tier0_or_non_active || has_propagation_grace;
    let raid_immune = propagation_exempt || has_raid_only_protection;

    ProtectionStatus {
        active_protections,
        raid_immune,
        propagation_exempt,
    }
}

/// Strict `AND` bubble predicate (§9.5): active iff both time window and < 20 transitions hold.
pub fn is_new_player_bubble(
    now_epoch_s: i64,
    new_player_bubble_until_s: i64,
    eligible_transition_count: i32,
) -> bool {
    now_epoch_s < new_player_bubble_until_s
        && eligible_transition_count < NEW_PLAYER_BUBBLE_TRANSITION_LIMIT
}

/// Brownout hysteresis (§9.3): initial `< 7000`, enter `< 6950`, exit `>= 7050`.
pub fn next_brownout_state(e_u_ticks: i64, previous_brownout: Option<bool>) -> bool {
    match previous_brownout {
        None => e_u_ticks < E_INITIAL,
        Some(true) => e_u_ticks < E_EXIT_BROWNOUT,
        Some(false) => e_u_ticks < E_ENTER_BROWNOUT,
    }
}

/// Total Tier Table over `C_scored` (§9.4, first match wins).
pub fn compute_tier(
    active_cards_count: usize,
    scored_cards_count: usize,
    reps_sum: u64,
    reps_min: u32,
    s_u_ticks: i64,
    e_u_ticks: i64,
    is_brownout: bool,
) -> (NodeState, Option<u8>) {
    if active_cards_count == 0 {
        return (NodeState::Inactive, None);
    }
    if scored_cards_count == 0 || reps_sum == 0 {
        return (NodeState::Active, Some(0));
    }
    if reps_min == 0 || is_brownout || s_u_ticks < S_PROTO_TICKS {
        return (NodeState::Active, Some(1));
    }
    if s_u_ticks >= S_APEX_TICKS && e_u_ticks >= E_APEX && !is_brownout {
        return (NodeState::Active, Some(3));
    }
    (NodeState::Active, Some(2))
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VersionGraceInput {
    /// `ended_at` from dormancy overlapping `version_activated_at`, `None` if still dormant (`+infinity`),
    /// or `version_activated_at` itself when not dormant at activation.
    pub effective_notice_anchor_epoch_s: Option<i64>,
    pub first_course_open_after_activation_epoch_s: Option<i64>,
    pub node_engaged_at_epoch_s: Option<i64>,
    pub new_ungraced_cards_count: u32,
    pub used_grace_sec_in_last_30d: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct VersionGraceWindow {
    pub start_epoch_s: i64,
    pub end_epoch_s: i64,
    pub granted_seconds: i64,
}

/// Computes non-negative `VERSION_GRACE` (§9.2, Audit Fix §8.3) with 14-day rolling cap.
pub fn compute_version_grace(input: &VersionGraceInput) -> Option<VersionGraceWindow> {
    let notice_cap_s = match input.effective_notice_anchor_epoch_s {
        Some(anchor) => anchor.saturating_add(VERSION_NOTICE_DAYS * 86_400),
        None => i64::MAX,
    };
    let abs_cap_s = match input.effective_notice_anchor_epoch_s {
        Some(anchor) => anchor.saturating_add(VERSION_ABSOLUTE_CAP_DAYS * 86_400),
        None => i64::MAX,
    };

    let course_open_s = input
        .first_course_open_after_activation_epoch_s
        .unwrap_or(i64::MAX);
    let course_seen_at_s = course_open_s.min(notice_cap_s);

    let grace_start_s = match input.node_engaged_at_epoch_s {
        Some(engaged_s) => engaged_s,
        None => course_seen_at_s.saturating_add(VERSION_NOTICE_DAYS * 86_400),
    };

    let grace_hours = (VERSION_GRACE_BASE_HOURS
        + VERSION_GRACE_PER_CARD_HOURS * i64::from(input.new_ungraced_cards_count))
    .clamp(VERSION_GRACE_BASE_HOURS, VERSION_GRACE_MAX_HOURS);
    let raw_end_s = grace_start_s
        .saturating_add(grace_hours * 3600)
        .min(abs_cap_s);

    if grace_start_s >= raw_end_s {
        return None;
    }

    let rolling_cap_s = VERSION_GRACE_ROLLING_CAP_HOURS * 3600;
    let remaining_allowance_s = (rolling_cap_s - input.used_grace_sec_in_last_30d.max(0)).max(0);
    if remaining_allowance_s == 0 {
        return None;
    }

    let clamped_end_s = raw_end_s.min(grace_start_s.saturating_add(remaining_allowance_s));
    if grace_start_s >= clamped_end_s {
        return None;
    }

    Some(VersionGraceWindow {
        start_epoch_s: grace_start_s,
        end_epoch_s: clamped_end_s,
        granted_seconds: clamped_end_s - grace_start_s,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ShieldCardInput {
    pub card_id: String,
    pub stability: f32,
    pub reps: u32,
    pub last_review_epoch_s: Option<i64>,
    #[serde(default)]
    pub decay: Option<f32>,
    #[serde(default)]
    pub version_grace_active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ShieldNodeInput {
    pub node_id: String,
    pub active_cards: Vec<ShieldCardInput>,
    #[serde(default)]
    pub previous_brownout: Option<bool>,
    #[serde(default)]
    pub unlocked_at_epoch_s: Option<i64>,
    #[serde(default)]
    pub post_raid_cooldown_until_epoch_s: Option<i64>,
    #[serde(default)]
    pub version_grace_active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PrerequisiteEdgeInput {
    pub parent_node_id: String,
    pub child_node_id: String,
    pub weight: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CourseShieldInput {
    pub now_epoch_s: i64,
    pub last_qualifying_study_at_epoch_s: i64,
    pub new_player_bubble_until_epoch_s: i64,
    pub eligible_transition_count: i32,
    #[serde(default)]
    pub resume_grace_until_epoch_s: Option<i64>,
    pub nodes: Vec<ShieldNodeInput>,
    pub edges: Vec<PrerequisiteEdgeInput>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct NodeShieldEvaluation {
    pub node_id: String,
    pub state: NodeState,
    pub tier: Option<u8>,
    pub e_u_ticks: Option<i64>,
    pub i_u_ticks: Option<i64>,
    pub s_u_ticks: Option<i64>,
    pub is_brownout: Option<bool>,
    pub unlocked_at_epoch_s: Option<i64>,
    pub scored_cards: usize,
    pub active_cards: usize,
    pub protection: ProtectionStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CourseShieldOutput {
    pub now_epoch_s: i64,
    pub is_dormant: bool,
    pub new_player_bubble_active: bool,
    pub resume_grace_active: bool,
    pub topological_order: Vec<String>,
    pub max_depth: usize,
    pub nodes: Vec<NodeShieldEvaluation>,
}

pub type ValidatedDag = (Vec<usize>, Vec<Vec<(usize, f32)>>, usize);

/// Validates S-05 DAG constraints and returns topological order `(index_order, parents, max_depth)`.
pub fn validate_prerequisite_dag(
    node_ids: &[String],
    edges: &[PrerequisiteEdgeInput],
) -> Result<ValidatedDag, String> {
    let mut id_to_idx: BTreeMap<&str, usize> = BTreeMap::new();
    for (idx, id) in node_ids.iter().enumerate() {
        if id.trim().is_empty() {
            return Err("empty node_id in shield DAG".into());
        }
        if id_to_idx.insert(id.as_str(), idx).is_some() {
            return Err(format!("duplicate node_id in shield DAG: {id}"));
        }
    }

    let n = node_ids.len();
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut parents: Vec<Vec<(usize, f32)>> = vec![Vec::new(); n];
    let mut in_degree: Vec<usize> = vec![0; n];
    let mut seen_edges: BTreeSet<(usize, usize)> = BTreeSet::new();

    for edge in edges {
        if !edge.weight.is_finite() || !(0.0..=1.0).contains(&edge.weight) {
            return Err(format!(
                "invalid prerequisite weight {} -> {}",
                edge.parent_node_id, edge.child_node_id
            ));
        }
        let Some(&p_idx) = id_to_idx.get(edge.parent_node_id.as_str()) else {
            return Err(format!(
                "unknown parent node in prerequisite edge: {}",
                edge.parent_node_id
            ));
        };
        let Some(&c_idx) = id_to_idx.get(edge.child_node_id.as_str()) else {
            return Err(format!(
                "unknown child node in prerequisite edge: {}",
                edge.child_node_id
            ));
        };
        if p_idx == c_idx {
            return Err(format!(
                "self-loop forbidden on node {}",
                edge.parent_node_id
            ));
        }
        if !seen_edges.insert((p_idx, c_idx)) {
            return Err(format!(
                "duplicate prerequisite edge {} -> {}",
                edge.parent_node_id, edge.child_node_id
            ));
        }
        children[p_idx].push(c_idx);
        parents[c_idx].push((p_idx, edge.weight));
        in_degree[c_idx] += 1;
    }

    let mut queue: VecDeque<usize> = in_degree
        .iter()
        .enumerate()
        .filter_map(|(idx, &deg)| if deg == 0 { Some(idx) } else { None })
        .collect();
    let mut topo_order: Vec<usize> = Vec::with_capacity(n);

    while let Some(u) = queue.pop_front() {
        topo_order.push(u);
        for &v in &children[u] {
            in_degree[v] -= 1;
            if in_degree[v] == 0 {
                queue.push_back(v);
            }
        }
    }

    if topo_order.len() != n {
        return Err("prerequisite DAG contains a cycle".into());
    }

    let mut depth: Vec<usize> = vec![0; n];
    let mut ancestors: Vec<BTreeSet<usize>> = vec![BTreeSet::new(); n];
    let mut max_depth = 0;

    for &u in &topo_order {
        let mut node_ancestors = BTreeSet::new();
        let mut node_depth = 0;
        for &(p, _) in &parents[u] {
            node_depth = node_depth.max(depth[p] + 1);
            node_ancestors.insert(p);
            node_ancestors.extend(ancestors[p].iter().copied());
        }
        if node_depth > MAX_DAG_DEPTH {
            return Err(format!(
                "prerequisite DAG depth {} exceeds limit {}",
                node_depth, MAX_DAG_DEPTH
            ));
        }
        if node_ancestors.len() > MAX_DAG_CLOSURE {
            return Err(format!(
                "prerequisite DAG transitive closure {} exceeds limit {}",
                node_ancestors.len(),
                MAX_DAG_CLOSURE
            ));
        }
        depth[u] = node_depth;
        max_depth = max_depth.max(node_depth);
        ancestors[u] = node_ancestors;
    }

    Ok((topo_order, parents, max_depth))
}

/// Evaluates all node shields and tiers in topological order (§9.1–§9.6, S-05, DEC-004).
pub fn evaluate_course_shields(input: &CourseShieldInput) -> Result<CourseShieldOutput, String> {
    let node_ids: Vec<String> = input.nodes.iter().map(|n| n.node_id.clone()).collect();
    let (topo_indices, parents, max_depth) = validate_prerequisite_dag(&node_ids, &input.edges)?;

    let is_dormant = (input.now_epoch_s - input.last_qualifying_study_at_epoch_s)
        >= DORMANCY_THRESHOLD_DAYS * 86_400;
    let new_player_bubble_active = is_new_player_bubble(
        input.now_epoch_s,
        input.new_player_bubble_until_epoch_s,
        input.eligible_transition_count,
    );
    let resume_grace_active = input
        .resume_grace_until_epoch_s
        .is_some_and(|until| input.now_epoch_s < until);

    let mut results: Vec<Option<NodeShieldEvaluation>> = vec![None; input.nodes.len()];
    let mut topological_order: Vec<String> = Vec::with_capacity(topo_indices.len());

    for &u_idx in &topo_indices {
        let node = &input.nodes[u_idx];
        topological_order.push(node.node_id.clone());

        let active_cards_count = node.active_cards.len();
        let mut unlocked_at = node.unlocked_at_epoch_s;

        let mut base_protections = Vec::new();
        if node.version_grace_active {
            base_protections.push(ProtectionKind::VersionGrace);
        }
        if new_player_bubble_active {
            base_protections.push(ProtectionKind::NewPlayerBubble);
        }
        if node
            .post_raid_cooldown_until_epoch_s
            .is_some_and(|until| input.now_epoch_s < until)
        {
            base_protections.push(ProtectionKind::PostRaidCooldown);
        }
        if resume_grace_active {
            base_protections.push(ProtectionKind::ResumeGrace);
        }
        if is_dormant {
            base_protections.push(ProtectionKind::Dormant);
        }

        if active_cards_count == 0 {
            let status = split_protection_predicates(NodeState::Inactive, None, &base_protections);
            results[u_idx] = Some(NodeShieldEvaluation {
                node_id: node.node_id.clone(),
                state: NodeState::Inactive,
                tier: None,
                e_u_ticks: None,
                i_u_ticks: None,
                s_u_ticks: None,
                is_brownout: None,
                unlocked_at_epoch_s: unlocked_at,
                scored_cards: 0,
                active_cards: 0,
                protection: status,
            });
            continue;
        }

        unlocked_at = unlocked_at.or(Some(input.now_epoch_s));

        // Compute R_c over C_scored = C_active \ C_graced; fail node closed to INVALID on non-finite values.
        let mut invalid_input = false;
        let mut scored_r: Vec<f32> = Vec::new();
        let mut reps_sum: u64 = 0;
        let mut reps_min: u32 = u32::MAX;
        let mut s_min: f32 = f32::INFINITY;

        for card in &node.active_cards {
            if !card.stability.is_finite() || card.stability < 0.0 || card.stability > 36_500.0 {
                invalid_input = true;
                break;
            }
            if let Some(d) = card.decay
                && (!d.is_finite() || !(0.05..=0.9).contains(&d))
            {
                invalid_input = true;
                break;
            }
            if card.version_grace_active {
                continue;
            }

            let r_c = if card.reps == 0 || card.stability == 0.0 {
                0.0
            } else {
                let Some(last_rev_s) = card.last_review_epoch_s else {
                    invalid_input = true;
                    break;
                };
                let elapsed_days =
                    ((input.now_epoch_s - last_rev_s).max(0) as f64 / 86_400.0) as f32;
                if elapsed_days > 36_500.0 {
                    invalid_input = true;
                    break;
                }
                match shield(&ShieldInput {
                    stability: card.stability,
                    elapsed_days,
                    decay: card.decay,
                }) {
                    Ok(r) if r.is_finite() && (0.0..=1.0).contains(&r) => r,
                    _ => {
                        invalid_input = true;
                        break;
                    }
                }
            };

            scored_r.push(r_c);
            reps_sum = reps_sum.saturating_add(u64::from(card.reps));
            reps_min = reps_min.min(card.reps);
            s_min = s_min.min(card.stability);
        }

        if invalid_input {
            let status = split_protection_predicates(NodeState::Invalid, None, &base_protections);
            results[u_idx] = Some(NodeShieldEvaluation {
                node_id: node.node_id.clone(),
                state: NodeState::Invalid,
                tier: None,
                e_u_ticks: None,
                i_u_ticks: None,
                s_u_ticks: None,
                is_brownout: None,
                unlocked_at_epoch_s: unlocked_at,
                scored_cards: 0,
                active_cards: active_cards_count,
                protection: status,
            });
            continue;
        }

        let scored_cards = scored_r.len();
        if scored_cards == 0 {
            let is_brownout = node.previous_brownout.unwrap_or(false);
            let mut protections = base_protections.clone();
            if unlocked_at.is_some_and(|u| input.now_epoch_s < u + NEW_NODE_GRACE_HOURS * 3600) {
                protections.push(ProtectionKind::NewNodeGrace);
            }
            let status = split_protection_predicates(NodeState::Active, Some(0), &protections);
            results[u_idx] = Some(NodeShieldEvaluation {
                node_id: node.node_id.clone(),
                state: NodeState::Active,
                tier: Some(0),
                e_u_ticks: Some(0),
                i_u_ticks: Some(0),
                s_u_ticks: Some(0),
                is_brownout: Some(is_brownout),
                unlocked_at_epoch_s: unlocked_at,
                scored_cards: 0,
                active_cards: active_cards_count,
                protection: status,
            });
            continue;
        }

        let i_u =
            (scored_r.iter().map(|&r| f64::from(r)).sum::<f64>() / scored_cards as f64) as f32;
        let s_u = if reps_min == 0 { 0.0 } else { s_min };

        let mut multiplier = 1.0_f64;
        for &(p_idx, w_pu) in &parents[u_idx] {
            let parent = results[p_idx]
                .as_ref()
                .expect("topological order guarantees parent evaluated");
            if parent.protection.propagation_exempt {
                continue;
            }
            let Some(e_p_ticks) = parent.e_u_ticks else {
                continue;
            };
            let penalty = (E_INITIAL - e_p_ticks).max(0) as f64 / (E_INITIAL as f64);
            multiplier *= (1.0 - f64::from(w_pu) * penalty).clamp(0.0, 1.0);
        }

        let e_u = ((f64::from(i_u)) * multiplier).clamp(0.0, 1.0) as f32;

        let (Some(i_u_ticks), Some(s_u_ticks), Some(e_u_ticks)) = (
            ticks_1e4_checked(i_u),
            ticks_1e4_checked(s_u),
            ticks_1e4_checked(e_u),
        ) else {
            let status = split_protection_predicates(NodeState::Invalid, None, &base_protections);
            results[u_idx] = Some(NodeShieldEvaluation {
                node_id: node.node_id.clone(),
                state: NodeState::Invalid,
                tier: None,
                e_u_ticks: None,
                i_u_ticks: None,
                s_u_ticks: None,
                is_brownout: None,
                unlocked_at_epoch_s: unlocked_at,
                scored_cards,
                active_cards: active_cards_count,
                protection: status,
            });
            continue;
        };

        let is_brownout = next_brownout_state(e_u_ticks, node.previous_brownout);
        let (state, tier) = compute_tier(
            active_cards_count,
            scored_cards,
            reps_sum,
            reps_min,
            s_u_ticks,
            e_u_ticks,
            is_brownout,
        );

        let mut protections = base_protections;
        if e_u_ticks < E_EXIT_BROWNOUT
            && unlocked_at.is_some_and(|u| input.now_epoch_s < u + NEW_NODE_GRACE_HOURS * 3600)
        {
            protections.push(ProtectionKind::NewNodeGrace);
        }

        let status = split_protection_predicates(state, tier, &protections);
        results[u_idx] = Some(NodeShieldEvaluation {
            node_id: node.node_id.clone(),
            state,
            tier,
            e_u_ticks: Some(e_u_ticks),
            i_u_ticks: Some(i_u_ticks),
            s_u_ticks: Some(s_u_ticks),
            is_brownout: Some(is_brownout),
            unlocked_at_epoch_s: unlocked_at,
            scored_cards,
            active_cards: active_cards_count,
            protection: status,
        });
    }

    Ok(CourseShieldOutput {
        now_epoch_s: input.now_epoch_s,
        is_dormant,
        new_player_bubble_active,
        resume_grace_active,
        topological_order,
        max_depth,
        nodes: results.into_iter().map(|item| item.unwrap()).collect(),
    })
}
