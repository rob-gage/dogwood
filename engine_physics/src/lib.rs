// Copyright Rob Gage 2026

pub mod actors;
pub mod chunks;
pub mod materials;
pub mod scenes;
pub mod simulation;
pub mod tiles;

#[cfg(test)]
pub(crate) static GPU_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
