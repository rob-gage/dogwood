// Copyright Rob Gage 2026

use crate::editor_view_mode::EditorViewMode;
use engine::physics::materials::MaterialIdentifier;
use engine_graphics::Color;
use engine_user_interface::{UserInterface, Widget};
use std::{cell::Cell, collections::HashSet, rc::Rc};

/// The docked editor chrome displayed around the scene viewport
pub struct EditorInterface {
    pub is_playing: bool,
    pub frames_per_second: u32,
    pub ticks_per_second: u32,
    pub free_fly_enabled: bool,
    pub return_enabled: bool,
    pub brush_is_square: bool,
    pub brush_size: u16,
    pub selected_tool: Option<MaterialIdentifier>,
    pub eraser_selected: bool,
    pub impulse_selected: bool,
    pub impulse_rate: f32,
    pub thermal_tool_active: bool,
    pub thermal_heat_mode: bool,
    pub thermal_rate: f32,
    pub thermal_rate_requested: Rc<Cell<Option<f32>>>,
    pub thermal_heat_requested: Rc<Cell<bool>>,
    pub thermal_cool_requested: Rc<Cell<bool>>,
    pub rigid_body_placement_enabled: bool,
    pub view_mode: EditorViewMode,
    pub show_tile_borders: bool,
    pub show_chunk_borders: bool,
    pub materials: Vec<(MaterialIdentifier, String, Color)>,
    pub preview_cells: Vec<[f32; 4]>,
    pub preview_color: Color,
    pub viewport_bounds: Rc<Cell<Option<[u32; 4]>>>,
    pub play_requested: Rc<Cell<bool>>,
    pub free_fly_requested: Rc<Cell<bool>>,
    pub return_requested: Rc<Cell<bool>>,
    pub square_requested: Rc<Cell<bool>>,
    pub circle_requested: Rc<Cell<bool>>,
    pub eraser_requested: Rc<Cell<bool>>,
    pub impulse_requested: Rc<Cell<bool>>,
    pub impulse_rate_requested: Rc<Cell<Option<f32>>>,
    pub rigid_body_placement_requested: Rc<Cell<Option<bool>>>,
    pub material_requested: Rc<Cell<Option<MaterialIdentifier>>>,
    pub view_mode_requested: Rc<Cell<EditorViewMode>>,
    pub tile_borders_requested: Rc<Cell<bool>>,
    pub chunk_borders_requested: Rc<Cell<bool>>,
}

impl EditorInterface {
    fn tool_button(ui: &mut egui::Ui, selected: bool, text: &str) -> bool {
        ui.add_sized(
            [(ui.available_width() - 22.0).max(0.0), 28.0],
            egui::Button::new(text).selected(selected),
        )
        .clicked()
    }

    fn material_button(ui: &mut egui::Ui, selected: bool, name: &str, color: Color) -> bool {
        let response = ui.horizontal(|ui| {
            let (row, _) = ui.allocate_exact_size(egui::vec2(16.0, 28.0), egui::Sense::hover());
            let swatch = egui::Rect::from_center_size(row.center(), egui::vec2(16.0, 16.0));
            ui.painter().rect_filled(
                swatch,
                3.0,
                egui::Color32::from_rgb(color.red(), color.green(), color.blue()),
            );
            ui.add_sized(
                [ui.available_width(), 28.0],
                egui::Button::new(name).selected(selected),
            )
            .clicked()
        });
        response.inner
    }
}

impl Widget for EditorInterface {
    fn display(&mut self, user_interface: &mut UserInterface) -> egui::Response {
        let ui = user_interface.egui();
        ui.style_mut().spacing.item_spacing = egui::vec2(6.0, 6.0);
        ui.style_mut().visuals.panel_fill = egui::Color32::from_rgb(30, 32, 36);
        ui.style_mut().visuals.window_fill = egui::Color32::from_rgb(36, 38, 43);
        ui.style_mut().visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(3);
        ui.style_mut().visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(3);
        ui.style_mut().visuals.widgets.active.corner_radius = egui::CornerRadius::same(3);
        egui::Panel::top("editor_top_bar")
            .exact_size(32.0)
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    if ui
                        .button(if self.is_playing { "Pause" } else { "Play" })
                        .clicked()
                    {
                        self.play_requested.set(true);
                    }
                    ui.menu_button("View", |ui| {
                        ui.label(egui::RichText::new("View mode").strong());
                        for (mode, label) in [
                            (EditorViewMode::Normal, "Normal"),
                            (EditorViewMode::MaterialForm, "Material form"),
                            (EditorViewMode::Pressure, "Pressure"),
                            (EditorViewMode::Temperature, "Thermal"),
                            (EditorViewMode::Gas, "Gas"),
                        ] {
                            if ui.radio(self.view_mode == mode, label).clicked() {
                                self.view_mode_requested.set(mode);
                                ui.close();
                            }
                        }
                        ui.separator();
                        let mut tile_borders = self.show_tile_borders;
                        if ui.checkbox(&mut tile_borders, "Tile borders").changed() {
                            self.tile_borders_requested.set(tile_borders);
                        }
                        let mut chunk_borders = self.show_chunk_borders;
                        if ui.checkbox(&mut chunk_borders, "Chunk borders").changed() {
                            self.chunk_borders_requested.set(chunk_borders);
                        }
                    });
                    if self.free_fly_enabled {
                        if ui.button("Detach").clicked() {
                            self.free_fly_requested.set(true);
                        }
                    } else if ui
                        .add_enabled(self.return_enabled, egui::Button::new("Attach"))
                        .clicked()
                    {
                        self.return_requested.set(true);
                    }
                    ui.separator();
                    if ui
                        .selectable_label(!self.brush_is_square, "Circle")
                        .clicked()
                    {
                        self.circle_requested.set(true);
                    }
                    if ui
                        .selectable_label(self.brush_is_square, "Square")
                        .clicked()
                    {
                        self.square_requested.set(true);
                    }
                    ui.label(format!("Brush {}", self.brush_size));
                    ui.separator();
                });
            });
        egui::Panel::bottom("editor_status_bar")
            .exact_size(22.0)
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.label(egui::RichText::new("DOGWOOD").small().strong());
                    ui.separator();
                    ui.label(
                        egui::RichText::new(if self.is_playing {
                            "SIMULATING"
                        } else {
                            "PAUSED"
                        })
                        .small(),
                    );
                    ui.separator();
                    ui.label(
                        egui::RichText::new(format!("FPS {}", self.frames_per_second)).small(),
                    );
                    ui.separator();
                    ui.label(egui::RichText::new(format!("TPS {}", self.ticks_per_second)).small());
                    ui.separator();
                });
            });
        egui::Panel::left("editor_tools")
            .exact_size(190.0)
            .resizable(false)
            .show(ui, |ui| {
                ui.add_space(6.0);
                ui.heading("Tools");
                ui.separator();
                egui::CollapsingHeader::new("General")
                    .default_open(true)
                    .show(ui, |ui| {
                        if Self::tool_button(ui, self.eraser_selected, "Eraser") {
                            self.eraser_requested.set(true);
                        }
                    });
                egui::CollapsingHeader::new("Materials")
                    .default_open(true)
                    .show(ui, |ui| {
                        let mut rigid_body_placement_enabled = self.rigid_body_placement_enabled;
                        if ui
                            .checkbox(&mut rigid_body_placement_enabled, "Rigid Body Placement")
                            .changed()
                        {
                            self.rigid_body_placement_requested
                                .set(Some(rigid_body_placement_enabled));
                        }
                        for (identifier, name, color) in &self.materials {
                            if Self::material_button(
                                ui,
                                self.selected_tool == Some(*identifier),
                                name,
                                *color,
                            ) {
                                self.material_requested.set(Some(*identifier));
                            }
                        }
                    });
                egui::CollapsingHeader::new("Pressure").show(ui, |ui| {
                    let response = ui.add_sized(
                        [(ui.available_width() - 22.0).max(0.0), 24.0],
                        egui::Slider::new(&mut self.impulse_rate, 1.0..=1000.0).text("Pressure/s"),
                    );
                    if response.changed() {
                        self.impulse_rate_requested.set(Some(self.impulse_rate));
                    }
                    if Self::tool_button(ui, self.impulse_selected, "Impulse") {
                        self.impulse_requested.set(true);
                    }
                });
                egui::CollapsingHeader::new("Thermal").show(ui, |ui| {
                    let response = ui.add_sized(
                        [(ui.available_width() - 22.0).max(0.0), 24.0],
                        egui::Slider::new(&mut self.thermal_rate, 1.0..=1000.0).text("K/s"),
                    );
                    if response.changed() {
                        self.thermal_rate_requested.set(Some(self.thermal_rate));
                    }
                    if Self::tool_button(
                        ui,
                        self.thermal_tool_active && self.thermal_heat_mode,
                        "Heat",
                    ) {
                        self.thermal_heat_requested.set(true);
                    }
                    if Self::tool_button(
                        ui,
                        self.thermal_tool_active && !self.thermal_heat_mode,
                        "Cool",
                    ) {
                        self.thermal_cool_requested.set(true);
                    }
                });
            });
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ui, |ui| {
                let rect = ui.available_rect_before_wrap();
                let scale = ui.ctx().pixels_per_point();
                self.viewport_bounds.set(Some([
                    (rect.min.x * scale).round() as u32,
                    (rect.min.y * scale).round() as u32,
                    (rect.width() * scale).round() as u32,
                    (rect.height() * scale).round() as u32,
                ]));
                let painter = ui.painter().with_clip_rect(rect);
                let preview_color: egui::Color32 = (&self.preview_color).into();
                let cell_size: [f32; 2] = self.preview_cells.iter().fold(
                    [0.0, 0.0],
                    |[width, height], [left, top, right, bottom]| {
                        [width.max(right - left), height.max(bottom - top)]
                    },
                );
                let preview_keys: HashSet<(i32, i32)> = self
                    .preview_cells
                    .iter()
                    .map(|[left, top, ..]| {
                        (
                            (left / cell_size[0]).round() as i32,
                            (top / cell_size[1]).round() as i32,
                        )
                    })
                    .collect();
                for [left, top, right, bottom] in &self.preview_cells {
                    let cell = egui::Rect::from_min_max(
                        egui::pos2(left / scale, top / scale),
                        egui::pos2(right / scale, bottom / scale),
                    );
                    painter.rect_filled(cell, 0.0, preview_color);
                    let key = (
                        (left / cell_size[0]).round() as i32,
                        (top / cell_size[1]).round() as i32,
                    );
                    let stroke = egui::Stroke::new(1.0, egui::Color32::WHITE);
                    let left_edge = egui::pos2(left / scale, top / scale);
                    let right_edge = egui::pos2(right / scale, bottom / scale);
                    if !preview_keys.contains(&(key.0 - 1, key.1)) {
                        painter.line_segment(
                            [left_edge, egui::pos2(left / scale, bottom / scale)],
                            stroke,
                        );
                    }
                    if !preview_keys.contains(&(key.0 + 1, key.1)) {
                        painter.line_segment(
                            [egui::pos2(right / scale, top / scale), right_edge],
                            stroke,
                        );
                    }
                    if !preview_keys.contains(&(key.0, key.1 - 1)) {
                        painter.line_segment(
                            [left_edge, egui::pos2(right / scale, top / scale)],
                            stroke,
                        );
                    }
                    if !preview_keys.contains(&(key.0, key.1 + 1)) {
                        painter.line_segment(
                            [egui::pos2(left / scale, bottom / scale), right_edge],
                            stroke,
                        );
                    }
                }
                ui.allocate_rect(rect, egui::Sense::hover());
            })
            .response
    }
}
