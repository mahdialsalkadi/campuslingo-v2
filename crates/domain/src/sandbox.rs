use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use crate::contracts::{Expression, MathOp, ParameterSpec, SandboxSpec};

#[derive(Debug, Clone, Serialize)]
pub struct ValidationResult {
    pub valid: bool,
    pub reason: String,
    pub states_checked: usize,
    pub witness: Option<BTreeMap<String, f64>>,
    pub proof_domain: &'static str,
}

fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 48
        && value.as_bytes()[0].is_ascii_lowercase()
        && value
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_')
}

impl ParameterSpec {
    pub fn values(&self) -> Result<Vec<f64>, String> {
        let numbers = [self.minimum, self.maximum, self.step, self.initial];
        if !identifier(&self.name)
            || self.label.is_empty()
            || self.label.len() > 120
            || numbers
                .iter()
                .any(|x| !x.is_finite() || x.abs() > 1_000_000.0)
            || self.step <= 0.0
            || self.maximum <= self.minimum
        {
            return Err("invalid finite parameter domain".into());
        }
        let steps = (self.maximum - self.minimum) / self.step;
        let offset = (self.initial - self.minimum) / self.step;
        if !(1.0..=100.0 + 1e-9).contains(&steps)
            || (steps - steps.round()).abs() > 1e-9
            || (offset - offset.round()).abs() > 1e-9
            || offset < -1e-9
            || offset > steps + 1e-9
        {
            return Err("parameter must use an integral grid with at most 101 points".into());
        }
        Ok((0..=steps.round() as usize)
            .map(|i| {
                if i == steps.round() as usize {
                    self.maximum
                } else {
                    self.minimum + i as f64 * self.step
                }
            })
            .collect())
    }
}

fn check_expression(
    expression: &Expression,
    names: &BTreeSet<&str>,
    depth: usize,
    nodes: &mut usize,
) -> Result<(), String> {
    *nodes += 1;
    if depth > 8 || *nodes > 64 {
        return Err("expression exceeds depth/node budget".into());
    }
    match expression {
        Expression::Constant { value } if !value.is_finite() || value.abs() > 1_000_000.0 => {
            Err("invalid constant".into())
        }
        Expression::Variable { name } if !names.contains(name.as_str()) => {
            Err("undefined variable".into())
        }
        Expression::Operation { op, args } => {
            if !(2..=4).contains(&args.len())
                || (matches!(op, MathOp::Subtract | MathOp::Divide) && args.len() != 2)
            {
                return Err("invalid operation arity".into());
            }
            for arg in args {
                check_expression(arg, names, depth + 1, nodes)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn evaluate(expression: &Expression, values: &BTreeMap<String, f64>) -> Result<f64, String> {
    let value = match expression {
        Expression::Constant { value } => *value,
        Expression::Variable { name } => *values.get(name).ok_or("undefined variable")?,
        Expression::Operation { op, args } => {
            let args: Vec<f64> = args
                .iter()
                .map(|arg| evaluate(arg, values))
                .collect::<Result<_, _>>()?;
            match op {
                MathOp::Add => args.iter().sum(),
                MathOp::Multiply => args.iter().product(),
                MathOp::Subtract => args[0] - args[1],
                MathOp::Divide => {
                    if args[1].abs() <= 1e-12 {
                        return Err("division by zero or near-zero denominator".into());
                    }
                    args[0] / args[1]
                }
                MathOp::Min => args.into_iter().fold(f64::INFINITY, f64::min),
                MathOp::Max => args.into_iter().fold(f64::NEG_INFINITY, f64::max),
            }
        }
    };
    if !value.is_finite() || value.abs() > 1e12 {
        return Err("non-finite or excessive intermediate output".into());
    }
    Ok(value)
}

pub fn verify(spec: &SandboxSpec) -> Result<ValidationResult, String> {
    if !spec.archetype.modifiers().contains(&spec.modifier.as_str()) {
        return Err("incompatible modifier".into());
    }
    if !(1..=4).contains(&spec.parameters.len())
        || !(1..=8).contains(&spec.targets.len())
        || !(1000..=30000).contains(&spec.time_limit_ms)
    {
        return Err("sandbox count or timing budget exceeded".into());
    }
    let names: BTreeSet<&str> = spec.parameters.iter().map(|p| p.name.as_str()).collect();
    if names.len() != spec.parameters.len() {
        return Err("duplicate parameter".into());
    }
    let grids: Vec<Vec<f64>> = spec
        .parameters
        .iter()
        .map(ParameterSpec::values)
        .collect::<Result<_, _>>()?;
    let count: usize = grids.iter().map(Vec::len).product();
    if count > 4096 {
        return Err("sandbox exceeds 4096-state budget".into());
    }
    let mut target_names = BTreeSet::new();
    for target in &spec.targets {
        if !identifier(&target.name)
            || !target_names.insert(&target.name)
            || !target.lower.is_finite()
            || !target.upper.is_finite()
            || target.lower.abs() > 1e6
            || target.upper.abs() > 1e6
            || target.upper < target.lower
        {
            return Err("invalid target".into());
        }
        check_expression(&target.expression, &names, 1, &mut 0)?;
    }
    let mut result = ValidationResult {
        valid: false,
        reason: "no reachable target in the declared grid".into(),
        states_checked: 0,
        witness: None,
        proof_domain: "exhaustive_declared_discrete_grid",
    };
    for mut index in 0..count {
        let mut values = BTreeMap::new();
        for (parameter, grid) in spec.parameters.iter().zip(&grids) {
            values.insert(parameter.name.clone(), grid[index % grid.len()]);
            index /= grid.len();
        }
        result.states_checked += 1;
        let mut success = true;
        for target in &spec.targets {
            match evaluate(&target.expression, &values) {
                Ok(output) => success &= output >= target.lower && output <= target.upper,
                Err(reason) => {
                    result.reason = reason;
                    result.witness = None;
                    return Ok(result);
                }
            }
        }
        if success && result.witness.is_none() {
            result.witness = Some(values);
        }
    }
    result.valid = result.witness.is_some();
    if result.valid {
        result.reason = "all states safe; target reachable".into();
    }
    Ok(result)
}
