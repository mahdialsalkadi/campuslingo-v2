//! Held-out temperature scaling and Expected Calibration Error (`P-03`).
//!
//! Cutoff thresholds for routing (`18 (Archetype, Modifier)` pairs or task gates)
//! may only be enforced after per-task, per-language temperature `T_q` has been
//! fitted on held-out validation chunks and verified to achieve `ECE < 0.05`.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const MAX_CALIBRATED_ECE: f64 = 0.05;
pub const DEFAULT_ECE_BINS: usize = 10;
pub const DEFAULT_MIN_HOLDOUT_SAMPLES: usize = 10;
pub const MIN_TEMPERATURE: f64 = 0.05;
pub const MAX_TEMPERATURE: f64 = 10.0;

#[derive(Debug, Error, PartialEq)]
pub enum CalibrationError {
    #[error("calibration task name must not be empty")]
    EmptyTask,
    #[error("held-out calibration set requires at least {required} samples (got {actual})")]
    InsufficientSamples { required: usize, actual: usize },
    #[error("invalid logit vector or class label")]
    InvalidSample,
    #[error("invalid calibration temperature or confidence threshold")]
    InvalidParameter,
    #[error(
        "router cutoff cannot be enforced before verified calibration (task={task}, lang={language:?})"
    )]
    UncalibratedTask {
        task: String,
        language: CalibrationLanguage,
    },
    #[error("fitted ECE {ece:.4} meets or exceeds the {limit:.2} P-03 gate")]
    ExceedsEceLimit { ece: f64, limit: f64 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CalibrationLanguage {
    En,
    Ar,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CalibrationKey {
    pub task: String,
    pub language: CalibrationLanguage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalibrationSample {
    pub logits: Vec<f64>,
    pub label: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FittedCalibration {
    pub task: String,
    pub language: CalibrationLanguage,
    pub temperature: f64,
    pub sample_count: usize,
    pub num_classes: usize,
    pub nll_before: f64,
    pub nll_after: f64,
    pub ece_before: f64,
    pub ece_after: f64,
    pub accuracy: f64,
    pub verified: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalibratedPrediction {
    pub predicted_class: usize,
    pub confidence: f64,
    pub passed_threshold: bool,
    pub probabilities: Vec<f64>,
    pub temperature_used: f64,
    pub ece_after: f64,
}

pub fn softmax_with_temperature(
    logits: &[f64],
    temperature: f64,
) -> Result<Vec<f64>, CalibrationError> {
    if logits.len() < 2 || !temperature.is_finite() || temperature <= 0.0 {
        return Err(CalibrationError::InvalidParameter);
    }
    let mut scaled = Vec::with_capacity(logits.len());
    let mut max_val = f64::NEG_INFINITY;
    for &z in logits {
        if !z.is_finite() {
            return Err(CalibrationError::InvalidSample);
        }
        let s = z / temperature;
        if s > max_val {
            max_val = s;
        }
        scaled.push(s);
    }
    let mut sum_exp = 0.0_f64;
    for val in &mut scaled {
        let e = (*val - max_val).exp();
        *val = e;
        sum_exp += e;
    }
    if !sum_exp.is_finite() || sum_exp <= 0.0 {
        return Err(CalibrationError::InvalidSample);
    }
    for val in &mut scaled {
        *val /= sum_exp;
    }
    Ok(scaled)
}

pub fn expected_calibration_error(
    confidences: &[f64],
    correctness: &[bool],
    num_bins: usize,
) -> Result<f64, CalibrationError> {
    if confidences.is_empty() || confidences.len() != correctness.len() || num_bins == 0 {
        return Err(CalibrationError::InvalidParameter);
    }
    let mut bin_counts = vec![0usize; num_bins];
    let mut bin_conf_sum = vec![0.0_f64; num_bins];
    let mut bin_acc_sum = vec![0.0_f64; num_bins];
    for (&conf, &correct) in confidences.iter().zip(correctness.iter()) {
        if !conf.is_finite() || !(0.0..=1.0).contains(&conf) {
            return Err(CalibrationError::InvalidParameter);
        }
        let mut idx = (conf * (num_bins as f64)).floor() as usize;
        if idx >= num_bins {
            idx = num_bins - 1;
        }
        bin_counts[idx] += 1;
        bin_conf_sum[idx] += conf;
        if correct {
            bin_acc_sum[idx] += 1.0;
        }
    }
    let total = confidences.len() as f64;
    let mut ece = 0.0_f64;
    for i in 0..num_bins {
        if bin_counts[i] > 0 {
            let count = bin_counts[i] as f64;
            let avg_conf = bin_conf_sum[i] / count;
            let avg_acc = bin_acc_sum[i] / count;
            ece += (count / total) * (avg_acc - avg_conf).abs();
        }
    }
    Ok(ece)
}

fn evaluate_temperature(
    samples: &[CalibrationSample],
    temperature: f64,
    num_bins: usize,
) -> Result<(f64, f64, f64), CalibrationError> {
    let mut total_nll = 0.0_f64;
    let mut confidences = Vec::with_capacity(samples.len());
    let mut correctness = Vec::with_capacity(samples.len());
    let mut hits = 0usize;
    for sample in samples {
        let probs = softmax_with_temperature(&sample.logits, temperature)?;
        let (pred_class, &max_prob) = probs
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.total_cmp(b))
            .ok_or(CalibrationError::InvalidSample)?;
        let p_true = probs[sample.label].max(1e-15);
        total_nll -= p_true.ln();
        let is_correct = pred_class == sample.label;
        if is_correct {
            hits += 1;
        }
        confidences.push(max_prob.clamp(0.0, 1.0));
        correctness.push(is_correct);
    }
    let n = samples.len() as f64;
    let ece = expected_calibration_error(&confidences, &correctness, num_bins)?;
    Ok((total_nll / n, ece, (hits as f64) / n))
}

fn validate_samples(
    task: &str,
    samples: &[CalibrationSample],
    min_samples: usize,
) -> Result<usize, CalibrationError> {
    if task.trim().is_empty() {
        return Err(CalibrationError::EmptyTask);
    }
    if samples.len() < min_samples {
        return Err(CalibrationError::InsufficientSamples {
            required: min_samples,
            actual: samples.len(),
        });
    }
    let num_classes = samples[0].logits.len();
    if num_classes < 2 {
        return Err(CalibrationError::InvalidSample);
    }
    for sample in samples {
        if sample.logits.len() != num_classes
            || sample.label >= num_classes
            || sample.logits.iter().any(|z| !z.is_finite())
        {
            return Err(CalibrationError::InvalidSample);
        }
    }
    Ok(num_classes)
}

pub fn fit_temperature_scaling(
    task: &str,
    language: CalibrationLanguage,
    samples: &[CalibrationSample],
) -> Result<FittedCalibration, CalibrationError> {
    fit_temperature_scaling_with_options(
        task,
        language,
        samples,
        DEFAULT_MIN_HOLDOUT_SAMPLES,
        DEFAULT_ECE_BINS,
    )
}

pub fn fit_temperature_scaling_with_options(
    task: &str,
    language: CalibrationLanguage,
    samples: &[CalibrationSample],
    min_samples: usize,
    num_bins: usize,
) -> Result<FittedCalibration, CalibrationError> {
    let num_classes = validate_samples(task, samples, min_samples)?;
    let (nll_before, ece_before, accuracy) = evaluate_temperature(samples, 1.0, num_bins)?;

    // Optimize strictly convex NLL over log(T) in [ln(MIN_TEMPERATURE), ln(MAX_TEMPERATURE)].
    let log_min = MIN_TEMPERATURE.ln();
    let log_max = MAX_TEMPERATURE.ln();
    let grid_steps = 128usize;
    let mut best_idx = 0usize;
    let mut best_nll = f64::INFINITY;
    for i in 0..=grid_steps {
        let frac = (i as f64) / (grid_steps as f64);
        let temp = (log_min + frac * (log_max - log_min)).exp();
        let (nll, _, _) = evaluate_temperature(samples, temp, num_bins)?;
        if nll < best_nll {
            best_nll = nll;
            best_idx = i;
        }
    }
    let mut a = if best_idx == 0 {
        log_min
    } else {
        log_min + ((best_idx - 1) as f64 / grid_steps as f64) * (log_max - log_min)
    };
    let mut b = if best_idx == grid_steps {
        log_max
    } else {
        log_min + ((best_idx + 1) as f64 / grid_steps as f64) * (log_max - log_min)
    };

    let phi = (5.0_f64.sqrt() - 1.0) / 2.0;
    let mut c = b - phi * (b - a);
    let mut d = a + phi * (b - a);
    let mut fc = evaluate_temperature(samples, c.exp(), num_bins)?.0;
    let mut fd = evaluate_temperature(samples, d.exp(), num_bins)?.0;
    for _ in 0..72 {
        if fc < fd {
            b = d;
            d = c;
            fd = fc;
            c = b - phi * (b - a);
            fc = evaluate_temperature(samples, c.exp(), num_bins)?.0;
        } else {
            a = c;
            c = d;
            fc = fd;
            d = a + phi * (b - a);
            fd = evaluate_temperature(samples, d.exp(), num_bins)?.0;
        }
    }
    let best_temp = ((a + b) * 0.5)
        .exp()
        .clamp(MIN_TEMPERATURE, MAX_TEMPERATURE);
    let (nll_after, ece_after, _) = evaluate_temperature(samples, best_temp, num_bins)?;

    Ok(FittedCalibration {
        task: task.to_owned(),
        language,
        temperature: best_temp,
        sample_count: samples.len(),
        num_classes,
        nll_before,
        nll_after,
        ece_before,
        ece_after,
        accuracy,
        verified: ece_after.is_finite() && ece_after < MAX_CALIBRATED_ECE,
    })
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CalibrationRegistry {
    entries: HashMap<CalibrationKey, FittedCalibration>,
}

impl CalibrationRegistry {
    pub fn fit_and_register(
        &mut self,
        task: &str,
        language: CalibrationLanguage,
        samples: &[CalibrationSample],
    ) -> Result<FittedCalibration, CalibrationError> {
        let fitted = fit_temperature_scaling(task, language, samples)?;
        if !fitted.verified {
            return Err(CalibrationError::ExceedsEceLimit {
                ece: fitted.ece_after,
                limit: MAX_CALIBRATED_ECE,
            });
        }
        self.entries.insert(
            CalibrationKey {
                task: task.to_owned(),
                language,
            },
            fitted.clone(),
        );
        Ok(fitted)
    }

    pub fn register_verified(&mut self, fitted: FittedCalibration) -> Result<(), CalibrationError> {
        if !fitted.verified
            || !fitted.ece_after.is_finite()
            || fitted.ece_after >= MAX_CALIBRATED_ECE
        {
            return Err(CalibrationError::ExceedsEceLimit {
                ece: fitted.ece_after,
                limit: MAX_CALIBRATED_ECE,
            });
        }
        self.entries.insert(
            CalibrationKey {
                task: fitted.task.clone(),
                language: fitted.language,
            },
            fitted,
        );
        Ok(())
    }

    pub fn get(&self, task: &str, language: CalibrationLanguage) -> Option<&FittedCalibration> {
        self.entries.get(&CalibrationKey {
            task: task.to_owned(),
            language,
        })
    }

    pub fn calibrated_decision(
        &self,
        task: &str,
        language: CalibrationLanguage,
        logits: &[f64],
        threshold: f64,
    ) -> Result<CalibratedPrediction, CalibrationError> {
        if !threshold.is_finite() || !(0.0..=1.0).contains(&threshold) {
            return Err(CalibrationError::InvalidParameter);
        }
        let fitted =
            self.get(task, language)
                .ok_or_else(|| CalibrationError::UncalibratedTask {
                    task: task.to_owned(),
                    language,
                })?;
        if !fitted.verified || fitted.ece_after >= MAX_CALIBRATED_ECE {
            return Err(CalibrationError::ExceedsEceLimit {
                ece: fitted.ece_after,
                limit: MAX_CALIBRATED_ECE,
            });
        }
        if logits.len() != fitted.num_classes {
            return Err(CalibrationError::InvalidSample);
        }
        let probabilities = softmax_with_temperature(logits, fitted.temperature)?;
        let (predicted_class, &confidence) = probabilities
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.total_cmp(b))
            .ok_or(CalibrationError::InvalidSample)?;
        Ok(CalibratedPrediction {
            predicted_class,
            confidence,
            passed_threshold: confidence >= threshold,
            probabilities,
            temperature_used: fitted.temperature,
            ece_after: fitted.ece_after,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn synthetic_holdout(
        num_classes: usize,
        scale: f64,
        count: usize,
        seed_offset: u64,
    ) -> Vec<CalibrationSample> {
        // Deterministic LCG generating true latent logits z_true ~ uniform,
        // labels drawn from exact softmax(z_true), and observed uncalibrated
        // model logits z_obs = scale * z_true.
        let mut state = 0x9e37_79b9_7f4a_7c15_u64.wrapping_add(seed_offset);
        let mut next_f64 = || {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            ((state >> 11) as f64) / ((1u64 << 53) as f64)
        };

        let mut samples = Vec::with_capacity(count);
        for i in 0..count {
            let mut true_logits = vec![0.0_f64; num_classes];
            for (c, slot) in true_logits.iter_mut().enumerate() {
                let u = next_f64();
                let bias = if c == (i % num_classes) { 1.4 } else { -0.4 };
                *slot = (u - 0.5) * 2.2 + bias;
            }
            let true_probs = softmax_with_temperature(&true_logits, 1.0).unwrap();
            let draw = next_f64();
            let mut acc = 0.0;
            let mut label = num_classes - 1;
            for (c, &p) in true_probs.iter().enumerate() {
                acc += p;
                if draw <= acc {
                    label = c;
                    break;
                }
            }
            let obs_logits: Vec<f64> = true_logits.iter().map(|&z| z * scale).collect();
            samples.push(CalibrationSample {
                logits: obs_logits,
                label,
            });
        }
        samples
    }

    #[test]
    fn fits_per_language_temperatures_below_ece_gate_and_enforces_fail_closed() {
        let en_holdout = synthetic_holdout(18, 3.2, 600, 17);
        let ar_holdout = synthetic_holdout(18, 0.35, 600, 991);

        let mut registry = CalibrationRegistry::default();

        // Fails closed before calibration.
        assert!(matches!(
            registry.calibrated_decision(
                "pair_router",
                CalibrationLanguage::En,
                &en_holdout[0].logits,
                0.6
            ),
            Err(CalibrationError::UncalibratedTask { .. })
        ));

        let fitted_en = registry
            .fit_and_register("pair_router", CalibrationLanguage::En, &en_holdout)
            .expect("overconfident EN holdout calibrates under 0.05 ECE");
        assert!(
            fitted_en.ece_before > 0.10,
            "expected uncalibrated ECE > 0.10, got {}",
            fitted_en.ece_before
        );
        assert!(
            fitted_en.ece_after < MAX_CALIBRATED_ECE,
            "ece_after={}",
            fitted_en.ece_after
        );
        assert!(fitted_en.nll_after < fitted_en.nll_before);
        assert!(
            fitted_en.temperature > 2.2 && fitted_en.temperature < 4.5,
            "T_en={}",
            fitted_en.temperature
        );

        let fitted_ar = registry
            .fit_and_register("pair_router", CalibrationLanguage::Ar, &ar_holdout)
            .expect("underconfident AR holdout calibrates independently under 0.05 ECE");
        assert!(
            fitted_ar.ece_before > 0.10,
            "expected uncalibrated AR ECE > 0.10, got {}",
            fitted_ar.ece_before
        );
        assert!(
            fitted_ar.ece_after < MAX_CALIBRATED_ECE,
            "ece_after={}",
            fitted_ar.ece_after
        );
        assert!(
            fitted_ar.temperature < 0.7,
            "T_ar={}",
            fitted_ar.temperature
        );

        let decision = registry
            .calibrated_decision(
                "pair_router",
                CalibrationLanguage::En,
                &en_holdout[0].logits,
                0.25,
            )
            .unwrap();
        assert!(decision.ece_after < MAX_CALIBRATED_ECE);
        assert!((decision.temperature_used - fitted_en.temperature).abs() < 1e-12);
    }

    #[test]
    fn rejects_calibration_when_ece_cannot_clear_point_zero_five() {
        // Pathological constant high-logit bias on half the errors inside MAX_TEMPERATURE=10
        // cannot be calibrated below 0.05 ECE.
        let mut samples = Vec::new();
        for i in 0..20 {
            let logits = vec![40.0, 0.0];
            let label = usize::from(i % 4 == 0); // only 75% accuracy while conf > 98% even at T=10
            samples.push(CalibrationSample { logits, label });
        }
        let mut registry = CalibrationRegistry::default();
        let err = registry
            .fit_and_register("bad_router", CalibrationLanguage::En, &samples)
            .unwrap_err();
        assert!(matches!(err, CalibrationError::ExceedsEceLimit { .. }));
        assert!(
            registry
                .get("bad_router", CalibrationLanguage::En)
                .is_none()
        );
    }
}
