use campuslingo_domain::memory::{
    MemoryInput, ReviewInput, ShieldInput, fsrs6, fsrs6_with_parameters, review, shield,
    validate_fsrs6_parameters,
};
use fsrs::{FSRS6_DEFAULT_DECAY, FSRS6_DEFAULT_PARAMETERS};

const CUSTOM_S19_WEIGHTS: [f32; 21] = [
    0.2540, 1.3800, 2.5200, 8.9500, 6.2100, 0.8900, 2.8500, 0.0025, 1.9100, 0.1580, 0.8200, 1.5100,
    0.0580, 0.2800, 1.5900, 0.5800, 1.9400, 0.5100, 0.0880, 0.0710, 0.2100,
];

type ChainScenario = (&'static str, f32, Option<[f32; 21]>, &'static [(u32, u8)]);

#[test]
fn sequential_60_step_chains_remain_finite_with_default_and_custom_s19_weights() {
    let scenarios: &[ChainScenario] = &[
        (
            "steady_good_easy_default",
            0.90,
            None,
            &[(0, 3), (1, 3), (3, 4), (6, 3), (12, 3), (24, 4)],
        ),
        (
            "lapses_recovery_default",
            0.90,
            None,
            &[(0, 3), (1, 1), (1, 2), (2, 3), (5, 1), (1, 3), (4, 4)],
        ),
        (
            "high_retention_s19_custom",
            0.95,
            Some(CUSTOM_S19_WEIGHTS),
            &[
                (0, 2),
                (1, 3),
                (2, 3),
                (4, 4),
                (1, 1),
                (1, 3),
                (7, 3),
                (14, 4),
            ],
        ),
    ];

    for (name, retention, params, pattern) in scenarios {
        let mut memory: Option<MemoryInput> = None;
        let decay = params.map_or(FSRS6_DEFAULT_DECAY, |w| w[20]);
        for step in 0..64 {
            let (elapsed_days, rating) = if step == 0 {
                (0, 3)
            } else {
                pattern[step % pattern.len()]
            };
            let out = review(&ReviewInput {
                memory,
                elapsed_days,
                rating,
                desired_retention: *retention,
                parameters: *params,
            })
            .unwrap_or_else(|e| panic!("scenario {name} step {step} failed: {e}"));

            assert_eq!(out.algorithm, "FSRS-6");
            assert!(out.stability.is_finite() && out.stability >= 0.001);
            assert!(out.difficulty.is_finite() && (1.0..=10.0).contains(&out.difficulty));
            assert!(out.interval_days.is_finite() && out.interval_days >= 0.0);
            assert!(out.scheduled_days >= 1 && out.scheduled_days <= 36500);
            assert!(out.retrievability_before.is_finite());

            let continuous_shield = shield(&ShieldInput {
                stability: out.stability,
                elapsed_days: 0.375 * ((step % 9) as f32 + 1.0),
                decay: Some(decay),
            })
            .unwrap();
            assert!(
                continuous_shield.is_finite() && (0.0..=1.0).contains(&continuous_shield),
                "scenario {name} step {step} non-finite shield {continuous_shield}"
            );

            memory = Some(MemoryInput {
                stability: out.stability,
                difficulty: out.difficulty,
            });
        }
    }
}

#[test]
fn custom_s19_weights_change_model_output_and_reject_bad_vectors() {
    let default_out = review(&ReviewInput {
        memory: None,
        elapsed_days: 0,
        rating: 3,
        desired_retention: 0.9,
        parameters: None,
    })
    .unwrap();
    let custom_out = review(&ReviewInput {
        memory: None,
        elapsed_days: 0,
        rating: 3,
        desired_retention: 0.9,
        parameters: Some(CUSTOM_S19_WEIGHTS),
    })
    .unwrap();
    assert!((default_out.stability - custom_out.stability).abs() > 0.05);
    assert_eq!(
        fsrs6().version(),
        fsrs6_with_parameters(&CUSTOM_S19_WEIGHTS)
            .unwrap()
            .version()
    );

    assert!(validate_fsrs6_parameters(&FSRS6_DEFAULT_PARAMETERS).is_ok());
    assert!(validate_fsrs6_parameters(&CUSTOM_S19_WEIGHTS[..19]).is_err());

    let mut bad_nan = CUSTOM_S19_WEIGHTS;
    bad_nan[3] = f32::NAN;
    assert!(validate_fsrs6_parameters(&bad_nan).is_err());

    let mut bad_decay = CUSTOM_S19_WEIGHTS;
    bad_decay[20] = 0.0;
    assert!(validate_fsrs6_parameters(&bad_decay).is_err());

    assert!(
        shield(&ShieldInput {
            stability: 5.0,
            elapsed_days: 1.0,
            decay: Some(0.0),
        })
        .is_err()
    );
}
