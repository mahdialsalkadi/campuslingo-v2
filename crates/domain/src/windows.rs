//! Frozen W_base v1 windows: integer milliseconds and explicit live-mode gates.
//!
//! Deep-Analysis base interactions remain async/solo-only. Their live duel
//! forms use the separate 10s drill override. QUICK_BLITZ_CHAIN is a sub-mode,
//! never an ordinary primary live card. No ambient clock or floating cutoff
//! arithmetic is used here.

pub const WINDOW_TABLE_VERSION: u32 = 1;

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
    let row = entry(archetype, modifier).ok_or(WindowError::UnknownPair)?;
    let base_ms = if row.live_duel_eligible {
        row.w_base_ms
    } else {
        row.live_duel_variant_base_ms
            .ok_or(WindowError::NotPrimaryLiveDuelForm)?
    };
    Ok(cut_ms(base_ms, accommodation))
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
}
