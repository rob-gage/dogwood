use engine::user_interface::{UserInterface, UserInterfaceContext};

pub fn draw_hud(
    context: &UserInterfaceContext,
    collected: u32,
    materials: Vec<(String, f32)>,
    latest_pickup: Vec<(String, f32)>,
) {
    context.add_contents(move |ui: &mut UserInterface| {
        let egui = ui.egui();
        egui.label(format!("Squares: {collected}"));
        if !materials.is_empty() {
            egui.label("Materials:");
            for (name, amount) in materials {
                egui.label(format!("{name} {amount:.2}"));
            }
        }
        if !latest_pickup.is_empty() {
            let pickup = latest_pickup
                .into_iter()
                .map(|(name, amount)| format!("{name} +{amount:.2}"))
                .collect::<Vec<_>>()
                .join(", ");
            egui.label(format!("Picked up: {pickup}"));
        }
    });
}
