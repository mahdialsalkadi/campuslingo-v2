// G-6b window-table sub-gate: frozen fixture and compiled table must match.
// Install at crates/domain/tests/w_base_v1.rs; integer milliseconds only.
// Pinned counts: 18 base / 1 chain / 13 live base / 5 Deep-Analysis overrides.
// multipliers: m1_0 = 1/1; m1_25 = 5/4; m1_5 = 3/2.
use campuslingo_domain::windows::{self, WindowEntry, WindowError};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};

const MULTIPLIERS: [(&str, u32, u32); 3] = [("m1_0", 1, 1), ("m1_25", 5, 4), ("m1_5", 3, 2)];
const DEEP_PAIRS: [(&str, &str); 5] = [
    ("TRADEOFF", "BUDGET_CAP"),
    ("TRADEOFF", "PARETO_BALANCE"),
    ("CONTRAST", "CROSS_CALIBRATION"),
    ("TRAP", "FORENSIC_AUDIT"),
    ("TRAP", "COUNTEREXAMPLE_PROBE"),
];

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Fixture {
    schema_version: u32,
    window_table_version: u32,
    authority: String,
    formula: Formula,
    multipliers: Vec<Mult>,
    expected_counts: Counts,
    entries: Vec<Entry>,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Formula {
    w_eff_ms: String,
    tail_ms: String,
    w_cut_ms: String,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Mult {
    label: String,
    num: u32,
    den: u32,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Counts {
    base_pairs: usize,
    chain_sub_modes: usize,
    live_duel_eligible_base_pairs: usize,
    deep_analysis_pairs_with_live_drill_override: usize,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    kind: String,
    archetype: String,
    modifier: String,
    w_base_ms: u32,
    live_duel_eligible: bool,
    live_duel_variant_base_ms: Option<u32>,
    w_cut_ms: BTreeMap<String, u32>,
}

fn fixture() -> Fixture {
    serde_json::from_str(include_str!("../../../tests/fixtures/w_base_v1.json"))
        .expect("the complete frozen fixture must be committed and parse")
}

fn cut_ms(base_ms: u32, num: u32, den: u32) -> u32 {
    assert_eq!(base_ms * num % den, 0, "non-integral frozen window");
    let eff = base_ms * num / den;
    assert_eq!(eff % 5, 0, "non-integral frozen tail");
    eff + std::cmp::min(2000, eff / 5)
}

fn assert_frozen_contract(fx: &Fixture, compiled: &[WindowEntry]) {
    assert_eq!(fx.schema_version, 1);
    assert_eq!(fx.window_table_version, 1);
    assert_eq!(windows::WINDOW_TABLE_VERSION, 1);
    assert_eq!(
        fx.authority,
        "CL2-MASTER-UNIFIED-v1.8.1 §8.1 (restored ADR-003 §2.6 values)"
    );
    assert_eq!(fx.formula.w_eff_ms, "w_base_ms * M_num / M_den");
    assert_eq!(fx.formula.tail_ms, "min(2000, w_eff_ms / 5)");
    assert_eq!(fx.formula.w_cut_ms, "w_eff_ms + tail_ms");

    let multipliers: Vec<_> = fx
        .multipliers
        .iter()
        .map(|m| (m.label.as_str(), m.num, m.den))
        .collect();
    assert_eq!(multipliers.as_slice(), MULTIPLIERS);
    let labels: BTreeSet<_> = MULTIPLIERS.iter().map(|m| m.0).collect();

    let expected = &fx.expected_counts;
    assert_eq!(
        (
            expected.base_pairs,
            expected.chain_sub_modes,
            expected.live_duel_eligible_base_pairs,
            expected.deep_analysis_pairs_with_live_drill_override,
        ),
        (18, 1, 13, 5),
        "fixture header counts must remain pinned"
    );
    let mut seen = BTreeSet::new();
    for e in &fx.entries {
        assert!(
            seen.insert((e.archetype.as_str(), e.modifier.as_str())),
            "duplicate pair: {}/{}",
            e.archetype,
            e.modifier
        );
        assert!(matches!(e.kind.as_str(), "BASE_PAIR" | "CHAIN_SUB_MODE"));
    }
    let compiled_pairs: BTreeSet<_> = compiled.iter().map(|e| (e.archetype, e.modifier)).collect();
    assert_eq!(compiled.len(), compiled_pairs.len(), "compiled duplicates");
    assert_eq!(
        seen, compiled_pairs,
        "compiled/fixture pair sets must match in both directions"
    );
    assert_eq!(fx.entries.len(), 19);
    assert_eq!(
        fx.entries.iter().filter(|e| e.kind == "BASE_PAIR").count(),
        18
    );
    assert_eq!(
        fx.entries
            .iter()
            .filter(|e| e.kind == "CHAIN_SUB_MODE")
            .count(),
        1
    );
    assert_eq!(
        fx.entries
            .iter()
            .filter(|e| e.kind == "BASE_PAIR" && e.live_duel_eligible)
            .count(),
        13
    );
    let deep_pairs: BTreeSet<_> = fx
        .entries
        .iter()
        .filter(|e| e.live_duel_variant_base_ms.is_some())
        .map(|e| (e.archetype.as_str(), e.modifier.as_str()))
        .collect();
    assert_eq!(deep_pairs, DEEP_PAIRS.into_iter().collect());
    assert_eq!(deep_pairs.len(), 5);

    for e in &fx.entries {
        let pair = (e.archetype.as_str(), e.modifier.as_str());
        let row = compiled
            .iter()
            .find(|row| (row.archetype, row.modifier) == pair)
            .expect("compiled pair exists");
        assert_eq!(row.kind.label(), e.kind, "kind: {pair:?}");
        assert_eq!(row.w_base_ms, e.w_base_ms, "base window: {pair:?}");
        assert_eq!(
            row.live_duel_eligible, e.live_duel_eligible,
            "compiled live gating: {pair:?}"
        );
        assert_eq!(
            row.live_duel_variant_base_ms, e.live_duel_variant_base_ms,
            "compiled drill override: {pair:?}"
        );
        assert_eq!(
            windows::live_duel_eligible(&e.archetype, &e.modifier),
            e.live_duel_eligible,
            "live gating: {pair:?}"
        );
        assert_eq!(
            windows::live_duel_variant_base_ms(&e.archetype, &e.modifier),
            e.live_duel_variant_base_ms,
            "drill override: {pair:?}"
        );
        let row_labels: BTreeSet<_> = e.w_cut_ms.keys().map(String::as_str).collect();
        assert_eq!(row_labels, labels, "cutoff multiplier keys: {pair:?}");

        if deep_pairs.contains(&pair) {
            assert_eq!(e.kind, "BASE_PAIR");
            assert!(
                !e.live_duel_eligible,
                "deep base must not be live: {pair:?}"
            );
            assert!(matches!(e.w_base_ms, 11000 | 12000));
            assert_eq!(e.live_duel_variant_base_ms, Some(10000));
        } else {
            assert_eq!(e.live_duel_variant_base_ms, None);
            assert_eq!(e.live_duel_eligible, e.kind == "BASE_PAIR");
        }

        for (index, &(label, num, den)) in MULTIPLIERS.iter().enumerate() {
            assert_eq!(
                cut_ms(e.w_base_ms, num, den),
                e.w_cut_ms[label],
                "fixture self-consistency: {pair:?} @ {label}"
            );
            assert_eq!(
                windows::w_personal_cut_ms(&e.archetype, &e.modifier, num, den),
                Ok(e.w_cut_ms[label]),
                "compiled base cutoff: {pair:?} @ {label}"
            );
            if deep_pairs.contains(&pair) {
                let expected_live_cut = [12000, 14500, 17000][index];
                assert_eq!(cut_ms(10000, num, den), expected_live_cut);
                assert_eq!(
                    windows::w_live_duel_cut_ms(&e.archetype, &e.modifier, num, den),
                    Ok(expected_live_cut),
                    "live drill cutoff: {pair:?} @ {label}"
                );
            } else if e.live_duel_eligible {
                assert_eq!(
                    windows::w_live_duel_cut_ms(&e.archetype, &e.modifier, num, den),
                    Ok(e.w_cut_ms[label]),
                    "live base cutoff: {pair:?} @ {label}"
                );
            } else {
                assert_eq!(pair, ("AXIOM", "QUICK_BLITZ_CHAIN"));
                assert_eq!(
                    windows::w_live_duel_cut_ms(&e.archetype, &e.modifier, num, den),
                    Err(WindowError::NotPrimaryLiveDuelForm),
                    "bonus chain must not become an ordinary live primary"
                );
            }
        }
    }
}

#[test]
fn w_base_v1_matches_frozen_fixture() {
    assert_frozen_contract(&fixture(), windows::window_table());
}

#[test]
#[should_panic(expected = "duplicate pair")]
fn gate_rejects_duplicate_fixture_pairs() {
    let mut fx = fixture();
    fx.entries[1] = fx.entries[0].clone();
    assert_frozen_contract(&fx, windows::window_table());
}

#[test]
#[should_panic(expected = "cutoff multiplier keys")]
fn gate_rejects_extra_cutoff_multiplier_keys() {
    let mut fx = fixture();
    fx.entries[0].w_cut_ms.insert("m2_0".into(), 22000);
    assert_frozen_contract(&fx, windows::window_table());
}

#[test]
#[should_panic(expected = "compiled/fixture pair sets")]
fn gate_rejects_an_extra_compiled_pair() {
    let mut compiled = windows::window_table().to_vec();
    let mut extra = compiled[0];
    extra.modifier = "UNAPPROVED_EXTRA";
    compiled.push(extra);
    assert_frozen_contract(&fixture(), &compiled);
}

#[test]
#[should_panic(expected = "compiled live gating")]
fn gate_rejects_deep_base_wrongly_enabled_for_live_duels() {
    let mut compiled = windows::window_table().to_vec();
    compiled
        .iter_mut()
        .find(|e| e.modifier == "PARETO_BALANCE")
        .unwrap()
        .live_duel_eligible = true;
    assert_frozen_contract(&fixture(), &compiled);
}

#[test]
#[should_panic(expected = "compiled drill override")]
fn gate_rejects_missing_deep_live_override() {
    let mut compiled = windows::window_table().to_vec();
    compiled
        .iter_mut()
        .find(|e| e.modifier == "PARETO_BALANCE")
        .unwrap()
        .live_duel_variant_base_ms = None;
    assert_frozen_contract(&fixture(), &compiled);
}
