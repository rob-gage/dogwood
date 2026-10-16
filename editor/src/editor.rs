// Copyright Rob Gage 2026

use crate::{
    editor_brush::EditorBrush,
    editor_tab::EditorTab,
    editor_tool::EditorTool,
    editor_viewport::EditorViewport,
};
use engine::{
    actors::{
        components::{
            ActorCameraComponent,
            ActorKinematicsPositionComponent,
            ActorPersistenceComponent,
        },
        Actor,
        ActorRegistry,
    },
    games::{
        Game,
        GameInstance,
    },
    materials::{
        MaterialForm,
        MaterialIdentifier,
        MaterialTable,
    },
    runtime::{
        Engine,
        EngineError,
    },
    scenes::{
        cellular_tiles::{
            CellCoordinates,
            CellularTileRegion,
            CELLULAR_TILE_SIZE,
        },
        chunks::CHUNK_SIZE,
        edits::{
            Edit,
            EditError,
            EditMode,
        },
        ScenePosition,
    },
    system_events::{
        KeyCode,
        PhysicalKey,
        SystemKeyboardEvent,
        SystemMouseEvent,
        SystemWindowEvent,
    },
    user_interface::{
        advanced::DockWorkspaceState,
        components::{
            InspectorSection,
            Notifications,
            SettingsRow,
            StatusBar,
            Toolbar,
        },
        egui,
        egui_dock,
        wgpu,
        UserInterfaceContext,
        UserInterfaceRenderer,
    },
    Accelerator,
    Processor,
};
use engine_host::windows::{
    Window,
    WindowConfiguration,
};
use std::{
    cell::RefCell,
    collections::HashSet,
    mem,
    time::{
        Duration,
        Instant,
    },
};

/// Fixed update frequency in updates per second
const EDITOR_UPDATES_PER_SECOND: u32 = 60;

/// Maximum number of fixed updates retained after a delayed event-loop iteration
const EDITOR_MAXIMUM_ACCUMULATED_UPDATES: u32 = 6;

/// Initial width of the tools tab in points, leaving the rest of the window to the viewport
const EDITOR_TOOLS_WIDTH: f32 = 260.0;

/// Largest brush size in cells
const EDITOR_BRUSH_SIZE_MAXIMUM: u32 = 64;

/// Zoom range of the free camera, within the region streamed for the game camera
const EDITOR_CAMERA_ZOOM_RANGE: (f32, f32) = (0.25, 2.0);

/// Editor hosting a game in a scalable viewport beside its scene editing tools
pub struct Editor<G: Game> {
    /// The game drawn into the viewport
    game_instance: GameInstance<G>,
    /// User interface of the editor around the viewport
    user_interface_context: UserInterfaceContext,
    /// Renderer of the editor user interface, absent until the first frame reveals its format
    user_interface_renderer: Option<UserInterfaceRenderer>,
    /// Docked viewport and tool tabs, taken while the dock is shown
    dock_workspace_state: Option<DockWorkspaceState<EditorTab>>,
    /// Whether the tools tab has been given its initial width
    is_dock_arranged: bool,
    /// Texture the game is drawn into, absent until the viewport tab is first shown
    viewport: Option<EditorViewport>,
    /// Viewport tab rectangle in points, absent when the tab was not shown in the last frame
    viewport_rect: Option<egui::Rect>,
    /// Physical pixels per point of the editor user interface
    pixels_per_point: f32,
    /// Whether the pointer is over the viewport
    is_viewport_hovered: bool,
    /// Whether the viewport was clicked since another part of the editor was
    is_viewport_focused: bool,
    /// Whether an editor control is using the keyboard
    is_keyboard_wanted: bool,
    /// Whether the game updates and simulates rather than only streaming its scene
    is_playing: bool,
    /// Free camera replacing the game's camera, absent while viewing through the game
    camera_actor: Option<Actor>,
    /// Keys held to move the free camera
    held_keys: HashSet<KeyCode>,
    /// Action applied by strokes
    tool: EditorTool,
    /// Whether material strokes keep or replace existing matter
    mode: EditMode,
    /// Shape stamped along strokes
    brush: EditorBrush,
    /// Width of the brush in cells
    brush_size: u32,
    /// Color of cells filled by material strokes
    color: egui::Color32,
    /// Whether cellular tile boundaries are drawn over the viewport
    is_tile_grid_shown: bool,
    /// Whether chunk boundaries are drawn over the viewport
    is_chunk_grid_shown: bool,
    /// Cell under the pointer, absent while the pointer is outside the viewport
    cursor_cell: Option<CellCoordinates>,
    /// Cell of the previous sample of the current stroke, absent between strokes
    stroke_cell: Option<CellCoordinates>,
    /// Error of the latest rejected edit, cleared by the next accepted edit
    edit_error: Option<EditError>,
    /// Whether edits were requested since the last save request
    is_edited: bool,
    /// Whether a requested save has not yet completed
    is_save_pending: bool,
    /// Expiring messages shown over the editor
    notifications: Notifications,
    /// Frames and updates counted since the rates were last measured
    counts: (u32, u32),
    /// Time the rates were last measured
    measurement_time: Instant,
    /// Measured frames and updates per second
    rates: (u32, u32),
}

impl<G: Game> Editor<G> {

    /// Launches a game inside the editor using an `Engine`
    pub fn launch(engine: Engine, game: G) -> Result<(), EngineError> {
        let window_configuration: WindowConfiguration = game.window_configuration();
        let mut game_instance: GameInstance<G> = GameInstance::new(engine.accelerator(), game);
        // start with the first registered material when the game has a scene
        let tool: EditorTool = game_instance.game_mutable().scene()
            .and_then(|(scene, _)| Self::materials(scene.material_table()).into_iter().next())
            .map_or(EditorTool::Eraser, |(material_identifier, _)| {
                EditorTool::Material(material_identifier)
            });
        let mut dock_workspace_state: DockWorkspaceState<EditorTab> =
            DockWorkspaceState::new(vec![EditorTab::Viewport]);
        dock_workspace_state.dock_state_mut().main_surface_mut().split_left(
            egui_dock::NodeIndex::root(),
            0.5,
            vec![EditorTab::Tools],
        );
        let editor: RefCell<Editor<G>> = RefCell::new(Editor {
            game_instance,
            user_interface_context: UserInterfaceContext::new(),
            user_interface_renderer: None,
            dock_workspace_state: Some(dock_workspace_state),
            is_dock_arranged: false,
            viewport: None,
            viewport_rect: None,
            pixels_per_point: 1.0,
            is_viewport_hovered: false,
            is_viewport_focused: false,
            is_keyboard_wanted: false,
            is_playing: false,
            camera_actor: None,
            held_keys: HashSet::new(),
            tool,
            mode: EditMode::Insert,
            brush: EditorBrush::Circle,
            brush_size: 8,
            color: egui::Color32::WHITE,
            is_tile_grid_shown: false,
            is_chunk_grid_shown: false,
            cursor_cell: None,
            stroke_cell: None,
            edit_error: None,
            is_edited: false,
            is_save_pending: false,
            notifications: Notifications::default(),
            counts: (0, 0),
            measurement_time: Instant::now(),
            rates: (0, 0),
        });
        let user_interface_context: UserInterfaceContext =
            editor.borrow().user_interface_context.clone();
        let mut window_event_handler: Box<dyn FnMut(SystemWindowEvent)> =
            Box::new(|event: SystemWindowEvent| editor.borrow_mut().window_event(event));
        let mut keyboard_event_handler: Box<dyn FnMut(SystemKeyboardEvent)> =
            Box::new(|event: SystemKeyboardEvent| editor.borrow_mut().keyboard_event(event));
        let mut mouse_event_handler: Box<dyn FnMut(SystemMouseEvent)> =
            Box::new(|event: SystemMouseEvent| editor.borrow_mut().mouse_event(event));
        let mut update_handler: Box<dyn FnMut(Duration)> = Box::new(|elapsed_time: Duration| {
            editor.borrow_mut().update(engine.accelerator(), engine.processor(), elapsed_time);
        });
        let mut render_handler: Box<
            dyn FnMut(&wgpu::TextureView, egui::RawInput) -> egui::PlatformOutput,
        > = Box::new(|texture_view: &wgpu::TextureView, input: egui::RawInput| {
            editor.borrow_mut().render(engine.accelerator(), texture_view, input)
        });
        let update_interval: Duration = Duration::from_nanos(
            1_000_000_000 / u64::from(EDITOR_UPDATES_PER_SECOND),
        );
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
            update_interval * EDITOR_MAXIMUM_ACCUMULATED_UPDATES,
        ).map_err(EngineError::from)
    }

    /// Returns whether viewport input goes to the game rather than to editing
    fn is_game_input(&self) -> bool { self.is_playing && self.camera_actor.is_none() }

    /// Handles a host window event, releasing held keys when focus is lost
    fn window_event(&mut self, event: SystemWindowEvent) {
        if let SystemWindowEvent::FocusReleased = event { self.held_keys.clear(); }
        self.game_instance.window_event(event);
    }

    /// Routes a key to the free camera or the focused game, releasing keys in both
    fn keyboard_event(&mut self, event: SystemKeyboardEvent) {
        if !event.pressed {
            if let PhysicalKey::Code(key_code) = event.physical_key {
                self.held_keys.remove(&key_code);
            }
            self.game_instance.keyboard_event(event);
            return;
        }
        if self.is_keyboard_wanted { return; }
        if self.camera_actor.is_some() && (self.is_viewport_hovered || self.is_viewport_focused) {
            if let PhysicalKey::Code(key_code) = event.physical_key {
                self.held_keys.insert(key_code);
            }
        } else if self.is_game_input() && self.is_viewport_focused {
            self.game_instance.keyboard_event(event);
        }
    }

    /// Forwards pointer input over the viewport to the game in viewport pixels
    fn mouse_event(&mut self, event: SystemMouseEvent) {
        let Some(rect) = self.viewport_rect else { return; };
        if !self.is_game_input() || !self.is_viewport_hovered { return; }
        let event: SystemMouseEvent = match event {
            SystemMouseEvent::CursorMoved { x, y } => SystemMouseEvent::CursorMoved {
                x: x - f64::from(rect.min.x * self.pixels_per_point),
                y: y - f64::from(rect.min.y * self.pixels_per_point),
            },
            event => event,
        };
        self.game_instance.mouse_event(event);
    }

    /// Moves the free camera, then plays the game or only streams its scene while paused
    fn update(&mut self, accelerator: &Accelerator, processor: &Processor, elapsed_time: Duration) {
        self.counts.1 += 1;
        if let Some(camera_actor) = self.camera_actor
            && let Some((scene, _)) = self.game_instance.game_mutable().scene()
        {
            let is_held = |key_codes: [KeyCode; 2]| -> bool {
                key_codes.iter().any(|key_code| self.held_keys.contains(key_code))
            };
            let horizontal: f64 = f64::from(is_held([KeyCode::KeyD, KeyCode::ArrowRight]) as u8)
                - f64::from(is_held([KeyCode::KeyA, KeyCode::ArrowLeft]) as u8);
            let vertical: f64 = f64::from(is_held([KeyCode::KeyW, KeyCode::ArrowUp]) as u8)
                - f64::from(is_held([KeyCode::KeyS, KeyCode::ArrowDown]) as u8);
            let actor_registry: &mut ActorRegistry = scene.actor_registry_mutable();
            // the camera crosses its visible width in one second
            let speed: f64 = actor_registry.component::<ActorCameraComponent>(camera_actor)
                .map_or(0.0, |camera| f64::from(camera.visible_width()))
                * elapsed_time.as_secs_f64();
            if let Ok(position) =
                actor_registry.component_mutable::<ActorKinematicsPositionComponent>(camera_actor)
            {
                position.position =
                    position.position.translate(horizontal * speed, vertical * speed);
            }
        }
        if self.is_playing {
            self.game_instance.update(accelerator, processor, elapsed_time);
        } else {
            self.game_instance.stream(accelerator, processor);
        }
    }

    /// Composes the editor, draws the game into the viewport, then draws the editor
    fn render(
        &mut self,
        accelerator: &Accelerator,
        texture_view: &wgpu::TextureView,
        input: egui::RawInput,
    ) -> egui::PlatformOutput {
        let device: &wgpu::Device = accelerator.device();
        let texture: &wgpu::Texture = texture_view.texture();
        self.counts.0 += 1;
        if self.measurement_time.elapsed() >= Duration::from_secs(1) {
            self.rates = mem::take(&mut self.counts);
            self.measurement_time = Instant::now();
        }
        // the game receives input placed by the viewport of the previous frame
        let game_input: egui::RawInput = self.render_game_input(&input);
        let user_interface_context: UserInterfaceContext = self.user_interface_context.clone();
        let mut output: egui::FullOutput = user_interface_context
            .run(input, |user_interface| self.render_editor(user_interface));
        let platform_output: egui::PlatformOutput = mem::take(&mut output.platform_output);
        self.is_keyboard_wanted = user_interface_context.egui_context().egui_wants_keyboard_input();
        let user_interface_renderer: &mut UserInterfaceRenderer = self.user_interface_renderer
            .get_or_insert_with(|| UserInterfaceRenderer::new(device, texture.format()));
        // size the viewport texture to its tab, then draw the game into it
        if let Some(rect) = self.viewport_rect {
            let width: u32 = ((rect.width() * self.pixels_per_point).round() as u32).max(1);
            let height: u32 = ((rect.height() * self.pixels_per_point).round() as u32).max(1);
            let viewport: &mut EditorViewport = match self.viewport.as_mut() {
                Some(viewport) => {
                    viewport.resize(accelerator, user_interface_renderer, width, height);
                    viewport
                }
                None => self.viewport.insert(
                    EditorViewport::new(accelerator, user_interface_renderer, width, height),
                ),
            };
            let _ = self.game_instance.render(accelerator, viewport.texture_view(), game_input);
        }
        // draw the editor over a cleared window
        let mut command_encoder: wgpu::CommandEncoder = device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("`Editor::render`"),
            });
        command_encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("`Editor::render` clear"),
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
        let callback_command_buffers: Vec<wgpu::CommandBuffer> = user_interface_renderer.render(
            user_interface_context.egui_context(),
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

    /// Returns the editor input that belongs to the game, in viewport points
    fn render_game_input(&self, input: &egui::RawInput) -> egui::RawInput {
        let Some(rect) = self.viewport_rect else { return egui::RawInput::default(); };
        let offset: egui::Vec2 = rect.min.to_vec2();
        let events: Vec<egui::Event> = if !self.is_game_input() { Vec::new() } else {
            input.events.iter()
                .filter_map(|event| match event {
                    egui::Event::PointerMoved(position) if self.is_viewport_hovered => {
                        Some(egui::Event::PointerMoved(*position - offset))
                    }
                    // releases always reach the game so no button stays held
                    egui::Event::PointerButton { pos, button, pressed, modifiers }
                        if self.is_viewport_hovered || !pressed => {
                        Some(egui::Event::PointerButton {
                            pos: *pos - offset,
                            button: *button,
                            pressed: *pressed,
                            modifiers: *modifiers,
                        })
                    }
                    egui::Event::MouseWheel { .. } if self.is_viewport_hovered => {
                        Some(event.clone())
                    }
                    egui::Event::Key { .. }
                    | egui::Event::Text(_)
                    | egui::Event::Copy
                    | egui::Event::Cut
                    | egui::Event::Paste(_) if self.is_viewport_focused => Some(event.clone()),
                    egui::Event::PointerGone => Some(egui::Event::PointerGone),
                    _ => None,
                })
                .collect()
        };
        egui::RawInput {
            viewport_id: input.viewport_id,
            viewports: input.viewports.clone(),
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, rect.size())),
            max_texture_side: input.max_texture_side,
            time: input.time,
            predicted_dt: input.predicted_dt,
            events,
            focused: input.focused && self.is_viewport_focused,
            ..egui::RawInput::default()
        }
    }

    /// Composes the toolbar, status bar, and docked tabs of the editor
    fn render_editor(&mut self, user_interface: &mut egui::Ui) {
        let save_shortcut: egui::KeyboardShortcut =
            egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::S);
        if user_interface.input_mut(|input| input.consume_shortcut(&save_shortcut)) {
            self.render_save();
        }
        if self.is_save_pending
            && self.game_instance.game_mutable().scene().is_some_and(|(scene, _)| scene.is_saved())
        {
            self.is_save_pending = false;
            self.notifications
                .push("Scene saved", egui::Color32::LIGHT_GREEN, Duration::from_secs(3));
        }
        egui::Panel::top("editor-toolbar").show(user_interface, |user_interface| {
            Toolbar::show(user_interface, |user_interface| {
                let play_text: &str = if self.is_playing { "Pause" } else { "Play" };
                if user_interface.button(play_text).clicked() {
                    self.is_playing = !self.is_playing;
                }
                let mut is_camera_free: bool = self.camera_actor.is_some();
                if user_interface.toggle_value(&mut is_camera_free, "Free camera").clicked() {
                    self.render_camera();
                }
                user_interface.separator();
                if user_interface.button("Save").on_hover_text("Ctrl+S").clicked() {
                    self.render_save();
                }
            });
        });
        egui::Panel::bottom("editor-status-bar").show(user_interface, |user_interface| {
            StatusBar::show(user_interface, |user_interface| {
                user_interface.label(if self.is_playing { "Playing" } else { "Paused" });
                user_interface.separator();
                user_interface.label(match self.cursor_cell {
                    Some(cell) => format!("Cell {}, {}", cell.x, cell.y),
                    None => String::from("Cell –"),
                });
                if let Some(edit_error) = &self.edit_error {
                    user_interface.separator();
                    user_interface.colored_label(
                        egui::Color32::LIGHT_RED,
                        format!("Edit rejected: {edit_error:?}"),
                    );
                }
                user_interface.with_layout(
                    egui::Layout::right_to_left(egui::Align::Center),
                    |user_interface| {
                        user_interface.label(format!("{} FPS  {} TPS", self.rates.0, self.rates.1));
                        user_interface.separator();
                        user_interface.label(if self.is_save_pending { "Saving…" }
                            else if self.is_edited { "Unsaved changes" }
                            else { "Saved" });
                    },
                );
            });
        });
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(user_interface, |user_interface| {
                self.viewport_rect = None;
                let mut dock_workspace_state: DockWorkspaceState<EditorTab> = self
                    .dock_workspace_state
                    .take()
                    .expect("dock workspace should be present outside its own composition");
                // splits are fractions of the window, known only once the editor is first shown
                if !self.is_dock_arranged {
                    self.is_dock_arranged = true;
                    if let egui_dock::Node::Horizontal(split) = &mut dock_workspace_state
                        .dock_state_mut()
                        .main_surface_mut()[egui_dock::NodeIndex::root()]
                    {
                        split.fraction = (EDITOR_TOOLS_WIDTH / user_interface.available_width())
                            .clamp(0.05, 0.5);
                    }
                }
                // panels fill the window edge to edge, so their bodies and tab bars are square
                let mut dock_style: egui_dock::Style =
                    egui_dock::Style::from_egui(user_interface.style());
                dock_style.tab_bar.corner_radius = egui::CornerRadius::ZERO;
                dock_style.tab.tab_body.corner_radius = egui::CornerRadius::ZERO;
                // the workspace always holds both tabs, so none can be closed or collapsed
                egui_dock::DockArea::new(dock_workspace_state.dock_state_mut())
                    .id(egui::Id::new("editor-dock"))
                    .style(dock_style)
                    .show_close_buttons(false)
                    .show_leaf_close_all_buttons(false)
                    .show_leaf_collapse_buttons(false)
                    .show_inside(user_interface, self);
                self.dock_workspace_state = Some(dock_workspace_state);
            });
        self.notifications.show(user_interface.ctx());
    }

    /// Requests that the scene be saved
    fn render_save(&mut self) {
        let Some((scene, _)) = self.game_instance.game_mutable().scene() else { return; };
        scene.save();
        self.is_save_pending = true;
        self.is_edited = false;
    }

    /// Switches between the game's camera and a free camera starting where the game's camera is
    fn render_camera(&mut self) {
        let Some((scene, game_camera_actor)) = self.game_instance.game_mutable().scene()
        else { return; };
        let actor_registry: &mut ActorRegistry = scene.actor_registry_mutable();
        match self.camera_actor.take() {
            Some(camera_actor) => { let _ = actor_registry.despawn(camera_actor); }
            None => {
                // the free camera shares the game camera's dimensions and so its streamed region
                let (Ok(position), Ok(camera)) = (
                    actor_registry
                        .component::<ActorKinematicsPositionComponent>(game_camera_actor)
                        .copied(),
                    actor_registry.component::<ActorCameraComponent>(game_camera_actor).copied(),
                ) else { return; };
                self.camera_actor =
                    actor_registry.spawn((position, camera, ActorPersistenceComponent)).ok();
            }
        }
        self.game_instance.set_camera_actor(self.camera_actor);
        self.held_keys.clear();
    }

    /// Returns the position and visible dimensions of the camera drawn into the viewport
    fn render_camera_view(&mut self) -> Option<(ScenePosition, f32, f32)> {
        let camera_actor: Option<Actor> = self.camera_actor;
        let (scene, game_camera_actor) = self.game_instance.game_mutable().scene()?;
        let camera_actor: Actor = camera_actor.unwrap_or(game_camera_actor);
        let actor_registry: &mut ActorRegistry = scene.actor_registry_mutable();
        let position: ScenePosition = actor_registry
            .component::<ActorKinematicsPositionComponent>(camera_actor)
            .ok()?
            .position;
        let camera: &ActorCameraComponent = actor_registry.component(camera_actor).ok()?;
        Some((position, camera.visible_width(), camera.visible_height()))
    }

    /// Shows the game, grids, and brush, applying strokes and free camera zoom
    fn render_viewport(&mut self, user_interface: &mut egui::Ui) {
        let rect: egui::Rect = user_interface.available_rect_before_wrap();
        let response: egui::Response =
            user_interface.allocate_rect(rect, egui::Sense::click_and_drag());
        self.viewport_rect = Some(rect);
        self.pixels_per_point = user_interface.ctx().pixels_per_point();
        self.is_viewport_hovered = response.hovered();
        // the game receives the keyboard after the viewport is clicked, until escape or a click
        // elsewhere in the editor
        let (is_escape_pressed, is_pointer_pressed): (bool, bool) = user_interface.input(|input| {
            (input.key_pressed(egui::Key::Escape), input.pointer.any_pressed())
        });
        if response.clicked() || response.drag_started() {
            self.is_viewport_focused = true;
        } else if is_escape_pressed || (is_pointer_pressed && !response.hovered()) {
            self.is_viewport_focused = false;
        }
        // zoom the free camera with the scroll wheel
        if let Some(camera_actor) = self.camera_actor && response.hovered() {
            let scroll: f32 = user_interface.input(|input| input.smooth_scroll_delta.y);
            if scroll != 0.0 && let Some((scene, _)) = self.game_instance.game_mutable().scene()
                && let Ok(camera) = scene.actor_registry_mutable()
                    .component_mutable::<ActorCameraComponent>(camera_actor)
            {
                camera.zoom = (camera.zoom * (1.0 - scroll * 0.002))
                    .clamp(EDITOR_CAMERA_ZOOM_RANGE.0, EDITOR_CAMERA_ZOOM_RANGE.1);
            }
        }
        let camera_view: Option<(ScenePosition, f32, f32)> = self.render_camera_view();
        let Some(viewport) = self.viewport.as_ref() else { return; };
        let painter: egui::Painter = user_interface.painter_at(rect);
        painter.image(
            viewport.texture_id(),
            rect,
            egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
            egui::Color32::WHITE,
        );
        let Some((camera_position, visible_width, visible_height)) = camera_view else { return; };
        // the texture may lag the tab by one frame while resizing, so map through its size
        let texture: &wgpu::Texture = viewport.texture_view().texture();
        let scale: egui::Vec2 = egui::vec2(
            rect.width() / texture.width() as f32,
            rect.height() / texture.height() as f32,
        );
        let point = |position: ScenePosition| -> egui::Pos2 {
            let (x, y): (f32, f32) =
                viewport.pixel_position(camera_position, visible_width, visible_height, position);
            rect.min + egui::vec2(x * scale.x, y * scale.y)
        };
        let pixels_per_tile: f32 = viewport.pixels_per_tile(visible_width, visible_height);
        if self.is_tile_grid_shown || self.is_chunk_grid_shown {
            Self::render_grids(
                &painter,
                &point,
                camera_position,
                visible_width,
                visible_height,
                self.is_tile_grid_shown,
                self.is_chunk_grid_shown,
            );
        }
        let cursor_position: Option<ScenePosition> = response.hover_pos().map(|pointer| {
            let pixel: egui::Vec2 = (pointer - rect.min) / scale;
            viewport.scene_position(
                camera_position,
                visible_width,
                visible_height,
                pixel.x,
                pixel.y,
            )
        });
        self.cursor_cell = cursor_position.map(ScenePosition::cell_coordinates);
        // outline the cells the brush covers around the hovered cell
        if let Some(cell) = self.cursor_cell {
            let stroke: egui::Stroke = egui::Stroke::new(1.0, egui::Color32::WHITE);
            let half_size: f32 = self.brush_size as f32 / 2.0;
            match self.brush {
                EditorBrush::Circle => {
                    let half_cell_size: f64 = 0.5 / CELLULAR_TILE_SIZE as f64;
                    painter.circle_stroke(
                        point(Self::cell_position(cell).translate(half_cell_size, half_cell_size)),
                        half_size * pixels_per_tile / CELLULAR_TILE_SIZE as f32 * scale.x,
                        stroke,
                    );
                }
                EditorBrush::Square => {
                    let low: i64 = -(i64::from(self.brush_size) - 1) / 2;
                    let high: i64 = i64::from(self.brush_size) / 2 + 1;
                    painter.rect_stroke(
                        egui::Rect::from_two_pos(
                            point(Self::cell_position(CellCoordinates {
                                x: cell.x + low,
                                y: cell.y + low,
                            })),
                            point(Self::cell_position(CellCoordinates {
                                x: cell.x + high,
                                y: cell.y + high,
                            })),
                        ),
                        0.0,
                        stroke,
                        egui::StrokeKind::Inside,
                    );
                }
            }
        }
        // stroke from the previous sample so fast pointer movement leaves no gaps
        let is_stroking: bool = response.is_pointer_button_down_on()
            && user_interface.input(|input| input.pointer.primary_down());
        match (is_stroking, self.cursor_cell) {
            (true, Some(cell)) if self.stroke_cell != Some(cell) => {
                self.render_stroke(self.stroke_cell.unwrap_or(cell), cell);
                self.stroke_cell = Some(cell);
            }
            (true, _) => {}
            (false, _) => self.stroke_cell = None,
        }
    }

    /// Requests an edit filling the cells covered by a stroke within the resident region
    fn render_stroke(&mut self, start: CellCoordinates, end: CellCoordinates) {
        let (mode, material_identifier, color): (EditMode, MaterialIdentifier, u32) =
            match self.tool {
                EditorTool::Material(material_identifier) => (
                    self.mode,
                    material_identifier,
                    u32::from_le_bytes(self.color.to_srgba_unmultiplied()),
                ),
                EditorTool::Eraser => (EditMode::Replace, MaterialIdentifier::EMPTY, 0),
            };
        let mut edit: Edit = Edit::new(mode);
        let result: Result<(), EditError> = self.brush
            .fill_stroke(&mut edit, start, end, self.brush_size, material_identifier, color)
            .and_then(|()| {
                let Some((scene, _)) = self.game_instance.game_mutable().scene() else {
                    return Ok(());
                };
                // only resident cells can be edited, so the brush is clipped to them
                let active_region: Option<CellularTileRegion> = scene.active_cellular_tile_region();
                edit.cellular_tiles.retain(|coordinates, _| {
                    active_region.is_some_and(|region| region.contains(*coordinates))
                });
                if edit.cellular_tiles.is_empty() { return Ok(()); }
                scene.edit(edit)
            });
        match result {
            Ok(()) => {
                self.edit_error = None;
                self.is_edited = true;
            }
            Err(error) => self.edit_error = Some(error),
        }
    }

    /// Draws tile and chunk boundaries across the visible area of the camera
    fn render_grids(
        painter: &egui::Painter,
        point: &impl Fn(ScenePosition) -> egui::Pos2,
        camera_position: ScenePosition,
        visible_width: f32,
        visible_height: f32,
        is_tile_grid_shown: bool,
        is_chunk_grid_shown: bool,
    ) {
        let tiles_per_chunk: i64 = CHUNK_SIZE / CELLULAR_TILE_SIZE;
        let tile_stroke: egui::Stroke =
            egui::Stroke::new(1.0, egui::Color32::from_white_alpha(32));
        let chunk_stroke: egui::Stroke =
            egui::Stroke::new(1.5, egui::Color32::from_rgba_unmultiplied(255, 196, 64, 160));
        // the visible area extends half its size beyond the camera along each axis
        let corner = |x: f32, y: f32| -> ScenePosition {
            camera_position.translate(f64::from(x * visible_width), f64::from(y * visible_height))
        };
        let bottom_left: ScenePosition = corner(-0.5, -0.5);
        let top_right: ScenePosition = corner(0.5, 0.5);
        let (bottom_left_point, top_right_point): (egui::Pos2, egui::Pos2) =
            (point(bottom_left), point(top_right));
        for tile_x in bottom_left.tile_x + 1..=top_right.tile_x {
            let is_chunk_boundary: bool = tile_x.rem_euclid(tiles_per_chunk) == 0;
            if !(is_chunk_boundary && is_chunk_grid_shown) && !is_tile_grid_shown { continue; }
            let x: f32 = point(ScenePosition { tile_x, offset_x: 0, ..camera_position }).x;
            painter.line_segment(
                [egui::pos2(x, top_right_point.y), egui::pos2(x, bottom_left_point.y)],
                if is_chunk_boundary && is_chunk_grid_shown { chunk_stroke } else { tile_stroke },
            );
        }
        for tile_y in bottom_left.tile_y + 1..=top_right.tile_y {
            let is_chunk_boundary: bool = tile_y.rem_euclid(tiles_per_chunk) == 0;
            if !(is_chunk_boundary && is_chunk_grid_shown) && !is_tile_grid_shown { continue; }
            let y: f32 = point(ScenePosition { tile_y, offset_y: 0, ..camera_position }).y;
            painter.line_segment(
                [egui::pos2(bottom_left_point.x, y), egui::pos2(top_right_point.x, y)],
                if is_chunk_boundary && is_chunk_grid_shown { chunk_stroke } else { tile_stroke },
            );
        }
    }

    /// Shows the material, brush, and view settings
    fn render_tools(&mut self, user_interface: &mut egui::Ui) {
        let materials: Vec<(MaterialIdentifier, String)> = self.game_instance.game_mutable()
            .scene()
            .map(|(scene, _)| Self::materials(scene.material_table()))
            .unwrap_or_default();
        InspectorSection::new("Materials")
            .id_salt("editor-materials")
            .show(user_interface, |user_interface| {
                for (material_identifier, name) in materials {
                    user_interface.selectable_value(
                        &mut self.tool,
                        EditorTool::Material(material_identifier),
                        name,
                    );
                }
                user_interface.selectable_value(&mut self.tool, EditorTool::Eraser, "Eraser");
            });
        InspectorSection::new("Brush")
            .id_salt("editor-brush")
            .show(user_interface, |user_interface| {
                SettingsRow::show(user_interface, "Shape", |user_interface| {
                    user_interface.selectable_value(&mut self.brush, EditorBrush::Circle, "Circle");
                    user_interface.selectable_value(&mut self.brush, EditorBrush::Square, "Square");
                });
                SettingsRow::show(user_interface, "Size", |user_interface| {
                    user_interface.add(
                        egui::Slider::new(&mut self.brush_size, 1..=EDITOR_BRUSH_SIZE_MAXIMUM)
                            .suffix(" cells"),
                    );
                });
                // erasing always replaces, so the mode only applies to materials
                let is_material: bool = matches!(self.tool, EditorTool::Material(_));
                SettingsRow::show(user_interface, "Mode", |user_interface| {
                    user_interface.add_enabled_ui(is_material, |user_interface| {
                        user_interface.radio_value(&mut self.mode, EditMode::Insert, "Insert")
                            .on_hover_text("Fill only empty cells not occupied by actors");
                        user_interface.radio_value(&mut self.mode, EditMode::Replace, "Replace")
                            .on_hover_text("Fill every cell, replacing existing matter");
                    });
                });
                SettingsRow::show(user_interface, "Color", |user_interface| {
                    user_interface.add_enabled_ui(is_material, |user_interface| {
                        egui::color_picker::color_edit_button_srgba(
                            user_interface,
                            &mut self.color,
                            egui::color_picker::Alpha::Opaque,
                        );
                    });
                });
            });
        InspectorSection::new("View")
            .id_salt("editor-view")
            .show(user_interface, |user_interface| {
                user_interface.checkbox(&mut self.is_tile_grid_shown, "Tile grid");
                user_interface.checkbox(&mut self.is_chunk_grid_shown, "Chunk grid");
            });
    }

    /// Returns the registered cellular materials and their names
    fn materials(material_table: &MaterialTable) -> Vec<(MaterialIdentifier, String)> {
        let mut materials: Vec<(MaterialIdentifier, String)> = Vec::new();
        // static indices start after the empty material, dynamic indices at zero
        for (material_form, first_index) in
            [(MaterialForm::CellularStatic, 1), (MaterialForm::CellularDynamic, 0)]
        {
            for index in first_index.. {
                let Ok(material_identifier) = MaterialIdentifier::new(material_form, index)
                else { break; };
                let Some(material) = material_table.get(material_identifier) else { break; };
                materials.push((material_identifier, material.name().to_owned()));
            }
        }
        materials
    }

    /// Returns the position of a cell's bottom-left corner
    fn cell_position(cell: CellCoordinates) -> ScenePosition {
        let offset = |coordinate: i64| -> u32 {
            (coordinate.rem_euclid(CELLULAR_TILE_SIZE) as f64 / CELLULAR_TILE_SIZE as f64
                * ScenePosition::OFFSET_UNITS_PER_TILE) as u32
        };
        ScenePosition {
            tile_x: cell.x.div_euclid(CELLULAR_TILE_SIZE),
            tile_y: cell.y.div_euclid(CELLULAR_TILE_SIZE),
            offset_x: offset(cell.x),
            offset_y: offset(cell.y),
        }
    }

}

impl<G: Game> egui_dock::TabViewer for Editor<G> {

    type Tab = EditorTab;

    fn id(&mut self, tab: &mut EditorTab) -> egui::Id { egui::Id::new(*tab) }

    fn title(&mut self, tab: &mut EditorTab) -> egui::WidgetText {
        match tab {
            EditorTab::Viewport => "Viewport",
            EditorTab::Tools => "Tools",
        }.into()
    }

    fn ui(&mut self, user_interface: &mut egui::Ui, tab: &mut EditorTab) {
        match tab {
            EditorTab::Viewport => self.render_viewport(user_interface),
            EditorTab::Tools => self.render_tools(user_interface),
        }
    }

    fn is_closeable(&self, _tab: &EditorTab) -> bool { false }

    fn allowed_in_windows(&self, _tab: &mut EditorTab) -> bool { false }

    fn clear_background(&self, tab: &EditorTab) -> bool { *tab != EditorTab::Viewport }

    fn scroll_bars(&self, tab: &EditorTab) -> [bool; 2] { [false, *tab != EditorTab::Viewport] }

}
