// Copyright Rob Gage 2026

use crate::UserInterface;
use std::cell::RefCell;

/// The user interface displayed by a `Game`
pub struct UserInterfaceContext {
    egui_context: egui::Context,
    window_state: RefCell<Option<egui_winit::State>>,
    contents: RefCell<Vec<Box<dyn FnOnce(&mut UserInterface)>>>,
    output: RefCell<egui::FullOutput>,
}

impl Default for UserInterfaceContext {
    fn default() -> Self {
        Self::new()
    }
}

impl UserInterfaceContext {
    /// Creates an empty `UserInterfaceContext`
    pub fn new() -> Self {
        let egui_context: egui::Context = egui::Context::default();
        egui_context.set_fonts(egui::FontDefinitions::default());
        Self {
            egui_context,
            window_state: RefCell::new(None),
            contents: RefCell::new(Vec::new()),
            output: RefCell::new(egui::FullOutput::default()),
        }
    }

    /// Runs the queued contents as one user-interface frame.
    fn run(&self, input: egui::RawInput) {
        let contents: Vec<Box<dyn FnOnce(&mut UserInterface)>> =
            std::mem::take(&mut *self.contents.borrow_mut());
        let mut contents: Option<Vec<Box<dyn FnOnce(&mut UserInterface)>>> = Some(contents);
        let output: egui::FullOutput = self.egui_context.run_ui(input, |ui| {
            let mut ui: UserInterface = UserInterface(ui);
            for add_contents in contents.take().unwrap_or_default() {
                add_contents(&mut ui);
            }
        });
        *self.output.borrow_mut() = output;
    }

    /// Runs the queued contents with the current window input as one frame.
    pub fn compose(&self, size: [u32; 2], window: &egui_winit::winit::window::Window) {
        let input: egui::RawInput = self
            .window_state
            .borrow_mut()
            .as_mut()
            .map(|state| state.take_egui_input(window))
            .unwrap_or_else(|| egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(size[0] as f32, size[1] as f32),
                )),
                ..Default::default()
            });
        self.run(input);
        let mut output: egui::FullOutput = self.take_output();
        let platform_output: egui::PlatformOutput = std::mem::take(&mut output.platform_output);
        if let Some(state) = self.window_state.borrow_mut().as_mut() {
            state.handle_platform_output(window, platform_output);
        }
        *self.output.borrow_mut() = output;
    }

    /// Queues game-owned contents for the next shared user-interface frame.
    pub fn add_contents(&self, add_contents: impl FnOnce(&mut UserInterface) + 'static) {
        self.contents.borrow_mut().push(Box::new(add_contents));
    }

    /// Initializes pointer and window input handling for this user interface
    pub fn initialize_window(
        &self,
        window: &egui_winit::winit::window::Window,
        max_texture_side: usize,
    ) {
        *self.window_state.borrow_mut() = Some(egui_winit::State::new(
            self.egui_context.clone(),
            egui::ViewportId::ROOT,
            window,
            Some(window.scale_factor() as f32),
            window.theme(),
            Some(max_texture_side),
        ));
    }

    /// Passes a window event to egui and returns whether egui consumed it
    pub fn process_window_event(
        &self,
        window: &egui_winit::winit::window::Window,
        event: &egui_winit::winit::event::WindowEvent,
    ) -> bool {
        self.window_state
            .borrow_mut()
            .as_mut()
            .is_some_and(|state| state.on_window_event(window, event).consumed)
    }

    /// Takes the latest UI output for rendering.
    pub fn take_output(&self) -> egui::FullOutput {
        std::mem::take(&mut *self.output.borrow_mut())
    }

    /// Returns a reference to this `UserInterfaceContext`'s `egui::Context`
    pub fn egui_context(&self) -> &egui::Context {
        &self.egui_context
    }
}

#[cfg(test)]
mod tests {
    use super::UserInterfaceContext;
    use std::cell::RefCell;
    use std::rc::Rc;

    #[test]
    fn one_frame_contains_multiple_ui_contributions_until_consumed() {
        let context = UserInterfaceContext::new();
        context.add_contents(|ui| {
            ui.egui().label("game");
        });
        context.add_contents(|ui| {
            ui.egui().label("editor");
        });
        context.run(egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(320.0, 200.0),
            )),
            time: Some(1.0),
            ..Default::default()
        });
        let output = context.take_output();
        assert!(output.shapes.len() >= 2);
        output.drop_without_applying_deltas();
        assert!(context.take_output().shapes.is_empty());
    }

    #[test]
    fn deferred_editor_actions_and_viewport_are_available_after_composition() {
        let context = UserInterfaceContext::new();
        let play_requested: Rc<RefCell<bool>> = Rc::new(RefCell::new(false));
        let play_for_editor: Rc<RefCell<bool>> = play_requested.clone();
        let viewport_bounds: Rc<RefCell<Option<[u32; 4]>>> = Rc::new(RefCell::new(None));
        let bounds_for_editor: Rc<RefCell<Option<[u32; 4]>>> = viewport_bounds.clone();
        context.add_contents(|ui| {
            ui.egui().label("game");
        });
        context.add_contents(move |ui| {
            ui.egui().label("editor");
            *play_for_editor.borrow_mut() = true;
            *bounds_for_editor.borrow_mut() = Some([10, 20, 300, 180]);
        });
        context.run(egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(320.0, 240.0),
            )),
            time: Some(1.0),
            ..Default::default()
        });
        let output = context.take_output();
        assert!(output.shapes.len() >= 2);
        assert!(*play_requested.borrow());
        let mut is_playing = false;
        if *play_requested.borrow() {
            is_playing = !is_playing;
        }
        assert!(is_playing);
        assert_eq!(*viewport_bounds.borrow(), Some([10, 20, 300, 180]));
        output.drop_without_applying_deltas();
    }

    #[test]
    fn queued_contributions_do_not_advance_time_between_real_inputs() {
        let context = UserInterfaceContext::new();
        context.add_contents(|ui| {
            ui.egui().label("game");
        });
        context.run(egui::RawInput {
            time: Some(1.0),
            ..Default::default()
        });
        context.take_output().drop_without_applying_deltas();
        context.add_contents(|ui| {
            ui.egui().label("editor");
        });
        context.run(egui::RawInput {
            time: Some(1.01),
            ..Default::default()
        });
        context.take_output().drop_without_applying_deltas();
    }

    #[test]
    fn one_frame_preserves_texture_set_and_free_deltas() {
        let context = UserInterfaceContext::new();
        let texture = Rc::new(RefCell::new(None));
        let texture_for_ui: Rc<RefCell<Option<egui::TextureHandle>>> = texture.clone();
        context.add_contents(move |ui| {
            *texture_for_ui.borrow_mut() = Some(ui.egui().ctx().load_texture(
                "test",
                egui::ColorImage::example(),
                egui::TextureOptions::LINEAR,
            ));
        });
        context.run(egui::RawInput::default());
        let output = context.take_output();
        assert!(!output.textures_delta.set.is_empty());
        output.drop_without_applying_deltas();

        texture.borrow_mut().take();
        context.run(egui::RawInput::default());
        let output = context.take_output();
        assert!(!output.textures_delta.free.is_empty());
        output.drop_without_applying_deltas();
    }
}

impl Drop for UserInterfaceContext {
    fn drop(&mut self) {
        self.output.get_mut().textures_delta.clear();
    }
}
