// Copyright Rob Gage 2026

//! Independent immediate-mode composition, native input, and GPU UI rendering

mod user_interface_context;
mod user_interface_window_state;
mod user_interface_renderer;
mod user_interface_theme;

pub mod components;
pub mod advanced;
pub mod game_ui;

pub use user_interface_context::UserInterfaceContext;
pub use user_interface_window_state::UserInterfaceWindowState;
pub use user_interface_renderer::UserInterfaceRenderer;
pub use user_interface_theme::UserInterfaceTheme;
pub use egui;
pub use egui_dock;
pub use egui_extras;
pub use egui_wgpu;
pub use wgpu;
pub use winit;

/// Native egui composition surface for nested widgets and custom painting
pub type UserInterface = egui::Ui;
