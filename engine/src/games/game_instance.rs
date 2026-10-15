// Copyright Rob Gage 2026

use super::Game;
use engine_graphics::{
    renders::SceneRenderer,
    world::scenes::SceneGraphics,
};
use engine_host::{
    acceleration::Accelerator,
    processors::Processor,
    system_events::{
        KeyCode,
        PhysicalKey,
        SystemKeyboardEvent,
        SystemMouseEvent,
        SystemWindowEvent,
    },
};
use engine_user_interface::{
    egui,
    UserInterfaceContext,
    UserInterfaceRenderer,
};
use engine_world::actors::{
    components::{
        ActorIdentityComponent,
        ActorPawnControlComponent,
        ActorPawnPossessionComponent,
    },
    Actor,
};
use std::{
    collections::HashSet,
    mem,
    time::Duration,
};

/// A `Game` running on the `Engine`, drawn with its user interface into any texture
pub struct GameInstance<G: Game> {
    /// The `Game` running in this `GameInstance`
    game: G,
    /// Keys held since focus was last released, controlling possessed pawns
    held_keys: HashSet<KeyCode>,
    /// Camera replacing the game's own camera, used by hosts such as the editor
    camera_actor: Option<Actor>,
    /// Renderer of the game's scene
    scene_renderer: SceneRenderer,
    /// User interface composed by the game over its scene
    user_interface_context: UserInterfaceContext,
    /// Renderer of the game's user interface, absent until the first frame reveals its format
    user_interface_renderer: Option<UserInterfaceRenderer>,
}

impl<G: Game> GameInstance<G> {

    /// Creates a new `GameInstance` of a `Game` drawn with an accelerator
    pub fn new(accelerator: &Accelerator, game: G) -> GameInstance<G> {
        GameInstance {
            game,
            held_keys: HashSet::new(),
            camera_actor: None,
            scene_renderer: SceneRenderer::new(accelerator),
            user_interface_context: UserInterfaceContext::new(),
            user_interface_renderer: None,
        }
    }

    /// Returns the `Game` running in this `GameInstance` for modification
    pub fn game_mutable(&mut self) -> &mut G { &mut self.game }

    /// Returns the user interface composed by the game
    pub fn user_interface_context(&self) -> &UserInterfaceContext { &self.user_interface_context }

    /// Sets a camera replacing the game's own camera for streaming and drawing, or `None`
    pub fn set_camera_actor(&mut self, camera_actor: Option<Actor>) {
        self.camera_actor = camera_actor;
    }

    /// Handles a host window event, releasing held keys when focus is lost
    pub fn window_event(&mut self, event: SystemWindowEvent) {
        if let SystemWindowEvent::FocusReleased = event { self.held_keys.clear(); }
        self.game.window_event(event);
    }

    /// Handles a host keyboard event, recording held keys for pawn control
    pub fn keyboard_event(&mut self, event: SystemKeyboardEvent) {
        if let PhysicalKey::Code(key_code) = event.physical_key {
            if event.pressed {
                self.held_keys.insert(key_code);
            } else {
                self.held_keys.remove(&key_code);
            }
        }
        self.game.keyboard_event(event);
    }

    /// Handles a host mouse event
    pub fn mouse_event(&mut self, event: SystemMouseEvent) { self.game.mouse_event(event); }

    /// Advances the game by one fixed update, then streams and simulates its scene
    pub fn update(
        &mut self,
        accelerator: &Accelerator,
        processor: &Processor,
        elapsed_time: Duration,
    ) {
        // the keyboard controls possessed pawns before the game updates them
        if let Some((scene, _)) = self.game.scene() {
            let is_held = |key_codes: [KeyCode; 2]| -> bool {
                key_codes.iter().any(|key_code| self.held_keys.contains(key_code))
            };
            let left: bool = is_held([KeyCode::KeyA, KeyCode::ArrowLeft]);
            let right: bool = is_held([KeyCode::KeyD, KeyCode::ArrowRight]);
            let pawn_control: ActorPawnControlComponent = ActorPawnControlComponent {
                movement: f32::from(right as u8) - f32::from(left as u8),
                is_jumping: is_held([KeyCode::KeyW, KeyCode::ArrowUp]),
                is_crouching: is_held([KeyCode::KeyS, KeyCode::ArrowDown]),
            };
            let possessed_actors: Vec<Actor> = scene.actor_registry_mutable()
                .components::<ActorIdentityComponent, ActorPawnPossessionComponent>()
                .map(|(actor_identity, _)| actor_identity.actor())
                .collect();
            for actor in possessed_actors {
                scene.actor_registry_mutable().insert(actor, pawn_control)
                    .unwrap_or_else(|error| panic!("Pawn possession failed: {error:?}"));
            }
        }
        self.game.update(elapsed_time);
        let camera_actor: Option<Actor> = self.camera_actor;
        let Some((scene, game_camera_actor)) = self.game.scene() else { return; };
        scene.stream(accelerator, processor, camera_actor.unwrap_or(game_camera_actor))
            .unwrap_or_else(|error| panic!("Scene streaming failed: {error:?}"));
        scene.simulate(accelerator)
            .unwrap_or_else(|error| panic!("Scene simulation failed: {error:?}"));
    }

    /// Streams the game's scene around its camera without updating or simulating the game
    pub fn stream(&mut self, accelerator: &Accelerator, processor: &Processor) {
        let camera_actor: Option<Actor> = self.camera_actor;
        let Some((scene, game_camera_actor)) = self.game.scene() else { return; };
        scene.stream(accelerator, processor, camera_actor.unwrap_or(game_camera_actor))
            .unwrap_or_else(|error| panic!("Scene streaming failed: {error:?}"));
    }

    /// Draws the scene and composed user interface into a texture, returning its platform output
    pub fn render(
        &mut self,
        accelerator: &Accelerator,
        texture_view: &wgpu::TextureView,
        input: egui::RawInput,
    ) -> egui::PlatformOutput {
        let device: &wgpu::Device = accelerator.device();
        let mut command_encoder: wgpu::CommandEncoder = device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("`GameInstance::render`"),
            });
        // draw the scene, or clear the frame when the game shows none
        let camera_actor: Option<Actor> = self.camera_actor;
        match self.game.scene() {
            Some((scene, game_camera_actor)) => {
                let graphics: SceneGraphics<'_> = scene
                    .graphics(camera_actor.unwrap_or(game_camera_actor))
                    .unwrap_or_else(|error| panic!("Scene graphics failed: {error:?}"));
                self.scene_renderer.render(accelerator, texture_view, &graphics);
            }
            None => {
                command_encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("`GameInstance::render` clear"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: texture_view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
            }
        }
        // compose the user interface and draw it over the scene
        let game: &mut G = &mut self.game;
        let mut output: egui::FullOutput = self.user_interface_context
            .run(input, |user_interface| game.user_interface(user_interface));
        let platform_output: egui::PlatformOutput = mem::take(&mut output.platform_output);
        let texture: &wgpu::Texture = texture_view.texture();
        let callback_command_buffers: Vec<wgpu::CommandBuffer> = self.user_interface_renderer
            .get_or_insert_with(|| UserInterfaceRenderer::new(device, texture.format()))
            .render(
                self.user_interface_context.egui_context(),
                device,
                accelerator.queue(),
                &mut command_encoder,
                texture_view,
                output,
                [texture.width(), texture.height()],
            );
        accelerator.queue().submit(
            callback_command_buffers.into_iter().chain(Some(command_encoder.finish())),
        );
        platform_output
    }

}
