use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

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
    if input.energy_milli > 10_000
        || !(1..=6).contains(&input.card_cost)
        || input.lane > 1
        || input.remaining_ms == 0
        || input.remaining_ms > 90_000
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
