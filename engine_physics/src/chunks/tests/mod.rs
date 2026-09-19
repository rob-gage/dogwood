// Copyright Rob Gage 2026

use std::io::Cursor;

use super::*;
use crate::materials::MaterialForm;
use crate::materials::MaterialIdentifier;
use crate::tiles::CellularAppearance;
use crate::tiles::TileArea;
use crate::tiles::TileCoordinates;
use crate::tiles::TileData;

fn particle(index: u32) -> ChunkFluidParticle {
    ChunkFluidParticle {
        material_identifier: MaterialIdentifier::new(MaterialForm::Fluid, index),
        position: [index as f32 + 0.25, -(index as f32) - 0.5],
        velocity: [index as f32 + 1.0, -(index as f32) - 2.0],
        amount: index as f32 + 0.5,
        temperature: 273.15 + index as f32,
    }
}

#[test]
fn test_accelerator_record_is_complete_and_round_trips() {
    let source = particle(1);
    let mut bytes = Vec::new();
    source.serialize_gpu(&mut bytes).unwrap();
    assert_eq!(bytes.len(), ChunkFluidParticle::GPU_SIZE);
    let decoded = ChunkFluidParticle::deserialize_gpu(&bytes).unwrap();
    assert_eq!(decoded.material_identifier, source.material_identifier);
    assert_eq!(decoded.position, source.position);
    assert_eq!(decoded.velocity, source.velocity);
    assert_eq!(decoded.amount, source.amount);
    assert_eq!(decoded.temperature, source.temperature);
}

#[test]
fn test_accelerator_records_keep_their_stride() {
    let sources: Vec<_> = (0..3).map(particle).collect();
    let mut bytes = Vec::new();
    for source in &sources {
        source.serialize_gpu(&mut bytes).unwrap();
    }
    assert_eq!(bytes.len(), sources.len() * ChunkFluidParticle::GPU_SIZE);
    for (index, source) in sources.iter().enumerate() {
        let start = index * ChunkFluidParticle::GPU_SIZE;
        let decoded = ChunkFluidParticle::deserialize_gpu(
            &bytes[start..start + ChunkFluidParticle::GPU_SIZE],
        )
        .unwrap();
        assert_eq!(decoded.position, source.position);
        assert_eq!(decoded.temperature, source.temperature);
    }
}

#[test]
fn test_persistent_serialization_keeps_validation_strict() {
    let mut invalid = particle(0);
    invalid.amount = 0.0;
    assert!(invalid.serialize(&mut Cursor::new(Vec::new())).is_err());
    assert!(invalid.serialize_gpu(&mut Vec::new()).is_err());
}

#[test]
fn test_dormant_fluid_round_trips_and_old_chunks_remain_readable() {
    let coordinates: TileCoordinates = TileCoordinates { x: -64, y: -64 };
    let mut chunk: Chunk = Chunk::new_empty(coordinates);
    let tile_coordinates = coordinates;
    let cellular = MaterialIdentifier::new(MaterialForm::CellularStatic, 1);
    let mut tile = TileData::EMPTY;
    tile.set_cell_with_integrity(2, 3, cellular, CellularAppearance(12), 0.4);
    tile.set_cell_state(2, 3, 0.37, 777.0);
    chunk.set_tile_unchecked(tile_coordinates, tile);
    let particle: ChunkFluidParticle = ChunkFluidParticle {
        material_identifier: MaterialIdentifier::new(MaterialForm::Fluid, 7),
        position: [-0.25, -63.5],
        velocity: [1.25, -2.5],
        amount: 0.37,
        temperature: 777.0,
    };
    chunk.insert_dormant_fluid_particle(particle).unwrap();
    let gas_cell: ChunkGasCell = ChunkGasCell {
        coordinates: crate::tiles::CellCoordinates { x: -2, y: -510 },
        velocity: [0.5, 1.25],
        temperature: 293.15,
        species: vec![(MaterialIdentifier::new(MaterialForm::Gas, 0), 0.75)],
    };
    chunk.insert_dormant_gas_cell(gas_cell.clone()).unwrap();
    let mut bytes: Vec<u8> = Vec::new();
    chunk.serialize(&mut bytes).unwrap();
    let mut reader: &[u8] = &bytes;
    let mut loaded: Chunk = Chunk::deserialize(&mut reader).unwrap();
    let loaded_tile = loaded.get_tile_unchecked(tile_coordinates);
    assert_eq!(loaded_tile.cell_amount(2, 3), 0.37);
    assert_eq!(loaded_tile.cell_temperature(2, 3), 777.0);
    let loaded_particles: Vec<ChunkFluidParticle> =
        loaded.take_dormant_fluid_particles(TileArea::new(TileCoordinates { x: -1, y: -64 }, 1, 1));
    assert!(loaded_particles.len() == 1);
    assert!(loaded_particles[0].material_identifier == particle.material_identifier);
    assert!(loaded_particles[0].position == particle.position);
    assert!(loaded_particles[0].velocity == particle.velocity);
    let loaded_gas: Vec<ChunkGasCell> =
        loaded.take_dormant_gas_cells(TileArea::new(TileCoordinates { x: -1, y: -64 }, 1, 1));
    assert!(loaded_gas.len() == 1);
    assert!(loaded_gas[0].coordinates == gas_cell.coordinates);
    assert!(loaded_gas[0].velocity == gas_cell.velocity);
    assert!(loaded_gas[0].species == gas_cell.species);

    let gas_section_offset: usize =
        16 + 4096 * TileData::SERIALIZED_SIZE + 8 + 4 + ChunkFluidParticle::SERIALIZED_SIZE;
    let mut fluid_only_bytes: Vec<u8> = bytes.clone();
    fluid_only_bytes.truncate(gas_section_offset);
    let mut fluid_only_reader: &[u8] = &fluid_only_bytes;
    let mut fluid_only_chunk: Chunk = Chunk::deserialize(&mut fluid_only_reader).unwrap();
    assert!(
        fluid_only_chunk
            .take_dormant_gas_cells(TileArea::new(coordinates, Chunk::WIDTH, Chunk::WIDTH))
            .is_empty()
    );

    bytes.truncate(16 + 4096 * TileData::SERIALIZED_SIZE);
    let mut old_reader: &[u8] = &bytes;
    let mut old_chunk: Chunk = Chunk::deserialize(&mut old_reader).unwrap();
    let full_chunk_area = TileArea::new(coordinates, Chunk::WIDTH, Chunk::WIDTH);
    assert!(
        old_chunk
            .take_dormant_fluid_particles(full_chunk_area)
            .is_empty()
    );
    assert!(old_chunk.take_dormant_gas_cells(full_chunk_area).is_empty());
}

#[test]
fn test_legacy_cellular_tiles_load_with_unresolved_temperature() {
    let coordinates = TileCoordinates { x: 0, y: 0 };
    let material = MaterialIdentifier::new(MaterialForm::CellularStatic, 0);
    let mut tile = TileData::EMPTY;
    tile.set_cell(0, 0, material, CellularAppearance(9));
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"dogwood_");
    bytes.extend_from_slice(&coordinates.x.to_le_bytes());
    bytes.extend_from_slice(&coordinates.y.to_le_bytes());
    tile.serialize_material_identifiers(&mut bytes).unwrap();
    tile.serialize_appearances(&mut bytes).unwrap();
    tile.serialize_integrities(&mut bytes).unwrap();
    for _ in 1..4096 {
        TileData::EMPTY
            .serialize_material_identifiers(&mut bytes)
            .unwrap();
        TileData::EMPTY.serialize_appearances(&mut bytes).unwrap();
        TileData::EMPTY.serialize_integrities(&mut bytes).unwrap();
    }
    let chunk = Chunk::deserialize(&mut bytes.as_slice()).unwrap();
    let loaded = chunk.get_tile_unchecked(coordinates);
    assert_eq!(loaded.cell_amount(0, 0), 1.0);
    assert!(loaded.cell_temperature(0, 0).is_nan());
    assert_eq!(loaded.cell_amount(1, 0), 0.0);
    assert_eq!(loaded.cell_temperature(1, 0), 0.0);
}

#[test]
fn test_occupied_cells_normalize_before_persistence() {
    let coordinates = TileCoordinates { x: 0, y: 0 };
    let material = MaterialIdentifier::new(MaterialForm::CellularStatic, 0);
    let mut chunk = Chunk::new_empty(coordinates);
    let mut tile = TileData::EMPTY;
    tile.set_cell(1, 2, material, CellularAppearance::NEUTRAL);
    chunk.set_tile_unchecked(coordinates, tile);

    chunk.resolve_uninitialized_temperatures(|_| 301.5);
    let mut bytes = Vec::new();
    chunk.serialize(&mut bytes).unwrap();
    let loaded = Chunk::deserialize(&mut bytes.as_slice()).unwrap();
    let tile = loaded.get_tile_unchecked(coordinates);
    assert_eq!(tile.cell_amount(1, 2), 1.0);
    assert_eq!(tile.cell_temperature(1, 2), 301.5);
}

#[test]
fn test_chunk_initialization_writer_uses_absolute_bounds() {
    let coordinates: TileCoordinates = TileCoordinates { x: -64, y: 32 };
    let region: ChunkGenerationRegion = ChunkGenerationRegion::new(coordinates);
    assert_eq!(region.cell_origin.x, -512);
    assert_eq!(region.cell_origin.y, 256);
    assert!(region.contains_cell(crate::tiles::CellCoordinates { x: -1, y: 767 }));
    assert!(!region.contains_cell(crate::tiles::CellCoordinates { x: 0, y: 768 }));

    let material: MaterialIdentifier = MaterialIdentifier::new(MaterialForm::CellularStatic, 0);
    let mut chunk: Chunk = Chunk::new_empty(coordinates);
    let mut writer: ChunkInitializationWriter<'_> = ChunkInitializationWriter::new(&mut chunk);
    writer.fill_cells(
        region.cell_origin,
        8,
        8,
        material,
        CellularAppearance::NEUTRAL,
    );
    assert_eq!(writer.initialized_cell_count(), 64);
    assert_eq!(
        chunk
            .get_tile_unchecked(coordinates)
            .cell_material_identifier(0, 0),
        material
    );
}

#[test]
fn test_absolute_generation_writes_are_order_independent() {
    let coordinates: TileCoordinates = TileCoordinates { x: 0, y: 0 };
    let region: ChunkGenerationRegion = ChunkGenerationRegion::new(coordinates);
    let material: MaterialIdentifier = MaterialIdentifier::new(MaterialForm::CellularStatic, 0);
    let generate = |reverse: bool| {
        let mut chunk: Chunk = Chunk::new_empty(coordinates);
        let mut writer: ChunkInitializationWriter<'_> = ChunkInitializationWriter::new(&mut chunk);
        let mut cells: Vec<crate::tiles::CellCoordinates> = (0..8)
            .flat_map(|y| (0..8).map(move |x| crate::tiles::CellCoordinates { x, y }))
            .collect();
        if reverse {
            cells.reverse();
        }
        for cell in cells {
            writer.set_cell(
                crate::tiles::CellCoordinates {
                    x: cell.x + region.cell_origin.x,
                    y: cell.y + region.cell_origin.y,
                },
                material,
                CellularAppearance::NEUTRAL,
            );
        }
        chunk.resolve_uninitialized_temperatures(|_| 293.15);
        let mut bytes: Vec<u8> = Vec::new();
        chunk.serialize(&mut bytes).unwrap();
        bytes
    };
    assert_eq!(generate(false), generate(true));
}
