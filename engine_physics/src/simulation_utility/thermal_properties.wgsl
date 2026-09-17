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

fn thermal_material_cold_enabled(record: ThermalMaterialRecord) -> bool {
    return record.words[6] != 0u;
}

fn thermal_material_cold_threshold(record: ThermalMaterialRecord) -> f32 {
    return bitcast<f32>(record.words[2]);
}

fn thermal_material_cold_target(record: ThermalMaterialRecord) -> u32 {
    return record.words[3];
}

fn thermal_material_cold_yield(record: ThermalMaterialRecord) -> f32 {
    return bitcast<f32>(record.words[4]);
}

fn thermal_material_cold_latent_energy(record: ThermalMaterialRecord) -> f32 {
    return bitcast<f32>(record.words[5]);
}

fn thermal_material_hot_enabled(record: ThermalMaterialRecord) -> bool {
    return record.words[12] != 0u;
}

fn thermal_material_hot_threshold(record: ThermalMaterialRecord) -> f32 {
    return bitcast<f32>(record.words[8]);
}

fn thermal_material_hot_target(record: ThermalMaterialRecord) -> u32 {
    return record.words[9];
}

fn thermal_material_hot_yield(record: ThermalMaterialRecord) -> f32 {
    return bitcast<f32>(record.words[10]);
}

fn thermal_material_hot_latent_energy(record: ThermalMaterialRecord) -> f32 {
    return bitcast<f32>(record.words[11]);
}
