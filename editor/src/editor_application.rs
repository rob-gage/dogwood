// Copyright Rob Gage 2026

use engine::{
    Game,
    GameApplication,
    physics::{
        actors::{
            Actor,
            ActorPawn,
            ActorPawnMovement,
            ActorPawnNoclipConfiguration,
        },
        scenes::{
            ScenePosition,
            SceneVelocity,
        },
        tiles::CellCoordinates,
    },
};
use engine_user_interface::widgets::{
    Button,
    Spacer,
    StackHorizontal,
    StackVertical,
};
use engine_graphics::Color;
use std::{
    cell::Cell,
    error::Error,
    rc::Rc,
    sync::Arc,
};
use crate::viewport_area::ViewportArea;

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
}

impl<G: Game> EditorApplication<G> {

    fn new(accelerator: Arc<engine::compute::Accelerator>, game: G) -> Self {
        let mut application: GameApplication<G> = GameApplication::new_with_title(
            accelerator,
            game,
            format!("Engine Editor: {}", G::TITLE)
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
        }
    }

    /// Returns the cell currently beneath the cursor in the rendered Scene viewport
    fn hovered_cell(&self) -> Option<CellCoordinates> {
        self.application.scene_world_position(self.cursor_position?)
            .map(CellCoordinates::from_world_position)
    }

    /// Updates the editor's pointer state after the user interface handles an event
    fn handle_pointer_event(&mut self, event: &winit::event::WindowEvent, ui_consumed: bool) {
        use winit::event::{
            ElementState,
            MouseButton,
            WindowEvent::*,
        };
        match event {
            CursorMoved { position, .. } => {
                self.cursor_position = Some([position.x as f32, position.y as f32]);
            }
            CursorLeft { .. } => self.cursor_position = None,
            MouseInput { state: ElementState::Pressed, button: MouseButton::Left, .. } => {
                if !self.is_primary_scene_interaction_held {
                    self.is_primary_scene_interaction_held =
                        !ui_consumed && self.hovered_cell().is_some();
                }
            }
            MouseInput { state: ElementState::Released, button: MouseButton::Left, .. } => {
                self.is_primary_scene_interaction_held = false;
            }
            Focused(false) => {
                self.cursor_position = None;
                self.is_primary_scene_interaction_held = false;
            }
            _ => {}
        }
    }

    /// Enters free-fly using the reusable editor noclip pawn
    fn enter_free_fly(&mut self) {
        let position: ScenePosition = self.application.game().camera_target();
        let Some(scene) = self.application.game_mutable().scene_mutable() else { return; };
        if scene.possessed_actor() == self.editor_pawn { return; }
        self.original_pawn = scene.possessed_actor();
        self.original_pawn_position = self.original_pawn.and_then(|actor| {
            scene.actor_registry().get_position(actor).copied()
        });
        let editor_pawn: Actor = match self.editor_pawn.filter(|actor| {
            scene.actor_registry().contains(*actor)
        }) {
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
        scene.actor_registry_mutable().set_position(editor_pawn, position);
        scene.actor_registry_mutable().set_velocity(
            editor_pawn,
            SceneVelocity { x: 0.0, y: 0.0 },
        );
        scene.possess_actor(editor_pawn);
    }

    /// Requests the saved pawn's area and restores possession when it is resident
    fn update_return(&mut self) {
        let Some(original_pawn) = self.original_pawn else { return; };
        let Some(scene) = self.application.game_mutable().scene_mutable() else { return; };
        if !scene.actor_registry().contains(original_pawn) ||
                !scene.actor_registry().is_possessable(original_pawn) {
            self.original_pawn = None;
            self.original_pawn_position = None;
            self.is_return_pending = false;
            return;
        }
        if !self.is_return_pending { return; }
        let Some(position): Option<ScenePosition> = self.original_pawn_position else { return; };
        scene.request_area_around(position);
        if scene.is_position_resident(position) {
            scene.actor_registry_mutable().set_position(original_pawn, position);
            scene.possess_actor(original_pawn);
            self.is_return_pending = false;
        }
    }

    /// Builds the editor layout and handles free-fly controls for this frame
    fn display(&mut self) {
        self.update_return();
        let free_fly_enabled: bool = self.application.game().scene().is_some_and(|scene| {
            scene.possessed_actor() != self.editor_pawn
        });
        let return_enabled: bool = !self.is_return_pending &&
            self.application.game().scene().is_some_and(|scene| {
            scene.possessed_actor() == self.editor_pawn && self.original_pawn.is_some_and(|actor| {
                scene.actor_registry().contains(actor) &&
                    scene.actor_registry().is_possessable(actor)
            })
        });
        let free_fly_requested: Rc<Cell<bool>> = Rc::new(Cell::new(false));
        let return_requested: Rc<Cell<bool>> = Rc::new(Cell::new(false));
        let play_requested: Rc<Cell<bool>> = Rc::new(Cell::new(false));
        let free_fly_action: Rc<Cell<bool>> = free_fly_requested.clone();
        let return_action: Rc<Cell<bool>> = return_requested.clone();
        let play_action: Rc<Cell<bool>> = play_requested.clone();
        let background: Color = Color::new_rgba(47, 47, 47, 255);
        let background_dark: Color = Color::new_rgba(37, 37, 37, 255);
        let viewport_bounds: Rc<Cell<Option<[u32; 4]>>> = Rc::new(Cell::new(None));
        let top_bar: StackHorizontal = StackHorizontal::new()
            .with_height(32.0)
            .with_spacing(4.0)
            .with_background_color(&background_dark)
            .with_child(Button::new(if self.is_playing { "Pause" } else { "Play" }, move || {
                play_action.set(true)
            }))
            .with_child(Button::new("Free Fly", move || free_fly_action.set(true))
                .with_enabled(free_fly_enabled))
            .with_child(Button::new("Return", move || return_action.set(true))
                .with_enabled(return_enabled))
            .with_child(Spacer::new_flexible());
        let content: StackHorizontal = StackHorizontal::new()
            .with_child(Spacer::new(128.0).with_background_color(&background))
            .with_child(ViewportArea(viewport_bounds.clone()))
            .with_child(Spacer::new(32.0).with_background_color(&background));
        let mut layout: StackVertical = StackVertical::new()
            .with_child(top_bar)
            .with_child(content)
            .with_child(Spacer::new(16.0).with_background_color(&background_dark));
        self.application.add_widget(&mut layout);
        self.application.set_scene_viewport_bounds(viewport_bounds.get());
        if play_requested.get() {
            self.is_playing = !self.is_playing;
            self.application.set_simulation_enabled(self.is_playing);
        }
        if free_fly_requested.get() { self.enter_free_fly(); }
        if return_requested.get() {
            self.is_return_pending = true;
            self.update_return();
        }
    }
    pub fn launch(
        accelerator: Arc<engine::compute::Accelerator>,
        game: G,
    ) -> Result<(), Box<dyn Error>> {
        let event_loop: winit::event_loop::EventLoop<()> =
            winit::event_loop::EventLoop::builder().build()?;
        let mut application: EditorApplication<G> = Self::new(accelerator, game);
        event_loop.run_app(&mut application)?;
        application.application.finish()
    }

}

impl<G: Game> winit::application::ApplicationHandler for EditorApplication<G> {

    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop)
    { self.application.window_initialize(event_loop); }

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
