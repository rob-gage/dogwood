// Copyright Rob Gage 2026

//! Runtime layout, painting, and interaction for game UI descriptions

use super::{UiComponentKind, UiElement};
use crate::game_ui::{UiAppearance, UiBrush, UiResponses, UiTextStyle, UiVisual};
use egui::{Align, Id, Layout, Rect, Response, Sense, Ui, UiBuilder, Vec2};
use std::collections::HashSet;
use std::sync::Arc;

/// Stateful runtime for one game UI composition surface
#[derive(Debug, Default)]
pub struct GameUi {
    layout_engine: crate::game_ui::layout::UiLayoutEngine,
    namespace: Option<Id>,
}

impl GameUi {
    /// Creates an empty reusable game UI runtime
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a game UI runtime with a stable namespace for anonymous elements.
    ///
    /// Explicit element IDs remain global egui IDs. Namespace those IDs at the
    /// call site when the same explicit ID is used by multiple surfaces.
    #[must_use]
    pub fn with_id(id: impl std::hash::Hash + std::fmt::Debug) -> Self {
        Self { layout_engine: crate::game_ui::layout::UiLayoutEngine::new(), namespace: Some(Id::new(id)) }
    }

    /// Enables or disables logical-unit rounding in the layout engine.
    pub fn set_layout_rounding(&mut self, enabled: bool) {
        self.layout_engine.set_rounding(enabled);
    }

    /// Shows a root element in the current available egui rectangle
    pub fn show(&mut self, ui: &mut Ui, root: UiElement<'_>) -> Result<UiResponses, taffy::TaffyError> {
        let bounds: Rect = ui.available_rect_before_wrap();
        self.show_in(ui, bounds, root)
    }

    /// Shows a root element in an explicit egui rectangle
    pub fn show_in(&mut self, ui: &mut Ui, bounds: Rect, root: UiElement<'_>) -> Result<UiResponses, taffy::TaffyError> {
        let mut elements: Vec<UiElement<'_>> = Vec::new();
        let mut nodes: Vec<crate::game_ui::layout::UiLayoutNode> = Vec::new();
        let mut explicit_ids: HashSet<Id> = HashSet::new();
        let root_namespace: Id = self.namespace.unwrap_or_else(|| {
            root.explicit_id
                .map(|root_id: Id| Id::new(("game-ui", ui.id(), root_id)))
                .unwrap_or_else(|| Id::new(("game-ui", ui.id())))
        });
        flatten_element(root, root_namespace, &mut elements, &mut nodes, &mut explicit_ids, &mut Vec::new(), None, false, true, false)?;
        let computed_layouts: Vec<crate::game_ui::layout::UiComputedLayout> = self.layout_engine.compute(
            &nodes,
            0,
            bounds,
            |node_index, known_dimensions, available_space| {
                measure_kind(&elements[node_index], ui, known_dimensions, available_space)
            },
        )?;
        let mut responses: UiResponses = UiResponses::new();
        let root_clip: Rect = ui.clip_rect().intersect(bounds);
        ui.advance_cursor_after_rect(bounds);
        render_element_at(ui, &mut elements, &nodes, &computed_layouts, 0, Vec2::ZERO, root_clip, false, &mut responses);
        Ok(responses)
    }
}

fn flatten_element<'a>(
    element: UiElement<'a>,
    namespace: Id,
    elements: &mut Vec<UiElement<'a>>,
    nodes: &mut Vec<crate::game_ui::layout::UiLayoutNode>,
    explicit_ids: &mut HashSet<Id>,
    path: &mut Vec<usize>,
    parent_direction: Option<taffy::FlexDirection>,
    parent_overlay: bool,
    parent_enabled: bool,
    parent_scroll: bool,
) -> Result<usize, taffy::TaffyError> {
    let UiElement { kind, mut layout, appearance, typography, enabled, selected, explicit_id, children } = element;
    let element_id: Id = explicit_id.unwrap_or_else(|| Id::new(("game-ui", namespace, path.clone())));
    if explicit_id.is_some() && !explicit_ids.insert(element_id) {
        return Err(invalid_node_error(elements.len()));
    }
    let effective_enabled: bool = parent_enabled && enabled;
    if parent_scroll {
        layout.style.flex_shrink = 0.0;
    }
    if let UiComponentKind::Spacer { flex } = kind {
        let weight: f32 = if flex.is_finite() { flex.max(0.0) } else { 0.0 };
        let row_parent: bool = matches!(parent_direction, Some(taffy::FlexDirection::Row | taffy::FlexDirection::RowReverse));
        if row_parent && layout.style.size.width == taffy::Dimension::auto() {
            layout = layout.with_width(crate::game_ui::UiLength::Flex(weight));
        } else if !row_parent && layout.style.size.height == taffy::Dimension::auto() {
            layout = layout.with_height(crate::game_ui::UiLength::Flex(weight));
        }
    }
    let is_scroll: bool = matches!(&kind, UiComponentKind::Scroll);
    let is_overlay: bool = matches!(&kind, UiComponentKind::Overlay);
    if is_overlay {
        layout.style.display = taffy::Display::Grid;
        layout.style.grid_template_columns = vec![taffy::GridTemplateComponent::Single(taffy::TrackSizingFunction::from(taffy::Dimension::auto()))];
        layout.style.grid_template_rows = vec![taffy::GridTemplateComponent::Single(taffy::TrackSizingFunction::from(taffy::Dimension::auto()))];
    }
    if is_scroll {
        layout.clip = true;
        layout.style.overflow = taffy::Point { x: taffy::Overflow::Scroll, y: taffy::Overflow::Scroll };
    }
    let node_index: usize = nodes.len();
    let current_direction: taffy::FlexDirection = layout.style.flex_direction;
    let overlay_grid_child: bool = parent_overlay && layout.style.position != taffy::Position::Absolute;
    elements.push(UiElement { kind, layout: layout.clone(), appearance, typography, enabled: effective_enabled, selected, explicit_id, children: Vec::new() });
    nodes.push(crate::game_ui::layout::UiLayoutNode::new(element_id, layout));
    if overlay_grid_child {
        nodes[node_index].layout.style.grid_column = taffy::style_helpers::line(1);
        nodes[node_index].layout.style.grid_row = taffy::style_helpers::line(1);
    }
    for (child_position, child) in children.into_iter().enumerate() {
        path.push(child_position);
        let child_index: usize = flatten_element(child, namespace, elements, nodes, explicit_ids, path, Some(current_direction), is_overlay, effective_enabled, is_scroll)?;
        path.pop();
        nodes[node_index].children.push(child_index);
    }
    Ok(node_index)
}

fn measure_kind(
    element: &UiElement<'_>,
    ui: &Ui,
    known_dimensions: taffy::geometry::Size<Option<f32>>,
    available_space: taffy::geometry::Size<taffy::AvailableSpace>,
) -> Vec2 {
    let width: f32 = match known_dimensions.width {
        Some(width) if width > 0.0 || !matches!(available_space.width, taffy::AvailableSpace::MinContent) => width.max(0.0),
        _ => resolve_available_width(available_space.width, element, ui),
    };
    match &element.kind {
        UiComponentKind::Text(text) => element.typography.layout(ui, text, width).size(),
        UiComponentKind::Image(image) => {
            let uv_size: Vec2 = image.uv.size();
            image.texture.intrinsic_size() * Vec2::new(uv_size.x.abs(), uv_size.y.abs())
        }
        UiComponentKind::Separator { .. } => Vec2::new(1.0, 1.0),
        UiComponentKind::Progress { .. } => Vec2::new(64.0, ui.spacing().interact_size.y),
        UiComponentKind::Button { label } => measure_label(&element.typography, ui, label, width),
        UiComponentKind::Toggle { label, .. } => {
            let indicator_width: f32 = ui.spacing().icon_width + ui.spacing().item_spacing.x;
            let label_size: Vec2 = element.typography.layout(ui, label, (width - indicator_width).max(0.0)).size();
            Vec2::new(label_size.x + indicator_width, label_size.y.max(ui.spacing().icon_width))
        },
        UiComponentKind::ImageButton { image } => image.source_size(),
        UiComponentKind::Slider { .. } | UiComponentKind::TextInput { .. } => ui.spacing().interact_size,
        UiComponentKind::Spacer { .. }
        | UiComponentKind::Panel
        | UiComponentKind::Row
        | UiComponentKind::Column
        | UiComponentKind::Overlay
        | UiComponentKind::Grid
        | UiComponentKind::Scroll
        | UiComponentKind::Rectangle(_) => Vec2::ZERO,
    }
}

fn resolve_available_width(space: taffy::AvailableSpace, element: &UiElement<'_>, ui: &Ui) -> f32 {
    match space {
        taffy::AvailableSpace::Definite(width) => width.max(0.0),
        taffy::AvailableSpace::MinContent => match &element.kind {
            UiComponentKind::Text(text) => {
                if element.typography.wrap { longest_word_width(&element.typography, ui, text) } else { element.typography.layout(ui, text, f32::INFINITY).size().x }
            }
            UiComponentKind::Button { label } | UiComponentKind::Toggle { label, .. } => {
                let text_width: f32 = if element.typography.wrap { longest_word_width(&element.typography, ui, label) } else { element.typography.layout(ui, label, f32::INFINITY).size().x };
                let accessory_width: f32 = if matches!(element.kind, UiComponentKind::Toggle { .. }) {
                    ui.spacing().icon_width + ui.spacing().item_spacing.x
                } else {
                    ui.spacing().button_padding.x * 2.0
                };
                text_width + accessory_width
            }
            _ => 0.0,
        },
        taffy::AvailableSpace::MaxContent => f32::INFINITY,
    }
}

fn longest_word_width(typography: &UiTextStyle, ui: &Ui, text: &str) -> f32 {
    text.split_whitespace().map(|word| typography.layout(ui, word, f32::INFINITY).size().x).fold(0.0, f32::max)
}

fn measure_label(typography: &UiTextStyle, ui: &Ui, label: &str, width: f32) -> Vec2 {
    typography.layout(ui, label, width).size() + ui.spacing().button_padding * 2.0
}

fn render_element_at(
    ui: &mut Ui,
    elements: &mut [UiElement<'_>],
    nodes: &[crate::game_ui::layout::UiLayoutNode],
    computed_layouts: &[crate::game_ui::layout::UiComputedLayout],
    node_index: usize,
    translation: Vec2,
    inherited_clip: Rect,
    decoration_only: bool,
    responses: &mut UiResponses,
) {
    if matches!(&elements[node_index].kind, UiComponentKind::Scroll) && !decoration_only {
        let border_rect: Rect = computed_layouts[node_index].rect.translate(translation);
        let viewport_rect: Rect = computed_layouts[node_index].content_rect.translate(translation);
        render_scroll_element(ui, elements, nodes, computed_layouts, node_index, border_rect, viewport_rect, translation, inherited_clip, responses);
        return;
    }
    let border_rect: Rect = computed_layouts[node_index].rect.translate(translation);
    let content_rect: Rect = computed_layouts[node_index].content_rect.translate(translation);
    let element_id: Id = nodes[node_index].id;
    let (clips_children, child_decorations): (bool, bool) = {
        let element: &mut UiElement<'_> = &mut elements[node_index];
        let content_clip: Rect = inherited_clip.intersect(content_rect);
        let paint_clip: Rect = inherited_clip;
        let native: bool = matches!(element.kind, UiComponentKind::Toggle { .. } | UiComponentKind::Slider { .. } | UiComponentKind::TextInput { .. });
        let typography: UiTextStyle = element.typography.clone();
        let appearance: UiAppearance = element.appearance;
        let enabled: bool = element.enabled;
        let interaction_rect: Rect = match element.kind {
            UiComponentKind::Button { .. } | UiComponentKind::ImageButton { .. } => border_rect,
            UiComponentKind::Toggle { .. } | UiComponentKind::Slider { .. } | UiComponentKind::TextInput { .. } => content_rect,
            _ => border_rect,
        };
        let native_paint_slot: Option<(egui::Painter, egui::layers::ShapeIdx)> = if native {
            Some(reserve_native_visual(ui, border_rect, paint_clip))
        } else {
            None
        };
        let response: Response = if decoration_only {
            interact_at(ui, interaction_rect, element_id, Sense::hover(), false, content_clip)
        } else {
            match &mut element.kind {
                UiComponentKind::Toggle { value, label } => render_toggle(ui, &typography, &appearance, enabled, content_rect, content_clip, element_id, &mut **value, label),
                UiComponentKind::Slider { value, range } => render_slider(ui, &typography, &appearance, enabled, content_rect, content_clip, element_id, &mut **value, range),
                UiComponentKind::TextInput { value } => render_text_input(ui, &typography, &appearance, enabled, content_rect, content_clip, element_id, &mut **value),
                UiComponentKind::Button { .. } | UiComponentKind::ImageButton { .. } => {
                    let response: Response = interact_at(ui, interaction_rect, element_id, Sense::click(), element.enabled, paint_clip);
                    if response.clicked() { response.request_focus(); }
                    response
                }
                _ => interact_at(ui, interaction_rect, element_id, Sense::hover(), element.enabled, paint_clip),
            }
        };
        let visual_selected: bool = match &element.kind {
            UiComponentKind::Toggle { value, .. } => **value,
            _ => element.selected,
        };
        if native && !decoration_only {
            paint_reserved_native_visual(native_paint_slot, appearance.visual(&response, enabled, visual_selected), border_rect);
            if let UiComponentKind::Toggle { value, label } = &element.kind {
                paint_toggle(ui, &typography, content_rect, paint_clip, &response, **value, label);
            }
        } else if !native || decoration_only {
            paint_element(ui, element, border_rect, content_rect, paint_clip, &response);
        }
        responses.insert_for(element_id, response);
        (element.layout.clip, decoration_only || matches!(element.kind, UiComponentKind::Button { .. } | UiComponentKind::ImageButton { .. }))
    };
    let child_clip: Rect = if clips_children { inherited_clip.intersect(content_rect) } else { inherited_clip };
    paint_children(ui, elements, nodes, computed_layouts, node_index, translation, child_clip, child_decorations, responses);
}

fn paint_children(
    ui: &mut Ui,
    elements: &mut [UiElement<'_>],
    nodes: &[crate::game_ui::layout::UiLayoutNode],
    computed_layouts: &[crate::game_ui::layout::UiComputedLayout],
    node_index: usize,
    translation: Vec2,
    clip: Rect,
    decoration_only: bool,
    responses: &mut UiResponses,
) {
    for child_index in nodes[node_index].children.iter().copied() {
        render_element_at(ui, elements, nodes, computed_layouts, child_index, translation, clip, decoration_only, responses);
    }
}

fn render_scroll_element(
    ui: &mut Ui,
    elements: &mut [UiElement<'_>],
    nodes: &[crate::game_ui::layout::UiLayoutNode],
    computed_layouts: &[crate::game_ui::layout::UiComputedLayout],
    node_index: usize,
    border_rect: Rect,
    viewport_rect: Rect,
    translation: Vec2,
    inherited_clip: Rect,
    responses: &mut UiResponses,
) {
    let element_id: Id = nodes[node_index].id;
    let (enabled, appearance, selected): (bool, UiAppearance, bool) = { let element: &UiElement<'_> = &elements[node_index]; (element.enabled, element.appearance, element.selected) };
    let clip: Rect = inherited_clip.intersect(viewport_rect);
    let response: Response = interact_at(ui, viewport_rect, element_id, Sense::hover(), enabled, clip);
    paint_appearance(ui, &appearance, border_rect, inherited_clip, &response, enabled, selected);
    responses.insert_for(element_id, response);
    let mut scroll_ui: Ui = child_ui(ui, viewport_rect, element_id, enabled, Layout::top_down(Align::Min));
    scroll_ui.set_clip_rect(clip);
    let content_size: Vec2 = computed_layouts[node_index].content_size.max(viewport_rect.size());
    egui::ScrollArea::both()
        .id_salt(element_id)
        .max_width(viewport_rect.width())
        .max_height(viewport_rect.height())
        .auto_shrink([false, false])
        .show_viewport(&mut scroll_ui, |viewport_ui, _viewport| {
            viewport_ui.set_min_size(content_size);
            let delta: Vec2 = viewport_ui.max_rect().min.to_vec2() - viewport_rect.min.to_vec2();
            let child_clip: Rect = viewport_ui.clip_rect().intersect(clip);
            paint_children(viewport_ui, elements, nodes, computed_layouts, node_index, translation + delta, child_clip, false, responses);
        });
}

fn render_toggle(ui: &mut Ui, typography: &UiTextStyle, appearance: &UiAppearance, enabled: bool, rect: Rect, clip: Rect, id: Id, value: &mut bool, label: &str) -> Response {
    let mut toggle_ui: Ui = child_ui(ui, rect, Id::new(("game-ui-toggle", id)), enabled, Layout::top_down(Align::Min));
    toggle_ui.set_clip_rect(clip);
    apply_local_native_style(&mut toggle_ui, typography, appearance, enabled);
    let mut response: Response = toggle_ui.interact(rect, id, Sense::click());
    if response.clicked() {
        *value = !*value;
        response.mark_changed();
    }
    response.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Checkbox, enabled, *value, label));
    response
}

fn render_slider(ui: &mut Ui, typography: &UiTextStyle, appearance: &UiAppearance, enabled: bool, rect: Rect, clip: Rect, id: Id, value: &mut f32, range: &std::ops::RangeInclusive<f32>) -> Response {
    let mut native_ui: Ui = native_ui(ui, rect, clip, id, typography, appearance, enabled);
    native_ui.spacing_mut().slider_width = rect.width();
    native_ui.spacing_mut().interact_size.y = rect.height();
    let slider: egui::Slider<'_> = egui::Slider::new(value, range.clone()).show_value(false).text_color(typography.color).trailing_fill(false);
    native_ui.put(rect, slider)
}

fn render_text_input(ui: &mut Ui, typography: &UiTextStyle, appearance: &UiAppearance, enabled: bool, rect: Rect, clip: Rect, id: Id, value: &mut String) -> Response {
    let mut native_ui: Ui = native_ui(ui, rect, clip, id, typography, appearance, enabled);
    let native_id: Id = Id::new(("game-ui-native-widget", id));
    let text_edit: egui::TextEdit<'_> = egui::TextEdit::singleline(value)
        .id(native_id)
        .font(typography.font.clone())
        .text_color(typography.color)
        .frame(egui::Frame::NONE)
        .desired_width(rect.width())
        .min_size(rect.size());
    native_ui.put(rect, text_edit)
}

fn child_ui(ui: &mut Ui, rect: Rect, id: Id, enabled: bool, layout: Layout) -> Ui {
    let mut builder: UiBuilder = UiBuilder::new().id_salt(("game-ui-child", id)).max_rect(rect).layout(layout);
    if !enabled { builder = builder.disabled(); }
    ui.new_child(builder)
}

fn native_ui(ui: &mut Ui, rect: Rect, clip: Rect, id: Id, typography: &UiTextStyle, appearance: &UiAppearance, enabled: bool) -> Ui {
    let mut child: Ui = child_ui(ui, rect, Id::new(("game-ui-native", id)), enabled, Layout::top_down(Align::Min));
    child.set_clip_rect(clip);
    apply_local_native_style(&mut child, typography, appearance, enabled);
    child
}

fn interact_at(ui: &mut Ui, rect: Rect, id: Id, sense: Sense, enabled: bool, clip: Rect) -> Response {
    let mut child: Ui = child_ui(ui, rect, Id::new(("game-ui-interact", id)), enabled, Layout::top_down(Align::Min));
    child.set_clip_rect(clip);
    child.interact(rect, id, sense)
}

fn apply_local_native_style(ui: &mut Ui, typography: &UiTextStyle, appearance: &UiAppearance, enabled: bool) {
    let normal: &UiVisual = if enabled { &appearance.normal } else { appearance.disabled.as_ref().unwrap_or(&appearance.normal) };
    let hovered: &UiVisual = if enabled { appearance.hovered.as_ref().unwrap_or(normal) } else { normal };
    let pressed: &UiVisual = if enabled { appearance.pressed.as_ref().or(appearance.focused.as_ref()).or(appearance.hovered.as_ref()).unwrap_or(normal) } else { normal };
    let selected: &UiVisual = if enabled { appearance.selected.as_ref().unwrap_or(normal) } else { normal };
    let style: &mut egui::Style = ui.style_mut();
    let fallback_fill: egui::Color32 = style.visuals.widgets.inactive.bg_fill;
    style.override_font_id = Some(typography.font.clone());
    style.visuals.override_text_color = Some(typography.color);
    apply_native_widget_style(&mut style.visuals.widgets.noninteractive, normal, fallback_fill, typography.color);
    apply_native_widget_style(&mut style.visuals.widgets.inactive, normal, fallback_fill, typography.color);
    apply_native_widget_style(&mut style.visuals.widgets.hovered, hovered, fallback_fill, typography.color);
    apply_native_widget_style(&mut style.visuals.widgets.active, pressed, fallback_fill, typography.color);
    apply_native_widget_style(&mut style.visuals.widgets.open, selected, fallback_fill, typography.color);
    style.visuals.selection.bg_fill = native_fill(selected, fallback_fill);
    style.visuals.selection.stroke.color = typography.color;
    style.visuals.text_edit_bg_color = Some(egui::Color32::TRANSPARENT);
}

fn apply_native_widget_style(widget: &mut egui::style::WidgetVisuals, visual: &UiVisual, fallback_fill: egui::Color32, text_color: egui::Color32) {
    widget.fg_stroke = egui::Stroke { width: widget.fg_stroke.width, color: text_color };
    widget.bg_stroke = native_stroke(visual);
    widget.corner_radius = visual.corner_radius;
    widget.bg_fill = native_fill(visual, fallback_fill);
    widget.weak_bg_fill = widget.bg_fill;
}

fn native_fill(visual: &UiVisual, fallback_fill: egui::Color32) -> egui::Color32 {
    match visual.background {
        UiBrush::Color(color) => color * visual.tint.gamma_multiply(visual.opacity.clamp(0.0, 1.0)),
        UiBrush::None => fallback_fill,
        UiBrush::Image(_) | UiBrush::NineSlice(_) => egui::Color32::TRANSPARENT,
    }
}

fn native_stroke(visual: &UiVisual) -> egui::Stroke {
    egui::Stroke { width: visual.border.width, color: visual.border.color * visual.tint.gamma_multiply(visual.opacity.clamp(0.0, 1.0)) }
}

fn paint_element(ui: &Ui, element: &UiElement<'_>, border_rect: Rect, content_rect: Rect, clip: Rect, response: &Response) {
    let painter: egui::Painter = ui.painter_at(border_rect).with_clip_rect(clip);
    element.appearance.visual(response, element.enabled, element.selected).paint(&painter, border_rect);
    match &element.kind {
        UiComponentKind::Text(text) => paint_text(ui, element, &painter, content_rect, text, Align::Min),
        UiComponentKind::Image(image) => image.paint(&painter, content_rect),
        UiComponentKind::Rectangle(brush) => brush.paint(&painter, border_rect),
        UiComponentKind::Progress { value, foreground } => {
            let progress: f32 = if value.is_finite() { value.clamp(0.0, 1.0) } else { 0.0 };
            foreground.paint(&painter, Rect::from_min_size(content_rect.min, Vec2::new(content_rect.width() * progress, content_rect.height())));
        }
        UiComponentKind::Separator { foreground } => {
            let y: f32 = content_rect.center().y;
            foreground.unwrap_or(UiBrush::Color(element.typography.color)).paint(&painter, Rect::from_min_max(egui::pos2(content_rect.left(), y), egui::pos2(content_rect.right(), y + 1.0)));
        }
        UiComponentKind::Button { label } => paint_text(ui, element, &painter, content_rect, label, Align::Center),
        UiComponentKind::ImageButton { image } => image.paint(&painter, content_rect),
        UiComponentKind::Toggle { .. } | UiComponentKind::Slider { .. } | UiComponentKind::TextInput { .. } | UiComponentKind::Spacer { .. } | UiComponentKind::Panel | UiComponentKind::Row | UiComponentKind::Column | UiComponentKind::Overlay | UiComponentKind::Grid | UiComponentKind::Scroll => {}
    }
}

fn paint_appearance(ui: &Ui, appearance: &UiAppearance, rect: Rect, clip: Rect, response: &Response, enabled: bool, selected: bool) {
    appearance.visual(response, enabled, selected).paint(&ui.painter_at(rect).with_clip_rect(clip), rect);
}

fn reserve_native_visual(ui: &Ui, rect: Rect, clip: Rect) -> (egui::Painter, egui::layers::ShapeIdx) {
    let painter: egui::Painter = ui.painter_at(rect).with_clip_rect(clip.intersect(rect));
    let shape_index: egui::layers::ShapeIdx = painter.add(egui::Shape::Noop);
    (painter, shape_index)
}

fn paint_reserved_native_visual(slot: Option<(egui::Painter, egui::layers::ShapeIdx)>, visual: &UiVisual, rect: Rect) {
    if let Some((painter, shape_index)) = slot {
        painter.set(shape_index, visual.shape(rect));
    }
}

fn paint_text(ui: &Ui, element: &UiElement<'_>, painter: &egui::Painter, rect: Rect, text: &str, vertical_align: Align) {
    paint_text_with_style(ui, &element.typography, painter, rect, text, vertical_align);
}

fn paint_text_with_style(ui: &Ui, typography: &UiTextStyle, painter: &egui::Painter, rect: Rect, text: &str, vertical_align: Align) {
    let galley: Arc<egui::Galley> = typography.layout(ui, text, rect.width());
    let x: f32 = match typography.align {
        Align::Min => rect.left() - galley.rect.left(),
        Align::Center => rect.center().x - galley.rect.center().x,
        Align::Max => rect.right() - galley.rect.right(),
    };
    let y: f32 = match vertical_align {
        Align::Min => rect.top() - galley.rect.top(),
        Align::Center => rect.center().y - galley.rect.center().y,
        Align::Max => rect.bottom() - galley.rect.bottom(),
    };
    painter.galley(egui::pos2(x, y), galley, typography.color);
}

fn paint_toggle(ui: &Ui, typography: &UiTextStyle, rect: Rect, clip: Rect, response: &Response, value: bool, label: &str) {
    let painter: egui::Painter = ui.painter_at(rect).with_clip_rect(clip);
    let content_rect: Rect = rect;
    let icon_size: f32 = ui.spacing().icon_width.min(content_rect.height()).max(0.0);
    if icon_size > 0.0 {
        let icon_rect: Rect = Rect::from_min_size(
            egui::pos2(content_rect.left(), content_rect.center().y - icon_size * 0.5),
            Vec2::splat(icon_size),
        );
        let widget: &egui::style::WidgetVisuals = ui.style().interact(response);
        painter.rect_filled(icon_rect, widget.corner_radius, widget.bg_fill);
        let mut stroke: egui::Stroke = widget.bg_stroke;
        if stroke.width <= 0.0 {
            stroke = egui::Stroke { width: 1.0, color: typography.color };
        }
        painter.rect_stroke(icon_rect, widget.corner_radius, stroke, egui::StrokeKind::Inside);
        if value {
            let check_rect: Rect = icon_rect.shrink(icon_size * 0.24);
            painter.line_segment(
                [egui::pos2(check_rect.left(), check_rect.center().y), egui::pos2(check_rect.center().x, check_rect.bottom())],
                egui::Stroke { width: widget.fg_stroke.width.max(1.0), color: typography.color },
            );
            painter.line_segment(
                [egui::pos2(check_rect.center().x, check_rect.bottom()), egui::pos2(check_rect.right(), check_rect.top())],
                egui::Stroke { width: widget.fg_stroke.width.max(1.0), color: typography.color },
            );
        }
    }
    let label_left: f32 = (content_rect.left() + icon_size + ui.spacing().item_spacing.x).min(content_rect.right());
    let label_rect: Rect = Rect::from_min_max(egui::pos2(label_left, content_rect.top()), content_rect.max);
    paint_text_with_style(ui, typography, &painter, label_rect, label, Align::Center);
}

fn invalid_node_error(node_index: usize) -> taffy::TaffyError {
    taffy::TaffyError::InvalidInputNode(taffy::NodeId::new(node_index as u64))
}
