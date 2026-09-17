// Copyright Rob Gage 2026

//! Dogwood editor application, tools, and viewport integration.

extern crate dogwood_engine as engine;
extern crate dogwood_engine_compute as engine_compute;
extern crate dogwood_engine_graphics as engine_graphics;
extern crate dogwood_engine_user_interface as engine_user_interface;

mod editor_application;
mod editor_brush;
mod editor_game;
mod editor_interface;
mod editor_tool;
mod editor_view_mode;

pub use editor_game::EditorGame;
