// Copyright Rob Gage 2026

use crate::materials::MaterialIdentifier;
use crate::tiles::{TileCoordinates, TileData};
use std::{io, task::Waker};

/// Tracks a nonblocking native-to-`Accelerator` tile upload
pub struct TileUpload {
    /// The world tile being uploaded
    pub coordinates: TileCoordinates,
    /// Material identifiers in Accelerator cell order
    pub material_identifiers: Vec<u8>,
    /// Persistent appearances in matching Accelerator cell order
    pub appearances: Vec<u8>,
    /// Persistent integrities in matching Accelerator cell order
    pub integrities: Vec<u8>,
    /// Persistent normalized material inventories in matching Accelerator cell order
    pub amounts: Vec<u8>,
    /// Persistent material temperatures in matching Accelerator cell order
    pub temperatures: Vec<u8>,
    /// Whether the upload has completed
    pub is_complete: bool,
    /// The completed upload result
    pub result: Option<Result<(), io::Error>>,
    /// The task woken when the upload completes
    pub waker: Option<Waker>,
}

impl TileUpload {
    /// Creates a pending tile upload with serialized `TileData`
    pub fn new(coordinates: TileCoordinates, tile_data: &TileData) -> Self {
        let mut material_identifiers: Vec<u8> =
            Vec::with_capacity(TileData::CELL_FIELD_SERIALIZED_SIZE);
        tile_data
            .serialize_material_identifiers(&mut material_identifiers)
            .expect("Writing to a Vec cannot fail");
        let mut appearances: Vec<u8> = Vec::with_capacity(TileData::CELL_FIELD_SERIALIZED_SIZE);
        tile_data
            .serialize_appearances(&mut appearances)
            .expect("Writing to a Vec cannot fail");
        let mut integrities: Vec<u8> = Vec::with_capacity(TileData::CELL_FIELD_SERIALIZED_SIZE);
        tile_data
            .serialize_integrities(&mut integrities)
            .expect("Writing to a Vec cannot fail");
        let mut amounts: Vec<u8> = Vec::with_capacity(TileData::CELL_FIELD_SERIALIZED_SIZE);
        tile_data
            .serialize_amounts(&mut amounts)
            .expect("Writing to a Vec cannot fail");
        let mut temperatures: Vec<u8> = Vec::with_capacity(TileData::CELL_FIELD_SERIALIZED_SIZE);
        tile_data
            .serialize_temperatures(&mut temperatures)
            .expect("Writing to a Vec cannot fail");
        Self {
            coordinates,
            material_identifiers,
            appearances,
            integrities,
            amounts,
            temperatures,
            result: None,
            is_complete: false,
            waker: None,
        }
    }

    /// Resolves legacy dormant temperatures immediately before Accelerator upload.
    pub fn resolve_uninitialized_state(
        &mut self,
        initial_temperature: impl Fn(MaterialIdentifier) -> f32,
    ) {
        for index in 0..64 {
            let offset: usize = index * 4;
            let identifier: MaterialIdentifier = MaterialIdentifier::from_u32(u32::from_le_bytes(
                self.material_identifiers[offset..offset + 4]
                    .try_into()
                    .unwrap(),
            ));
            if identifier == MaterialIdentifier::NULL {
                self.amounts[offset..offset + 4].copy_from_slice(&0.0f32.to_bits().to_le_bytes());
                self.temperatures[offset..offset + 4]
                    .copy_from_slice(&0.0f32.to_bits().to_le_bytes());
                continue;
            }
            let temperature: f32 = f32::from_bits(u32::from_le_bytes(
                self.temperatures[offset..offset + 4].try_into().unwrap(),
            ));
            if !temperature.is_finite() {
                self.amounts[offset..offset + 4].copy_from_slice(&1.0f32.to_bits().to_le_bytes());
                self.temperatures[offset..offset + 4]
                    .copy_from_slice(&initial_temperature(identifier).to_bits().to_le_bytes());
            }
        }
    }
}
