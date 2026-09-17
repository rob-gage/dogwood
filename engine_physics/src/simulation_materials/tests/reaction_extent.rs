// Copyright Rob Gage 2026

/// Calculates the inventory-limited reaction extent
pub(crate) fn extent(
    maximum: f32,
    available: impl IntoIterator<Item = f32>,
    coefficients: impl IntoIterator<Item = f32>,
) -> f32 {
    let inventory_limit = available
        .into_iter()
        .zip(coefficients)
        .map(|(amount, coefficient)| amount / coefficient)
        .fold(f32::INFINITY, f32::min);
    maximum.min(inventory_limit).max(0.0)
}
