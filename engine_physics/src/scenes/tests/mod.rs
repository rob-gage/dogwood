// Copyright Rob Gage 2026

use super::{GasDownload, GasUpload};
use crate::chunks::ChunkGasCell;
use crate::materials::{MaterialForm, MaterialIdentifier, MaterialRegistry};
use crate::tiles::{CellCoordinates, TileArea, TileCoordinates};

#[test]
fn test_rejects_a_cell_outside_its_upload_area() {
    let upload: GasUpload = GasUpload::new(
        TileArea::new(TileCoordinates { x: 0, y: 0 }, 1, 1),
        vec![ChunkGasCell {
            coordinates: CellCoordinates { x: 8, y: 0 },
            velocity: [0.0; 2],
            species: vec![(MaterialIdentifier::from_u32(1), 1.0)],
            temperature: 293.15,
        }],
    );
    assert!(upload.validate(&MaterialRegistry::new()).is_err());
}

#[test]
fn test_rejects_invalid_temperature_before_constructing_gas_cells() {
    let area = TileArea::new(TileCoordinates { x: 0, y: 0 }, 1, 1);
    let identifier = MaterialIdentifier::new(MaterialForm::Gas, 0);
    let stride = 5 + 1;
    let mut bytes = vec![0u8; 64 * 64 * stride * 4];
    bytes[16..20].copy_from_slice(&f32::NAN.to_bits().to_le_bytes());
    bytes[20..24].copy_from_slice(&1.0f32.to_bits().to_le_bytes());
    assert!(GasDownload::deserialize(&bytes, area, &[identifier]).is_err());
}
