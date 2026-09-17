// Copyright Rob Gage 2026

mod accelerator_test_lock;

pub(crate) use accelerator_test_lock::acquire_accelerator_test_lock;

use crate::simulation::thermal_phase_transitions::test_rigid_phase_readback_len;

fn transitioned_temperature(
    amount: f32,
    source_cp: f32,
    target_cp: f32,
    threshold: f32,
    latent: f32,
    temperature: f32,
    hot: bool,
) -> Option<f32> {
    let sensible = amount
        * source_cp
        * if hot {
            (temperature - threshold).max(0.0)
        } else {
            (threshold - temperature).max(0.0)
        };
    let required = amount * latent.max(0.0);
    if amount <= 0.0 || target_cp <= 0.0 || (latent > 0.0 && sensible < required) {
        return None;
    }
    Some(
        (threshold
            + if hot { 1.0 } else { -1.0 } * (sensible - required).max(0.0) / (amount * target_cp))
            .max(0.0),
    )
}

#[test]
fn test_rigid_readback_size_scales_with_submitted_rigid_count() {
    assert_eq!(test_rigid_phase_readback_len(0), 256);
    assert_eq!(test_rigid_phase_readback_len(128), 256 + 128 * 40);
    assert!(test_rigid_phase_readback_len(128) < test_rigid_phase_readback_len(100_000));
}

#[test]
fn test_latent_arithmetic_is_symmetric() {
    assert_eq!(
        transitioned_temperature(1.0, 2.0, 4.0, 10.0, 3.0, 11.0, true),
        None
    );
    assert_eq!(
        transitioned_temperature(1.0, 2.0, 4.0, 10.0, 3.0, 11.5, true),
        Some(10.0)
    );
    assert_eq!(
        transitioned_temperature(1.0, 2.0, 4.0, 10.0, 3.0, 13.5, true),
        Some(11.0)
    );
    assert_eq!(
        transitioned_temperature(1.0, 2.0, 4.0, 10.0, 3.0, 9.0, false),
        None
    );
    assert_eq!(
        transitioned_temperature(1.0, 2.0, 4.0, 10.0, 3.0, 8.5, false),
        Some(10.0)
    );
    assert_eq!(
        transitioned_temperature(1.0, 2.0, 4.0, 10.0, 3.0, 6.5, false),
        Some(9.0)
    );
    assert_eq!(
        transitioned_temperature(1.0, 2.0, 4.0, 10.0, 0.0, 11.0, true),
        Some(10.5)
    );
}
