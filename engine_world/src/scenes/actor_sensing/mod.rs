// Copyright Rob Gage 2026

mod actor_sensing_pass;

pub(crate) use actor_sensing_pass::ActorSensingPass;
use crate::materials::{
    MaterialForm,
    MaterialTable,
};

/// Maximum number of material tags whose touching cells `Scene` counts for each actor
pub const SCENE_ACTOR_SENSING_TAG_COUNT_MAXIMUM: usize = 8;

/// What `Scene` senses around actors each simulation step
#[derive(Clone, Debug, PartialEq)]
pub struct SceneActorSensingConfiguration {
    /// Material tags counted, in the order of `SceneActorSensing::tag_cell_counts`; at most 8
    pub tags: Vec<String>,
    /// Distance outside an actor's collider, in cells, within which a cell touches the actor
    pub contact_margin: f32,
    /// Distance outside an actor's collider, in cells, of the band whose cells it breathes from
    pub breathing_margin: f32,
    /// Largest total amount of species gas in a cell, in atmospheres, that is still breathable;
    /// the ambient air of empty cells is always breathable
    pub breathable_gas_amount: f32,
    /// Minimum species-gas amount, in atmospheres, for its material tags to count
    pub gas_tag_amount: f32,
}

impl Default for SceneActorSensingConfiguration {

    /// Returns no tags, a 1.5-cell contact margin, a 2.5-cell breathing band, and half an
    /// atmosphere of breathable species gas
    fn default() -> Self {
        Self {
            tags: Vec::new(),
            contact_margin: 1.5,
            breathing_margin: 2.5,
            breathable_gas_amount: 0.5,
            gas_tag_amount: 0.05,
        }
    }

}

/// Cells sensed around one actor at a recent simulation step
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct SceneActorSensing {
    /// Cells touching the actor whose material carries each configured tag, by tag order
    pub tag_cell_counts: [u32; SCENE_ACTOR_SENSING_TAG_COUNT_MAXIMUM],
    /// Cells of the breathing band that are breathable
    pub breathable_cell_count: u32,
    /// All cells of the breathing band inside the active region
    pub breathing_cell_count: u32,
    /// Highest valid cell temperature, in kelvin, in the breathing band or touching cells
    pub maximum_temperature: f32,
    /// Highest absolute published pressure, in gauge atmospheres, in those cells
    pub maximum_pressure: f32,
}

/// Returns whether an actor sensing configuration has at most eight tags and finite,
/// nonnegative margins and gas amount
pub(super) fn is_valid_configuration(configuration: &SceneActorSensingConfiguration) -> bool {
    configuration.tags.len() <= SCENE_ACTOR_SENSING_TAG_COUNT_MAXIMUM
        && [
            configuration.contact_margin,
            configuration.breathing_margin,
            configuration.breathable_gas_amount,
            configuration.gas_tag_amount,
        ].into_iter().all(|value: f32| value.is_finite() && value >= 0.0)
}

/// Returns the slot of a form in the tag mask layout: cellular static, cellular dynamic, fluid,
/// then gas
fn form_slot(form: MaterialForm) -> Option<usize> {
    match form {
        MaterialForm::CellularStatic => Some(0),
        MaterialForm::CellularDynamic => Some(1),
        MaterialForm::Fluid => Some(2),
        MaterialForm::Gas => Some(3),
    }
}

/// Returns the number of material indices of each form, each one past its highest registered index
pub(crate) fn form_index_counts(material_table: &MaterialTable) -> [u32; 4] {
    let mut counts: [u32; 4] = [0; 4];
    for (identifier, _, form) in material_table.materials() {
        let Some(slot) = form_slot(form) else { continue; };
        counts[slot] = counts[slot].max(identifier.index() + 1);
    }
    counts
}

/// Returns masks for cellular static, dynamic, fluid, then gas indices; bit `i` is set when the
/// material carries configured tag `i`
pub(crate) fn tag_masks(material_table: &MaterialTable, tags: &[String]) -> Vec<u32> {
    let counts: [u32; 4] = form_index_counts(material_table);
    let offsets: [u32; 4] = [0, counts[0], counts[0] + counts[1], counts[0] + counts[1] + counts[2]];
    let mut masks: Vec<u32> = vec![0; counts.iter().sum::<u32>() as usize];
    for (identifier, _, form) in material_table.materials() {
        let Some(slot) = form_slot(form) else { continue; };
        let material_tags: Vec<String> = material_table.tags(identifier);
        let mask: u32 = tags.iter().enumerate()
            .filter(|(_, tag)| material_tags.contains(tag))
            .fold(0, |mask: u32, (tag_index, _)| mask | 1 << tag_index);
        masks[(offsets[slot] + identifier.index()) as usize] = mask;
    }
    masks
}

#[cfg(test)]
mod tests {

    use super::{
        is_valid_configuration,
        SceneActorSensingConfiguration,
    };

    #[test]
    fn configuration_limits_are_validated() {
        let default: SceneActorSensingConfiguration = SceneActorSensingConfiguration::default();
        assert!(is_valid_configuration(&default));
        let mut invalid: SceneActorSensingConfiguration = default.clone();
        invalid.contact_margin = f32::NAN;
        assert!(!is_valid_configuration(&invalid));
        invalid = default.clone();
        invalid.breathing_margin = -1.0;
        assert!(!is_valid_configuration(&invalid));
        invalid = default.clone();
        invalid.breathable_gas_amount = f32::INFINITY;
        assert!(!is_valid_configuration(&invalid));
        invalid = default.clone();
        invalid.gas_tag_amount = -0.01;
        assert!(!is_valid_configuration(&invalid));
        invalid = default;
        invalid.tags = (0..9).map(|index: i32| index.to_string()).collect();
        assert!(!is_valid_configuration(&invalid));
    }

}
