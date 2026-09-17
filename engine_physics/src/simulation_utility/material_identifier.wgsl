// Copyright Rob Gage 2026

#define_import_path utility::material_identifier
#import utility::simulation_constants::{CELLS_PER_TILE, CELLS_PER_TILE_FLOAT, CELL_COUNT_PER_TILE, EMPTY_MATERIAL_IDENTIFIER, GAS_MATERIAL_FORM, CELLULAR_STATIC_MATERIAL_FORM, CELLULAR_DYNAMIC_MATERIAL_FORM, FLUID_MATERIAL_FORM, MATERIAL_IDENTIFIER_INDEX_MASK, INVALID_MATERIAL_DENSE_INDEX, FLUID_EDIT_ERASE, INVALID_PHYSICAL_CELL_INDEX, INVALID_CELLULAR_DYNAMIC_CLAIM_INDEX, INVALID_FLUID_PARTICLE_INDEX, INVALID_FLUID_BUCKET_INDEX, ACTOR_SHAPE_CIRCLE, ACTOR_SHAPE_CAPSULE, ACTOR_SHAPE_RECTANGLE, PI, PBF_SUBSTEP_COUNT, PBF_CONSTRAINT_ITERATION_COUNT, CONSTRAINT_EPSILON, ARTIFICIAL_PRESSURE_DELTA_Q_RATIO, MAXIMUM_CORRECTION_CELLS, HARD_EXTERNAL_BODY_OCCUPANCY, SWIMMER_EXTERNAL_BODY_OCCUPANCY, RIGID_EXTERNAL_BODY_OCCUPANCY, IMMOVABLE_CONTACT_MASS, CONTACT_PRESSURE_TRANSFER, LINEAR_FIXED_SCALE, ANGULAR_FIXED_SCALE, CELL_SIZE, CELL_HALF, CELL_RADIUS, INCOMPRESSIBILITY_MIXING, RESERVATION_SCALE, RESERVATION_SCALE_U32}

// Decodes the physical form stored in a material identifier
fn material_form_from_identifier(material_identifier: u32) -> u32 {
    return material_identifier >> 30u;
}

// Decodes the form-local registry index stored in a material identifier
fn material_index_from_identifier(material_identifier: u32) -> u32 {
    let encoded_index: u32 = material_identifier & MATERIAL_IDENTIFIER_INDEX_MASK;
    if material_form_from_identifier(material_identifier) == GAS_MATERIAL_FORM {
        return max(encoded_index, 1u) - 1u;
    }
    return encoded_index;
}

// Maps a raw identifier to form-major dense table order. `offsets` contains
// gas, static, dynamic, and fluid offsets respectively; `counts` contains the
// corresponding form counts.
fn material_dense_index(material_identifier: u32, offsets: vec4<u32>, counts: vec4<u32>) -> u32 {
    if (material_identifier == EMPTY_MATERIAL_IDENTIFIER) { return INVALID_MATERIAL_DENSE_INDEX; }
    let form = material_form_from_identifier(material_identifier);
    let index = material_index_from_identifier(material_identifier);
    if (form >= 4u || index >= counts[form]) { return INVALID_MATERIAL_DENSE_INDEX; }
    return offsets[form] + index;
}
