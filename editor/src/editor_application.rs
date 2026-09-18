// Copyright Rob Gage 2026

use crate::{editor_brush::EditorBrush, editor_tool::EditorTool, editor_view_mode::EditorViewMode};
use engine::{
    Game, GameApplication,
    physics::{
        actors::{Actor, ActorPawn, ActorPawnMovement, ActorPawnNoclipConfiguration},
        materials::{Material, MaterialIdentifier},
        scenes::{SceneEditBatch, SceneEditCellPlacement, ScenePosition, SceneVelocity},
        tiles::{CellCoordinates, CellularAppearance},
    },
};
use engine_graphics::Color;
use std::{
    collections::HashSet,
    error::Error,
    sync::Arc,
    time::{Duration, Instant},
};

#[path = "editor_application_display.rs"]
mod editor_application_display;
#[path = "editor_application_pointer.rs"]
mod editor_application_pointer;

const RIGID_BODY_PLACEMENT_INTERVAL: Duration = Duration::from_millis(150);

/// A windowed editor application for a `Game`
pub struct EditorApplication<G: Game> {
    /// The game application hosted by this editor
    application: GameApplication<G>,
    /// The reusable invisible pawn used for free-fly
    editor_pawn: Option<Actor>,
    /// The pawn possessed before entering free-fly
    original_pawn: Option<Actor>,
    /// The exact position to restore when returning from free-fly
    original_pawn_position: Option<ScenePosition>,
    /// Whether streaming is currently returning to the original pawn
    is_return_pending: bool,
    /// Whether ordinary hosted scene simulation is playing
    is_playing: bool,
    /// The latest physical cursor position, if the cursor is inside the window
    cursor_position: Option<[f32; 2]>,
    /// Whether the primary button is held for future Scene interaction
    is_primary_scene_interaction_held: bool,
    /// The brush used for Scene interaction
    brush: EditorBrush,
    /// The preceding anchor of the current primary Scene interaction
    stroke_anchor: Option<CellCoordinates>,
    thermal_rate: f32,
    thermal_heat: bool,
    impulse_rate: f32,
    last_thermal_edit: Option<Instant>,
    last_impulse_edit: Option<Instant>,
    /// A single rigid placement click waiting for the normal editor update
    rigid_body_placement_requested: Option<CellCoordinates>,
    /// Time of the last rigid placement submitted to the Scene queue
    last_rigid_body_placement: Option<Instant>,
    /// Whether static material clicks create rigid bodies
    rigid_body_placement_enabled: bool,
    /// Unconsumed high-resolution wheel movement in physical pixels
    pixel_scroll_y: f64,
    /// The concrete editor action selected for the primary interaction
    tool: EditorTool,
    /// The visualization used by the scene viewport
    view_mode: EditorViewMode,
    /// Whether boundaries between streamed tiles are visible
    show_tile_borders: bool,
    /// Whether boundaries between persistent chunks are visible
    show_chunk_borders: bool,
}

impl<G: Game> EditorApplication<G> {
    fn new(accelerator: Arc<engine::compute::Accelerator>, game: G) -> Self {
        let mut application: GameApplication<G> = GameApplication::new_with_title(
            accelerator,
            game,
            format!("Engine Editor: {}", G::TITLE),
        );
        application.set_simulation_enabled(false);
        Self {
            application,
            editor_pawn: None,
            original_pawn: None,
            original_pawn_position: None,
            is_return_pending: false,
            is_playing: false,
            cursor_position: None,
            is_primary_scene_interaction_held: false,
            brush: EditorBrush::new(),
            stroke_anchor: None,
            thermal_rate: 100.0,
            thermal_heat: true,
            impulse_rate: 100.0,
            last_thermal_edit: None,
            last_impulse_edit: None,
            rigid_body_placement_requested: None,
            last_rigid_body_placement: None,
            rigid_body_placement_enabled: false,
            pixel_scroll_y: 0.0,
            tool: EditorTool::Eraser,
            view_mode: EditorViewMode::Normal,
            show_tile_borders: false,
            show_chunk_borders: false,
        }
    }

    /// Returns the clipped physical preview rectangles for the current brush footprint
    fn brush_preview(&self) -> (Vec<[f32; 4]>, Color) {
        let Some(anchor): Option<CellCoordinates> = self.hovered_cell() else {
            return (Vec::new(), Color::new_rgba(255, 80, 80, 96));
        };
        let material_identifier: Option<MaterialIdentifier> = match self.tool {
            EditorTool::Material(material_identifier) => Some(material_identifier),
            _ => None,
        };
        let color: Color = material_identifier
            .and_then(|material_identifier| {
                self.application.game().scene().and_then(|scene| {
                    scene.materials().get(material_identifier).map(|material| {
                        let color: Color = material.appearance().base_color();
                        Color::new_rgba(color.red(), color.green(), color.blue(), 96)
                    })
                })
            })
            .unwrap_or(match self.tool {
                EditorTool::Impulse => Color::new_rgba(255, 190, 60, 0),
                EditorTool::Thermal if self.thermal_heat => Color::new_rgba(255, 130, 30, 96),
                EditorTool::Thermal => Color::new_rgba(45, 110, 230, 96),
                _ => Color::new_rgba(255, 80, 80, 96),
            });
        let cells: Vec<[f32; 4]> = self
            .brush
            .cells(anchor)
            .into_iter()
            .filter_map(|coordinates| {
                self.application.scene_surface_rectangle([
                    coordinates.x as f32 / 8.0,
                    coordinates.y as f32 / 8.0,
                    (coordinates.x as f32 + 1.0) / 8.0,
                    (coordinates.y as f32 + 1.0) / 8.0,
                ])
            })
            .collect();
        (cells, color)
    }

    /// Applies the current brush stroke through the scene edit boundary
    fn paint_hovered_cells(&mut self) {
        if !self.is_primary_scene_interaction_held {
            return;
        }
        if self.rigid_body_placement_enabled
            && matches!(self.tool, EditorTool::Material(identifier)
            if self.application.game().scene().is_some_and(|scene| matches!(
                scene.materials().get(identifier), Some(Material::CellularStatic { .. })
            )))
        {
            return;
        }
        let Some(anchor): Option<CellCoordinates> = self.hovered_cell() else {
            self.stroke_anchor = None;
            return;
        };
        let anchors: Vec<CellCoordinates> = match self.stroke_anchor {
            None => vec![anchor],
            Some(previous) if previous == anchor => {
                if matches!(self.tool, EditorTool::Thermal | EditorTool::Impulse) {
                    vec![anchor]
                } else {
                    return;
                }
            }
            Some(previous) => EditorBrush::stroke_anchors(previous, anchor)
                .into_iter()
                .skip(1)
                .collect(),
        };
        let mut cells: HashSet<CellCoordinates> = HashSet::new();
        for anchor in &anchors {
            cells.extend(self.brush.cells(*anchor));
        }
        let tool: EditorTool = match self.tool {
            EditorTool::Eraser => EditorTool::Eraser,
            EditorTool::Impulse => EditorTool::Impulse,
            EditorTool::Material(material_identifier) => EditorTool::Material(material_identifier),
            EditorTool::Thermal => EditorTool::Thermal,
        };
        let Some(scene) = self.application.game_mutable().scene_mutable() else {
            return;
        };
        if matches!(tool, EditorTool::Thermal) {
            let now: Instant = Instant::now();
            let delta_time: f32 = self
                .last_thermal_edit
                .map_or(0.0, |last| now.duration_since(last).as_secs_f32().min(0.25));
            self.last_thermal_edit = Some(now);
            if delta_time > 0.0 {
                let delta: f32 =
                    self.thermal_rate * delta_time * if self.thermal_heat { 1.0 } else { -1.0 };
                let mut edit: SceneEditBatch = SceneEditBatch::new();
                edit.thermal(cells.into_iter().collect(), delta);
                scene.queue_edits(edit);
            }
            self.stroke_anchor = Some(anchor);
            return;
        }
        if matches!(tool, EditorTool::Impulse) {
            let now: Instant = Instant::now();
            let delta_time: f32 = self.last_impulse_edit.map_or(1.0 / 60.0, |last| {
                now.duration_since(last).as_secs_f32().min(0.25)
            });
            self.last_impulse_edit = Some(now);
            let radius_cells: f32 = (self.brush.size() as f32 * 0.5).max(0.75);
            let strength: f32 = self.impulse_rate * delta_time;
            for impulse_anchor in &anchors {
                scene.apply_cellular_radial_impulse(*impulse_anchor, radius_cells, strength);
            }
            self.stroke_anchor = Some(anchor);
            return;
        }
        let mut edits: SceneEditBatch = SceneEditBatch::new();
        match tool {
            EditorTool::Material(material_identifier) => {
                let Some(material) = scene.materials().get(material_identifier) else {
                    return;
                };
                let variation: [f32; 4] = material.appearance().variation();
                edits.place_cells(
                    cells
                        .into_iter()
                        .map(|coordinates| SceneEditCellPlacement {
                            coordinates,
                            material_identifier,
                            appearance: CellularAppearance::from_seed(
                                coordinates.appearance_seed(),
                                variation,
                            ),
                        })
                        .collect(),
                );
            }
            EditorTool::Eraser => edits.destroy_cells(cells.into_iter().collect()),
            EditorTool::Impulse => unreachable!(),
            EditorTool::Thermal => unreachable!(),
        }
        scene.queue_edits(edits);
        self.stroke_anchor = Some(anchor);
    }

    /// Queues one body for one accepted static-material click.
    fn place_requested_rigid_body(&mut self) {
        let Some(anchor) = self.rigid_body_placement_requested.take() else {
            return;
        };
        let EditorTool::Material(material_identifier) = self.tool else {
            return;
        };
        let Some(scene) = self.application.game().scene() else {
            return;
        };
        let Some(Material::CellularStatic { graphics, .. }) =
            scene.materials().get(material_identifier)
        else {
            return;
        };
        let now: Instant = Instant::now();
        if self
            .last_rigid_body_placement
            .is_some_and(|last| now.duration_since(last) < RIGID_BODY_PLACEMENT_INTERVAL)
        {
            return;
        }
        let variation: [f32; 4] = graphics.variation();
        let cells: Vec<SceneEditCellPlacement> = self
            .brush
            .cells(anchor)
            .into_iter()
            .map(|coordinates| SceneEditCellPlacement {
                coordinates,
                material_identifier,
                appearance: CellularAppearance::from_seed(coordinates.appearance_seed(), variation),
            })
            .collect();
        let Some(scene) = self.application.game_mutable().scene_mutable() else {
            return;
        };
        let mut edits: SceneEditBatch = SceneEditBatch::new();
        edits.place_rigid_body(cells);
        scene.queue_edits(edits);
        self.last_rigid_body_placement = Some(now);
    }

    /// Adjusts the brush size and restarts the current stamp when it changes
    fn adjust_brush_size(&mut self, adjustment: i32) {
        if self.brush.adjust_size(adjustment) {
            self.stroke_anchor = None;
        }
    }

    /// Updates the editor's pointer state after the user interface handles an event
    fn handle_pointer_event(&mut self, event: &winit::event::WindowEvent, ui_consumed: bool) {
        use winit::event::{ElementState, MouseButton, WindowEvent::*};
        match event {
            CursorMoved { position, .. } => {
                self.cursor_position = Some([position.x as f32, position.y as f32]);
            }
            CursorLeft { .. } => {
                self.cursor_position = None;
                self.stroke_anchor = None;
                self.last_thermal_edit = None;
                self.last_impulse_edit = None;
                self.rigid_body_placement_requested = None;
            }
            MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } => {
                if ui_consumed && self.hovered_cell().is_none() {
                    return;
                }
                if !self.is_primary_scene_interaction_held {
                    self.is_primary_scene_interaction_held = self.hovered_cell().is_some();
                    if self.is_primary_scene_interaction_held {
                        self.stroke_anchor = None;
                        self.last_thermal_edit = None;
                        self.last_impulse_edit = None;
                        let static_material: Option<MaterialIdentifier> = match self.tool {
                            EditorTool::Material(material_identifier) => self
                                .application
                                .game()
                                .scene()
                                .filter(|scene| {
                                    matches!(
                                        scene.materials().get(material_identifier),
                                        Some(Material::CellularStatic { .. })
                                    )
                                })
                                .map(|_| material_identifier),
                            _ => None,
                        };
                        if self.rigid_body_placement_enabled && static_material.is_some() {
                            self.rigid_body_placement_requested = self.hovered_cell();
                        }
                    }
                }
            }
            MouseInput {
                state: ElementState::Released,
                button: MouseButton::Left,
                ..
            } => {
                self.is_primary_scene_interaction_held = false;
                self.stroke_anchor = None;
                self.last_thermal_edit = None;
                self.last_impulse_edit = None;
            }
            MouseWheel { delta, .. } => {
                if ui_consumed && self.hovered_cell().is_none() {
                    return;
                }
                if self.hovered_cell().is_none() {
                    self.pixel_scroll_y = 0.0;
                    return;
                }
                use winit::event::MouseScrollDelta::*;
                match delta {
                    LineDelta(_, y) => self.adjust_brush_size(y.round() as i32),
                    PixelDelta(position) => {
                        self.pixel_scroll_y += position.y;
                        let adjustment: i32 = (self.pixel_scroll_y / 40.0).trunc() as i32;
                        if adjustment != 0 {
                            self.pixel_scroll_y -= f64::from(adjustment) * 40.0;
                            self.adjust_brush_size(adjustment);
                        }
                    }
                }
            }
            Focused(false) => {
                self.cursor_position = None;
                self.is_primary_scene_interaction_held = false;
                self.stroke_anchor = None;
                self.last_thermal_edit = None;
                self.last_impulse_edit = None;
                self.rigid_body_placement_requested = None;
            }
            _ => {}
        }
    }

    /// Enters free-fly using the reusable editor noclip pawn
    fn enter_free_fly(&mut self) {
        let position: ScenePosition = self.application.game().camera_target();
        let Some(scene) = self.application.game_mutable().scene_mutable() else {
            return;
        };
        if scene.possessed_actor() == self.editor_pawn {
            return;
        }
        self.original_pawn = scene.possessed_actor();
        self.original_pawn_position = self
            .original_pawn
            .and_then(|actor| scene.actor_registry().get_position(actor).copied());
        let editor_pawn: Actor = match self
            .editor_pawn
            .filter(|actor| scene.actor_registry().contains(*actor))
        {
            Some(actor) => actor,
            None => {
                let mut pawn: ActorPawn = ActorPawn::new();
                pawn.noclip = Some(ActorPawnNoclipConfiguration { speed: 8.0 });
                pawn.movement = Some(ActorPawnMovement::Noclip);
                pawn.simulate_when_paused = true;
                let actor: Actor = scene.actor_registry_mutable().spawn_possessable_pawn(
                    pawn,
                    position,
                    SceneVelocity { x: 0.0, y: 0.0 },
                );
                self.editor_pawn = Some(actor);
                actor
            }
        };
        scene
            .actor_registry_mutable()
            .set_position(editor_pawn, position);
        scene
            .actor_registry_mutable()
            .set_velocity(editor_pawn, SceneVelocity { x: 0.0, y: 0.0 });
        scene.possess_actor(editor_pawn);
    }

    /// Requests the saved pawn's area and restores possession when it is resident
    fn update_return(&mut self) {
        let Some(original_pawn) = self.original_pawn else {
            return;
        };
        let Some(scene) = self.application.game_mutable().scene_mutable() else {
            return;
        };
        if !scene.actor_registry().contains(original_pawn)
            || !scene.actor_registry().is_possessable(original_pawn)
        {
            self.original_pawn = None;
            self.original_pawn_position = None;
            self.is_return_pending = false;
            return;
        }
        if !self.is_return_pending {
            return;
        }
        let Some(position): Option<ScenePosition> = self.original_pawn_position else {
            return;
        };
        scene.request_area_around(position);
        if scene.is_position_resident(position) {
            scene
                .actor_registry_mutable()
                .set_position(original_pawn, position);
            scene.possess_actor(original_pawn);
            self.is_return_pending = false;
        }
    }

    pub fn launch(
        accelerator: Arc<engine::compute::Accelerator>,
        game: G,
    ) -> Result<(), Box<dyn Error>> {
        let _application_span =
            tracing::info_span!("application", title = G::TITLE, kind = "editor",).entered();
        tracing::info!("launching editor");
        let event_loop: winit::event_loop::EventLoop<()> = winit::event_loop::EventLoop::builder()
            .build()
            .inspect_err(|error| tracing::error!(%error, "failed to create event loop"))?;
        let mut application: EditorApplication<G> = Self::new(accelerator, game);
        event_loop.run_app(&mut application)?;
        application.application.finish()
    }
}

impl<G: Game> winit::application::ApplicationHandler for EditorApplication<G> {
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        self.application.window_initialize(event_loop);
    }

    fn window_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        _window_id: winit::window::WindowId,
        event: winit::event::WindowEvent,
    ) {
        let ui_consumed: bool = self.application.handle_window_event(event_loop, &event);
        self.handle_pointer_event(&event, ui_consumed);
    }

    fn about_to_wait(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        self.display();
        winit::application::ApplicationHandler::about_to_wait(&mut self.application, event_loop);
    }
}
