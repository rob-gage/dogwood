// Copyright Rob Gage 2026

//! Scene, material, actor, and cellular simulation systems for Dogwood.

extern crate dogwood_engine_compute as engine_compute;
extern crate dogwood_engine_graphics as engine_graphics;
extern crate dogwood_engine_input as engine_input;

pub mod actors;
mod actors_utility;
pub mod chunks;
pub mod materials;
mod scene_editing;
mod scene_geometry;
mod scene_data;
mod scene_simulation;
mod scene_streaming;
pub mod scenes;
pub mod simulation;
mod simulation_cellulars;
mod simulation_fluids;
mod simulation_gases;
mod simulation_materials;
mod simulation_rigid_bodies;
mod simulation_thermal;
mod simulation_utility;
pub mod tiles;
