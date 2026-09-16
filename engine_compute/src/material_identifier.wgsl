// Copyright Rob Gage 2026

#define_import_path utility::material_identifier

const EMPTY_MATERIAL_IDENTIFIER: u32 = 0u;
const GAS_MATERIAL_FORM: u32 = 0u;
const CELLULAR_STATIC_MATERIAL_FORM: u32 = 1u;
const CELLULAR_DYNAMIC_MATERIAL_FORM: u32 = 2u;
const FLUID_MATERIAL_FORM: u32 = 3u;
const MATERIAL_IDENTIFIER_INDEX_MASK: u32 = 0x3fffffffu;

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

const INVALID_MATERIAL_DENSE_INDEX: u32 = 0xffffffffu;

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
