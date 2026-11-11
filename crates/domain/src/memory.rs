use fsrs::{FSRS, FSRS6_DEFAULT_DECAY, FSRS6_DEFAULT_PARAMETERS, MemoryState, ModelVersion};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MemoryInput {
    pub stability: f32,
    pub difficulty: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReviewInput {
    pub memory: Option<MemoryInput>,
    pub elapsed_days: u32,
    pub rating: u8,
    pub desired_retention: f32,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReviewOutput {
    pub algorithm: &'static str,
    pub stability: f32,
    pub difficulty: f32,
    pub interval_days: f32,
    pub scheduled_days: u32,
    pub retrievability_before: f32,
}

pub fn fsrs6() -> FSRS {
    let model = FSRS::new(&FSRS6_DEFAULT_PARAMETERS).expect("pinned FSRS-6 parameters are valid");
    debug_assert_eq!(model.version(), ModelVersion::Fsrs6);
    model
}

pub fn review(input: &ReviewInput) -> Result<ReviewOutput, String> {
    if !(1..=4).contains(&input.rating)
        || input.elapsed_days > 36500
        || !input.desired_retention.is_finite()
        || !(0.7..=0.99).contains(&input.desired_retention)
    {
        return Err("invalid rating, elapsed time, or retention".into());
    }
    let state = input.memory.map(|m| MemoryState {
        stability: m.stability,
        difficulty: m.difficulty,
        stability_fast: m.stability,
    });
    if state.is_some_and(|m| {
        !m.stability.is_finite()
            || !(0.001..=36500.0).contains(&m.stability)
            || !m.difficulty.is_finite()
            || !(1.0..=10.0).contains(&m.difficulty)
    }) {
        return Err("invalid memory state".into());
    }
    let model = fsrs6();
    // Upstream FSRS-6 rounds fractional days. Scheduling therefore accepts
    // complete elapsed days from trusted history; shields are continuous below.
    let next = model
        .next_states(state, input.desired_retention, input.elapsed_days)
        .map_err(|e| e.to_string())?;
    let selected = match input.rating {
        1 => next.again,
        2 => next.hard,
        3 => next.good,
        _ => next.easy,
    };
    Ok(ReviewOutput {
        algorithm: "FSRS-6",
        stability: selected.memory.stability,
        difficulty: selected.memory.difficulty,
        interval_days: selected.interval,
        scheduled_days: selected.interval.round().clamp(1.0, 36500.0) as u32,
        retrievability_before: state.map_or(0.0, |m| {
            model.current_retrievability(m, input.elapsed_days as f32)
        }),
    })
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ShieldInput {
    pub stability: f32,
    pub elapsed_days: f32,
}

pub fn shield(input: &ShieldInput) -> Result<f32, String> {
    if !input.stability.is_finite()
        || !(0.0..=36500.0).contains(&input.stability)
        || !input.elapsed_days.is_finite()
        || !(0.0..=36500.0).contains(&input.elapsed_days)
    {
        return Err("invalid continuous shield inputs".into());
    }
    if input.stability == 0.0 {
        return Ok(0.0);
    }
    // This free upstream function does not round time, unlike the method on FSRS.
    Ok(fsrs::current_retrievability(
        MemoryState {
            stability: input.stability,
            difficulty: 5.0,
            stability_fast: input.stability,
        },
        input.elapsed_days,
        FSRS6_DEFAULT_DECAY,
    ))
}
