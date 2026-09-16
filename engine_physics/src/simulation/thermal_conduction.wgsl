#define_import_path compute::thermal_conduction
#import utility::tile_ring::physical_cell_index_from_world_cell
#import utility::cell_coordinates::world_cell_from_logical_tile_major_index
struct Parameters { dt:f32, cell_count:u32, padding:vec2<u32>, origin:vec2<i32>, tiles:vec2<u32>, ring:vec2<u32> }
@group(0) @binding(0) var<storage,read> interaction:array<vec4<f32>>;
@group(0) @binding(1) var<storage,read_write> face_flux:array<vec2<f32>>;
@group(0) @binding(2) var<storage,read_write> solved:array<vec4<f32>>;
@group(0) @binding(3) var<uniform> parameters:Parameters;
fn index(cell:vec2<i32>)->u32{return physical_cell_index_from_world_cell(cell,parameters.origin,parameters.tiles,parameters.ring);}
fn inside(cell:vec2<i32>)->bool{return all(cell>=parameters.origin*8) && all(cell<(parameters.origin+vec2<i32>(parameters.tiles))*8);}
fn flux(a:u32,b:u32)->f32 { let A=interaction[a]; let B=interaction[b]; if(A.x<=0.000001||B.x<=0.000001||A.z<=0.000001||B.z<=0.000001){return 0.0;} let k=2.0*A.z*B.z/(A.z+B.z); let raw=k*parameters.dt*(A.w-B.w); let eq=abs(A.w-B.w)/(1.0/A.x+1.0/B.x); let source=select(B.y,A.y,raw>=0.0)/4.0; return sign(raw)*min(abs(raw),min(eq,max(source,0.0))); }
@compute @workgroup_size(64) fn calculate_thermal_face_flux(@builtin(global_invocation_id) id:vec3<u32>){if(id.x>=parameters.cell_count){return;} let c=world_cell_from_logical_tile_major_index(id.x,parameters.origin,parameters.tiles); let i=index(c); var x=0.0;var y=0.0;if(inside(c+vec2<i32>(1,0))){x=flux(i,index(c+vec2<i32>(1,0)));}if(inside(c+vec2<i32>(0,1))){y=flux(i,index(c+vec2<i32>(0,1)));}face_flux[i]=vec2<f32>(x,y);}
@compute @workgroup_size(64) fn resolve_thermal_conduction(@builtin(global_invocation_id) id:vec3<u32>){if(id.x>=parameters.cell_count){return;} let c=world_cell_from_logical_tile_major_index(id.x,parameters.origin,parameters.tiles); let i=index(c); let own=face_flux[i]; var incoming=0.0;if(inside(c-vec2<i32>(1,0))){incoming+=face_flux[index(c-vec2<i32>(1,0))].x;}if(inside(c-vec2<i32>(0,1))){incoming+=face_flux[index(c-vec2<i32>(0,1))].y;}let src=interaction[i];let e=max(src.y+incoming-own.x-own.y,0.0);let t=select(src.w,e/src.x,src.x>0.000001);solved[i]=vec4<f32>(src.x,e,src.z,max(t,0.0));}
