// Copyright Rob Gage 2026

use std::time::{
    Duration,
    Instant,
};
use egui::{
    Align2,
    Area,
    Color32,
    Context,
    Frame,
    Order,
    RichText,
};

/// Expiring foreground notifications
#[derive(Debug)]
pub struct Notifications {
    messages: Vec<(String, Color32, Instant, Duration)>,
    id_salt: String,
}

impl Default for Notifications {

    fn default() -> Self {
        Self {
            messages: Vec::new(),
            id_salt: String::from("user-interface-notifications"),
        }
    }

}

impl Notifications {

    /// Add a notification that remains visible for the supplied duration
    pub fn push(&mut self, message: impl Into<String>, color: Color32, duration: Duration) {
        self.messages.push((message.into(), color, Instant::now(), duration));
    }

    /// Remove all currently visible notifications
    pub fn clear(&mut self) {
        self.messages.clear();
    }

    /// Set the stable egui identifier salt for this notification surface
    pub fn id_salt(mut self, id_salt: impl Into<String>) -> Self {
        self.id_salt = id_salt.into();
        self
    }

    /// Paint visible notifications and schedule repaint while any remain
    pub fn show(&mut self, context: &Context) {
        let current_time: Instant = Instant::now();
        self.messages.retain(|message: &(String, Color32, Instant, Duration)| current_time.duration_since(message.2) < message.3);
        let next_expiry: Option<Duration> = self.messages.iter().map(|message: &(String, Color32, Instant, Duration)| message.3.saturating_sub(current_time.duration_since(message.2))).min();
        if let Some(next_expiry) = next_expiry {
            context.request_repaint_after(next_expiry);
        }
        if self.messages.is_empty() {
            return;
        }
        Area::new(egui::Id::new(("user-interface-notifications", &self.id_salt)))
            .anchor(Align2::RIGHT_TOP, egui::vec2(-16.0, 16.0))
            .order(Order::Foreground)
            .show(context, |ui: &mut egui::Ui| {
                for (message, color, _, _) in &self.messages {
                    let frame: Frame = Frame::popup(ui.style()).fill(*color).inner_margin(8.0);
                    frame.show(ui, |ui: &mut egui::Ui| {
                        ui.label(RichText::new(message).color(Color32::WHITE));
                    });
                    ui.add_space(6.0);
                }
            });
    }

}
