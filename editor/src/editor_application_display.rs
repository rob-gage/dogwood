// Copyright Rob Gage 2026

use super::EditorApplication;
use crate::{
    editor_interface::EditorInterface, editor_tool::EditorTool, editor_view_mode::EditorViewMode,
};
use engine::{Game, physics::materials::MaterialIdentifier};
use engine_graphics::Color;
use std::{cell::Cell, rc::Rc};

impl<G: Game> EditorApplication<G> {
    /// Builds the editor layout and handles free-fly controls for this frame
    pub(super) fn display(&mut self) {
        self.update_return();
        let free_fly_enabled: bool = self
            .application
            .game()
            .scene()
            .is_some_and(|scene| scene.possessed_actor() != self.editor_pawn);
        let return_enabled: bool = !self.is_return_pending
            && self.application.game().scene().is_some_and(|scene| {
                scene.possessed_actor() == self.editor_pawn
                    && self.original_pawn.is_some_and(|actor| {
                        scene.actor_registry().contains(actor)
                            && scene.actor_registry().is_possessable(actor)
                    })
            });
        let free_fly_requested: Rc<Cell<bool>> = Rc::new(Cell::new(false));
        let return_requested: Rc<Cell<bool>> = Rc::new(Cell::new(false));
        let play_requested: Rc<Cell<bool>> = Rc::new(Cell::new(false));
        let square_requested: Rc<Cell<bool>> = Rc::new(Cell::new(false));
        let circle_requested: Rc<Cell<bool>> = Rc::new(Cell::new(false));
        let eraser_requested: Rc<Cell<bool>> = Rc::new(Cell::new(false));
        let impulse_requested: Rc<Cell<bool>> = Rc::new(Cell::new(false));
        let thermal_heat_requested: Rc<Cell<bool>> = Rc::new(Cell::new(false));
        let thermal_cool_requested: Rc<Cell<bool>> = Rc::new(Cell::new(false));
        let thermal_rate_requested: Rc<Cell<Option<f32>>> = Rc::new(Cell::new(None));
        let impulse_rate_requested: Rc<Cell<Option<f32>>> = Rc::new(Cell::new(None));
        let rigid_body_placement_requested: Rc<Cell<Option<bool>>> = Rc::new(Cell::new(None));
        let material_requested: Rc<Cell<Option<MaterialIdentifier>>> = Rc::new(Cell::new(None));
        let free_fly_action: Rc<Cell<bool>> = free_fly_requested.clone();
        let return_action: Rc<Cell<bool>> = return_requested.clone();
        let play_action: Rc<Cell<bool>> = play_requested.clone();
        let square_action: Rc<Cell<bool>> = square_requested.clone();
        let circle_action: Rc<Cell<bool>> = circle_requested.clone();
        let eraser_action: Rc<Cell<bool>> = eraser_requested.clone();
        let material_entries: Vec<(MaterialIdentifier, String, Color)> = self
            .application
            .game()
            .scene()
            .map_or_else(Vec::new, |scene| {
                scene
                    .materials()
                    .iter()
                    .map(|(material_identifier, material)| {
                        (
                            material_identifier,
                            material.name().into(),
                            material.appearance().base_color(),
                        )
                    })
                    .collect()
            });
        let viewport_bounds: Rc<Cell<Option<[u32; 4]>>> = Rc::new(Cell::new(None));
        let (preview_cells, preview_color): (Vec<[f32; 4]>, Color) = self.brush_preview();
        let view_mode_requested: Rc<Cell<EditorViewMode>> = Rc::new(Cell::new(self.view_mode));
        let tile_borders_requested: Rc<Cell<bool>> = Rc::new(Cell::new(self.show_tile_borders));
        let chunk_borders_requested: Rc<Cell<bool>> = Rc::new(Cell::new(self.show_chunk_borders));
        let [frames_per_second, ticks_per_second]: [u32; 2] = self.application.performance_rates();
        let layout: EditorInterface = EditorInterface {
            is_playing: self.is_playing,
            frames_per_second,
            ticks_per_second,
            free_fly_enabled,
            return_enabled,
            brush_is_square: self.brush.is_square(),
            brush_size: self.brush.size(),
            selected_tool: match self.tool {
                EditorTool::Material(identifier) => Some(identifier),
                _ => None,
            },
            eraser_selected: matches!(self.tool, EditorTool::Eraser),
            impulse_selected: matches!(self.tool, EditorTool::Impulse),
            thermal_tool_active: matches!(self.tool, EditorTool::Thermal),
            thermal_heat_mode: self.thermal_heat,
            thermal_rate: self.thermal_rate,
            impulse_rate: self.impulse_rate,
            thermal_rate_requested: thermal_rate_requested.clone(),
            thermal_heat_requested: thermal_heat_requested.clone(),
            thermal_cool_requested: thermal_cool_requested.clone(),
            rigid_body_placement_enabled: self.rigid_body_placement_enabled,
            view_mode: self.view_mode,
            show_tile_borders: self.show_tile_borders,
            show_chunk_borders: self.show_chunk_borders,
            materials: material_entries,
            preview_cells,
            preview_color,
            viewport_bounds: viewport_bounds.clone(),
            play_requested: play_action,
            free_fly_requested: free_fly_action,
            return_requested: return_action,
            square_requested: square_action,
            circle_requested: circle_action,
            eraser_requested: eraser_action,
            impulse_requested: impulse_requested.clone(),
            impulse_rate_requested: impulse_rate_requested.clone(),
            rigid_body_placement_requested: rigid_body_placement_requested.clone(),
            material_requested: material_requested.clone(),
            view_mode_requested: view_mode_requested.clone(),
            tile_borders_requested: tile_borders_requested.clone(),
            chunk_borders_requested: chunk_borders_requested.clone(),
        };
        self.application.add_widget(layout);
        if let Some(rate) = thermal_rate_requested.get() {
            self.thermal_rate = rate.clamp(1.0, 1000.0);
        }
        if let Some(rate) = impulse_rate_requested.get() {
            self.impulse_rate = rate.clamp(1.0, 1000.0);
        }
        if let Some(bounds) = viewport_bounds.get() {
            self.application.set_scene_viewport_bounds(Some(bounds));
        }
        if eraser_requested.get() {
            self.tool = EditorTool::Eraser;
            self.stroke_anchor = None;
            self.last_thermal_edit = None;
            self.last_impulse_edit = None;
        }
        if impulse_requested.get() {
            self.tool = EditorTool::Impulse;
            self.stroke_anchor = None;
            self.last_thermal_edit = None;
            self.last_impulse_edit = None;
            self.rigid_body_placement_requested = None;
        }
        if thermal_heat_requested.get() {
            self.tool = EditorTool::Thermal;
            self.thermal_heat = true;
            self.stroke_anchor = None;
            self.last_thermal_edit = None;
            self.last_impulse_edit = None;
        }
        if thermal_cool_requested.get() {
            self.tool = EditorTool::Thermal;
            self.thermal_heat = false;
            self.stroke_anchor = None;
            self.last_thermal_edit = None;
            self.last_impulse_edit = None;
        }
        if let Some(material_identifier) = material_requested.get() {
            self.tool = EditorTool::Material(material_identifier);
            self.stroke_anchor = None;
            self.last_thermal_edit = None;
            self.last_impulse_edit = None;
            self.rigid_body_placement_requested = None;
        }
        if let Some(enabled) = rigid_body_placement_requested.get() {
            self.rigid_body_placement_enabled = enabled;
            self.stroke_anchor = None;
            self.rigid_body_placement_requested = None;
        }
        if square_requested.get() && self.brush.select_square() {
            self.stroke_anchor = None;
        }
        if circle_requested.get() && self.brush.select_circle() {
            self.stroke_anchor = None;
        }
        self.view_mode = view_mode_requested.get();
        self.show_tile_borders = tile_borders_requested.get();
        self.show_chunk_borders = chunk_borders_requested.get();
        self.application.set_scene_view_settings(
            self.view_mode.shader_value(),
            self.show_tile_borders,
            self.show_chunk_borders,
        );
        if play_requested.get() {
            self.is_playing = !self.is_playing;
            self.application.set_simulation_enabled(self.is_playing);
        }
        if free_fly_requested.get() {
            self.enter_free_fly();
        }
        if return_requested.get() {
            self.is_return_pending = true;
            self.update_return();
        }
        self.place_requested_rigid_body();
        self.paint_hovered_cells();
    }
}
