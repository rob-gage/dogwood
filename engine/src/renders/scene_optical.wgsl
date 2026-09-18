// Copyright Rob Gage 2026

#define_import_path graphics::scene_optical

#import utility::simulation_constants::{
    CELL_COUNT_PER_TILE,
    CELLS_PER_TILE_FLOAT,
    CELLULAR_DYNAMIC_MATERIAL_FORM,
    CELLULAR_STATIC_MATERIAL_FORM,
    EMPTY_MATERIAL_IDENTIFIER,
    FLUID_MATERIAL_FORM,
    INVALID_PHYSICAL_CELL_INDEX,
}
#import utility::material_identifier::{
    material_form_from_identifier,
    material_index_from_identifier,
}
#import utility::tile_ring::physical_cell_index_from_world_cell

struct Uniforms {
    camera_position: vec2<f32>,
    window_size: vec2<f32>,
    camera_size: vec2<f32>,
    walking_pawn_position: vec2<f32>,
    buffered_origin: vec2<i32>,
    buffered_tile_size: vec2<u32>,
    ring_offset: vec2<u32>,
    walking_pawn_size: vec2<f32>,
    viewport_origin: vec2<f32>,
    view_mode: u32,
    show_tile_borders: u32,
    show_chunk_borders: u32,
    gas_count: u32,
    _padding: vec2<u32>,
    actor_count: u32,
    overlay_count: u32,
    _padding_end: vec2<u32>,
    lighting_size: vec2<u32>,
    _lighting_padding: vec2<u32>,
}

struct MaterialAppearance {
    color_freezing: u32,
    color_melting: u32,
    radiance_freezing: u32,
    radiance_melting: u32,
    variation: vec4<f32>,
    color_influence: vec4<f32>,
    radiance_influence: vec4<f32>,
    extinction: f32,
    _optics_padding: vec3<f32>,
}

struct SceneCellSample {
    material_identifier: u32,
    fluid_coverage: f32,
    is_fluid: bool,
    appearance: u32,
}

@group(0) @binding(0) var<storage, read> cellular_material_identifiers: array<u32>;
@group(0) @binding(1) var<storage, read> cellular_appearances: array<u32>;
@group(0) @binding(2) var<storage, read> cellular_statics: array<MaterialAppearance>;
@group(0) @binding(3) var<storage, read> cellular_dynamics: array<MaterialAppearance>;
@group(0) @binding(4) var<storage, read> fluids: array<MaterialAppearance>;
@group(0) @binding(5) var<uniform> uniforms: Uniforms;
@group(0) @binding(6) var<storage, read> fluid_material_identifiers: array<u32>;
@group(0) @binding(7) var<storage, read> fluid_coverage: array<f32>;
@group(0) @binding(8) var<storage, read> cellular_pressure: array<vec4<f32>>;
@group(0) @binding(9) var<storage, read> gases: array<MaterialAppearance>;
@group(0) @binding(10) var<storage, read> gas_properties: array<vec4<f32>>;
@group(0) @binding(11) var<storage, read> gas_concentrations: array<f32>;
@group(0) @binding(12) var<storage, read> rigid_material_identifiers: array<u32>;
@group(0) @binding(13) var<storage, read> rigid_appearances: array<u32>;

@vertex
fn vertex(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0), vec2<f32>(3.0, -1.0), vec2<f32>(-1.0, 3.0),
    );
    return vec4<f32>(positions[index], 0.0, 1.0);
}

@fragment
fn fragment(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let normalized: vec2<f32> = vec2<f32>(
        (position.x - uniforms.viewport_origin.x) / uniforms.window_size.x,
        1.0 - (position.y - uniforms.viewport_origin.y) / uniforms.window_size.y,
    );
    let world: vec2<f32> = uniforms.camera_position +
        (normalized - vec2<f32>(0.5)) * uniforms.camera_size;
    let cell: vec2<i32> = vec2<i32>(floor(world * CELLS_PER_TILE_FLOAT));
    let index: u32 = physical_cell_index_from_world_cell(
        cell, uniforms.buffered_origin, uniforms.buffered_tile_size, uniforms.ring_offset,
    );
    if index == INVALID_PHYSICAL_CELL_INDEX {
        return vec4<f32>(0.0);
    }
    let sample: SceneCellSample = resolve_scene_cell(cell, index);
    if sample.material_identifier != EMPTY_MATERIAL_IDENTIFIER {
        let form: u32 = material_form_from_identifier(sample.material_identifier);
        let material_index: u32 = material_index_from_identifier(sample.material_identifier);
        var properties: MaterialAppearance;
        switch form {
            case CELLULAR_STATIC_MATERIAL_FORM: { properties = cellular_statics[material_index]; }
            case CELLULAR_DYNAMIC_MATERIAL_FORM: { properties = cellular_dynamics[material_index]; }
            case FLUID_MATERIAL_FORM: { properties = fluids[material_index]; }
            default: { return vec4<f32>(0.0); }
        }
        let variation: vec4<f32> = unpack_sample(select(sample.appearance, 0u, sample.is_fluid));
        let coverage: f32 = select(1.0, sample.fluid_coverage, sample.is_fluid);
        let emission: vec3<f32> = unpack_color(properties.radiance_freezing).rgb *
            (vec3<f32>(1.0) + variation.rgb * properties.radiance_influence.rgb) * coverage;
        return vec4<f32>(max(emission, vec3<f32>(0.0)), max(properties.extinction * coverage, 0.0));
    }
    return gas_optics(cell);
}

fn resolve_scene_cell(cell: vec2<i32>, index: u32) -> SceneCellSample {
    var material: u32 = cellular_material_identifiers[index];
    var appearance: u32 = cellular_appearances[index];
    if rigid_material_identifiers[index] != EMPTY_MATERIAL_IDENTIFIER {
        material = rigid_material_identifiers[index];
        appearance = rigid_appearances[index];
    }
    var coverage: f32 = fluid_coverage[index];
    var fluid_material: u32 = fluid_material_identifiers[index];
    if material == EMPTY_MATERIAL_IDENTIFIER {
        var neighbors: u32 = 0u;
        var strongest: f32 = 0.0;
        var strongest_material: u32 = EMPTY_MATERIAL_IDENTIFIER;
        for (var y: i32 = -1; y <= 1; y++) {
            for (var x: i32 = -1; x <= 1; x++) {
                if x == 0 && y == 0 { continue; }
                let neighbor: u32 = scene_index(cell + vec2<i32>(x, y));
                if neighbor == INVALID_PHYSICAL_CELL_INDEX || fluid_coverage[neighbor] <= 0.35 {
                    continue;
                }
                neighbors += 1u;
                if fluid_coverage[neighbor] > strongest {
                    strongest = fluid_coverage[neighbor];
                    strongest_material = fluid_material_identifiers[neighbor];
                }
            }
        }
        if neighbors >= 4u {
            coverage = 1.0;
            fluid_material = strongest_material;
        } else if coverage == 0.0 && neighbors >= 2u {
            coverage = strongest * 0.2;
            fluid_material = strongest_material;
        }
    }
    let is_fluid: bool = material == EMPTY_MATERIAL_IDENTIFIER && coverage > 0.0;
    if is_fluid { material = fluid_material; }
    return SceneCellSample(material, coverage, is_fluid, appearance);
}

fn gas_optics(cell: vec2<i32>) -> vec4<f32> {
    var emission: vec3<f32> = vec3<f32>(0.0);
    var extinction: f32 = 0.0;
    for (var species: u32 = 0u; species < uniforms.gas_count; species++) {
        let concentration: f32 = sample_gas(species, vec2<f32>(cell) + vec2<f32>(0.5));
        let properties: MaterialAppearance = gases[species];
        extinction += concentration * max(properties.extinction, 0.0);
        emission += unpack_color(properties.radiance_freezing).rgb * concentration;
    }
    return vec4<f32>(emission, extinction);
}

fn sample_gas(species: u32, position: vec2<f32>) -> f32 {
    let base: vec2<i32> = vec2<i32>(floor(position - vec2<f32>(0.5)));
    let fraction: vec2<f32> = fract(position - vec2<f32>(0.5));
    let bottom: f32 = mix(gas_at(species, base), gas_at(species, base + vec2<i32>(1, 0)), fraction.x);
    let top: f32 = mix(gas_at(species, base + vec2<i32>(0, 1)), gas_at(species, base + vec2<i32>(1)), fraction.x);
    return mix(bottom, top, fraction.y);
}

fn gas_at(species: u32, cell: vec2<i32>) -> f32 {
    let index: u32 = scene_index(cell);
    if index == INVALID_PHYSICAL_CELL_INDEX { return 0.0; }
    let count: u32 = uniforms.buffered_tile_size.x * uniforms.buffered_tile_size.y * CELL_COUNT_PER_TILE;
    return gas_concentrations[species * count + index];
}

fn scene_index(cell: vec2<i32>) -> u32 {
    return physical_cell_index_from_world_cell(
        cell, uniforms.buffered_origin, uniforms.buffered_tile_size, uniforms.ring_offset,
    );
}

fn unpack_color(color: u32) -> vec4<f32> {
    return vec4<f32>(
        f32(color & 0xffu) / 255.0,
        f32((color >> 8u) & 0xffu) / 255.0,
        f32((color >> 16u) & 0xffu) / 255.0,
        f32(color >> 24u) / 255.0,
    );
}

fn unpack_sample(packed: u32) -> vec4<f32> {
    var sample: vec4<f32> = vec4<f32>(0.0);
    for (var channel: u32 = 0u; channel < 4u; channel++) {
        let byte: u32 = (packed >> (channel * 8u)) & 0xffu;
        let signed_byte: i32 = select(i32(byte), i32(byte) - 256, byte >= 128u);
        sample[channel] = max(f32(signed_byte), -127.0) / 127.0;
    }
    return sample;
}
