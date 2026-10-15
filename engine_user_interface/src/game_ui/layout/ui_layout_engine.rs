// Copyright Rob Gage 2026

//! Reusable Taffy layout adapter for game UI nodes

use super::{UiComputedLayout, UiLayout, UiLayoutNode, UiLength};
use std::collections::{HashMap, HashSet};

/// Reusable Taffy layout tree and node cache
#[derive(Debug)]
pub struct UiLayoutEngine {
    taffy_tree: taffy::TaffyTree,
    node_cache: HashMap<egui::Id, taffy::NodeId>,
    rounding_enabled: bool,
}

impl Default for UiLayoutEngine {

    fn default() -> Self {
        Self::new()
    }
}

impl UiLayoutEngine {

    /// Creates a layout engine with smooth, unrounded logical coordinates
    #[must_use]
    pub fn new() -> Self {
        let mut taffy_tree: taffy::TaffyTree = taffy::TaffyTree::new();
        taffy_tree.disable_rounding();
        Self { taffy_tree, node_cache: HashMap::new(), rounding_enabled: false }
    }

    /// Creates an engine with configurable logical-unit rounding
    #[must_use]
    pub fn with_rounding(mut self, enabled: bool) -> Self {
        self.set_rounding(enabled);
        self
    }

    /// Enables or disables Taffy logical-unit rounding
    pub fn set_rounding(&mut self, enabled: bool) {
        self.rounding_enabled = enabled;
        if enabled {
            self.taffy_tree.enable_rounding();
        } else {
            self.taffy_tree.disable_rounding();
        }
    }

    /// Returns whether logical-unit rounding is enabled
    #[must_use]
    pub fn rounding_enabled(&self) -> bool {
        self.rounding_enabled
    }

    /// Computes absolute geometry for a flattened layout tree
    ///
    /// Percentages resolve against each Taffy parent content box. Taffy's
    /// `MinContent` and `MaxContent` available-space behavior remains the
    /// fallback for an indefinite axis supplied by a parent style.
    pub fn compute(
        &mut self,
        nodes: &[UiLayoutNode],
        root: usize,
        bounds: egui::Rect,
        mut measure: impl FnMut(usize, taffy::geometry::Size<Option<f32>>, taffy::geometry::Size<taffy::AvailableSpace>) -> egui::Vec2,
    ) -> Result<Vec<UiComputedLayout>, taffy::TaffyError> {
        if nodes.is_empty() {
            return Ok(Vec::new());
        }
        validate_nodes(nodes, root)?;
        let safe_bounds: egui::Rect = sanitize_bounds(bounds);
        let normalized_styles: Vec<taffy::Style> = normalize_styles(nodes, root, safe_bounds);

        let mut node_ids: Vec<taffy::NodeId> = Vec::with_capacity(nodes.len());
        let mut active_ids: HashSet<egui::Id> = HashSet::with_capacity(nodes.len());
        for node in nodes {
            active_ids.insert(node.id);
        }
        for (node_index, node) in nodes.iter().enumerate() {
            let node_id: taffy::NodeId = match self.node_cache.get(&node.id).copied() {
                Some(node_id) => node_id,
                None => {
                    let new_node_id: taffy::NodeId = self.taffy_tree.new_leaf(normalized_styles[node_index].clone())?;
                    self.node_cache.insert(node.id, new_node_id);
                    new_node_id
                }
            };
            node_ids.push(node_id);
        }

        for (node_index, node) in nodes.iter().enumerate() {
            self.taffy_tree.set_style(node_ids[node_index], normalized_styles[node_index].clone())?;
            let child_ids: Vec<taffy::NodeId> = node.children.iter().map(|child_index: &usize| node_ids[*child_index]).collect();
            self.taffy_tree.set_children(node_ids[node_index], &child_ids)?;
        }

        let stale_ids: Vec<egui::Id> = self
            .node_cache
            .keys()
            .copied()
            .filter(|cached_id: &egui::Id| !active_ids.contains(cached_id))
            .collect();
        for stale_id in stale_ids {
            if let Some(stale_node_id) = self.node_cache.remove(&stale_id) {
                self.taffy_tree.remove(stale_node_id)?;
            }
        }

        for (node_index, node) in nodes.iter().enumerate() {
            if node.children.is_empty() {
                self.taffy_tree.mark_dirty(node_ids[node_index])?;
            }
        }

        let available_space: taffy::Size<taffy::AvailableSpace> = taffy::Size {
            width: taffy::AvailableSpace::Definite(safe_bounds.width()),
            height: taffy::AvailableSpace::Definite(safe_bounds.height()),
        };
        let mut node_indexes: HashMap<taffy::NodeId, usize> = HashMap::with_capacity(node_ids.len());
        for (node_index, node_id) in node_ids.iter().copied().enumerate() {
            node_indexes.insert(node_id, node_index);
        }
        self.taffy_tree.compute_layout_with_measure(node_ids[root], available_space, |inputs, node_id, _context, style| {
            let node_index: usize = node_indexes.get(&node_id).copied().unwrap_or_default();
            taffy::compute_leaf_layout(
                inputs,
                style,
                |_pointer: *const (), _basis: f32| 0.0,
                |known_dimensions, available_space| {
                    let measured_size: egui::Vec2 = measure(node_index, known_dimensions, available_space);
                    taffy::Size {
                        width: sanitize_measurement(measured_size.x),
                        height: sanitize_measurement(measured_size.y),
                    }
                },
            )
        })?;

        let mut computed_layouts: Vec<UiComputedLayout> = vec![UiComputedLayout::default(); nodes.len()];
        let root_origin: egui::Pos2 = safe_bounds.min;
        let mut pending_nodes: Vec<(usize, egui::Pos2)> = vec![(root, root_origin)];
        while let Some((node_index, parent_origin)) = pending_nodes.pop() {
            let layout: taffy::Layout = *self.taffy_tree.layout(node_ids[node_index])?;
            let node_origin: egui::Pos2 = parent_origin
                + egui::vec2(sanitize_coordinate(layout.location.x), sanitize_coordinate(layout.location.y));
            let border_box_size: egui::Vec2 = egui::vec2(
                sanitize_measurement(layout.size.width),
                sanitize_measurement(layout.size.height),
            );
            let allocated_content_size: egui::Vec2 = egui::vec2(
                sanitize_measurement(layout.content_box_width()),
                sanitize_measurement(layout.content_box_height()),
            );
            let content_size: egui::Vec2 = egui::vec2(
                allocated_content_size.x.max(scrollable_overflow_width(&layout)),
                allocated_content_size.y.max(scrollable_overflow_height(&layout)),
            );
            let rect: egui::Rect = egui::Rect::from_min_size(node_origin, border_box_size);
            let content_inset_left: f32 = sanitize_measurement(layout.border.left + layout.padding.left);
            let content_inset_top: f32 = sanitize_measurement(layout.border.top + layout.padding.top);
            let content_origin: egui::Pos2 = node_origin
                + egui::vec2(content_inset_left, content_inset_top);
            let content_rect: egui::Rect = egui::Rect::from_min_size(content_origin, allocated_content_size);
            computed_layouts[node_index] = UiComputedLayout { rect, content_rect, content_size };
            for child_index in nodes[node_index].children.iter().rev().copied() {
                pending_nodes.push((child_index, node_origin));
            }
        }
        Ok(computed_layouts)
    }
}

fn normalize_styles(
    nodes: &[UiLayoutNode],
    root: usize,
    bounds: egui::Rect,
) -> Vec<taffy::Style> {
    let mut parent_indexes: Vec<Option<usize>> = vec![None; nodes.len()];
    for (parent_index, node) in nodes.iter().enumerate() {
        for child_index in node.children.iter().copied() {
            if child_index < nodes.len() {
                parent_indexes[child_index] = Some(parent_index);
            }
        }
    }

    let mut styles: Vec<taffy::Style> = nodes.iter().map(|node: &UiLayoutNode| node.layout.style.clone()).collect();
    for node_index in 0..nodes.len() {
        if node_index == root {
            apply_root_flex_intent(&mut styles[node_index], &nodes[node_index].layout, bounds);
            continue;
        }
        let Some(parent_index) = parent_indexes[node_index] else {
            continue;
        };
        let parent_style: taffy::Style = styles[parent_index].clone();
        normalize_child_style(
            &mut styles[node_index],
            &nodes[node_index].layout,
            &parent_style,
        );
    }
    styles
}

fn apply_root_flex_intent(style: &mut taffy::Style, layout: &UiLayout, bounds: egui::Rect) {
    if layout.desired_width.and_then(UiLength::flex_weight).is_some() && !raw_dimension_override(style, true) {
        style.size.width = taffy::Dimension::length(sanitize_measurement(bounds.width()));
    }
    if layout.desired_height.and_then(UiLength::flex_weight).is_some() && !raw_dimension_override(style, false) {
        style.size.height = taffy::Dimension::length(sanitize_measurement(bounds.height()));
    }
}

fn normalize_child_style(style: &mut taffy::Style, layout: &UiLayout, parent_style: &taffy::Style) {
    if style.position == taffy::Position::Absolute {
        apply_absolute_fill(style, layout);
        return;
    }

    if parent_style.display == taffy::Display::Grid {
        // Grid has no flex main axis. Fill/Flex therefore use the grid cell's
        // percentage/stretch behavior as the documented fallback.
        apply_grid_fill(style, layout);
        return;
    }
    if parent_style.display != taffy::Display::Flex {
        return;
    }

    let main_axis_is_width: bool = matches!(
        parent_style.flex_direction,
        taffy::FlexDirection::Row | taffy::FlexDirection::RowReverse
    );
    if main_axis_is_width {
        apply_main_axis_flex(style, layout.desired_width, true);
        apply_cross_axis_fill(style, layout.desired_height, false);
    } else {
        apply_main_axis_flex(style, layout.desired_height, false);
        apply_cross_axis_fill(style, layout.desired_width, true);
    }
}

fn apply_main_axis_flex(style: &mut taffy::Style, desired: Option<UiLength>, width: bool) {
    if raw_flex_override(style) || raw_dimension_override(style, width) {
        return;
    }
    if let Some(flex_weight) = desired.and_then(UiLength::flex_weight) {
        style.flex_grow = flex_weight;
        style.flex_basis = taffy::Dimension::length(0.0);
    }
}

fn apply_cross_axis_fill(style: &mut taffy::Style, desired: Option<UiLength>, width: bool) {
    if desired.and_then(UiLength::flex_weight).is_none() || raw_dimension_override(style, width) {
        return;
    }
    // A flex weight has no cross-axis meaning; both Fill and Flex therefore
    // use the useful cross-axis fill fallback rather than changing flex_grow.
    let dimension: taffy::Dimension = taffy::Dimension::percent(1.0);
    if width {
        style.size.width = dimension;
    } else {
        style.size.height = dimension;
    }
}

fn apply_grid_fill(style: &mut taffy::Style, layout: &UiLayout) {
    if !raw_flex_override(style) {
        if layout.desired_width.and_then(UiLength::flex_weight).is_some() && !raw_dimension_override(style, true) {
            style.size.width = taffy::Dimension::percent(1.0);
        }
        if layout.desired_height.and_then(UiLength::flex_weight).is_some() && !raw_dimension_override(style, false) {
            style.size.height = taffy::Dimension::percent(1.0);
        }
    }
}

fn apply_absolute_fill(style: &mut taffy::Style, layout: &UiLayout) {
    let anchored_width: bool = style.inset.left != taffy::LengthPercentageAuto::auto() && style.inset.right != taffy::LengthPercentageAuto::auto();
    let anchored_height: bool = style.inset.top != taffy::LengthPercentageAuto::auto() && style.inset.bottom != taffy::LengthPercentageAuto::auto();
    if !anchored_width && layout.desired_width.and_then(UiLength::flex_weight).is_some() && !raw_dimension_override(style, true) {
        style.size.width = taffy::Dimension::percent(1.0);
    }
    if !anchored_height && layout.desired_height.and_then(UiLength::flex_weight).is_some() && !raw_dimension_override(style, false) {
        style.size.height = taffy::Dimension::percent(1.0);
    }
}

fn raw_dimension_override(style: &taffy::Style, width: bool) -> bool {
    let dimension: taffy::Dimension = if width { style.size.width } else { style.size.height };
    dimension != taffy::Dimension::auto()
}

fn raw_flex_override(style: &taffy::Style) -> bool {
    style.flex_grow != 0.0 || style.flex_basis != taffy::Dimension::auto()
}

fn scrollable_overflow_width(layout: &taffy::Layout) -> f32 {
    sanitize_measurement(layout.scrollable_overflow_rect.right - layout.scrollable_overflow_rect.left)
}

fn scrollable_overflow_height(layout: &taffy::Layout) -> f32 {
    sanitize_measurement(layout.scrollable_overflow_rect.bottom - layout.scrollable_overflow_rect.top)
}

fn validate_nodes(nodes: &[UiLayoutNode], root: usize) -> Result<(), taffy::TaffyError> {
    if root >= nodes.len() {
        return Err(taffy::TaffyError::InvalidInputNode(taffy::NodeId::new(root as u64)));
    }
    let mut seen_ids: HashSet<egui::Id> = HashSet::with_capacity(nodes.len());
    for (node_index, node) in nodes.iter().enumerate() {
        if !seen_ids.insert(node.id) {
            return Err(invalid_node_error(node_index));
        }
    }
    let mut visit_states: Vec<u8> = vec![0; nodes.len()];
    let mut parent_counts: Vec<usize> = vec![0; nodes.len()];
    visit_node(nodes, root, &mut visit_states, &mut parent_counts)?;
    if visit_states.iter().any(|state: &u8| *state == 0) {
        return Err(invalid_node_error(
            visit_states.iter().position(|state: &u8| *state == 0).unwrap_or(root),
        ));
    }
    Ok(())
}

fn visit_node(
    nodes: &[UiLayoutNode],
    node_index: usize,
    visit_states: &mut [u8],
    parent_counts: &mut [usize],
) -> Result<(), taffy::TaffyError> {
    if visit_states[node_index] == 1 {
        return Err(invalid_node_error(node_index));
    }
    if visit_states[node_index] == 2 {
        return Ok(());
    }
    visit_states[node_index] = 1;
    for child_index in nodes[node_index].children.iter().copied() {
        if child_index >= nodes.len() {
            return Err(invalid_node_error(node_index));
        }
        parent_counts[child_index] += 1;
        if parent_counts[child_index] > 1 {
            return Err(invalid_node_error(child_index));
        }
        visit_node(nodes, child_index, visit_states, parent_counts)?;
    }
    visit_states[node_index] = 2;
    Ok(())
}

fn invalid_node_error(node_index: usize) -> taffy::TaffyError {
    taffy::TaffyError::InvalidInputNode(taffy::NodeId::new(node_index as u64))
}

fn sanitize_bounds(bounds: egui::Rect) -> egui::Rect {
    let min_x: f32 = sanitize_coordinate(bounds.min.x);
    let min_y: f32 = sanitize_coordinate(bounds.min.y);
    let width: f32 = sanitize_measurement(bounds.width());
    let height: f32 = sanitize_measurement(bounds.height());
    egui::Rect::from_min_size(egui::pos2(min_x, min_y), egui::vec2(width, height))
}

fn sanitize_coordinate(value: f32) -> f32 {
    if value.is_finite() { value } else { 0.0 }
}

fn sanitize_measurement(value: f32) -> f32 {
    if value.is_finite() { value.max(0.0) } else { 0.0 }
}
