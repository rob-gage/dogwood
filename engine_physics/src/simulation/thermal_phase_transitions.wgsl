// Copyright Rob Gage 2026
#define_import_path compute::thermal_phase_transitions
#import utility::material_identifier::{EMPTY_MATERIAL_IDENTIFIER, material_dense_index}
#import utility::thermal_material::{ThermalMaterialRecord, ThermalMaterialParameters, thermal_material_specific_heat_capacity, thermal_material_cold_enabled, thermal_material_cold_threshold, thermal_material_cold_target, thermal_material_cold_yield, thermal_material_cold_latent_energy, thermal_material_hot_enabled, thermal_material_hot_threshold, thermal_material_hot_target, thermal_material_hot_yield, thermal_material_hot_latent_energy}
#import utility::tile_ring::{physical_cell_index_from_world_cell, world_cell_from_physical_tile_ring_index}
#import utility::cell_coordinates::CELLS_PER_TILE_FLOAT

struct Particle { material_identifier:u32, is_active:u32, position:vec2<f32>, velocity:vec2<f32>, prediction_collision_displacement:vec2<f32>, amount:f32, temperature:f32 }
// A request is deliberately authority-addressed: cell, kind (0 cell/1 particle/2 gas), locator,
// expected source, replacement, amount, temperature, and source world position.
struct Request { cell:u32, kind:u32, locator:u32, expected_source:u32, replacement:u32, amount:u32, temperature:u32, world_x:u32, world_y:u32 }
struct Parameters { origin:vec2<i32>, tiles:vec2<u32>, ring:vec2<u32>, cell_count:u32, particle_count:u32, gas_count:u32, tick:u32 }
@group(0) @binding(0) var<storage,read> cells:array<u32>;
@group(0) @binding(1) var<storage,read> amounts:array<f32>;
@group(0) @binding(2) var<storage,read> temperatures:array<f32>;
@group(0) @binding(3) var<storage,read> particles:array<Particle>;
@group(0) @binding(4) var<storage,read> concentrations:array<f32>;
@group(0) @binding(5) var<storage,read> gas_temperatures:array<f32>;
@group(0) @binding(6) var<storage,read> properties:array<ThermalMaterialRecord>;
@group(0) @binding(7) var<uniform> thermal:ThermalMaterialParameters;
@group(0) @binding(8) var<storage,read_write> requests:array<Request>;
@group(0) @binding(9) var<storage,read_write> request_count:array<atomic<u32>>;
@group(0) @binding(10) var<uniform> parameters:Parameters;
@group(0) @binding(11) var<storage,read> rigid_claims:array<atomic<u32>>;

fn should_yield(rate:f32, cell:u32, source:u32) -> bool {
 if(rate >= 1.0) { return true; } if(rate <= 0.0) { return false; }
 let h = (cell * 1664525u + source * 1013904223u + parameters.tick * 747796405u);
 return f32(h & 0x00ffffffu) / 16777216.0 < rate;
}
fn transition(source:u32, amount:f32, temperature:f32, cell:u32, kind:u32, locator:u32, world:vec2<i32>) {
 if (!(amount > 0.000001) || temperature != temperature || abs(temperature) > 3.4e38) { return; }
 let dense=material_dense_index(source,thermal.offsets,thermal.counts);
 if(dense == 0xffffffffu || dense >= arrayLength(&properties)) { return; }
 let record=properties[dense]; let cp=thermal_material_specific_heat_capacity(record); if(!(cp > 0.000001)){return;}
 var replacement=EMPTY_MATERIAL_IDENTIFIER; var threshold=0.0; var latent=0.0; var rate=0.0; var signed=0.0;
 if(thermal_material_hot_enabled(record) && temperature >= thermal_material_hot_threshold(record)) { replacement=thermal_material_hot_target(record); threshold=thermal_material_hot_threshold(record); latent=thermal_material_hot_latent_energy(record); rate=thermal_material_hot_yield(record); signed=1.0; }
 else if(thermal_material_cold_enabled(record) && temperature <= thermal_material_cold_threshold(record)) { replacement=thermal_material_cold_target(record); threshold=thermal_material_cold_threshold(record); latent=thermal_material_cold_latent_energy(record); rate=thermal_material_cold_yield(record); signed=-1.0; }
 else{return;}
 let target_dense=material_dense_index(replacement,thermal.offsets,thermal.counts); if(replacement==EMPTY_MATERIAL_IDENTIFIER || replacement==source || target_dense==0xffffffffu || target_dense>=arrayLength(&properties)){return;}
 let target_cp=thermal_material_specific_heat_capacity(properties[target_dense]); if(!(target_cp>0.000001)){return;}
 let sensible=amount*cp*max(signed*(temperature-threshold),0.0); let required=amount*max(latent,0.0); if(latent>0.0 && sensible<required){return;} if(!should_yield(rate,cell,source)){return;}
 let target_temperature=max(threshold + signed * max(sensible-required,0.0)/(amount*target_cp),0.0);
 let n=atomicAdd(&request_count[0],1u); if(n>=arrayLength(&requests)){return;}
 let position=(vec2<f32>(world)+vec2<f32>(0.5))/CELLS_PER_TILE_FLOAT;
 requests[n]=Request(cell,kind,locator,source,replacement,bitcast<u32>(amount),bitcast<u32>(target_temperature),bitcast<u32>(position.x),bitcast<u32>(position.y));
}
// TODO: rigid topology needs CPU/Rapier ownership changes; do not mutate its raster claim here.
@compute @workgroup_size(64) fn phase_cells(@builtin(global_invocation_id) id:vec3<u32>){if(id.x>=parameters.cell_count||atomicLoad(&rigid_claims[id.x])!=0xffffffffu){return;} let w=world_cell_from_physical_tile_ring_index(id.x,parameters.origin,parameters.tiles,parameters.ring); transition(cells[id.x],amounts[id.x],temperatures[id.x],id.x,0u,id.x,w);}
@compute @workgroup_size(64) fn phase_particles(@builtin(global_invocation_id) id:vec3<u32>){if(id.x>=parameters.particle_count){return;} let p=particles[id.x]; if(p.is_active==0u||p.material_identifier==EMPTY_MATERIAL_IDENTIFIER){return;} let world=vec2<i32>(floor(p.position*CELLS_PER_TILE_FLOAT)); let cell=physical_cell_index_from_world_cell(world,parameters.origin,parameters.tiles,parameters.ring); if(cell==0xffffffffu){return;} transition(p.material_identifier,p.amount,p.temperature,cell,1u,id.x,world);}
@compute @workgroup_size(64) fn phase_gases(@builtin(global_invocation_id) id:vec3<u32>){let n=parameters.cell_count*parameters.gas_count;if(id.x>=n){return;}let species=id.x/parameters.cell_count;let cell=id.x%parameters.cell_count;let source=species+1u;let world=world_cell_from_physical_tile_ring_index(cell,parameters.origin,parameters.tiles,parameters.ring);transition(source,concentrations[id.x],gas_temperatures[cell],cell,2u,species,world);}
