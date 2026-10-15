// Copyright Rob Gage 2026

use super::EngineError;
use crate::games::{
    Game,
    GameInstance,
};
use engine_graphics::{
    renders::SceneRenderer,
    world::scenes::SceneGraphics,
};
use engine_world::actors::{
    components::{
        ActorIdentityComponent,
        ActorPawnControlComponent,
        ActorPawnPossessionComponent,
    },
    Actor,
};
use engine_host::{
    acceleration::Accelerator,
    processors::Processor,
    system_events::{
        SystemKeyboardEvent,
        SystemMouseEvent,
        SystemWindowEvent,
    },
    windows::{
        Window,
        WindowConfiguration,
    },
};
use engine_user_interface::{
    egui,
    UserInterfaceContext,
    UserInterfaceRenderer,
};
use std::{
    cell::{
        Ref,
        RefCell,
        RefMut,
    },
    collections::HashSet,
    mem,
    time::Duration,
};

/// Fixed update frequency in updates per second
const UPDATES_PER_SECOND: u32 = 60;

/// Maximum number of fixed updates retained after a delayed event-loop iteration
const MAXIMUM_ACCUMULATED_UPDATES: u32 = 6;

/// An engine runtime to be used by `Game`s
pub struct Engine {
    /// Shared graphics and compute context used by this engine
    accelerator: Accelerator,
    /// Named CPU execution pools used by this engine
    processor: Processor,
}

impl Engine {

    /// Creates an engine from initialized graphics and CPU processing resources
    pub const fn new(accelerator: Accelerator, processor: Processor) -> Self {
        Self { accelerator, processor }
    }

    /// Returns the shared graphics and compute context
    pub const fn accelerator(&self) -> &Accelerator { &self.accelerator }

    /// Returns the CPU execution pool manager
    pub const fn processor(&self) -> &Processor { &self.processor }

    /// Launches a game using this `Engine`
    pub fn launch<G: Game>(self, game: G) -> Result<(), EngineError> {
        let game_instance: GameInstance<G> = GameInstance::new(self, game);
        let (engine, mut game): (Engine, G) = game_instance.into_parts();
        let window_configuration: WindowConfiguration = game.window_configuration();
        let shared_game: RefCell<&mut G> = RefCell::new(&mut game);
        // held keys survive between fixed updates and are released when focus is lost
        let held_keys: RefCell<HashSet<winit::keyboard::KeyCode>> = RefCell::new(HashSet::new());
        let mut window_event_handler: Box<dyn FnMut(SystemWindowEvent)> =
            Box::new(|event: SystemWindowEvent| {
                if let SystemWindowEvent::FocusReleased = event { held_keys.borrow_mut().clear(); }
                shared_game.borrow_mut().window_event(event);
            });
        let mut keyboard_event_handler: Box<dyn FnMut(SystemKeyboardEvent)> =
            Box::new(|event: SystemKeyboardEvent| {
                if let winit::keyboard::PhysicalKey::Code(key_code) = event.physical_key {
                    if event.pressed {
                        held_keys.borrow_mut().insert(key_code);
                    } else {
                        held_keys.borrow_mut().remove(&key_code);
                    }
                }
                shared_game.borrow_mut().keyboard_event(event);
            });
        let mut mouse_event_handler: Box<dyn FnMut(SystemMouseEvent)> =
            Box::new(|event: SystemMouseEvent| {
                shared_game.borrow_mut().mouse_event(event);
            });
        let mut update_handler: Box<dyn FnMut(Duration)> =
            Box::new(|elapsed_time: Duration| {
                let mut game: RefMut<'_, &mut G> = shared_game.borrow_mut();
                // the keyboard controls possessed pawns before the game updates them
                if let Some((scene, _)) = game.scene() {
                    let held_keys: Ref<'_, HashSet<winit::keyboard::KeyCode>> = held_keys.borrow();
                    let is_held = |key_codes: [winit::keyboard::KeyCode; 2]| -> bool {
                        key_codes.iter().any(|key_code| held_keys.contains(key_code))
                    };
                    let left: bool = is_held([
                        winit::keyboard::KeyCode::KeyA,
                        winit::keyboard::KeyCode::ArrowLeft,
                    ]);
                    let right: bool = is_held([
                        winit::keyboard::KeyCode::KeyD,
                        winit::keyboard::KeyCode::ArrowRight,
                    ]);
                    let pawn_control: ActorPawnControlComponent = ActorPawnControlComponent {
                        movement: f32::from(right as u8) - f32::from(left as u8),
                        is_jumping: is_held([
                            winit::keyboard::KeyCode::KeyW,
                            winit::keyboard::KeyCode::ArrowUp,
                        ]),
                        is_crouching: is_held([
                            winit::keyboard::KeyCode::KeyS,
                            winit::keyboard::KeyCode::ArrowDown,
                        ]),
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
                game.update(elapsed_time);
                let Some((scene, camera_actor)) = game.scene() else { return; };
                scene.stream(engine.accelerator(), engine.processor(), camera_actor)
                    .unwrap_or_else(|error| panic!("Scene streaming failed: {error:?}"));
                scene.simulate(engine.accelerator())
                    .unwrap_or_else(|error| panic!("Scene simulation failed: {error:?}"));
            });
        let mut scene_renderer: SceneRenderer = SceneRenderer::new(engine.accelerator());
        let user_interface_context: UserInterfaceContext = UserInterfaceContext::new();
        let mut user_interface_renderer: Option<UserInterfaceRenderer> = None;
        let mut render_handler: Box<
            dyn FnMut(&wgpu::TextureView, egui::RawInput) -> egui::PlatformOutput,
        > = Box::new(|texture_view: &wgpu::TextureView, input: egui::RawInput| {
            let mut game: RefMut<'_, &mut G> = shared_game.borrow_mut();
            let device: &wgpu::Device = engine.accelerator().device();
            let mut command_encoder: wgpu::CommandEncoder = device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Engine user interface"),
                });
            // draw the scene, or clear the frame when the game shows none
            match game.scene() {
                Some((scene, camera_actor)) => {
                    let graphics: SceneGraphics<'_> = scene.graphics(camera_actor)
                        .unwrap_or_else(|error| panic!("Scene graphics failed: {error:?}"));
                    scene_renderer.render(engine.accelerator(), texture_view, &graphics);
                }
                None => {
                    command_encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("Engine frame clear"),
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
            let mut output: egui::FullOutput = user_interface_context
                .run(input, |user_interface| game.user_interface(user_interface));
            let platform_output: egui::PlatformOutput = mem::take(&mut output.platform_output);
            let texture: &wgpu::Texture = texture_view.texture();
            let callback_command_buffers: Vec<wgpu::CommandBuffer> = user_interface_renderer
                .get_or_insert_with(|| UserInterfaceRenderer::new(device, texture.format()))
                .render(
                    user_interface_context.egui_context(),
                    device,
                    engine.accelerator().queue(),
                    &mut command_encoder,
                    texture_view,
                    output,
                    [texture.width(), texture.height()],
                );
            engine.accelerator().queue().submit(
                callback_command_buffers.into_iter().chain(Some(command_encoder.finish())),
            );
            platform_output
        });
        let update_interval: Duration = Duration::from_nanos(
            1_000_000_000 / u64::from(UPDATES_PER_SECOND),
        );
        let maximum_accumulated_time: Duration = update_interval * MAXIMUM_ACCUMULATED_UPDATES;
        Window::open(
            &mut window_event_handler,
            &mut keyboard_event_handler,
            &mut mouse_event_handler,
            &mut update_handler,
            &mut render_handler,
            engine.accelerator(),
            &user_interface_context,
            window_configuration,
            update_interval,
            maximum_accumulated_time,
        ).map_err(EngineError::from)
    }

}

impl Default for Engine {

    /// Creates an engine with the default accelerator and processor, panicking if either fails
    fn default() -> Self { Self::new(Accelerator::default(), Processor::default()) }

}
