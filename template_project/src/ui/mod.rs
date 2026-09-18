use engine::user_interface::{UserInterface, UserInterfaceContext};

pub fn draw_counter(context: &UserInterfaceContext, collected: u32) {
    context.add_contents(move |ui: &mut UserInterface| {
        ui.egui().label(format!("Collected: {collected}"));
    });
}

pub fn draw_extraction_demo(context: &UserInterfaceContext, amount: f32) {
    context.add_contents(move |ui: &mut UserInterface| {
        ui.egui()
            .label(format!("Extraction demo: Stone {amount:.2}"));
    });
}
