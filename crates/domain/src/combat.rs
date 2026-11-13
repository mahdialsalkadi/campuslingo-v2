use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::windows::{
    w_personal_cut_from_base_ms, w_personal_eff_ms, w_quick_blitz_chain_cut_ms,
    w_quick_blitz_chain_eff_ms, w_round_cut_and_m1_ms, w_sudden_death_cut_ms,
};

pub const PROTOCOL_VERSION: i16 = 2;
pub const RATING_MAP_VERSION: i16 = 2;
pub const SCORE_VERSION: i16 = 2;

pub const TOO_FAST_THRESHOLD_MS: u32 = 150;
pub const MAX_VOIDS_PER_MATCH: u8 = 2;
pub const MAX_TOO_FAST_PER_MATCH: u8 = 2;

pub const MATCH_DURATION_MS: u32 = 90_000;
pub const DOUBLE_ENERGY_AT_MS: u32 = 60_000;
pub const SUDDEN_DEATH_DURATION_MS: u32 = 30_000;
pub const SELECTION_DEADLINE_MS: u32 = 1_500;
pub const COUNTER_STRIKE_WINDOW_MS: u32 = 6_000;

pub const MAX_ENERGY_MILLI: u32 = 10_000;
pub const START_ENERGY_MILLI: u32 = 5_000;
pub const BASE_ENERGY_REGEN_PER_SEC_MILLI: u32 = 400;
pub const DOUBLE_ENERGY_REGEN_PER_SEC_MILLI: u32 = 800;

pub const TOURNAMENT_COMBAT_LEVEL: u8 = 11;
pub const LANE_TOWER_MAX_HP: i32 = 1_800;
pub const LANE_SHIELD_MAX: i32 = 300;
pub const CORE_TOWER_MAX_HP: i32 = 3_000;

pub const SAME_DAY_MIN_GAP_SECS: i64 = 1_800;
pub const CARD_DAILY_REWARD_CAP: i32 = 3;
pub const COURSE_DAILY_REWARD_CAP: i32 = 60;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DeploymentInput {
    pub energy_milli: u32,
    pub card_cost: u8,
    pub lane: u8,
    pub sequence: u32,
    pub last_sequence: Option<u32>,
    pub remaining_ms: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct DeploymentOutput {
    pub energy_after_milli: u32,
    pub accepted_sequence: u32,
}

pub fn validate_deployment(input: &DeploymentInput) -> Result<DeploymentOutput, String> {
    if input.energy_milli > MAX_ENERGY_MILLI
        || !(1..=6).contains(&input.card_cost)
        || input.lane > 1
        || input.remaining_ms == 0
        || input.remaining_ms > MATCH_DURATION_MS
    {
        return Err("invalid energy, cost, lane, or match clock".into());
    }
    if input
        .last_sequence
        .is_some_and(|last| input.sequence <= last)
    {
        return Err("duplicate or out-of-order action".into());
    }
    let cost = u32::from(input.card_cost) * 1000;
    let remaining = input
        .energy_milli
        .checked_sub(cost)
        .ok_or("insufficient energy")?;
    Ok(DeploymentOutput {
        energy_after_milli: remaining,
        accepted_sequence: input.sequence,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AttemptTermination {
    Submitted,
    VoidDelivery,
    VoidTransportLoss,
    Timeout,
    AbandonedAfterReveal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RatingReason {
    VoidDelivery,
    VoidTransportLoss,
    VoidTransportLossCapExceeded,
    TooFast,
    TooFastCapExceeded,
    Timeout,
    AbandonedAfterReveal,
    Incorrect,
    HintUsed,
    MultipleSandboxSubmits,
    PartialCredit,
    SlowAbovePersonalEff,
    FastCleanFullCredit,
    StandardGood,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RatingMapV2Input {
    pub termination: AttemptTermination,
    pub is_correct: bool,
    pub partial_credit: bool,
    pub hint_used: bool,
    pub server_sandbox_submits: u32,
    pub raw_elapsed_ms: u32,
    pub l_net_ms: u32,
    pub w_personal_eff_ms: u32,
    pub w_round_cut_ms: u32,
    pub prior_void_transport_count: u8,
    pub prior_too_fast_count: u8,
    #[serde(default)]
    pub is_live_duel: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RatingOutcome {
    pub rating_map_version: i16,
    pub fsrs_rating: Option<u8>,
    pub is_void: bool,
    pub reason: RatingReason,
    pub triggers_overcharge: bool,
    pub triggers_counter_strike: bool,
    pub voids_combat_effect: bool,
}

/// Computes server-authoritative `L_net` for Easy/Hard/Overcharge checks while leaving
/// `< 150ms` TOO_FAST and `> W_round_cut` timeout checks on raw monotonic `elapsed_ms` (`DEC-006`).
pub fn compute_l_net_ms(raw_elapsed_ms: u32, rtt_compensation_ms: u32) -> u32 {
    raw_elapsed_ms.saturating_sub(rtt_compensation_ms.min(500))
}

/// Evaluates `D-02-A` ordered precedence under `rating_map_version = 2`.
pub fn evaluate_rating_map_v2(input: &RatingMapV2Input) -> Result<RatingOutcome, String> {
    if input.w_personal_eff_ms == 0 || input.w_round_cut_ms == 0 {
        return Err("personal effective and round cutoff windows must be positive".into());
    }

    // Case 0: transport/delivery voids & sub-150ms too-fast checks (with 2/match cap escalation to Again = 1).
    match input.termination {
        AttemptTermination::VoidDelivery => {
            return Ok(RatingOutcome {
                rating_map_version: RATING_MAP_VERSION,
                fsrs_rating: None,
                is_void: true,
                reason: RatingReason::VoidDelivery,
                triggers_overcharge: false,
                triggers_counter_strike: false,
                voids_combat_effect: true,
            });
        }
        AttemptTermination::VoidTransportLoss => {
            if input.prior_void_transport_count >= MAX_VOIDS_PER_MATCH {
                return Ok(RatingOutcome {
                    rating_map_version: RATING_MAP_VERSION,
                    fsrs_rating: Some(1),
                    is_void: false,
                    reason: RatingReason::VoidTransportLossCapExceeded,
                    triggers_overcharge: false,
                    triggers_counter_strike: input.is_live_duel,
                    voids_combat_effect: true,
                });
            }
            return Ok(RatingOutcome {
                rating_map_version: RATING_MAP_VERSION,
                fsrs_rating: None,
                is_void: true,
                reason: RatingReason::VoidTransportLoss,
                triggers_overcharge: false,
                triggers_counter_strike: false,
                voids_combat_effect: true,
            });
        }
        AttemptTermination::Submitted if input.raw_elapsed_ms < TOO_FAST_THRESHOLD_MS => {
            if input.prior_too_fast_count >= MAX_TOO_FAST_PER_MATCH {
                return Ok(RatingOutcome {
                    rating_map_version: RATING_MAP_VERSION,
                    fsrs_rating: Some(1),
                    is_void: false,
                    reason: RatingReason::TooFastCapExceeded,
                    triggers_overcharge: false,
                    triggers_counter_strike: input.is_live_duel,
                    voids_combat_effect: true,
                });
            }
            return Ok(RatingOutcome {
                rating_map_version: RATING_MAP_VERSION,
                fsrs_rating: None,
                is_void: true,
                reason: RatingReason::TooFast,
                triggers_overcharge: false,
                triggers_counter_strike: false,
                voids_combat_effect: true,
            });
        }
        _ => {}
    }

    // Case 1: Again = 1 on TIMEOUT | ABANDONED_AFTER_REVEAL | !is_correct.
    if input.termination == AttemptTermination::Timeout
        || input.raw_elapsed_ms > input.w_round_cut_ms
    {
        return Ok(RatingOutcome {
            rating_map_version: RATING_MAP_VERSION,
            fsrs_rating: Some(1),
            is_void: false,
            reason: RatingReason::Timeout,
            triggers_overcharge: false,
            triggers_counter_strike: input.is_live_duel,
            voids_combat_effect: false,
        });
    }
    if input.termination == AttemptTermination::AbandonedAfterReveal {
        return Ok(RatingOutcome {
            rating_map_version: RATING_MAP_VERSION,
            fsrs_rating: Some(1),
            is_void: false,
            reason: RatingReason::AbandonedAfterReveal,
            triggers_overcharge: false,
            triggers_counter_strike: input.is_live_duel,
            voids_combat_effect: false,
        });
    }
    if !input.is_correct {
        return Ok(RatingOutcome {
            rating_map_version: RATING_MAP_VERSION,
            fsrs_rating: Some(1),
            is_void: false,
            reason: RatingReason::Incorrect,
            triggers_overcharge: false,
            triggers_counter_strike: input.is_live_duel,
            voids_combat_effect: false,
        });
    }

    // Case 2: Hard = 2 on hint_used | server_sandbox_submits > 1 | partial_credit | L_net > W_personal_eff.
    if input.hint_used {
        return Ok(RatingOutcome {
            rating_map_version: RATING_MAP_VERSION,
            fsrs_rating: Some(2),
            is_void: false,
            reason: RatingReason::HintUsed,
            triggers_overcharge: false,
            triggers_counter_strike: false,
            voids_combat_effect: false,
        });
    }
    if input.server_sandbox_submits > 1 {
        return Ok(RatingOutcome {
            rating_map_version: RATING_MAP_VERSION,
            fsrs_rating: Some(2),
            is_void: false,
            reason: RatingReason::MultipleSandboxSubmits,
            triggers_overcharge: false,
            triggers_counter_strike: false,
            voids_combat_effect: false,
        });
    }
    if input.partial_credit {
        return Ok(RatingOutcome {
            rating_map_version: RATING_MAP_VERSION,
            fsrs_rating: Some(2),
            is_void: false,
            reason: RatingReason::PartialCredit,
            triggers_overcharge: false,
            triggers_counter_strike: false,
            voids_combat_effect: false,
        });
    }
    if input.l_net_ms > input.w_personal_eff_ms {
        return Ok(RatingOutcome {
            rating_map_version: RATING_MAP_VERSION,
            fsrs_rating: Some(2),
            is_void: false,
            reason: RatingReason::SlowAbovePersonalEff,
            triggers_overcharge: false,
            triggers_counter_strike: false,
            voids_combat_effect: false,
        });
    }

    // Case 3: Easy = 4 on first-submit, no-hint, full credit with L_net <= 0.50 * W_personal_eff.
    if input.server_sandbox_submits <= 1 && input.l_net_ms <= input.w_personal_eff_ms / 2 {
        return Ok(RatingOutcome {
            rating_map_version: RATING_MAP_VERSION,
            fsrs_rating: Some(4),
            is_void: false,
            reason: RatingReason::FastCleanFullCredit,
            triggers_overcharge: input.is_live_duel,
            triggers_counter_strike: false,
            voids_combat_effect: false,
        });
    }

    // Case 4: Good = 3.
    Ok(RatingOutcome {
        rating_map_version: RATING_MAP_VERSION,
        fsrs_rating: Some(3),
        is_void: false,
        reason: RatingReason::StandardGood,
        triggers_overcharge: false,
        triggers_counter_strike: false,
        voids_combat_effect: false,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum InteractionSource {
    Match,
    StudySession,
    SpeedDrill,
    Practice,
    TutorialSandbox,
    Spectator,
    LiveDuel,
    GhostDuel,
    BotDuel,
    AsyncRaid,
    RevengeRaid,
    SoloBoss,
    CoopBoss,
    League,
    ClanWar,
    SoloRaid,
    PvpDuel,
    AsyncBaseRaid,
    RevengeDefense,
}

impl InteractionSource {
    /// Returns basis-point mode multiplier for `score_v2` (`15_000` for revenge raids).
    pub fn mode_bps(self) -> i64 {
        match self {
            Self::RevengeRaid | Self::RevengeDefense => 15_000,
            Self::Spectator | Self::TutorialSandbox => 0,
            _ => 10_000,
        }
    }

    /// True iff this interaction source is eligible for competitive mastery/economy rewards.
    pub fn is_competitive_reward_mode(self) -> bool {
        !matches!(
            self,
            Self::Practice | Self::TutorialSandbox | Self::Spectator
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EligibilityInput {
    pub is_void: bool,
    pub same_scope_duplicate: bool,
    pub same_day_gap_secs: Option<i64>,
    pub card_daily_rewarded_count: i32,
    pub course_daily_rewarded_count: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct EligibilityOutcome {
    pub fsrs_step_eligible: bool,
    pub reward_eligible: bool,
    pub blocker_reason: Option<String>,
}

pub fn evaluate_eligibility(input: &EligibilityInput) -> EligibilityOutcome {
    if input.is_void {
        return EligibilityOutcome {
            fsrs_step_eligible: false,
            reward_eligible: false,
            blocker_reason: Some("VOID_ATTEMPT".into()),
        };
    }
    if input.same_scope_duplicate {
        return EligibilityOutcome {
            fsrs_step_eligible: false,
            reward_eligible: false,
            blocker_reason: Some("SCOPE_DUPLICATE".into()),
        };
    }
    if input
        .same_day_gap_secs
        .is_some_and(|gap| gap < SAME_DAY_MIN_GAP_SECS)
    {
        return EligibilityOutcome {
            fsrs_step_eligible: false,
            reward_eligible: false,
            blocker_reason: Some("SAME_DAY_MIN_GAP".into()),
        };
    }
    if input.card_daily_rewarded_count >= CARD_DAILY_REWARD_CAP {
        return EligibilityOutcome {
            fsrs_step_eligible: true,
            reward_eligible: false,
            blocker_reason: Some("CARD_DAILY_CAP".into()),
        };
    }
    if input.course_daily_rewarded_count >= COURSE_DAILY_REWARD_CAP {
        return EligibilityOutcome {
            fsrs_step_eligible: true,
            reward_eligible: false,
            blocker_reason: Some("COURSE_DAILY_CAP".into()),
        };
    }
    EligibilityOutcome {
        fsrs_step_eligible: true,
        reward_eligible: true,
        blocker_reason: None,
    }
}

/// Exact integer round-half-to-even (`i128` -> `i64`), avoiding binary float precision drift.
pub fn round_ties_even_points(num: i128, den: i128) -> Result<i64, String> {
    if den <= 0 {
        return Err("denominator must be positive".into());
    }
    let q = num / den;
    let rem = num.abs() % den;
    let two_rem = rem
        .checked_mul(2)
        .ok_or_else(|| "overflow in ties-even comparison".to_string())?;
    let rounded = match two_rem.cmp(&den) {
        std::cmp::Ordering::Greater => {
            if num >= 0 {
                q + 1
            } else {
                q - 1
            }
        }
        std::cmp::Ordering::Equal => {
            if q % 2 != 0 {
                if num >= 0 { q + 1 } else { q - 1 }
            } else {
                q
            }
        }
        std::cmp::Ordering::Less => q,
    };
    i64::try_from(rounded).map_err(|_| "rounded points overflow i64".into())
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ScoreV2Input {
    pub base_points: i64,
    pub rating: u8,
    pub recall_bps: i64,
    pub mastery_bps: i64,
    pub mode_bps: i64,
    pub recovery_x1e4: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ScoreV2Output {
    pub score_version: i16,
    pub points: i64,
    pub raw_multiplier_bps: i64,
    pub effective_multiplier_bps: i64,
    pub clamped_at_3x: bool,
}

fn rating_weight_bps(rating: u8) -> Result<i128, String> {
    match rating {
        1 => Ok(2_500),
        2 => Ok(7_500),
        3 => Ok(10_000),
        4 => Ok(12_500),
        _ => Err("rating must be in 1..=4".into()),
    }
}

/// Computes `score_v2` in exact `i128` arithmetic (`DEC-008`: keeps accepting `rating ∈ 1..=4`).
/// Multipliers are clamped at `3.0x` (`30_000 bps` / `30 * den` per 10-point unit) prior to ties-even rounding.
pub fn compute_score_v2(input: &ScoreV2Input) -> Result<ScoreV2Output, String> {
    if !(0..=10_000).contains(&input.base_points) {
        return Err("base_points out of bounds".into());
    }
    for (name, val) in [
        ("recall_bps", input.recall_bps),
        ("mastery_bps", input.mastery_bps),
        ("mode_bps", input.mode_bps),
        ("recovery_x1e4", input.recovery_x1e4),
    ] {
        if !(0..=50_000).contains(&val) {
            return Err(format!("{name} out of bounds"));
        }
    }

    let rating_bps = rating_weight_bps(input.rating)?;
    let mult_num = rating_bps
        * i128::from(input.recall_bps)
        * i128::from(input.mastery_bps)
        * i128::from(input.mode_bps)
        * i128::from(input.recovery_x1e4);
    let den: i128 = 10_000_i128.pow(5);
    let raw_mult_bps_num = mult_num / 10_000_i128.pow(4);
    let raw_multiplier_bps = round_ties_even_points(raw_mult_bps_num, 1)?;
    let cap_num = 3 * den;
    let clamped_at_3x = mult_num > cap_num;
    let effective_num = mult_num.min(cap_num);
    let effective_multiplier_bps = round_ties_even_points(effective_num, 10_000_i128.pow(4))?;

    // Equivalent to clamping `base_points * mult` at `3 * base_points * den` (`30 * den` when base_points = 10)
    // prior to `round_ties_even_points`.
    let points_num = i128::from(input.base_points) * effective_num;
    let points = round_ties_even_points(points_num, den)?;

    Ok(ScoreV2Output {
        score_version: SCORE_VERSION,
        points,
        raw_multiplier_bps,
        effective_multiplier_bps,
        clamped_at_3x,
    })
}

/// Exact `i128` ties-even basis-point rounding helper.
pub fn round_comp_bps(num: i128, den: i128) -> Result<i64, String> {
    round_ties_even_points(num * 10_000, den)
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DuelContributionInput {
    pub card_points: i64,
    pub player_won: bool,
    pub is_draw: bool,
    pub tower_margin_bps: i64,
}

/// Exact integer contribution of a duel outcome into competitive rating / mastery ledger (`score_v2`).
pub fn compute_duel_contribution(input: &DuelContributionInput) -> Result<i64, String> {
    if !(0..=100_000).contains(&input.card_points)
        || !(0..=20_000).contains(&input.tower_margin_bps)
    {
        return Err("duel contribution inputs out of bounds".into());
    }
    let outcome_weight_bps: i128 = if input.is_draw {
        10_000
    } else if input.player_won {
        12_500 + i128::from(input.tower_margin_bps) / 4
    } else {
        7_500
    };
    round_ties_even_points(i128::from(input.card_points) * outcome_weight_bps, 10_000)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DuelSide {
    PlayerA,
    PlayerB,
}

impl DuelSide {
    pub fn opponent(self) -> Self {
        match self {
            DuelSide::PlayerA => DuelSide::PlayerB,
            DuelSide::PlayerB => DuelSide::PlayerA,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum OpponentKind {
    Human,
    Ghost,
    Bot,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DuelCardSlot {
    pub card_id: String,
    pub archetype: String,
    pub modifier: String,
    pub card_cost: u8,
    pub base_window_ms: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FighterArenaState {
    pub side: DuelSide,
    pub energy_milli: u32,
    pub tower_hp: [i32; 2],
    pub tower_shield: [i32; 2],
    pub core_hp: i32,
    pub accommodation_num: u32,
    pub accommodation_den: u32,
    pub combat_level: u8,
    pub void_transport_count: u8,
    pub too_fast_count: u8,
    pub overcharge_count: u16,
    pub counter_strike_until_ms: Option<u32>,
    pub quick_blitz_chain_available: bool,
    pub last_sequence: u32,
    pub hand: Vec<DuelCardSlot>,
    pub selected_card_id: Option<String>,
    pub selection_started_at_ms: Option<u32>,
}

impl FighterArenaState {
    pub fn new(
        side: DuelSide,
        accommodation_num: u32,
        accommodation_den: u32,
        hand: Vec<DuelCardSlot>,
    ) -> Self {
        Self {
            side,
            energy_milli: START_ENERGY_MILLI,
            tower_hp: [LANE_TOWER_MAX_HP, LANE_TOWER_MAX_HP],
            tower_shield: [LANE_SHIELD_MAX, LANE_SHIELD_MAX],
            core_hp: CORE_TOWER_MAX_HP,
            accommodation_num,
            accommodation_den,
            combat_level: TOURNAMENT_COMBAT_LEVEL,
            void_transport_count: 0,
            too_fast_count: 0,
            overcharge_count: 0,
            counter_strike_until_ms: None,
            quick_blitz_chain_available: false,
            last_sequence: 0,
            hand,
            selected_card_id: None,
            selection_started_at_ms: None,
        }
    }

    pub fn total_hp(&self) -> i32 {
        self.core_hp + self.tower_hp[0] + self.tower_hp[1]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DuelMatchStatus {
    InProgress,
    SuddenDeath,
    Completed,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DuelArenaState {
    pub protocol_version: i16,
    pub match_clock_ms: u32,
    pub sudden_death_elapsed_ms: u32,
    pub status: DuelMatchStatus,
    pub winner: Option<DuelSide>,
    pub player_a: FighterArenaState,
    pub player_b: FighterArenaState,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CombatStrikeInput {
    pub attacker: DuelSide,
    pub sequence: u32,
    pub lane: u8,
    pub card_id: String,
    pub use_quick_blitz_chain: bool,
    pub termination: AttemptTermination,
    pub is_correct: bool,
    pub partial_credit: bool,
    pub hint_used: bool,
    pub server_sandbox_submits: u32,
    pub raw_elapsed_ms: u32,
    pub rtt_compensation_ms: u32,
    pub opponent_card_base_ms: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CombatStrikeResult {
    pub accepted_sequence: u32,
    pub rating: RatingOutcome,
    pub damage_to_shield: i32,
    pub damage_to_tower: i32,
    pub damage_to_core: i32,
    pub overcharged: bool,
    pub counter_strike_consumed: bool,
    pub fizzled: bool,
    pub w_personal_eff_ms: u32,
    pub w_round_cut_ms: u32,
    pub w_cut_at_m1_ms: u32,
    pub winner: Option<DuelSide>,
}

impl DuelArenaState {
    pub fn new(player_a: FighterArenaState, player_b: FighterArenaState) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            match_clock_ms: 0,
            sudden_death_elapsed_ms: 0,
            status: DuelMatchStatus::InProgress,
            winner: None,
            player_a,
            player_b,
        }
    }

    pub fn is_double_energy(&self) -> bool {
        self.match_clock_ms >= DOUBLE_ENERGY_AT_MS || self.status == DuelMatchStatus::SuddenDeath
    }

    pub fn remaining_ms(&self) -> u32 {
        match self.status {
            DuelMatchStatus::InProgress => MATCH_DURATION_MS.saturating_sub(self.match_clock_ms),
            DuelMatchStatus::SuddenDeath => {
                SUDDEN_DEATH_DURATION_MS.saturating_sub(self.sudden_death_elapsed_ms)
            }
            DuelMatchStatus::Completed => 0,
        }
    }

    pub fn fighter(&self, side: DuelSide) -> &FighterArenaState {
        match side {
            DuelSide::PlayerA => &self.player_a,
            DuelSide::PlayerB => &self.player_b,
        }
    }

    pub fn fighter_mut(&mut self, side: DuelSide) -> &mut FighterArenaState {
        match side {
            DuelSide::PlayerA => &mut self.player_a,
            DuelSide::PlayerB => &mut self.player_b,
        }
    }

    /// Advances the match clock by `delta_ms`, regenerating energy (2x in final 30s and sudden death),
    /// expiring counter-strike windows, enforcing the `DEC-001` `1500ms` auto-pick selection deadline,
    /// and transitioning to Sudden Death or Completed at `90_000ms` / end of Sudden Death.
    pub fn advance_clock(&mut self, delta_ms: u32) -> Result<(), String> {
        if self.status == DuelMatchStatus::Completed {
            return Ok(());
        }
        let mut remaining_delta = delta_ms;
        while remaining_delta > 0 && self.status != DuelMatchStatus::Completed {
            let step = if self.status == DuelMatchStatus::InProgress
                && self.match_clock_ms < DOUBLE_ENERGY_AT_MS
            {
                remaining_delta.min(DOUBLE_ENERGY_AT_MS - self.match_clock_ms)
            } else if self.status == DuelMatchStatus::InProgress {
                remaining_delta.min(MATCH_DURATION_MS - self.match_clock_ms)
            } else {
                remaining_delta.min(SUDDEN_DEATH_DURATION_MS - self.sudden_death_elapsed_ms)
            };
            let regen_rate = if self.is_double_energy() {
                DOUBLE_ENERGY_REGEN_PER_SEC_MILLI
            } else {
                BASE_ENERGY_REGEN_PER_SEC_MILLI
            };
            let gain = regen_rate * step / 1_000;
            self.player_a.energy_milli = (self.player_a.energy_milli + gain).min(MAX_ENERGY_MILLI);
            self.player_b.energy_milli = (self.player_b.energy_milli + gain).min(MAX_ENERGY_MILLI);

            if self.status == DuelMatchStatus::InProgress {
                self.match_clock_ms += step;
            } else {
                self.sudden_death_elapsed_ms += step;
            }
            let total_t = self.total_elapsed_ms();
            for side in [DuelSide::PlayerA, DuelSide::PlayerB] {
                let f = self.fighter_mut(side);
                if f.counter_strike_until_ms
                    .is_some_and(|until| total_t > until)
                {
                    f.counter_strike_until_ms = None;
                }
                if f.selected_card_id.is_none()
                    && f.selection_started_at_ms
                        .is_some_and(|start| total_t.saturating_sub(start) >= SELECTION_DEADLINE_MS)
                    && let Some(pick) = auto_pick_lexicographically_smallest_card(&f.hand)
                {
                    f.selected_card_id = Some(pick.card_id.clone());
                }
            }

            if self.status == DuelMatchStatus::InProgress
                && self.match_clock_ms >= MATCH_DURATION_MS
            {
                let hp_a = self.player_a.total_hp();
                let hp_b = self.player_b.total_hp();
                if hp_a == hp_b {
                    self.status = DuelMatchStatus::SuddenDeath;
                } else {
                    self.status = DuelMatchStatus::Completed;
                    self.winner = Some(if hp_a > hp_b {
                        DuelSide::PlayerA
                    } else {
                        DuelSide::PlayerB
                    });
                }
            } else if self.status == DuelMatchStatus::SuddenDeath
                && self.sudden_death_elapsed_ms >= SUDDEN_DEATH_DURATION_MS
            {
                let hp_a = self.player_a.total_hp();
                let hp_b = self.player_b.total_hp();
                self.status = DuelMatchStatus::Completed;
                self.winner = if hp_a > hp_b {
                    Some(DuelSide::PlayerA)
                } else if hp_b > hp_a {
                    Some(DuelSide::PlayerB)
                } else {
                    None
                };
            }
            remaining_delta -= step;
        }
        Ok(())
    }

    pub fn total_elapsed_ms(&self) -> u32 {
        self.match_clock_ms + self.sudden_death_elapsed_ms
    }

    /// Resolves a player's submitted duel card strike cleanly through `rating_map_version = 2`
    /// and updates energy, shields, towers, Overcharge / Shield-Pierce, Counter-Strike, and fizzle caps.
    pub fn resolve_strike(
        &mut self,
        input: &CombatStrikeInput,
    ) -> Result<CombatStrikeResult, String> {
        if self.status == DuelMatchStatus::Completed {
            return Err("match already completed".into());
        }
        if input.lane > 1 {
            return Err("invalid lane index".into());
        }

        let attacker_snapshot = self.fighter(input.attacker).clone();
        let defender_snapshot = self.fighter(input.attacker.opponent()).clone();
        if input.sequence <= attacker_snapshot.last_sequence {
            return Err("duplicate or out-of-order combat sequence".into());
        }

        let card = attacker_snapshot
            .hand
            .iter()
            .find(|c| c.card_id == input.card_id)
            .ok_or_else(|| "card is not in attacker's active hand".to_string())?
            .clone();
        let cost_milli = u32::from(card.card_cost) * 1_000;
        if attacker_snapshot.energy_milli < cost_milli {
            return Err("insufficient energy for card".into());
        }

        if input.use_quick_blitz_chain && !attacker_snapshot.quick_blitz_chain_available {
            return Err("QUICK_BLITZ_CHAIN bonus slot is not armed".into());
        }

        let (w_personal_eff_ms, w_round_cut_ms, w_cut_at_m1_ms) = if input.use_quick_blitz_chain {
            let eff = w_quick_blitz_chain_eff_ms(
                attacker_snapshot.accommodation_num,
                attacker_snapshot.accommodation_den,
            )
            .map_err(|e| format!("{e:?}"))?;
            let cut_a = w_quick_blitz_chain_cut_ms(
                attacker_snapshot.accommodation_num,
                attacker_snapshot.accommodation_den,
            )
            .map_err(|e| format!("{e:?}"))?;
            let cut_b = w_quick_blitz_chain_cut_ms(
                defender_snapshot.accommodation_num,
                defender_snapshot.accommodation_den,
            )
            .map_err(|e| format!("{e:?}"))?;
            let cut_m1 = w_quick_blitz_chain_cut_ms(1, 1).map_err(|e| format!("{e:?}"))?;
            (eff, cut_a.max(cut_b), cut_m1)
        } else if self.status == DuelMatchStatus::SuddenDeath {
            let rem = self.remaining_ms();
            let base_eff = w_personal_eff_ms(
                6_000,
                attacker_snapshot.accommodation_num,
                attacker_snapshot.accommodation_den,
            )
            .map_err(|e| format!("{e:?}"))?
            .min(rem.max(1));
            let cut_a = w_sudden_death_cut_ms(
                attacker_snapshot.accommodation_num,
                attacker_snapshot.accommodation_den,
                rem,
            )
            .map_err(|e| format!("{e:?}"))?;
            let cut_b = w_sudden_death_cut_ms(
                defender_snapshot.accommodation_num,
                defender_snapshot.accommodation_den,
                rem,
            )
            .map_err(|e| format!("{e:?}"))?;
            let cut_m1 = w_sudden_death_cut_ms(1, 1, rem).map_err(|e| format!("{e:?}"))?;
            (base_eff, cut_a.max(cut_b), cut_m1)
        } else {
            let eff = w_personal_eff_ms(
                card.base_window_ms,
                attacker_snapshot.accommodation_num,
                attacker_snapshot.accommodation_den,
            )
            .map_err(|e| format!("{e:?}"))?;
            let opp_base = input.opponent_card_base_ms.unwrap_or(card.base_window_ms);
            let (round_cut, cut_m1) = w_round_cut_and_m1_ms(
                card.base_window_ms,
                attacker_snapshot.accommodation_num,
                attacker_snapshot.accommodation_den,
                opp_base,
                defender_snapshot.accommodation_num,
                defender_snapshot.accommodation_den,
            )
            .map_err(|e| format!("{e:?}"))?;
            // Dynamic tail when match clock has fewer ms left than nominal cutoff uses SPEED_BLITZ cap.
            let tail_rem = self.remaining_ms();
            let bounded_cut = if tail_rem > 0
                && tail_rem < round_cut
                && self.match_clock_ms >= DOUBLE_ENERGY_AT_MS
            {
                w_sudden_death_cut_ms(
                    attacker_snapshot.accommodation_num,
                    attacker_snapshot.accommodation_den,
                    tail_rem,
                )
                .map_err(|e| format!("{e:?}"))?
                .max(
                    w_personal_cut_from_base_ms(6_000, 1, 1)
                        .unwrap_or(7_200)
                        .min(tail_rem),
                )
            } else {
                round_cut
            };
            (eff, bounded_cut, cut_m1)
        };

        let l_net_ms = compute_l_net_ms(input.raw_elapsed_ms, input.rtt_compensation_ms);
        let rating = evaluate_rating_map_v2(&RatingMapV2Input {
            termination: input.termination,
            is_correct: input.is_correct,
            partial_credit: input.partial_credit,
            hint_used: input.hint_used,
            server_sandbox_submits: input.server_sandbox_submits,
            raw_elapsed_ms: input.raw_elapsed_ms,
            l_net_ms,
            w_personal_eff_ms,
            w_round_cut_ms,
            prior_void_transport_count: attacker_snapshot.void_transport_count,
            prior_too_fast_count: attacker_snapshot.too_fast_count,
            is_live_duel: true,
        })?;

        let total_t = self.total_elapsed_ms();
        let lane = usize::from(input.lane);

        // Deduct card energy (50% refunded on VOID_TRANSPORT_LOSS fizzle).
        {
            let attacker = self.fighter_mut(input.attacker);
            attacker.last_sequence = input.sequence;
            attacker.selected_card_id = None;
            attacker.selection_started_at_ms = None;
            if input.use_quick_blitz_chain {
                attacker.quick_blitz_chain_available = false;
            }
            if rating.reason == RatingReason::VoidTransportLoss {
                attacker.void_transport_count = attacker.void_transport_count.saturating_add(1);
                attacker.energy_milli = attacker.energy_milli.saturating_sub(cost_milli / 2);
            } else {
                attacker.energy_milli = attacker.energy_milli.saturating_sub(cost_milli);
                if rating.reason == RatingReason::TooFast {
                    attacker.too_fast_count = attacker.too_fast_count.saturating_add(1);
                }
            }
        }

        if rating.voids_combat_effect {
            return Ok(CombatStrikeResult {
                accepted_sequence: input.sequence,
                rating,
                damage_to_shield: 0,
                damage_to_tower: 0,
                damage_to_core: 0,
                overcharged: false,
                counter_strike_consumed: false,
                fizzled: true,
                w_personal_eff_ms,
                w_round_cut_ms,
                w_cut_at_m1_ms,
                winner: self.winner,
            });
        }

        if rating.fsrs_rating == Some(1) {
            if rating.triggers_counter_strike {
                self.fighter_mut(input.attacker.opponent())
                    .counter_strike_until_ms = Some(total_t + COUNTER_STRIKE_WINDOW_MS);
            }
            return Ok(CombatStrikeResult {
                accepted_sequence: input.sequence,
                rating,
                damage_to_shield: 0,
                damage_to_tower: 0,
                damage_to_core: 0,
                overcharged: false,
                counter_strike_consumed: false,
                fizzled: false,
                w_personal_eff_ms,
                w_round_cut_ms,
                w_cut_at_m1_ms,
                winner: self.winner,
            });
        }

        // Compute normalized combat damage (tournament level 11 = uniform).
        let base_damage = 120 * i32::from(card.card_cost);
        let rating_scaled = match rating.fsrs_rating {
            Some(4) => base_damage * 2,     // 2.0x Overcharge
            Some(2) => base_damage * 3 / 4, // Hard reduced strike
            _ => base_damage,
        };
        let mut final_damage = if input.use_quick_blitz_chain {
            rating_scaled * 5 / 4
        } else {
            rating_scaled
        };

        let mut counter_strike_consumed = false;
        {
            let attacker = self.fighter_mut(input.attacker);
            if attacker
                .counter_strike_until_ms
                .is_some_and(|until| total_t <= until)
            {
                final_damage = final_damage * 3 / 2;
                attacker.counter_strike_until_ms = None;
                counter_strike_consumed = true;
            }
            if rating.triggers_overcharge {
                attacker.overcharge_count = attacker.overcharge_count.saturating_add(1);
                if attacker.overcharge_count >= 2 && !input.use_quick_blitz_chain {
                    attacker.quick_blitz_chain_available = true;
                }
            }
        }

        let overcharged = rating.triggers_overcharge;
        let mut dmg_left = final_damage;
        let mut damage_to_shield = 0;
        let mut damage_to_tower = 0;
        let mut damage_to_core = 0;
        let is_sudden_death = self.status == DuelMatchStatus::SuddenDeath;

        {
            let defender = self.fighter_mut(input.attacker.opponent());
            if !overcharged && defender.tower_shield[lane] > 0 && defender.tower_hp[lane] > 0 {
                let absorbed = dmg_left.min(defender.tower_shield[lane]);
                defender.tower_shield[lane] -= absorbed;
                dmg_left -= absorbed;
                damage_to_shield = absorbed;
            }
            if dmg_left > 0 && defender.tower_hp[lane] > 0 {
                let tower_hit = dmg_left.min(defender.tower_hp[lane]);
                defender.tower_hp[lane] -= tower_hit;
                dmg_left -= tower_hit;
                damage_to_tower = tower_hit;
            }
            if dmg_left > 0 && defender.core_hp > 0 {
                let core_hit = dmg_left.min(defender.core_hp);
                defender.core_hp -= core_hit;
                damage_to_core = core_hit;
            }
            if defender.core_hp <= 0
                || (is_sudden_death && damage_to_tower > 0 && defender.tower_hp[lane] == 0)
            {
                self.status = DuelMatchStatus::Completed;
                self.winner = Some(input.attacker);
            }
        }

        Ok(CombatStrikeResult {
            accepted_sequence: input.sequence,
            rating,
            damage_to_shield,
            damage_to_tower,
            damage_to_core,
            overcharged,
            counter_strike_consumed,
            fizzled: false,
            w_personal_eff_ms,
            w_round_cut_ms,
            w_cut_at_m1_ms,
            winner: self.winner,
        })
    }
}

/// `DEC-001`: deterministic fallback pick when a player passes the 1500ms selection deadline.
pub fn auto_pick_lexicographically_smallest_card(hand: &[DuelCardSlot]) -> Option<&DuelCardSlot> {
    hand.iter().min_by(|a, b| a.card_id.cmp(&b.card_id))
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GhostReplayFrame {
    pub recorded_elapsed_ms: u32,
    pub preferred_lane: u8,
    pub preferred_card_cost: u8,
    pub historical_is_correct: bool,
    pub historical_response_ms: u32,
    pub historical_hint_used: bool,
    pub historical_damage_ignored: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AdaptedAiStrikeDecision {
    pub chosen_lane: u8,
    pub chosen_card_id: String,
    pub is_correct: bool,
    pub simulated_raw_elapsed_ms: u32,
    pub hint_used: bool,
    pub adapted_from_state: bool,
}

/// `F-03` honest Ghost adaptation: NEVER blindly applies `historical_damage_ignored` from a replay.
/// Instead, inspects the *current* match state (`arena`, `ghost_side`), checks affordable cards,
/// retargets lanes if a tower is already destroyed, and lets `DuelArenaState::resolve_strike` compute
/// legal current-state damage against the live opponent's shields/towers/round cutoff.
pub fn adapt_ghost_action(
    arena: &DuelArenaState,
    ghost_side: DuelSide,
    frame: &GhostReplayFrame,
) -> Option<AdaptedAiStrikeDecision> {
    if arena.status == DuelMatchStatus::Completed {
        return None;
    }
    let ghost = arena.fighter(ghost_side);
    let opponent = arena.fighter(ghost_side.opponent());

    let preferred_card = ghost
        .hand
        .iter()
        .filter(|c| u32::from(c.card_cost) * 1_000 <= ghost.energy_milli)
        .min_by_key(|c| c.card_cost.abs_diff(frame.preferred_card_cost))?;

    let pref_lane = usize::from(frame.preferred_lane.min(1));
    let chosen_lane = if opponent.tower_hp[pref_lane] > 0 {
        pref_lane as u8
    } else if opponent.tower_hp[1 - pref_lane] > 0 {
        (1 - pref_lane) as u8
    } else {
        pref_lane as u8
    };

    // Counter-strike opportunities or fallen-lane adaptation cannot cheat impossibly fast (< 150ms).
    let clamped_response = frame
        .historical_response_ms
        .max(TOO_FAST_THRESHOLD_MS + 250);
    Some(AdaptedAiStrikeDecision {
        chosen_lane,
        chosen_card_id: preferred_card.card_id.clone(),
        is_correct: frame.historical_is_correct,
        simulated_raw_elapsed_ms: clamped_response,
        hint_used: frame.historical_hint_used,
        adapted_from_state: true,
    })
}

/// Honest Bot fallback controller: deterministically selects an affordable card & healthiest living
/// target lane from current match state without inspecting hidden answers.
pub fn decide_bot_action(
    arena: &DuelArenaState,
    bot_side: DuelSide,
    step_seed: u64,
) -> Option<AdaptedAiStrikeDecision> {
    if arena.status == DuelMatchStatus::Completed {
        return None;
    }
    let bot = arena.fighter(bot_side);
    let opponent = arena.fighter(bot_side.opponent());

    let mut affordable: Vec<&DuelCardSlot> = bot
        .hand
        .iter()
        .filter(|c| u32::from(c.card_cost) * 1_000 <= bot.energy_milli)
        .collect();
    if affordable.is_empty() {
        return None;
    }
    affordable.sort_by(|a, b| a.card_id.cmp(&b.card_id));
    let pick_idx = (step_seed as usize) % affordable.len();
    let card = affordable[pick_idx];

    let chosen_lane = if opponent.tower_hp[0] <= 0 && opponent.tower_hp[1] > 0 {
        1
    } else if (opponent.tower_hp[1] <= 0 && opponent.tower_hp[0] > 0)
        || (opponent.tower_hp[0] > 0 && opponent.tower_hp[0] <= opponent.tower_hp[1])
    {
        0
    } else if opponent.tower_hp[1] > 0 {
        1
    } else {
        0
    };

    // ~80% accuracy deterministic bot response curve.
    let is_correct = (step_seed % 5) != 4;
    let speed_factor_pct = 35 + ((step_seed % 55) as u32);
    let simulated_raw_elapsed_ms =
        (card.base_window_ms * speed_factor_pct / 100).max(TOO_FAST_THRESHOLD_MS + 400);

    Some(AdaptedAiStrikeDecision {
        chosen_lane,
        chosen_card_id: card.card_id.clone(),
        is_correct,
        simulated_raw_elapsed_ms,
        hint_used: false,
        adapted_from_state: true,
    })
}
