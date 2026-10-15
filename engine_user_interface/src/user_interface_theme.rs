// Copyright Rob Gage 2026

use std::collections::BTreeMap;
use egui::{
    Color32,
    Context,
    CornerRadius,
    FontFamily,
    FontId,
    Stroke,
    Style,
    TextStyle,
    Theme,
    style::{
        WidgetVisuals,
        Widgets,
    },
};

/// Shared application theme with an editable egui style
#[derive(Clone, Debug, PartialEq)]
pub struct UserInterfaceTheme {
    /// Style used by the selected theme
    pub style: Style,
}

impl UserInterfaceTheme {

    /// Default dark application theme
    pub fn dark() -> Self {
        let style: Style = Self::configured_style(Theme::Dark.default_style(), true);
        Self {
            style,
        }.with_accent(Color32::from_rgb(100, 170, 215))
    }

    /// Default light application theme
    pub fn light() -> Self {
        let style: Style = Self::configured_style(Theme::Light.default_style(), false);
        Self {
            style,
        }.with_accent(Color32::from_rgb(100, 170, 215))
    }

    /// Install this theme and its style into an egui context
    pub fn apply(&self, context: &Context) {
        let theme: Theme = Theme::from_dark_mode(self.style.visuals.dark_mode);
        context.set_style_of(theme, self.style.clone());
        context.set_theme(theme);
    }

    /// Set the accent used for links and selection highlights
    pub fn with_accent(mut self, accent_color: Color32) -> Self {
        let foreground_color: Color32 = Self::readable_foreground(accent_color, self.style.visuals.window_fill);
        let accent_stroke: Stroke = Stroke::new(1.0, accent_color);
        self.style.visuals.hyperlink_color = accent_color;
        self.style.visuals.selection.bg_fill = accent_color;
        self.style.visuals.selection.stroke = Stroke::new(1.0, foreground_color);
        self.style.visuals.widgets.active.bg_stroke = accent_stroke;
        self.style.visuals.widgets.open.bg_stroke = accent_stroke;
        self
    }

    fn configured_style(mut style: Style, dark_mode: bool) -> Style {
        let (panel_fill, window_fill, extreme_fill, text_color, weak_text_color): (Color32, Color32, Color32, Color32, Color32) = if dark_mode {
            (
                Color32::from_rgb(28, 32, 38),
                Color32::from_rgb(35, 40, 48),
                Color32::from_rgb(20, 24, 30),
                Color32::from_rgb(232, 235, 240),
                Color32::from_rgb(164, 172, 184),
            )
        } else {
            (
                Color32::from_rgb(240, 242, 245),
                Color32::from_rgb(250, 251, 252),
                Color32::from_rgb(229, 232, 236),
                Color32::from_rgb(35, 39, 46),
                Color32::from_rgb(101, 109, 122),
            )
        };
        let border_color: Color32 = if dark_mode {
            Color32::from_rgb(67, 75, 88)
        } else {
            Color32::from_rgb(192, 198, 208)
        };
        let muted_fill: Color32 = if dark_mode {
            Color32::from_rgb(43, 48, 58)
        } else {
            Color32::from_rgb(248, 249, 251)
        };
        let hover_fill: Color32 = if dark_mode {
            Color32::from_rgb(55, 62, 74)
        } else {
            Color32::from_rgb(232, 236, 242)
        };
        let active_fill: Color32 = if dark_mode {
            Color32::from_rgb(67, 76, 91)
        } else {
            Color32::from_rgb(218, 224, 233)
        };
        let border_stroke: Stroke = Stroke::new(1.0, border_color);
        let corner_radius: CornerRadius = CornerRadius::same(4);
        let noninteractive: WidgetVisuals = WidgetVisuals {
            bg_fill: window_fill,
            weak_bg_fill: panel_fill,
            bg_stroke: border_stroke,
            corner_radius,
            fg_stroke: Stroke::new(1.0, text_color),
            expansion: 0.0,
        };
        let inactive: WidgetVisuals = WidgetVisuals {
            bg_fill: muted_fill,
            weak_bg_fill: muted_fill,
            bg_stroke: border_stroke,
            corner_radius,
            fg_stroke: Stroke::new(1.0, text_color),
            expansion: 0.0,
        };
        let hovered: WidgetVisuals = WidgetVisuals {
            bg_fill: hover_fill,
            weak_bg_fill: hover_fill,
            bg_stroke: Stroke::new(1.0, border_color),
            corner_radius,
            fg_stroke: Stroke::new(1.0, text_color),
            expansion: 0.0,
        };
        let active: WidgetVisuals = WidgetVisuals {
            bg_fill: active_fill,
            weak_bg_fill: active_fill,
            bg_stroke: Stroke::new(1.0, text_color),
            corner_radius,
            fg_stroke: Stroke::new(1.0, text_color),
            expansion: 0.0,
        };
        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
        style.spacing.window_margin = egui::Margin::same(12);
        style.spacing.button_padding = egui::vec2(10.0, 6.0);
        style.spacing.interact_size = egui::vec2(40.0, 28.0);
        style.spacing.icon_spacing = 6.0;
        style.visuals.panel_fill = panel_fill;
        style.visuals.window_fill = window_fill;
        style.visuals.extreme_bg_color = extreme_fill;
        style.visuals.text_edit_bg_color = Some(extreme_fill);
        style.visuals.override_text_color = None;
        style.visuals.weak_text_color = Some(weak_text_color);
        style.visuals.window_corner_radius = corner_radius;
        style.visuals.menu_corner_radius = corner_radius;
        style.visuals.window_stroke = border_stroke;
        style.visuals.widgets = Widgets {
            noninteractive,
            inactive,
            hovered,
            active,
            open: active,
        };
        style.visuals.button_frame = true;
        style.visuals.collapsing_header_frame = true;
        style.visuals.disabled_alpha = 0.55;
        style.text_styles = BTreeMap::from([
            (TextStyle::Body, FontId::new(14.0, FontFamily::Proportional)),
            (TextStyle::Button, FontId::new(14.0, FontFamily::Proportional)),
            (TextStyle::Small, FontId::new(12.0, FontFamily::Proportional)),
            (TextStyle::Heading, FontId::new(20.0, FontFamily::Proportional)),
            (TextStyle::Monospace, FontId::new(13.0, FontFamily::Monospace)),
        ]);
        style
    }

    fn readable_foreground(color: Color32, background: Color32) -> Color32 {
        let accent: egui::Rgba = egui::Rgba::from(color);
        let surface: egui::Rgba = egui::Rgba::from(background);
        let remaining_alpha: f32 = 1.0 - accent.a();
        let background_luminance: f32 = 0.2126 * (accent.r() + surface.r() * remaining_alpha)
            + 0.7152 * (accent.g() + surface.g() * remaining_alpha)
            + 0.0722 * (accent.b() + surface.b() * remaining_alpha);
        let dark_foreground: Color32 = Color32::from_rgb(20, 24, 30);
        let light_foreground: Color32 = Color32::from_rgb(248, 250, 252);
        let dark_linear: egui::Rgba = egui::Rgba::from(dark_foreground);
        let light_linear: egui::Rgba = egui::Rgba::from(light_foreground);
        let dark_luminance: f32 = 0.2126 * dark_linear.r() + 0.7152 * dark_linear.g() + 0.0722 * dark_linear.b();
        let light_luminance: f32 = 0.2126 * light_linear.r() + 0.7152 * light_linear.g() + 0.0722 * light_linear.b();
        let dark_contrast: f32 = (background_luminance.max(dark_luminance) + 0.05)
            / (background_luminance.min(dark_luminance) + 0.05);
        let light_contrast: f32 = (background_luminance.max(light_luminance) + 0.05)
            / (background_luminance.min(light_luminance) + 0.05);
        if dark_contrast >= light_contrast { dark_foreground } else { light_foreground }
    }

}

impl Default for UserInterfaceTheme {

    fn default() -> Self {
        Self::dark()
    }

}
