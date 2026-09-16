#define_import_path utility::thermal_material

struct ThermalMaterialRecord {
    words: array<u32, 16>,
}

struct ThermalMaterialParameters {
    offsets: vec4<u32>,
    counts: vec4<u32>,
}

fn thermal_material_conductivity(record: ThermalMaterialRecord) -> f32 {
    return bitcast<f32>(record.words[0]);
}

fn thermal_material_specific_heat_capacity(record: ThermalMaterialRecord) -> f32 {
    return bitcast<f32>(record.words[1]);
}
