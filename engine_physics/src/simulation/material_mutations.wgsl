// Copyright Rob Gage 2026
#define_import_path compute::material_mutations
#import utility::material_identifier::{EMPTY_MATERIAL_IDENTIFIER, CELLULAR_STATIC_MATERIAL_FORM, CELLULAR_DYNAMIC_MATERIAL_FORM, FLUID_MATERIAL_FORM, material_form_from_identifier, material_index_from_identifier}
#import utility::fluid_edit::FLUID_EDIT_ERASE
struct Request { cell: u32, expected_source: u32, replacement: u32, flags: u32 }
struct Parameters { buffered_cell_count: u32, gas_count: u32, padding: vec2<u32> }
@group(0) @binding(0) var<storage, read> requests: array<Request>;
@group(0) @binding(1) var<storage, read> request_count: array<atomic<u32>>;
@group(0) @binding(2) var<storage, read_write> cellular_material_identifiers: array<u32>;
@group(0) @binding(3) var<storage, read_write> cellular_appearances: array<u32>;
@group(0) @binding(4) var<storage, read> static_defaults: array<u32>;
@group(0) @binding(5) var<storage, read_write> cellular_integrities: array<f32>;
@group(0) @binding(6) var<storage, read_write> cellular_kinematics: array<vec4<f32>>;
@group(0) @binding(7) var<storage, read_write> fluid_edits: array<u32>;
@group(0) @binding(8) var<storage, read_write> gas_concentrations: array<f32>;
@group(0) @binding(9) var<uniform> parameters: Parameters;
@group(0) @binding(10) var<storage, read_write> fluid_edits_pending: array<atomic<u32>>;
@group(1) @binding(0) var<storage, read_write> indirect_dispatch: array<u32>;
@compute @workgroup_size(1) fn prepare_material_mutation_dispatch(@builtin(global_invocation_id) invocation: vec3<u32>) {
 if (invocation.x != 0u) { return; }
 let count: u32 = min(atomicLoad(&request_count[0]), parameters.buffered_cell_count);
 indirect_dispatch[0] = (count + 63u) / 64u;
 indirect_dispatch[1] = 1u;
 indirect_dispatch[2] = 1u;
}
@compute @workgroup_size(64) fn resolve_material_mutations(@builtin(global_invocation_id) invocation: vec3<u32>) {
 let i=invocation.x; if (i >= atomicLoad(&request_count[0]) || i >= parameters.buffered_cell_count) { return; }
 let request=requests[i]; if (request.cell >= parameters.buffered_cell_count || cellular_material_identifiers[request.cell] != request.expected_source) { return; }
 let form=material_form_from_identifier(request.replacement); cellular_kinematics[request.cell]=vec4<f32>(0.0);
 if (request.replacement == EMPTY_MATERIAL_IDENTIFIER) { cellular_material_identifiers[request.cell]=0u; cellular_appearances[request.cell]=0u; cellular_integrities[request.cell]=0.0; fluid_edits[request.cell]=FLUID_EDIT_ERASE; atomicStore(&fluid_edits_pending[0],1u); for(var s=0u;s<parameters.gas_count;s++){gas_concentrations[s*parameters.buffered_cell_count+request.cell]=0.0;} return; }
 if (form == CELLULAR_STATIC_MATERIAL_FORM) { let index=material_index_from_identifier(request.replacement); if(index >= arrayLength(&static_defaults)){return;} cellular_material_identifiers[request.cell]=request.replacement; cellular_integrities[request.cell]=bitcast<f32>(static_defaults[index]); fluid_edits[request.cell]=FLUID_EDIT_ERASE; atomicStore(&fluid_edits_pending[0],1u); for(var s=0u;s<parameters.gas_count;s++){gas_concentrations[s*parameters.buffered_cell_count+request.cell]=0.0;} return; }
 if (form == CELLULAR_DYNAMIC_MATERIAL_FORM) { cellular_material_identifiers[request.cell]=request.replacement; cellular_integrities[request.cell]=0.0; fluid_edits[request.cell]=FLUID_EDIT_ERASE; atomicStore(&fluid_edits_pending[0],1u); for(var s=0u;s<parameters.gas_count;s++){gas_concentrations[s*parameters.buffered_cell_count+request.cell]=0.0;} return; }
 cellular_material_identifiers[request.cell]=0u; cellular_appearances[request.cell]=0u; cellular_integrities[request.cell]=0.0;
 if (form == FLUID_MATERIAL_FORM) { fluid_edits[request.cell]=request.replacement; atomicStore(&fluid_edits_pending[0],1u); for(var s=0u;s<parameters.gas_count;s++){gas_concentrations[s*parameters.buffered_cell_count+request.cell]=0.0;} return; }
 let species=material_index_from_identifier(request.replacement); if(species >= parameters.gas_count){return;} fluid_edits[request.cell]=FLUID_EDIT_ERASE; atomicStore(&fluid_edits_pending[0],1u); for(var s=0u;s<parameters.gas_count;s++){gas_concentrations[s*parameters.buffered_cell_count+request.cell]=select(0.0,1.0,s==species);} }
