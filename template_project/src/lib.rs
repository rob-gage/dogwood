// Copyright Rob Gage 2026

//! Runnable template project built on the Dogwood engine.

extern crate dogwood_editor as editor;
extern crate dogwood_engine as engine;
extern crate dogwood_template_materials as template_materials;

mod template_project;
mod template_scene_generator;

pub use template_project::TemplateProject;
