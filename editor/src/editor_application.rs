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
}

impl<G: Game> EditorApplication<G> {

    fn new(accelerator: Arc<engine::compute::Accelerator>, game: G) -> Self {
        Self {
            application: GameApplication::new_with_title(
                accelerator,
                game,
                format!("Engine Editor: {}", G::TITLE)
            ),
            editor_pawn: None,
            original_pawn: None,
            original_pawn_position: None,
            is_return_pending: false,
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
        let free_fly_action: Rc<Cell<bool>> = free_fly_requested.clone();
        let return_action: Rc<Cell<bool>> = return_requested.clone();
        let background: Color = Color::new_rgba(47, 47, 47, 255);
        let background_dark: Color = Color::new_rgba(37, 37, 37, 255);
        let viewport_bounds: Rc<Cell<Option<[u32; 4]>>> = Rc::new(Cell::new(None));
        let top_bar: StackHorizontal = StackHorizontal::new()
            .with_height(32.0)
            .with_spacing(4.0)
            .with_background_color(&background_dark)
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
    ) { self.application.handle_window_event(event_loop, event); }

    fn about_to_wait(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        self.display();
        winit::application::ApplicationHandler::about_to_wait(&mut self.application, event_loop);
    }

}
