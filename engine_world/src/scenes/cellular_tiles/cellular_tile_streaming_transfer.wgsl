// Copyright Rob Gage 2026

// Word-wise transfer between packed per-tile records and the per-field cellular tile buffers.
// `scatter` writes one field of every record to its tile slot (or fills it with a constant);
// `gather` reads one field of every tile slot into its record, writing zeros for absent tiles.

const WORKGROUP_SIZE: u32 = 64u;
const ABSENT_TILE: u32 = 0xffffffffu;

struct Parameters {
    tile_count: u32,
    record_word_count: u32,
    field_word_offset: u32,
    field_word_count: u32,
    // 0 copies from the record, 1 fills with `constant`
    mode: u32,
    constant: u32,
    // invocations covered by one row of workgroups along x
    row_invocation_count: u32,
    padding: u32,
}

@group(0) @binding(0) var<uniform> parameters: Parameters;
@group(0) @binding(1) var<storage, read_write> records: array<u32>;
@group(0) @binding(2) var<storage, read> tile_indices: array<u32>;
@group(0) @binding(3) var<storage, read_write> field: array<u32>;

@compute @workgroup_size(WORKGROUP_SIZE)
fn scatter(@builtin(global_invocation_id) id: vec3<u32>) {
    let invocation = id.y * parameters.row_invocation_count + id.x;
    if invocation >= parameters.tile_count * parameters.field_word_count { return; }
    let tile = invocation / parameters.field_word_count;
    let word = invocation % parameters.field_word_count;
    let destination = tile_indices[tile] * parameters.field_word_count + word;
    if parameters.mode == 0u {
        field[destination] =
            records[tile * parameters.record_word_count + parameters.field_word_offset + word];
    } else {
        field[destination] = parameters.constant;
    }
}

@compute @workgroup_size(WORKGROUP_SIZE)
fn gather(@builtin(global_invocation_id) id: vec3<u32>) {
    let invocation = id.y * parameters.row_invocation_count + id.x;
    if invocation >= parameters.tile_count * parameters.field_word_count { return; }
    let tile = invocation / parameters.field_word_count;
    let word = invocation % parameters.field_word_count;
    let source = tile_indices[tile];
    var value = 0u;
    if source != ABSENT_TILE {
        value = field[source * parameters.field_word_count + word];
    }
    records[tile * parameters.record_word_count + parameters.field_word_offset + word] = value;
}
