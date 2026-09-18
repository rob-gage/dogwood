#define_import_path compute::thermal_conduction
#import utility::tile_ring::physical_cell_index_from_world_cell
#import utility::cell_coordinates::world_cell_from_logical_tile_major_index
struct ThermalConductionParameters {
    delta_time: f32,
    cell_count: u32,
    origin: vec2<i32>,
    tiles: vec2<u32>,
    ring: vec2<u32>
}

@group(0) @binding(0) var<storage, read> interaction: array<vec4<f32>>;
@group(0) @binding(1) var<storage, read_write> face_flux: array<vec2<f32>>;
@group(0) @binding(2) var<storage, read_write> face_conductance: array<vec2<f32>>;
@group(0) @binding(3) var<storage, read_write> conductance_sum: array<f32>;
@group(0) @binding(4) var<storage, read_write> solved: array<vec4<f32>>;
@group(0) @binding(5) var<uniform> thermal_conduction_parameters: ThermalConductionParameters;

fn thermal_conduction_physical_cell_index(c: vec2<i32>) -> u32 {
    return
        physical_cell_index_from_world_cell(
            c,
            thermal_conduction_parameters.origin,
            thermal_conduction_parameters.tiles,
            thermal_conduction_parameters.ring,
        );
}

fn inside(c: vec2<i32>) -> bool {
    return
        all(c >= thermal_conduction_parameters.origin * 8) && all(
            c < (thermal_conduction_parameters.origin + vec2<i32>(thermal_conduction_parameters.tiles)) * 8,
        );
}

fn base(a: u32, b: u32) -> f32 {
    let A = interaction[a];
    let B = interaction[b];
    if (A.x <= 0.000001 || B.x <= 0.000001 || A.z <= 0.000001 || B.z <= 0.000001) {
        return 0.0;
    }
    return (2.0 * A.z * B.z / (A.z + B.z)) * thermal_conduction_parameters.delta_time;
}

@compute @workgroup_size(64)
fn calculate_thermal_face_flux(@builtin(global_invocation_id) invocation: vec3<u32>) {
    if (invocation.x >= thermal_conduction_parameters.cell_count) {
        return;
    }
    let c =
        world_cell_from_logical_tile_major_index(invocation.x, thermal_conduction_parameters.origin, thermal_conduction_parameters.tiles);
    let i = thermal_conduction_physical_cell_index(c);
    var x = 0.0;
    var y = 0.0;
    if (inside(c + vec2<i32>(1, 0))) {
        x = base(i, thermal_conduction_physical_cell_index(c + vec2<i32>(1, 0)));
    }
    if (inside(c + vec2<i32>(0, 1))) {
        y = base(i, thermal_conduction_physical_cell_index(c + vec2<i32>(0, 1)));
    }
    face_conductance[i] = vec2<f32>(x, y);
}

@compute @workgroup_size(64)
fn calculate_thermal_conductance_sum(@builtin(global_invocation_id) invocation: vec3<u32>) {
    if (invocation.x >= thermal_conduction_parameters.cell_count) {
        return;
    }
    let c =
        world_cell_from_logical_tile_major_index(invocation.x, thermal_conduction_parameters.origin, thermal_conduction_parameters.tiles);
    let i = thermal_conduction_physical_cell_index(c);
    var s = face_conductance[i].x + face_conductance[i].y;
    if (inside(c - vec2<i32>(1, 0))) {
        s += face_conductance[thermal_conduction_physical_cell_index(c - vec2<i32>(1, 0))].x;
    }
    if (inside(c - vec2<i32>(0, 1))) {
        s += face_conductance[thermal_conduction_physical_cell_index(c - vec2<i32>(0, 1))].y;
    }
    conductance_sum[i] = s;
}

@compute @workgroup_size(64)
fn calculate_thermal_actual_flux(@builtin(global_invocation_id) invocation: vec3<u32>) {
    if (invocation.x >= thermal_conduction_parameters.cell_count) {
        return;
    }
    let c =
        world_cell_from_logical_tile_major_index(invocation.x, thermal_conduction_parameters.origin, thermal_conduction_parameters.tiles);
    let i = thermal_conduction_physical_cell_index(c);
    let A = interaction[i];
    var out = vec2<f32>(0.0);
    if (inside(c + vec2<i32>(1, 0))) {
        let j = thermal_conduction_physical_cell_index(c + vec2<i32>(1, 0));
        let B = interaction[j];
        let scale =
            min(
                1.0,
                min(
                    select(1.0, A.x / conductance_sum[i], conductance_sum[i] > 0.000001),
                    select(1.0, B.x / conductance_sum[j], conductance_sum[j] > 0.000001),
                ),
            );
        let raw = face_conductance[i].x * scale * (A.w - B.w);
        let eq = abs(A.w - B.w) / (1.0 / A.x + 1.0 / B.x);
        out.x = sign(raw) * min(abs(raw), eq);
    }
    if (inside(c + vec2<i32>(0, 1))) {
        let j = thermal_conduction_physical_cell_index(c + vec2<i32>(0, 1));
        let B = interaction[j];
        let scale =
            min(
                1.0,
                min(
                    select(1.0, A.x / conductance_sum[i], conductance_sum[i] > 0.000001),
                    select(1.0, B.x / conductance_sum[j], conductance_sum[j] > 0.000001),
                ),
            );
        let raw = face_conductance[i].y * scale * (A.w - B.w);
        let eq = abs(A.w - B.w) / (1.0 / A.x + 1.0 / B.x);
        out.y = sign(raw) * min(abs(raw), eq);
    }
    face_flux[i] = out;
}

@compute @workgroup_size(64)
fn resolve_thermal_conduction(@builtin(global_invocation_id) invocation: vec3<u32>) {
    if (invocation.x >= thermal_conduction_parameters.cell_count) {
        return;
    }
    let c =
        world_cell_from_logical_tile_major_index(invocation.x, thermal_conduction_parameters.origin, thermal_conduction_parameters.tiles);
    let i = thermal_conduction_physical_cell_index(c);
    let own = face_flux[i];
    var incoming = 0.0;
    if (inside(c - vec2<i32>(1, 0))) {
        incoming += face_flux[thermal_conduction_physical_cell_index(c - vec2<i32>(1, 0))].x;
    }
    if (inside(c - vec2<i32>(0, 1))) {
        incoming += face_flux[thermal_conduction_physical_cell_index(c - vec2<i32>(0, 1))].y;
    }
    let src = interaction[i];
    let e = max(src.y + incoming - own.x - own.y, 0.0);
    let t = select(src.w, e / src.x, src.x > 0.000001);
    solved[i] = vec4<f32>(src.x, e, src.z, max(t, 0.0));
}
