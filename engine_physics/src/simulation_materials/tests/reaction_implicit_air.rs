// Copyright Rob Gage 2026

/// Calculates implicit air available to an unoccupied canonical cell
pub(crate) fn implicit_air(
    canonical_empty: bool,
    rigid_or_external_blocked: bool,
    fluid_coverage: f32,
    explicit_gas: f32,
) -> f32 {
    if !canonical_empty || rigid_or_external_blocked {
        0.0
    } else {
        (1.0 - fluid_coverage.clamp(0.0, 1.0) - explicit_gas.max(0.0)).clamp(0.0, 1.0)
    }
}
