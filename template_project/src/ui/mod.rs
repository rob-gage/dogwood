use engine::user_interface::{UserInterface, UserInterfaceContext};

pub fn draw_counter(context: &UserInterfaceContext, collected: u32) {
    context.add_contents(|ui: &mut UserInterface| {
        ui.egui().label(format!("Collected: {collected}"));
    });
}
