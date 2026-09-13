// Copyright Rob Gage 2026

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
    _padding: u32,
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

@group(0) @binding(0) var<storage, read> cellular_material_identifiers: array<u32>;
@group(0) @binding(1) var<storage, read> cellular_appearances: array<u32>;

@group(0) @binding(2) var<storage, read> cellular_statics: array<MaterialAppearance>;

@group(0) @binding(3) var<storage, read> cellular_dynamics: array<MaterialAppearance>;

@group(0) @binding(4) var<storage, read> fluids: array<MaterialAppearance>;

@group(0) @binding(5) var<uniform> uniforms: Uniforms;

@group(0) @binding(6) var<storage, read> fluid_material_identifiers: array<u32>;

@group(0) @binding(7) var<storage, read> fluid_coverage: array<f32>;

@group(0) @binding(8) var<storage, read> cellular_pressure: array<vec4<f32>>;

const INVALID_INDEX: u32 = 0xffffffffu;

// A fullscreen triangle delegates all scene lookup to the fragment shader
@vertex
fn vertex(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0), vec2<f32>(3.0, -1.0), vec2<f32>(-1.0, 3.0),
    );
    return vec4<f32>(positions[index], 0.0, 1.0);
}

// convert each pixel to a world cell, then resolve it through the tile ring
@fragment
fn fragment(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    // convert the screen pixel into camera/world coordinates
    let normalized = vec2<f32>(
        (position.x - uniforms.viewport_origin.x) / uniforms.window_size.x,
        1.0 - (position.y - uniforms.viewport_origin.y) / uniforms.window_size.y,
    );
    let world = uniforms.camera_position + (normalized - vec2<f32>(0.5)) * uniforms.camera_size;
    // TEMPORARY: draw the possessed walking pawn over the cellular scene
    if all(abs(world - uniforms.walking_pawn_position) < uniforms.walking_pawn_size * 0.5) {
        return vec4<f32>(1.0);
    }
    // resolve the world cell through the streamed tile ring
    let cell = vec2<i32>(floor(world * 8.0));
    let tile = vec2<i32>(floor_divide(cell.x, 8), floor_divide(cell.y, 8));
    let relative = tile - uniforms.buffered_origin;
    if any(relative < vec2<i32>(0)) { return vec4<f32>(0.0, 0.0, 0.0, 1.0); }
    if relative.x >= i32(uniforms.buffered_tile_size.x) || relative.y >= i32(uniforms.buffered_tile_size.y) {
        return vec4<f32>(0.0, 0.0, 0.0, 1.0);
    }
    let physical = (vec2<u32>(relative) + uniforms.ring_offset) % uniforms.buffered_tile_size;
    let tile_index = physical.y * uniforms.buffered_tile_size.x + physical.x;
    let local = vec2<u32>(cell - tile * 8);
    let cell_index = tile_index * 64u + local.y * 8u + local.x;
    var material_identifier = cellular_material_identifiers[cell_index];
    var coverage: f32 = fluid_coverage[cell_index];
    var fluid_material_identifier: u32 = fluid_material_identifiers[cell_index];
    if material_identifier == 0u {
        var neighbor_count: u32 = 0u;
        var strongest_coverage: f32 = 0.0;
        var strongest_material_identifier: u32 = 0u;
        for (var offset_y: i32 = -1; offset_y <= 1; offset_y++) {
            for (var offset_x: i32 = -1; offset_x <= 1; offset_x++) {
                if offset_x == 0 && offset_y == 0 { continue; }
                let neighbor_index: u32 = physical_cell_index(
                    cell + vec2<i32>(offset_x, offset_y),
                );
                if neighbor_index == INVALID_INDEX || fluid_coverage[neighbor_index] <= 0.35 {
                    continue;
                }
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
    let is_fluid = material_identifier == 0u && coverage > 0.0;
    if is_fluid { material_identifier = fluid_material_identifier; }
    if uniforms.view_mode == 2u {
        let pressure = length(cellular_pressure[cell_index].xy);
        let heat = clamp(log2(1.0 + pressure) * 0.2, 0.0, 1.0);
        return apply_borders(vec4<f32>(heat, heat * heat * 0.55, 1.0 - heat, 1.0), cell, world);
    }
    // Temperature is selectable now but remains neutral until temperature state exists.
    if uniforms.view_mode == 3u {
        return apply_borders(vec4<f32>(0.12, 0.14, 0.18, 1.0), cell, world);
    }
    if material_identifier == 0u {
        return apply_borders(vec4<f32>(0.0, 0.0, 0.0, 1.0), cell, world);
    }
    // select the material form and its base appearance
    let form = material_identifier >> 30u;
    let index = material_identifier & 0x3fffffffu;
    var properties: MaterialAppearance;
    switch form {
        case 1u: { properties = cellular_statics[index]; }
        case 2u: { properties = cellular_dynamics[index]; }
        case 3u: { properties = fluids[index]; }
        default: { return vec4<f32>(0.0, 0.0, 0.0, 1.0); }
    }
    if uniforms.view_mode == 1u {
        let form_color = array<vec3<f32>, 4>(
            vec3<f32>(0.0),
            vec3<f32>(0.35, 0.58, 0.88),
            vec3<f32>(0.90, 0.62, 0.20),
            vec3<f32>(0.22, 0.72, 0.82),
        );
        return apply_borders(vec4<f32>(form_color[form], 1.0), cell, world);
    }
    let color = properties.color_freezing;
    // decode the persistent cell sample and apply material color influence
    let packed_appearance = select(cellular_appearances[cell_index], 0u, is_fluid);
    var sample = vec4<f32>(0.0);
    for (var channel = 0u; channel < 4u; channel++) {
        let byte = (packed_appearance >> (channel * 8u)) & 0xffu;
        let signed_byte = select(i32(byte), i32(byte) - 256, byte >= 128u);
        sample[channel] = max(f32(signed_byte), -127.0) / 127.0;
    }
    let base = vec4<f32>(
        f32(color & 0xffu) / 255.0,
        f32((color >> 8u) & 0xffu) / 255.0,
        f32((color >> 16u) & 0xffu) / 255.0,
        f32(color >> 24u) / 255.0,
    );
    let result = clamp(base * (vec4<f32>(1.0) + sample * properties.color_influence),
        vec4<f32>(0.0), vec4<f32>(1.0));
    return apply_borders(select(result, vec4<f32>(result.rgb * coverage, 1.0), is_fluid), cell, world);
}

fn apply_borders(color: vec4<f32>, cell: vec2<i32>, world: vec2<f32>) -> vec4<f32> {
    let cell_position = world * 8.0;
    let distance_to_edge = min(fract(cell_position.x), fract(cell_position.y));
    let pixel_width = max(fwidth(cell_position.x), fwidth(cell_position.y));
    if uniforms.show_chunk_borders != 0u &&
            (floor_modulo(cell.x, 512) == 0 || floor_modulo(cell.y, 512) == 0) &&
            distance_to_edge < pixel_width * 1.5 {
        return mix(color, vec4<f32>(0.95, 0.45, 0.12, 1.0), 0.85);
    }
    if uniforms.show_tile_borders != 0u &&
            (floor_modulo(cell.x, 8) == 0 || floor_modulo(cell.y, 8) == 0) &&
            distance_to_edge < pixel_width {
        return mix(color, vec4<f32>(0.35, 0.72, 1.0, 1.0), 0.65);
    }
    return color;
}

fn physical_cell_index(cell: vec2<i32>) -> u32 {
    let tile = vec2<i32>(floor_divide(cell.x, 8), floor_divide(cell.y, 8));
    let relative = tile - uniforms.buffered_origin;
    if any(relative < vec2<i32>(0)) || relative.x >= i32(uniforms.buffered_tile_size.x) ||
            relative.y >= i32(uniforms.buffered_tile_size.y) { return INVALID_INDEX; }
    let physical = (vec2<u32>(relative) + uniforms.ring_offset) % uniforms.buffered_tile_size;
    let local = vec2<u32>(cell - tile * 8);
    return (physical.y * uniforms.buffered_tile_size.x + physical.x) * 64u + local.y * 8u + local.x;
}

// signed floor division keeps negative world coordinates in the correct tile
fn floor_divide(value: i32, divisor: i32) -> i32 {
    if value < 0 { return (value - divisor + 1) / divisor; }
    return value / divisor;
}

fn floor_modulo(value: i32, divisor: i32) -> i32 {
    return value - floor_divide(value, divisor) * divisor;
}
