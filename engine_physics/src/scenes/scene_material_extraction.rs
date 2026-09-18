// Copyright Rob Gage 2026

use std::fmt;

use super::Scene;
use crate::materials::MaterialForm;
use crate::materials::MaterialIdentifier;

/// A resident world-space area from which authoritative material may be removed.
#[derive(Clone, Copy)]
pub enum SceneRegion {
    /// A circular gameplay collection area in world/tile units.
    Circle {
        /// Center of the circle in world space.
        center: super::ScenePosition,
        /// Radius in world/tile units.
        radius: f32,
    },
}

impl SceneRegion {
    /// Returns an error when the region contains invalid numeric values.
    pub fn validate(self) -> Result<(), MaterialExtractionError> {
        match self {
            Self::Circle { center, radius }
                if center.x_offset.is_finite()
                    && center.y_offset.is_finite()
                    && radius.is_finite()
                    && radius >= 0.0 =>
            {
                Ok(())
            }
            Self::Circle { .. } => Err(MaterialExtractionError::InvalidRegion),
        }
    }

    pub(crate) fn circle(self) -> (super::ScenePosition, f32) {
        match self {
            Self::Circle { center, radius } => (center, radius),
        }
    }

    /// Returns whether a world position lies inside this region.
    pub fn contains(self, position: super::ScenePosition) -> bool {
        let (center, radius) = self.circle();
        let center = [
            center.tile_coordinates.x as f32 + center.x_offset,
            center.tile_coordinates.y as f32 + center.y_offset,
        ];
        let position = [
            position.tile_coordinates.x as f32 + position.x_offset,
            position.tile_coordinates.y as f32 + position.y_offset,
        ];
        let difference = [position[0] - center[0], position[1] - center[1]];
        difference[0] * difference[0] + difference[1] * difference[1] <= radius * radius
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tiles::TileCoordinates;

    fn position(x: f32, y: f32) -> super::super::ScenePosition {
        super::super::ScenePosition {
            tile_coordinates: TileCoordinates { x: 0, y: 0 },
            x_offset: x,
            y_offset: y,
        }
    }

    #[test]
    fn circle_crosses_tile_boundaries_and_rejects_outside_positions() {
        let region = SceneRegion::Circle {
            center: position(0.95, 0.5),
            radius: 0.2,
        };
        assert!(region.contains(position(1.0, 0.5)));
        assert!(!region.contains(position(1.3, 0.5)));
    }

    #[test]
    fn invalid_region_values_are_rejected() {
        assert!(
            SceneRegion::Circle {
                center: position(f32::NAN, 0.0),
                radius: 1.0,
            }
            .validate()
            .is_err()
        );
        assert!(
            SceneRegion::Circle {
                center: position(0.0, 0.0),
                radius: -1.0,
            }
            .validate()
            .is_err()
        );
    }
}

/// The CPU-compiled material predicate used by an extraction request.
#[derive(Clone, Eq, PartialEq)]
pub enum MaterialFilter {
    /// Match every registered material.
    Any,
    /// Match one exact material identifier.
    Material(MaterialIdentifier),
    /// Match all members of one registered material tag.
    Tag(String),
    /// Match all materials with one material form.
    Form(MaterialForm),
}

/// A request to remove matching authoritative material from a resident region.
#[derive(Clone)]
pub struct MaterialExtraction {
    /// The world-space extraction area.
    pub region: SceneRegion,
    /// The material predicate compiled before GPU submission.
    pub filter: MaterialFilter,
}

/// Stable identity returned when an extraction request is accepted.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct MaterialExtractionRequest(u64);

impl MaterialExtractionRequest {
    /// Returns the stable numeric request identity.
    pub const fn as_u64(self) -> u64 {
        self.0
    }
}

/// One nonzero material total removed by an extraction request.
#[derive(Clone, Copy)]
pub struct MaterialExtractionAmount {
    /// Material removed from authoritative simulation state.
    pub material: MaterialIdentifier,
    /// Stored normalized amount removed from that material.
    pub amount: f32,
}

/// The asynchronously delivered aggregate result of one extraction request.
#[derive(Clone)]
pub struct MaterialExtractionResult {
    /// Request identity associated with this completion.
    pub request: MaterialExtractionRequest,
    /// Nonzero amounts sorted by dense material index.
    pub materials: Vec<MaterialExtractionAmount>,
}

/// Failure reported before an extraction request is submitted.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MaterialExtractionError {
    /// The region contains a non-finite coordinate or invalid radius.
    InvalidRegion,
    /// The filter references an unknown material identifier.
    UnknownMaterial,
    /// The filter references a tag that is not registered.
    UnknownTag,
    /// The bounded pending-request queue is full.
    QueueFull,
}

impl fmt::Display for MaterialExtractionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidRegion => "material extraction region is invalid",
            Self::UnknownMaterial => "material extraction references an unknown material",
            Self::UnknownTag => "material extraction references an unknown tag",
            Self::QueueFull => "material extraction queue is full",
        })
    }
}

impl std::error::Error for MaterialExtractionError {}

pub(crate) struct PendingMaterialExtraction {
    pub(crate) request: MaterialExtractionRequest,
    pub(crate) region: SceneRegion,
    pub(crate) material_mask: Vec<u32>,
}

impl Scene {
    /// Queues asynchronous removal of matching resident authoritative material.
    ///
    /// Material amount uses each representation's stored normalized amount. The
    /// resident portion is processed without waiting for chunk streaming; data
    /// outside the current resident simulation area remains untouched.
    pub fn extract_materials(
        &mut self,
        extraction: MaterialExtraction,
    ) -> Result<MaterialExtractionRequest, MaterialExtractionError> {
        extraction.region.validate()?;
        let material_count: usize = self.materials().material_count() as usize;
        let mut material_mask: Vec<u32> = vec![0; material_count.div_ceil(32)];
        match extraction.filter {
            MaterialFilter::Any => {
                for mask_word in &mut material_mask {
                    *mask_word = u32::MAX;
                }
                if !material_count.is_multiple_of(32)
                    && let Some(mask_word) = material_mask.last_mut()
                {
                    *mask_word = (1u32 << (material_count % 32)) - 1;
                }
            }
            MaterialFilter::Material(material_identifier) => {
                let Some(dense_index) = self.materials().dense_index(material_identifier) else {
                    return Err(MaterialExtractionError::UnknownMaterial);
                };
                material_mask[dense_index as usize / 32] |= 1u32 << (dense_index % 32);
            }
            MaterialFilter::Tag(tag) => {
                let Some(material_identifiers) = self.materials().tag_members(&tag) else {
                    return Err(MaterialExtractionError::UnknownTag);
                };
                for &material_identifier in material_identifiers {
                    if let Some(dense_index) = self.materials().dense_index(material_identifier) {
                        material_mask[dense_index as usize / 32] |= 1u32 << (dense_index % 32);
                    }
                }
            }
            MaterialFilter::Form(material_form) => {
                for (material_identifier, _) in self.materials().iter() {
                    if material_identifier.form() == material_form {
                        let dense_index =
                            self.materials().dense_index(material_identifier).unwrap();
                        material_mask[dense_index as usize / 32] |= 1u32 << (dense_index % 32);
                    }
                }
            }
        }
        if self.material_extractions_queue.len() >= 64 {
            return Err(MaterialExtractionError::QueueFull);
        }
        let request: MaterialExtractionRequest =
            MaterialExtractionRequest(self.material_extraction_request_next);
        self.material_extraction_request_next = self
            .material_extraction_request_next
            .checked_add(1)
            .expect("material extraction request identity exhausted");
        self.material_extractions_queue
            .push_back(PendingMaterialExtraction {
                request,
                region: extraction.region,
                material_mask,
            });
        Ok(request)
    }

    /// Takes completed extraction results exactly once.
    pub fn take_material_extraction_results(&mut self) -> Vec<MaterialExtractionResult> {
        std::mem::take(&mut self.material_extraction_results)
    }
}
