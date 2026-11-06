// Copyright Rob Gage 2026

/// Region layout, material layout, and sensing limits of one sensing step
struct ActorSensingUniforms {
    /// Active region width in tiles
    region_width: u32,
    /// Active region height in tiles
    region_height: u32,
    /// Horizontal ring offset of the active region in tiles
    ring_offset_x: u32,
    /// Vertical ring offset of the active region in tiles
    ring_offset_y: u32,
    /// Number of actor records sensed
    actor_count: u32,
    /// Number of configured tags
    tag_count: u32,
    /// Cellular static material indices in the tag masks
    static_count: u32,
    /// Cellular dynamic material indices in the tag masks
    dynamic_count: u32,
    /// Fluid material indices in the tag masks
    fluid_count: u32,
    /// Gas material indices in the tag masks
    gas_count: u32,
    /// Fixed-point units per mass unit in the fluid raster
    raster_scale: f32,
    /// Distance outside a collider, in cells, within which a cell touches it
    contact_margin: f32,
    /// Distance outside a collider, in cells, of the breathing band
    breathing_margin: f32,
    /// Largest breathable sum of species gas amounts in one cell
    breathable_gas_amount: f32,
    /// Fluid coverage of a cell, as raster mass over rest density, at which it touches an actor
    contact_coverage: f32,
    /// Fluid coverage of a cell and its eight neighbours at which it can no longer be breathed
    drowning_coverage: f32,
    /// Minimum species amount for a gas tag to count
    gas_tag_amount: f32,
    /// Alignment padding
    padding: u32,
    padding_2: u32,
    padding_3: u32,
};

/// Analytic cellular tile actor record shared with dynamic-cell collision; sensing reads only the
/// collider, but declares every field so records keep the shared layout
struct ActorSensingActor {
    /// Collider center relative to the active region, in cells
    center: vec2<f32>,
    /// Linear velocity in cells per tick
    velocity: vec2<f32>,
    /// Cosine and sine of collider rotation
    rotation: vec2<f32>,
    /// Circle radius, rectangle half-size, or capsule half-segment and radius
    size: vec2<f32>,
    /// Conservative center bounds used by other passes
    bounds: vec4<f32>,
    /// Circle, rectangle, or capsule shape tag
    shape: u32,
    /// Actor mass in cellular mass units
    mass: f32,
    /// Swimming flag and alignment padding
    swimming: u32,
    padding: u32,
};

/// Words of one actor's result record
const RESULT_WORD_COUNT: u32 = 16u;
/// Result word counting breathable cells of the breathing band
const BREATHABLE_WORD: u32 = 8u;
/// Result word counting every cell of the breathing band
const BREATHING_WORD: u32 = 9u;

@group(0) @binding(0)
var<uniform> uniforms: ActorSensingUniforms;

@group(0) @binding(1)
var<storage, read> actors: array<ActorSensingActor>;

@group(0) @binding(2)
var<storage, read> material_identifiers: array<u32>;

@group(0) @binding(3)
var<storage, read> rigid_body_cells: array<u32>;

/// Fluid raster: fixed-point mass, momentum x and y, and selected material per cell
@group(0) @binding(4)
var<storage, read> fluid_raster: array<u32>;

/// Fluid material properties, two records per fluid index with rest density in the first x
@group(0) @binding(5)
var<storage, read> fluid_materials: array<vec4<f32>>;

/// Gas species slots and the remaining gas words, two records per cell
@group(0) @binding(6)
var<storage, read> gas: array<vec4<u32>>;

/// Tag masks of cellular static, cellular dynamic, fluid, then gas material indices
@group(0) @binding(7)
var<storage, read> tag_masks: array<u32>;

@group(0) @binding(8)
var<storage, read> matter: array<vec2<u32>>;

@group(0) @binding(9)
var<storage, read> pressures: array<f32>;

@group(0) @binding(10)
var<storage, read_write> results: array<atomic<u32>>;

/// Returns the physical index of a logical tile inside the toroidal region
fn tile_index(tile: vec2<u32>) -> u32 {
    let physical_x: u32 = (tile.x + uniforms.ring_offset_x) % uniforms.region_width;
    let physical_y: u32 = (tile.y + uniforms.ring_offset_y) % uniforms.region_height;
    return physical_y * uniforms.region_width + physical_x;
}

/// Returns the physical cell-buffer index of a logical cell inside the region
fn cell_index(cell: vec2<i32>) -> u32 {
    let position: vec2<u32> = vec2<u32>(cell);
    return tile_index(position / 8u) * 64u + (position.y % 8u) * 8u + position.x % 8u;
}

/// Returns whether a logical cell is inside the active region
fn cell_inside(cell: vec2<i32>) -> bool {
    return cell.x >= 0 && cell.y >= 0
        && cell.x < i32(uniforms.region_width * 8u)
        && cell.y < i32(uniforms.region_height * 8u);
}

/// Returns the signed distance in cells from an actor's collider to a point, negative inside
fn actor_distance(position: vec2<f32>, actor: ActorSensingActor) -> f32 {
    let difference: vec2<f32> = position - actor.center;
    let local: vec2<f32> = vec2<f32>(
        difference.x * actor.rotation.x + difference.y * actor.rotation.y,
        -difference.x * actor.rotation.y + difference.y * actor.rotation.x,
    );
    if actor.shape == 0u { return length(local) - actor.size.x; }
    if actor.shape == 1u {
        let excess: vec2<f32> = abs(local) - actor.size;
        return length(max(excess, vec2<f32>(0.0))) + min(max(excess.x, excess.y), 0.0);
    }
    let closest: vec2<f32> = vec2<f32>(0.0, clamp(local.y, -actor.size.x, actor.size.x));
    return length(local - closest) - actor.size.y;
}

/// Returns the farthest extent of an actor's collider from its center, in cells
fn actor_extent(actor: ActorSensingActor) -> f32 {
    if actor.shape == 0u { return actor.size.x; }
    if actor.shape == 1u { return length(actor.size); }
    return actor.size.x + actor.size.y;
}

/// Returns the configured mask of a cellular static, dynamic, fluid, or gas material
fn material_tag_mask(identifier: u32) -> u32 {
    let form: u32 = identifier >> 30u;
    let index: u32 = identifier & 0x3fffffffu;
    var offset: u32 = 0u;
    var count: u32 = uniforms.static_count;
    if form == 1u {
        offset = uniforms.static_count;
        count = uniforms.dynamic_count;
    } else if form == 2u {
        offset = uniforms.static_count + uniforms.dynamic_count;
        count = uniforms.fluid_count;
    } else if form == 3u {
        offset = uniforms.static_count + uniforms.dynamic_count + uniforms.fluid_count;
        count = uniforms.gas_count;
    } else {
        return 0u;
    }
    if index >= count { return 0u; }
    return tag_masks[offset + index];
}

/// Returns a cell's fluid coverage, its raster mass over the rest density of its selected fluid;
/// every particle rasterizes its fluid's rest density into one cell, so coverage counts particles
fn fluid_coverage(index: u32) -> f32 {
    let raster_base: u32 = index * 4u;
    let mass: f32 =
        max(f32(bitcast<i32>(fluid_raster[raster_base])) / uniforms.raster_scale, 0.0);
    let fluid_index: u32 = fluid_raster[raster_base + 3u] & 0x3fffffffu;
    if mass <= 0.0 || fluid_index >= uniforms.fluid_count { return 0.0; }
    return mass / max(fluid_materials[fluid_index * 2u].x, 0.000001);
}

/// Counts the tagged cells touching each actor and the breathable cells of its breathing band,
/// one workgroup per actor striding over the cells around its collider
@compute @workgroup_size(64)
fn sense(
    @builtin(workgroup_id) workgroup: vec3<u32>,
    @builtin(local_invocation_index) local_index: u32,
) {
    if workgroup.x >= uniforms.actor_count { return; }
    let actor: ActorSensingActor = actors[workgroup.x];
    let reach: f32 = actor_extent(actor)
        + max(uniforms.contact_margin, uniforms.breathing_margin) + 1.0;
    let minimum: vec2<i32> = vec2<i32>(floor(actor.center - vec2<f32>(reach)));
    let maximum: vec2<i32> = vec2<i32>(ceil(actor.center + vec2<f32>(reach)));
    let span: vec2<i32> = maximum - minimum;
    let cell_count: u32 = u32(max(span.x, 0) * max(span.y, 0));
    let result_base: u32 = workgroup.x * RESULT_WORD_COUNT;
    for (var offset: u32 = local_index; offset < cell_count; offset += 64u) {
        let cell: vec2<i32> = minimum + vec2<i32>(
            i32(offset % u32(span.x)),
            i32(offset / u32(span.x)),
        );
        if !cell_inside(cell) { continue; }
        let distance: f32 = actor_distance(vec2<f32>(cell) + 0.5, actor);
        let is_touching: bool = distance <= uniforms.contact_margin;
        let is_breathing: bool = distance > 0.0 && distance <= uniforms.breathing_margin;
        if !is_touching && !is_breathing { continue; }
        let index: u32 = cell_index(cell);
        let material: u32 = material_identifiers[index];
        let is_rigid: bool = rigid_body_cells[index] != 0u;
        let coverage: f32 = fluid_coverage(index);
        let fluid_index: u32 = fluid_raster[index * 4u + 3u] & 0x3fffffffu;
        let is_matter: bool = material != 0u && (material >> 30u) != 3u;
        let slots: vec4<u32> = gas[index * 2u];
        if (is_touching || is_breathing) {
            // the bits of finite non-negative floats order like their values, so an atomic maximum
            // over the bits picks the largest value
            let temperature_bits: u32 = matter[index].x;
            if temperature_bits < 0x7f800000u {
                atomicMax(&results[result_base + 10u], temperature_bits);
            }
            let pressure_bits: u32 = bitcast<u32>(abs(pressures[index]));
            if pressure_bits < 0x7f800000u {
                atomicMax(&results[result_base + 11u], pressure_bits);
            }
        }
        if is_touching {
            var mask: u32 = 0u;
            if !is_rigid && is_matter {
                mask = material_tag_mask(material);
            } else if !is_rigid && coverage >= uniforms.contact_coverage {
                mask = material_tag_mask((2u << 30u) | fluid_index);
            }
            for (var slot: u32 = 0u; slot < 4u; slot++) {
                let species: u32 = slots[slot];
                let amount: u32 = species & 0xffffffu;
                let gas_index: u32 = species >> 24u;
                if amount > 0u && f32(amount) >= uniforms.gas_tag_amount
                    && gas_index < uniforms.gas_count
                {
                    mask |= material_tag_mask((3u << 30u) | gas_index);
                }
            }
            for (var tag: u32 = 0u; tag < uniforms.tag_count; tag++) {
                if ((mask >> tag) & 1u) != 0u { atomicAdd(&results[result_base + tag], 1u); }
            }
        }
        if is_breathing {
            atomicAdd(&results[result_base + BREATHING_WORD], 1u);
            let species_amount: f32 = f32(slots.x & 0xffffffu) + f32(slots.y & 0xffffffu)
                + f32(slots.z & 0xffffffu) + f32(slots.w & 0xffffffu);
            // resting fluid leaves some cells between its particles empty, so a cell is wet
            // when fluid fills any cell around it
            var neighbourhood_coverage: f32 = 0.0;
            for (var y: i32 = -1; y <= 1; y++) {
                for (var x: i32 = -1; x <= 1; x++) {
                    let neighbour: vec2<i32> = cell + vec2<i32>(x, y);
                    if cell_inside(neighbour) {
                        neighbourhood_coverage += fluid_coverage(cell_index(neighbour));
                    }
                }
            }
            if !is_matter && !is_rigid
                && neighbourhood_coverage < uniforms.drowning_coverage
                && species_amount <= uniforms.breathable_gas_amount
            {
                atomicAdd(&results[result_base + BREATHABLE_WORD], 1u);
            }
        }
    }
}
