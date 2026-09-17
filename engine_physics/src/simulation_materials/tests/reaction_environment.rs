// Copyright Rob Gage 2026

use crate::materials::CompiledMaterialReaction;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ReactionEnvironment {
    pub temperature: f32,
    pub pressure: f32,
    pub air: f32,
}

pub(crate) fn environment_matches(
    rule: &CompiledMaterialReaction,
    env: ReactionEnvironment,
) -> bool {
    (rule.minimum_temperature.is_nan() || env.temperature >= rule.minimum_temperature)
        && (rule.maximum_temperature.is_nan() || env.temperature <= rule.maximum_temperature)
        && (rule.minimum_pressure.is_nan() || env.pressure >= rule.minimum_pressure)
        && (rule.maximum_pressure.is_nan() || env.pressure <= rule.maximum_pressure)
        && (rule.minimum_air.is_nan() || env.air >= rule.minimum_air)
        && (rule.maximum_air.is_nan() || env.air <= rule.maximum_air)
}
