//! Frozen W_base v1 windows: integer milliseconds and explicit live-mode gates.
//!
//! Deep-Analysis base interactions remain async/solo-only. Their live duel
//! forms use the separate 10s drill override. QUICK_BLITZ_CHAIN is a sub-mode,
//! never an ordinary primary live card. No ambient clock or floating cutoff
//! arithmetic is used here.

pub const WINDOW_TABLE_VERSION: u32 = 1;
pub const MIN_STAGE3_BASE_WINDOW_MS: u32 = 8_000;
pub const MAX_STAGE3_BASE_WINDOW_MS: u32 = 15_000;
pub const AXIOM_PACK_MIN_CARDS: usize = 2;
pub const AXIOM_PACK_MAX_CARDS: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Accommodation {
    M100,
    M125,
    M150,
}

impl Accommodation {
    pub const fn num(self) -> u32 {
        match self {
            Self::M100 => 1,
            Self::M125 => 5,
            Self::M150 => 3,
        }
    }

    pub const fn den(self) -> u32 {
        match self {
            Self::M100 => 1,
            Self::M125 => 4,
            Self::M150 => 2,
        }
    }

    pub const fn from_ratio(num: u32, den: u32) -> Result<Self, WindowError> {
        match (num, den) {
            (1, 1) => Ok(Self::M100),
            (5, 4) => Ok(Self::M125),
            (3, 2) => Ok(Self::M150),
            _ => Err(WindowError::UnsupportedAccommodation),
        }
    }
}

impl TryFrom<f32> for Accommodation {
    type Error = WindowError;

    fn try_from(value: f32) -> Result<Self, Self::Error> {
        // DEC-010: all three SQL REAL values are exactly representable in f32.
        // Do not round an invalid stored multiplier into an allowed tier.
        match value {
            1.0 => Ok(Self::M100),
            1.25 => Ok(Self::M125),
            1.5 => Ok(Self::M150),
            _ => Err(WindowError::UnsupportedAccommodation),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowError {
    UnknownPair,
    UnsupportedAccommodation,
    NotPrimaryLiveDuelForm,
    NotPrimaryCard,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AxiomPackError {
    InvalidCount,
    EmptyCardId,
    DuplicateCardId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowKind {
    BasePair,
    ChainSubMode,
}

impl WindowKind {
    pub const fn label(self) -> &'static str {
        match self {
            Self::BasePair => "BASE_PAIR",
            Self::ChainSubMode => "CHAIN_SUB_MODE",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowEntry {
    pub kind: WindowKind,
    pub archetype: &'static str,
    pub modifier: &'static str,
    pub w_base_ms: u32,
    /// Eligibility of the original base interaction, not its live drill form.
    pub live_duel_eligible: bool,
    pub live_duel_variant_base_ms: Option<u32>,
}

const fn base(
    archetype: &'static str,
    modifier: &'static str,
    w_base_ms: u32,
    live_duel_eligible: bool,
    live_duel_variant_base_ms: Option<u32>,
) -> WindowEntry {
    WindowEntry {
        kind: WindowKind::BasePair,
        archetype,
        modifier,
        w_base_ms,
        live_duel_eligible,
        live_duel_variant_base_ms,
    }
}

const WINDOW_TABLE: [WindowEntry; 19] = [
    base("PIPELINE", "REVERSE_ENG", 10_000, true, None),
    base("PIPELINE", "BOTTLENECK_RELIEF", 10_000, true, None),
    base("PIPELINE", "FAULT_INJECTION", 9_000, true, None),
    base("TRADEOFF", "OVERLOAD_STRESS", 10_000, true, None),
    base("TRADEOFF", "BUDGET_CAP", 11_000, false, Some(10_000)),
    base("TRADEOFF", "PARETO_BALANCE", 12_000, false, Some(10_000)),
    base("SPATIAL", "BLIND_DIAGNOSIS", 10_000, true, None),
    base("SPATIAL", "SURGICAL_REPAIR", 10_000, true, None),
    base("SPATIAL", "CROSS_SECTION_SCAN", 9_000, true, None),
    base("CONTRAST", "STRESS_BENCHMARK", 10_000, true, None),
    base("CONTRAST", "BLIND_CHAMBER_ID", 9_000, true, None),
    base("CONTRAST", "CROSS_CALIBRATION", 11_000, false, Some(10_000)),
    base("TRAP", "FORENSIC_AUDIT", 12_000, false, Some(10_000)),
    base("TRAP", "COUNTEREXAMPLE_PROBE", 11_000, false, Some(10_000)),
    base("TRAP", "SAFEGUARD_PATCH", 10_000, true, None),
    base("AXIOM", "SPEED_BLITZ", 6_000, true, None),
    base("AXIOM", "COROLLARY_CHAIN", 8_000, true, None),
    base("AXIOM", "BOUNDARY_TEST", 8_000, true, None),
    WindowEntry {
        kind: WindowKind::ChainSubMode,
        archetype: "AXIOM",
        modifier: "QUICK_BLITZ_CHAIN",
        w_base_ms: 4_000,
        live_duel_eligible: false,
        live_duel_variant_base_ms: None,
    },
];

pub fn window_table() -> &'static [WindowEntry] {
    &WINDOW_TABLE
}

pub fn entry(archetype: &str, modifier: &str) -> Option<&'static WindowEntry> {
    WINDOW_TABLE
        .iter()
        .find(|row| row.archetype == archetype && row.modifier == modifier)
}

pub fn live_duel_eligible(archetype: &str, modifier: &str) -> bool {
    entry(archetype, modifier).is_some_and(|row| row.live_duel_eligible)
}

pub fn live_duel_variant_base_ms(archetype: &str, modifier: &str) -> Option<u32> {
    entry(archetype, modifier).and_then(|row| row.live_duel_variant_base_ms)
}

fn cut_ms(base_ms: u32, accommodation: Accommodation) -> u32 {
    let eff_ms = base_ms * accommodation.num() / accommodation.den();
    eff_ms + 2_000.min(eff_ms / 5)
}

/// Base-form cutoff (including the async/solo Deep-Analysis base windows).
/// Bonus chain scheduling must additionally enforce the full remaining-slice
/// window, distinct card, energy, and one-bonus-per-round gates from §10.4.
pub fn w_personal_cut_ms(
    archetype: &str,
    modifier: &str,
    num: u32,
    den: u32,
) -> Result<u32, WindowError> {
    let accommodation = Accommodation::from_ratio(num, den)?;
    let row = entry(archetype, modifier).ok_or(WindowError::UnknownPair)?;
    Ok(cut_ms(row.w_base_ms, accommodation))
}

/// Primary live-duel cutoff; never uses an 11–12s base interaction in a pod.
/// QUICK_BLITZ_CHAIN must use the separately gated bonus path, not this API.
pub fn w_live_duel_cut_ms(
    archetype: &str,
    modifier: &str,
    num: u32,
    den: u32,
) -> Result<u32, WindowError> {
    let accommodation = Accommodation::from_ratio(num, den)?;
    let base_ms = w_base_for_live_duel_ms(archetype, modifier)?;
    Ok(cut_ms(base_ms, accommodation))
}

/// Base millisecond window for a primary live-duel card (always <= 10_000 ms;
/// pods fall back to their 6_000 ms live-duel `SPEED_BLITZ` variant).
pub fn w_base_for_live_duel_ms(archetype: &str, modifier: &str) -> Result<u32, WindowError> {
    let row = entry(archetype, modifier).ok_or(WindowError::UnknownPair)?;
    if row.live_duel_eligible {
        Ok(row.w_base_ms)
    } else {
        row.live_duel_variant_base_ms
            .ok_or(WindowError::NotPrimaryLiveDuelForm)
    }
}

/// Personal effective mastery window `W_personal_eff = W_base * M_accommodation`.
pub fn w_personal_eff_ms(base_ms: u32, num: u32, den: u32) -> Result<u32, WindowError> {
    let accommodation = Accommodation::from_ratio(num, den)?;
    Ok(base_ms * accommodation.num() / accommodation.den())
}

/// Personal cutoff `W_cut = W_eff + min(2000, 0.20 * W_eff)` from an already
/// resolved base window in milliseconds.
pub fn w_personal_cut_from_base_ms(base_ms: u32, num: u32, den: u32) -> Result<u32, WindowError> {
    let accommodation = Accommodation::from_ratio(num, den)?;
    Ok(cut_ms(base_ms, accommodation))
}

/// Un-truncated 4.0s `QUICK_BLITZ_CHAIN` bonus slot personal effective window (`S-11`).
pub fn w_quick_blitz_chain_eff_ms(num: u32, den: u32) -> Result<u32, WindowError> {
    let row = entry("AXIOM", "QUICK_BLITZ_CHAIN").ok_or(WindowError::UnknownPair)?;
    w_personal_eff_ms(row.w_base_ms, num, den)
}

/// Un-truncated 4.0s `QUICK_BLITZ_CHAIN` bonus slot cutoff (`W_eff + min(2000, 0.20 * W_eff)`).
pub fn w_quick_blitz_chain_cut_ms(num: u32, den: u32) -> Result<u32, WindowError> {
    let row = entry("AXIOM", "QUICK_BLITZ_CHAIN").ok_or(WindowError::UnknownPair)?;
    let accommodation = Accommodation::from_ratio(num, den)?;
    Ok(cut_ms(row.w_base_ms, accommodation))
}

/// Dynamic tail Sudden-Death cutoff (`SPEED_BLITZ` 6000ms scaled by accommodation and
/// bounded by `remaining_ms`).
pub fn w_sudden_death_cut_ms(num: u32, den: u32, remaining_ms: u32) -> Result<u32, WindowError> {
    let accommodation = Accommodation::from_ratio(num, den)?;
    let blitz_cut = cut_ms(6_000, accommodation);
    Ok(blitz_cut.min(remaining_ms.max(1)))
}

/// Returns `(W_round_cut_ms, W_cut_at_M1_ms)` per `D-02-A` and `DEC-007`:
/// `W_round_cut = max(W_cut(player_a), W_cut(player_b))`,
/// `W_cut_at_M1 = max(W_cut(base_a, 1/1), W_cut(base_b, 1/1))`.
pub fn w_round_cut_and_m1_ms(
    base_a_ms: u32,
    num_a: u32,
    den_a: u32,
    base_b_ms: u32,
    num_b: u32,
    den_b: u32,
) -> Result<(u32, u32), WindowError> {
    let cut_a = w_personal_cut_from_base_ms(base_a_ms, num_a, den_a)?;
    let cut_b = w_personal_cut_from_base_ms(base_b_ms, num_b, den_b)?;
    let cut_a_m1 = w_personal_cut_from_base_ms(base_a_ms, 1, 1)?;
    let cut_b_m1 = w_personal_cut_from_base_ms(base_b_ms, 1, 1)?;
    Ok((cut_a.max(cut_b), cut_a_m1.max(cut_b_m1)))
}

/// Deterministic provisional energy cost in `1..=6` (`S-20`) derived from the
/// frozen base window table for primary cards. Final release balance remains open
/// in `balance-v1.md`.
pub fn card_elixir_cost(archetype: &str, modifier: &str) -> Result<u8, WindowError> {
    let row = entry(archetype, modifier).ok_or(WindowError::UnknownPair)?;
    if row.kind != WindowKind::BasePair {
        return Err(WindowError::NotPrimaryCard);
    }
    let cost = match row.w_base_ms {
        ..=6_000 => 1,
        6_001..=8_000 => 2,
        8_001..=9_000 => 3,
        9_001..=10_000 => 4,
        10_001..=11_000 => 5,
        _ => 6,
    };
    Ok(cost)
}

/// DEC-002 / S-11: Stage-3 card compaction requires an 8000..=15000 ms primary
/// base window. Sub-8s frozen forms (`SPEED_BLITZ` 6s, `QUICK_BLITZ_CHAIN` 4s)
/// are ineligible rather than stretched silently.
pub fn is_stage3_drill_eligible(archetype: &str, modifier: &str) -> Result<bool, WindowError> {
    let row = entry(archetype, modifier).ok_or(WindowError::UnknownPair)?;
    Ok(row.kind == WindowKind::BasePair
        && (MIN_STAGE3_BASE_WINDOW_MS..=MAX_STAGE3_BASE_WINDOW_MS).contains(&row.w_base_ms))
}

/// DEC-003: `AXIOM_PACK` compacts 2..=4 fast drills into one turn while
/// preserving distinct `card_id`s so per-card FSRS-6 state stays independent.
pub fn validate_axiom_pack_card_ids(card_ids: &[&str]) -> Result<(), AxiomPackError> {
    if !(AXIOM_PACK_MIN_CARDS..=AXIOM_PACK_MAX_CARDS).contains(&card_ids.len()) {
        return Err(AxiomPackError::InvalidCount);
    }
    let mut seen = std::collections::BTreeSet::new();
    for &card_id in card_ids {
        if card_id.trim().is_empty() {
            return Err(AxiomPackError::EmptyCardId);
        }
        if !seen.insert(card_id) {
            return Err(AxiomPackError::DuplicateCardId);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn real_storage_adapter_accepts_only_exact_approved_values() {
        for (stored, expected) in [
            (1.0, Accommodation::M100),
            (1.25, Accommodation::M125),
            (1.5, Accommodation::M150),
        ] {
            assert_eq!(Accommodation::try_from(stored), Ok(expected));
        }
        for invalid in [
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
            0.0,
            -1.0,
            1.1,
            f32::from_bits(1.25_f32.to_bits() - 1),
            f32::from_bits(1.25_f32.to_bits() + 1),
        ] {
            assert_eq!(
                Accommodation::try_from(invalid),
                Err(WindowError::UnsupportedAccommodation)
            );
        }
    }

    #[test]
    fn unknown_pairs_and_noncanonical_ratios_fail_closed() {
        assert!(!live_duel_eligible("TRADEOFF", "UNKNOWN"));
        assert_eq!(live_duel_variant_base_ms("UNKNOWN", "UNKNOWN"), None);
        assert_eq!(
            w_personal_cut_ms("TRADEOFF", "UNKNOWN", 1, 1),
            Err(WindowError::UnknownPair)
        );
        for (num, den) in [(0, 1), (1, 0), (2, 2), (5, 3), (u32::MAX, 1)] {
            assert_eq!(
                w_personal_cut_ms("AXIOM", "SPEED_BLITZ", num, den),
                Err(WindowError::UnsupportedAccommodation)
            );
            assert_eq!(
                w_live_duel_cut_ms("AXIOM", "SPEED_BLITZ", num, den),
                Err(WindowError::UnsupportedAccommodation)
            );
        }
        assert_eq!(
            w_live_duel_cut_ms("AXIOM", "QUICK_BLITZ_CHAIN", 1, 1),
            Err(WindowError::NotPrimaryLiveDuelForm)
        );
    }

    #[test]
    fn card_elixir_cost_stage3_eligibility_and_axiom_pack_enforce_invariants() {
        let mut base_pairs = 0;
        for row in window_table() {
            match row.kind {
                WindowKind::BasePair => {
                    base_pairs += 1;
                    let cost = card_elixir_cost(row.archetype, row.modifier).unwrap();
                    assert!((1..=6).contains(&cost));
                    let eligible = is_stage3_drill_eligible(row.archetype, row.modifier).unwrap();
                    if row.modifier == "SPEED_BLITZ" {
                        assert!(!eligible);
                    } else {
                        assert!(eligible);
                    }
                }
                WindowKind::ChainSubMode => {
                    assert_eq!(
                        card_elixir_cost(row.archetype, row.modifier),
                        Err(WindowError::NotPrimaryCard)
                    );
                    assert_eq!(
                        is_stage3_drill_eligible(row.archetype, row.modifier),
                        Ok(false)
                    );
                }
            }
        }
        assert_eq!(base_pairs, 18);
        assert_eq!(
            card_elixir_cost("UNKNOWN", "NOPE"),
            Err(WindowError::UnknownPair)
        );
        assert_eq!(
            is_stage3_drill_eligible("UNKNOWN", "NOPE"),
            Err(WindowError::UnknownPair)
        );

        assert_eq!(validate_axiom_pack_card_ids(&["c1", "c2"]), Ok(()));
        assert_eq!(
            validate_axiom_pack_card_ids(&["c1", "c2", "c3", "c4"]),
            Ok(())
        );
        assert_eq!(
            validate_axiom_pack_card_ids(&["c1"]),
            Err(AxiomPackError::InvalidCount)
        );
        assert_eq!(
            validate_axiom_pack_card_ids(&["c1", "c2", "c3", "c4", "c5"]),
            Err(AxiomPackError::InvalidCount)
        );
        assert_eq!(
            validate_axiom_pack_card_ids(&["c1", " "]),
            Err(AxiomPackError::EmptyCardId)
        );
        assert_eq!(
            validate_axiom_pack_card_ids(&["c1", "c1"]),
            Err(AxiomPackError::DuplicateCardId)
        );
    }
}
