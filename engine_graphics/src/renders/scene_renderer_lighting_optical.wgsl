// Copyright Rob Gage 2026

// Builds the optical field of the lighting domain: emission per cell of path and extinction per
// cell. The shader source is concatenated after `MATERIAL_RADIANCE_WGSL` and the lighting
// functions it shares with the appearance passes.

/// Optical field construction over the resident region
struct Optics {
    /// Lighting domain size in cells
    domain_size: vec2<u32>,
    /// Region width in tiles
    region_width: u32,
    /// Region height in tiles
    region_height: u32,
    /// Physical ring column holding the region's leftmost tiles
    ring_offset_x: u32,
    /// Physical ring row holding the region's bottom tiles
    ring_offset_y: u32,
    /// Number of cells along each side of a tile
    cellular_tile_size: u32,
    /// Gas amount in one atmosphere of one cell
    amount_per_atmosphere: f32,
    /// Whether fluid buffers are bound
    has_fluid: u32,
    /// Whether gas buffers are bound
    has_gas: u32,
    /// Alignment padding
    padding_0: u32,
    /// Alignment padding
    padding_1: u32,
};

@group(0) @binding(0)
var<uniform> optics: Optics;

@group(0) @binding(1)
var<storage, read> material_identifiers: array<u32>;

/// Packed color then packed radiance of every cell
@group(0) @binding(2)
var<storage, read> colors: array<vec2<u32>>;

/// Temperature bits then amount of every cell
@group(0) @binding(3)
var<storage, read> matter: array<vec2<u32>>;

/// Fixed-point mass, momentum, and material identifier of the fluid in every cell
@group(0) @binding(4)
var<storage, read> fluid_raster: array<i32>;

/// Mass-weighted color then radiance sums of the fluid in every cell
@group(0) @binding(5)
var<storage, read> fluid_colors: array<u32>;

/// Fixed-point heat capacity, energy, conduction, and temperature change of every cell's fluid
@group(0) @binding(6)
var<storage, read> fluid_heat: array<vec4<i32>>;

@group(0) @binding(7)
var<storage, read> fluid_materials: array<vec4<f32>>;

/// Four gas slots then phantom air, flows, and pressure of every cell
@group(0) @binding(8)
var<storage, read> gas_cells: array<vec4<u32>>;

@group(0) @binding(9)
var<storage, read> gas_materials: array<vec4<f32>>;

/// Emission per cell in `rgb` and transmission per cell in `a` of actors and lights
@group(0) @binding(10)
var actor_optics: texture_2d<f32>;

@group(0) @binding(11)
var optical: texture_storage_2d<rgba16float, write>;

/// Fixed-point scale of the fluid raster mass
const FLUID_RASTER_SCALE: f32 = 4096.0;

/// Fixed-point scale of fluid radiance emission sums
const FLUID_EMISSION_SCALE: f32 = 64.0;

/// Fixed-point scale of fluid heat capacity
const FLUID_CAPACITY_SCALE: f32 = 1024.0;

/// Fixed-point scale of fluid heat energy
const FLUID_ENERGY_SCALE: f32 = 64.0;

/// Largest extinction per cell, beyond which a cell is opaque
const EXTINCTION_MAXIMUM: f32 = 30.0;

/// Returns the physical ring index of a domain cell
fn physical_cell_index(cell: vec2<u32>) -> u32 {
    let size: u32 = optics.cellular_tile_size;
    let tile: vec2<u32> = cell / size;
    let in_tile: vec2<u32> = cell % size;
    let physical_x: u32 = (tile.x + optics.ring_offset_x) % optics.region_width;
    let physical_y: u32 = (tile.y + optics.ring_offset_y) % optics.region_height;
    let slot: u32 = physical_y * optics.region_width + physical_x;
    return slot * size * size + in_tile.y * size + in_tile.x;
}

/// Returns the extinction per cell of an occlusion per tile
fn extinction(occlusion: f32) -> f32 {
    return min(-log(max(1.0 - occlusion, 1e-12)) / f32(optics.cellular_tile_size), EXTINCTION_MAXIMUM);
}

/// Builds the emission and extinction of every domain cell
@compute @workgroup_size(8, 8, 1)
fn build(@builtin(global_invocation_id) invocation: vec3<u32>) {
    if any(invocation.xy >= optics.domain_size) {
        return;
    }
    let index: u32 = physical_cell_index(invocation.xy);
    let temperature: f32 = bitcast<f32>(matter[index].x);
    var emission: vec3<f32> = vec3<f32>(0.0);
    var cell_extinction: f32 = 0.0;
    let material_identifier: u32 = material_identifiers[index];
    if material_identifier != 0u {
        let radiance: vec4<f32> = temperature_color(
            thermal_index(material_identifier), temperature, unpack_radiance(colors[index].y), true);
        emission = radiance.rgb;
        cell_extinction = extinction(radiance.a);
    } else {
        if optics.has_fluid != 0u {
            let mass: f32 = f32(max(fluid_raster[index * 4u], 0));
            let fluid_identifier: u32 = bitcast<u32>(fluid_raster[index * 4u + 3u]);
            if mass > 0.0 {
                let density: f32 = fluid_materials[(fluid_identifier & 0x3fffffffu) * 2u].x;
                let occupancy: f32 = clamp(mass / FLUID_RASTER_SCALE / max(density, 1e-6), 0.0, 1.0);
                let base: u32 = index * 8u + 4u;
                var radiance: vec4<f32> = vec4<f32>(
                    f32(fluid_colors[base]) / FLUID_EMISSION_SCALE,
                    f32(fluid_colors[base + 1u]) / FLUID_EMISSION_SCALE,
                    f32(fluid_colors[base + 2u]) / FLUID_EMISSION_SCALE,
                    f32(fluid_colors[base + 3u]) / 255.0,
                ) / mass;
                let heat: vec4<i32> = fluid_heat[index];
                var fluid_temperature: f32 = temperature;
                if heat.x > 0 {
                    fluid_temperature = (f32(heat.y) / FLUID_ENERGY_SCALE)
                        / (f32(heat.x) / FLUID_CAPACITY_SCALE) + bitcast<f32>(heat.w);
                }
                radiance = temperature_color(
                    thermal_index(fluid_identifier), fluid_temperature, radiance, true);
                emission += radiance.rgb * occupancy;
                cell_extinction += extinction(radiance.a) * occupancy;
            }
        }
        if optics.has_gas != 0u {
            let slots: vec4<u32> = gas_cells[index * 2u];
            for (var slot: u32 = 0u; slot < 4u; slot++) {
                let amount: u32 = slots[slot] & 0xffffffu;
                if amount == 0u {
                    continue;
                }
                let gas_index: u32 = slots[slot] >> 24u;
                let atmospheres: f32 = f32(amount) / optics.amount_per_atmosphere;
                let radiance: vec4<f32> = temperature_color(
                    lighting_view.thermal_offsets.w + gas_index,
                    temperature,
                    unpack_radiance(bitcast<u32>(gas_materials[gas_index * 2u + 1u].y)),
                    true,
                );
                emission += radiance.rgb * atmospheres;
                cell_extinction += extinction(radiance.a) * atmospheres;
            }
        }
    }
    let actor: vec4<f32> = textureLoad(actor_optics, invocation.xy, 0);
    emission += actor.rgb;
    cell_extinction += -log(max(actor.a, 1e-12));
    textureStore(optical, invocation.xy, vec4<f32>(emission, min(cell_extinction, EXTINCTION_MAXIMUM)));
}
