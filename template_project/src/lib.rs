// Copyright Rob Gage 2026

//! Runnable template project built on the Dogwood engine.

extern crate dogwood_editor as editor;
extern crate dogwood_engine as engine;
extern crate dogwood_template_materials as template_materials;

mod actors;
mod gameplay;
mod project;
mod scene;
mod ui;

pub use project::TemplateProject;
