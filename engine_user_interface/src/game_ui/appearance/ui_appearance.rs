// Copyright Rob Gage 2026

use egui::Response;

use super::UiVisual;

/// Visual states available to a game UI element
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct UiAppearance {
    /// The fallback visual state
    pub normal: UiVisual,
    /// The pointer-hover visual state
    pub hovered: Option<UiVisual>,
    /// The pointer-pressed visual state
    pub pressed: Option<UiVisual>,
    /// The disabled visual state
    pub disabled: Option<UiVisual>,
    /// The selected visual state
    pub selected: Option<UiVisual>,
    /// The keyboard-focused visual state
    pub focused: Option<UiVisual>,
}

impl UiAppearance {

    /// Selects a visual, falling back Pressed -> Hovered -> Selected -> Focused -> Normal.
    /// Disabled falls back directly to Normal; a provided Selected state wins over hover
    /// and focus when the element is selected and not pressed.
    pub fn visual(&self, response: &Response, enabled: bool, selected: bool) -> &UiVisual {
        if !enabled {
            return self.disabled.as_ref().unwrap_or(&self.normal);
        }
        if response.is_pointer_button_down_on() {
            return self
                .pressed
                .as_ref()
                .or(self.hovered.as_ref())
                .or(if selected { self.selected.as_ref() } else { None })
                .or(self.focused.as_ref())
                .unwrap_or(&self.normal);
        }
        if selected {
            return self
                .selected
                .as_ref()
                .or(if response.hovered() { self.hovered.as_ref() } else { None })
                .or(if response.has_focus() { self.focused.as_ref() } else { None })
                .unwrap_or(&self.normal);
        }
        if response.hovered() {
            return self
                .hovered
                .as_ref()
                .or(self.focused.as_ref())
                .unwrap_or(&self.normal);
        }
        if response.has_focus() {
            return self.focused.as_ref().unwrap_or(&self.normal);
        }
        &self.normal
    }
}
