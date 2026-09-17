// Copyright Rob Gage 2026

use super::{CellCoordinates, CellularAppearance, TileData};
use crate::materials::{MaterialForm, MaterialIdentifier};

#[test]
fn test_from_world_position_floors_cell_coordinates() {
    assert!(CellCoordinates::from_world_position([0.01, 0.0]) == CellCoordinates { x: 0, y: 0 });
    assert!(CellCoordinates::from_world_position([0.99, 0.0]) == CellCoordinates { x: 7, y: 0 });
    assert!(CellCoordinates::from_world_position([1.0, 0.0]) == CellCoordinates { x: 8, y: 0 });
    assert!(CellCoordinates::from_world_position([-0.01, 0.0]) == CellCoordinates { x: -1, y: 0 });
    assert!(
        CellCoordinates::from_world_position([-1.0, -0.01]) == CellCoordinates { x: -8, y: -1 }
    );
}

#[test]
fn test_current_format_round_trips_amount_and_temperature() {
    let material = MaterialIdentifier::new(MaterialForm::CellularStatic, 2);
    let mut tile = TileData::EMPTY;
    tile.set_cell_with_integrity(3, 4, material, CellularAppearance(17), 0.25);
    tile.set_cell_state(3, 4, 0.37, 777.0);
    let mut bytes = Vec::new();
    tile.serialize(&mut bytes).unwrap();
    let loaded = TileData::deserialize(&mut bytes.as_slice()).unwrap();
    assert_eq!(loaded.cell_material_identifier(3, 4), material);
    assert_eq!(loaded.cell_appearance(3, 4).0, 17);
    assert_eq!(loaded.cell_integrity(3, 4), 0.25);
    assert_eq!(loaded.cell_amount(3, 4), 0.37);
    assert_eq!(loaded.cell_temperature(3, 4), 777.0);
}

#[test]
fn test_legacy_format_marks_occupied_temperature_unresolved() {
    let material = MaterialIdentifier::new(MaterialForm::CellularStatic, 0);
    let mut tile = TileData::EMPTY;
    tile.set_cell(1, 2, material, CellularAppearance(3));
    let mut bytes = Vec::new();
    tile.serialize_material_identifiers(&mut bytes).unwrap();
    tile.serialize_appearances(&mut bytes).unwrap();
    tile.serialize_integrities(&mut bytes).unwrap();
    let loaded = TileData::deserialize_legacy(&mut bytes.as_slice()).unwrap();
    assert_eq!(loaded.cell_amount(1, 2), 1.0);
    assert!(loaded.cell_temperature(1, 2).is_nan());
    assert_eq!(loaded.cell_amount(0, 0), 0.0);
    assert_eq!(loaded.cell_temperature(0, 0), 0.0);
}
