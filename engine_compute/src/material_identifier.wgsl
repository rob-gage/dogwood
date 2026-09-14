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
