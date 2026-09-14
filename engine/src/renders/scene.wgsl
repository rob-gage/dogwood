// Copyright Rob Gage 2026

#define_import_path graphics::scene

#import utility::cell_coordinates::{
    CELL_COUNT_PER_TILE,
    CELLS_PER_TILE,
    CELLS_PER_TILE_FLOAT,
    floor_modulo_signed_coordinate,
}
#import utility::material_identifier::{
    CELLULAR_DYNAMIC_MATERIAL_FORM,
    CELLULAR_STATIC_MATERIAL_FORM,
    EMPTY_MATERIAL_IDENTIFIER,
    FLUID_MATERIAL_FORM,
    material_form_from_identifier,
    material_index_from_identifier,
}
#import utility::tile_ring::{INVALID_PHYSICAL_CELL_INDEX, physical_cell_index_from_world_cell}

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
}

struct MaterialAppearance {
    color_freezing: u32,
    color_melting: u32,
    radiance_freezing: u32,
    radiance_melting: u32,
    variation: vec4<f32>,
    color_influence: vec4<f32>,
    radiance_influence: vec4<f32>,
}

struct SceneCellSample {
    material_identifier: u32,
    fluid_coverage: f32,
    is_fluid: bool,
    appearance: u32,
}

const CELLS_PER_CHUNK_EDGE: i32 = 512;

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

// A fullscreen triangle delegates all scene lookup to the fragment shader
@vertex
fn render_scene_fullscreen_triangle_vertex(
    @builtin(vertex_index) index: u32,
) -> @builtin(position) vec4<f32> {
    let positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0), vec2<f32>(3.0, -1.0), vec2<f32>(-1.0, 3.0),
    );
    return vec4<f32>(positions[index], 0.0, 1.0);
}

// Resolves one scene fragment through camera, tile-ring, material, and gas state
@fragment
fn render_scene_fragment(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let world: vec2<f32> = scene_world_position_from_fragment_position(position.xy);
    // TEMPORARY: draw the possessed walking pawn over the cellular scene
    if all(abs(world - uniforms.walking_pawn_position) < uniforms.walking_pawn_size * 0.5) {
        return vec4<f32>(1.0);
    }
    let cell: vec2<i32> = vec2<i32>(floor(world * CELLS_PER_TILE_FLOAT));
    let cell_index: u32 = physical_cell_index_from_world_cell(
        cell, uniforms.buffered_origin, uniforms.buffered_tile_size, uniforms.ring_offset,
    );
    if cell_index == INVALID_PHYSICAL_CELL_INDEX {
        return vec4<f32>(0.0, 0.0, 0.0, 1.0);
    }
    let scene_cell: SceneCellSample = resolve_scene_cellular_or_fluid_sample(cell, cell_index);
    if uniforms.view_mode >= 1u && uniforms.view_mode <= 4u {
        return render_scene_debug_view(scene_cell, cell, cell_index, world);
    }
    if scene_cell.material_identifier != EMPTY_MATERIAL_IDENTIFIER {
        let material_form: u32 = material_form_from_identifier(
            scene_cell.material_identifier,
        );
        if material_form != CELLULAR_STATIC_MATERIAL_FORM &&
                material_form != CELLULAR_DYNAMIC_MATERIAL_FORM &&
                material_form != FLUID_MATERIAL_FORM {
            return vec4<f32>(0.0, 0.0, 0.0, 1.0);
        }
    }
    let result: vec4<f32> = render_scene_material_color(scene_cell, cell_index);
    let gas: vec4<f32> = render_gas_scattering_at_cell_position(
        world * CELLS_PER_TILE_FLOAT,
    );
    return apply_scene_grid_borders(vec4<f32>(mix(result.rgb, gas.rgb, gas.a), 1.0), cell, world);
}

// Converts a viewport fragment position into continuous scene-world coordinates
fn scene_world_position_from_fragment_position(position: vec2<f32>) -> vec2<f32> {
    let normalized: vec2<f32> = vec2<f32>(
        (position.x - uniforms.viewport_origin.x) / uniforms.window_size.x,
        1.0 - (position.y - uniforms.viewport_origin.y) / uniforms.window_size.y,
    );
    return uniforms.camera_position +
        (normalized - vec2<f32>(0.5)) * uniforms.camera_size;
}

// Resolves cellular material or the display-smoothed derived fluid sample
fn resolve_scene_cellular_or_fluid_sample(
    cell: vec2<i32>,
    cell_index: u32,
) -> SceneCellSample {
    var material_identifier: u32 = cellular_material_identifiers[cell_index];
    var appearance: u32 = cellular_appearances[cell_index];
    if rigid_material_identifiers[cell_index] != EMPTY_MATERIAL_IDENTIFIER {
        material_identifier = rigid_material_identifiers[cell_index];
        appearance = rigid_appearances[cell_index];
    }
    var coverage: f32 = fluid_coverage[cell_index];
    var fluid_material_identifier: u32 = fluid_material_identifiers[cell_index];
    if material_identifier == EMPTY_MATERIAL_IDENTIFIER {
        var neighbor_count: u32 = 0u;
        var strongest_coverage: f32 = 0.0;
        var strongest_material_identifier: u32 = EMPTY_MATERIAL_IDENTIFIER;
        for (var offset_y: i32 = -1; offset_y <= 1; offset_y++) {
            for (var offset_x: i32 = -1; offset_x <= 1; offset_x++) {
                if offset_x == 0 && offset_y == 0 { continue; }
                let neighbor_index: u32 = scene_physical_cell_index_from_world_cell(
                    cell + vec2<i32>(offset_x, offset_y),
                );
                if neighbor_index == INVALID_PHYSICAL_CELL_INDEX ||
                        fluid_coverage[neighbor_index] <= 0.35 { continue; }
                neighbor_count += 1u;
                if fluid_coverage[neighbor_index] > strongest_coverage {
                    strongest_coverage = fluid_coverage[neighbor_index];
                    strongest_material_identifier = fluid_material_identifiers[neighbor_index];
                }
            }
        }
        if neighbor_count >= 4u {
            coverage = 1.0;
            fluid_material_identifier = strongest_material_identifier;
        } else if coverage == 0.0 && neighbor_count >= 2u {
            coverage = strongest_coverage * 0.2;
            fluid_material_identifier = strongest_material_identifier;
        }
    }
    let is_fluid: bool = material_identifier == EMPTY_MATERIAL_IDENTIFIER && coverage > 0.0;
    if is_fluid { material_identifier = fluid_material_identifier; }
    return SceneCellSample(material_identifier, coverage, is_fluid, appearance);
}

// Renders the selected material-form, pressure, temperature, or gas diagnostic
fn render_scene_debug_view(
    scene_cell: SceneCellSample,
    cell: vec2<i32>,
    cell_index: u32,
    world: vec2<f32>,
) -> vec4<f32> {
    if uniforms.view_mode == 2u {
        let pressure: f32 = length(cellular_pressure[cell_index].xy);
        let heat: f32 = clamp(log2(1.0 + pressure) * 0.2, 0.0, 1.0);
        return apply_scene_grid_borders(
            vec4<f32>(heat, heat * heat * 0.55, 1.0 - heat, 1.0), cell, world,
        );
    }
    // Temperature remains neutral until temperature state exists
    if uniforms.view_mode == 3u {
        return apply_scene_grid_borders(
            vec4<f32>(0.12, 0.14, 0.18, 1.0), cell, world,
        );
    }
    let total_gas: f32 = total_gas_concentration_at_cell_position(
        world * CELLS_PER_TILE_FLOAT,
    );
    if uniforms.view_mode == 4u {
        let intensity: f32 = 1.0 - exp(-total_gas * 2.0);
        return apply_scene_grid_borders(vec4<f32>(
            intensity, intensity * intensity * 0.35, 1.0 - intensity * 0.65, 1.0,
        ), cell, world);
    }
    let form_color = array<vec3<f32>, 4>(
        vec3<f32>(0.72, 0.32, 0.88),
        vec3<f32>(0.35, 0.58, 0.88),
        vec3<f32>(0.90, 0.62, 0.20),
        vec3<f32>(0.22, 0.72, 0.82),
    );
    if scene_cell.material_identifier == EMPTY_MATERIAL_IDENTIFIER && total_gas > 0.0001 {
        return apply_scene_grid_borders(vec4<f32>(form_color[0], 1.0), cell, world);
    }
    if scene_cell.material_identifier == EMPTY_MATERIAL_IDENTIFIER {
        return apply_scene_grid_borders(vec4<f32>(0.0, 0.0, 0.0, 1.0), cell, world);
    }
    let material_form: u32 = material_form_from_identifier(scene_cell.material_identifier);
    return apply_scene_grid_borders(vec4<f32>(form_color[material_form], 1.0), cell, world);
}

// Resolves one material identifier and appearance sample into display color
fn render_scene_material_color(scene_cell: SceneCellSample, cell_index: u32) -> vec4<f32> {
    if scene_cell.material_identifier == EMPTY_MATERIAL_IDENTIFIER {
        return vec4<f32>(0.0, 0.0, 0.0, 1.0);
    }
    let material_form: u32 = material_form_from_identifier(scene_cell.material_identifier);
    let material_index: u32 = material_index_from_identifier(scene_cell.material_identifier);
    var properties: MaterialAppearance;
    switch material_form {
        case CELLULAR_STATIC_MATERIAL_FORM: { properties = cellular_statics[material_index]; }
        case CELLULAR_DYNAMIC_MATERIAL_FORM: { properties = cellular_dynamics[material_index]; }
        case FLUID_MATERIAL_FORM: { properties = fluids[material_index]; }
        default: { return vec4<f32>(0.0, 0.0, 0.0, 1.0); }
    }
    let packed_appearance: u32 = select(scene_cell.appearance, 0u, scene_cell.is_fluid);
    var appearance_sample: vec4<f32> = vec4<f32>(0.0);
    for (var channel: u32 = 0u; channel < 4u; channel++) {
        let byte: u32 = (packed_appearance >> (channel * 8u)) & 0xffu;
        let signed_byte: i32 = select(i32(byte), i32(byte) - 256, byte >= 128u);
        appearance_sample[channel] = max(f32(signed_byte), -127.0) / 127.0;
    }
    let base_color: vec4<f32> = unpack_rgba8_color(properties.color_freezing);
    var result: vec4<f32> = clamp(
        base_color * (vec4<f32>(1.0) + appearance_sample * properties.color_influence),
        vec4<f32>(0.0), vec4<f32>(1.0),
    );
    if scene_cell.is_fluid {
        result = vec4<f32>(result.rgb * scene_cell.fluid_coverage, 1.0);
    }
    return result;
}

// Sums bilinearly sampled concentration across every registered gas species
fn total_gas_concentration_at_cell_position(cell_position: vec2<f32>) -> f32 {
    var total: f32 = 0.0;
    for (var species: u32 = 0u; species < uniforms.gas_count; species++) {
        total += sample_gas_concentration_bilinear_at_cell_position(species, cell_position);
    }
    return total;
}

// Calculates gas color and opacity from concentration-weighted extinction
fn render_gas_scattering_at_cell_position(cell_position: vec2<f32>) -> vec4<f32> {
    var optical_depth: f32 = 0.0;
    var weighted_color: vec3<f32> = vec3<f32>(0.0);
    for (var species: u32 = 0u; species < uniforms.gas_count; species++) {
        let concentration: f32 = sample_gas_concentration_bilinear_at_cell_position(
            species, cell_position,
        );
        let extinction: f32 = max(gas_properties[species * 2u].z, 0.0);
        let weight: f32 = concentration * extinction;
        optical_depth += weight;
        weighted_color += unpack_rgba8_color(gases[species].color_freezing).rgb * weight;
    }
    let color: vec3<f32> = select(
        vec3<f32>(0.0), weighted_color / max(optical_depth, 0.000001), optical_depth > 0.0,
    );
    return vec4<f32>(color, 1.0 - exp(-optical_depth));
}

// Bilinearly samples one gas species at a continuous cell-space position
fn sample_gas_concentration_bilinear_at_cell_position(
    species: u32,
    cell_position: vec2<f32>,
) -> f32 {
    let shifted: vec2<f32> = cell_position - vec2<f32>(0.5);
    let base: vec2<i32> = vec2<i32>(floor(shifted));
    let fraction: vec2<f32> = fract(shifted);
    let bottom: f32 = mix(
        gas_concentration_at_world_cell(species, base),
        gas_concentration_at_world_cell(species, base + vec2<i32>(1, 0)), fraction.x,
    );
    let top: f32 = mix(
        gas_concentration_at_world_cell(species, base + vec2<i32>(0, 1)),
        gas_concentration_at_world_cell(species, base + vec2<i32>(1, 1)), fraction.x,
    );
    return mix(bottom, top, fraction.y);
}

// Reads one gas species at a resident world cell with an empty boundary
fn gas_concentration_at_world_cell(species: u32, cell: vec2<i32>) -> f32 {
    let index: u32 = scene_physical_cell_index_from_world_cell(cell);
    if index == INVALID_PHYSICAL_CELL_INDEX { return 0.0; }
    let buffered_cell_count: u32 = uniforms.buffered_tile_size.x *
        uniforms.buffered_tile_size.y * CELL_COUNT_PER_TILE;
    return gas_concentrations[species * buffered_cell_count + index];
}

// Unpacks little-endian RGBA8 channels into normalized color
fn unpack_rgba8_color(color: u32) -> vec4<f32> {
    return vec4<f32>(
        f32(color & 0xffu) / 255.0,
        f32((color >> 8u) & 0xffu) / 255.0,
        f32((color >> 16u) & 0xffu) / 255.0,
        f32(color >> 24u) / 255.0,
    );
}

// Overlays optional tile and chunk boundaries on a scene color
fn apply_scene_grid_borders(color: vec4<f32>, cell: vec2<i32>, world: vec2<f32>) -> vec4<f32> {
    let cell_position = world * CELLS_PER_TILE_FLOAT;
    let distance_to_edge = min(fract(cell_position.x), fract(cell_position.y));
    let pixel_width = max(fwidth(cell_position.x), fwidth(cell_position.y));
    if uniforms.show_chunk_borders != 0u &&
            (floor_modulo_signed_coordinate(cell.x, CELLS_PER_CHUNK_EDGE) == 0 ||
                floor_modulo_signed_coordinate(cell.y, CELLS_PER_CHUNK_EDGE) == 0) &&
            distance_to_edge < pixel_width * 1.5 {
        return mix(color, vec4<f32>(0.95, 0.45, 0.12, 1.0), 0.85);
    }
    if uniforms.show_tile_borders != 0u &&
            (floor_modulo_signed_coordinate(cell.x, i32(CELLS_PER_TILE)) == 0 ||
                floor_modulo_signed_coordinate(cell.y, i32(CELLS_PER_TILE)) == 0) &&
            distance_to_edge < pixel_width {
        return mix(color, vec4<f32>(0.35, 0.72, 1.0, 1.0), 0.65);
    }
    return color;
}

// Maps a world cell through the scene renderer's current tile ring
fn scene_physical_cell_index_from_world_cell(world_cell: vec2<i32>) -> u32 {
    return physical_cell_index_from_world_cell(
        world_cell, uniforms.buffered_origin, uniforms.buffered_tile_size, uniforms.ring_offset,
    );
}
