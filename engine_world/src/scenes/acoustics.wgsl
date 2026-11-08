// Copyright Rob Gage 2026

// Coarse observations of the authoritative simulation; no simulation buffers are written.
struct Params { region: vec4<u32>, bounds: vec4<u32>, counts: vec4<u32>, grid: vec4<u32> }
@group(0) @binding(0) var<uniform> p: Params;
@group(0) @binding(1) var<storage, read> ids: array<u32>;
@group(0) @binding(2) var<storage, read> motion: array<vec2<u32>>;
@group(0) @binding(3) var<storage, read> fluid: array<vec4<u32>>;
@group(0) @binding(4) var<storage, read> gas: array<vec4<u32>>;
@group(0) @binding(5) var<storage, read> pressure: array<f32>;
@group(0) @binding(6) var<storage, read> masks: array<u32>;
@group(0) @binding(7) var<storage, read_write> results: array<f32>;
var<workgroup> sums: array<array<f32,64>,6>;
fn index(c: vec2<u32>) -> u32 {
 let tile = (c/8u+p.region.zw)%p.region.xy;
 return (tile.y*p.region.x+tile.x)*64u+(c.y%8u)*8u+c.x%8u;
}
fn mask(id: u32) -> u32 {
 let form = id>>30u; let i = id&0x3fffffffu;
 if i>=p.counts[form] { return 0u; }
 var offset=0u; for(var f=0u;f<form;f++){offset+=p.counts[f];}
 return masks[offset+i];
}
@compute @workgroup_size(64)
fn observe(@builtin(workgroup_id) group: vec3<u32>, @builtin(local_invocation_index) lane: u32) {
 var values: array<f32,6>;
 let start=p.bounds.xy+group.xy*p.grid.x;
 let stride=max(p.grid.w,1u);
 let side=(p.grid.x+stride-1u)/stride;
 for(var j=lane;j<side*side;j+=64u){
  let c=start+vec2<u32>(j%side,j/side)*stride;
  if any(c>=p.bounds.zw) {continue;}
  let i=index(c); let id=ids[i]; let m=mask(id);
  let r=fluid[i]; let mass=max(f32(bitcast<i32>(r.x)),0.0);
  let fm=mask((r.w&0x3fffffffu)|(2u<<30u));
  let speed=length(vec2<f32>(vec2<i32>(bitcast<i32>(r.y),bitcast<i32>(r.z))))/max(mass,1.0);
  // Moving ordinary fluids, and moving hot fluids; resting pools remain silent.
  if mass>256.0 && speed>0.03 {
   if (fm&4u)!=0u {values[3]+=min(speed,2.0);} else if (fm&2u)==0u {values[0]+=min(speed,2.0);}
  }
  // Acid must be contacting corrodible matter; its mere presence makes no sound.
  if mass>256.0 && (fm&2u)!=0u {
   for(var axis=0u;axis<2u;axis++){
    var n=c; n[axis]+=1u;
    if all(n<p.bounds.zw) && (mask(ids[index(n)])&8u)!=0u {values[2]+=1.0;}
   }
  }
  // Gas species have 24-bit amounts and 8-bit indices. Count burning gas only.
  let slots=gas[i*2u];
  for(var g=0u;g<4u;g++){
   if (mask((3u<<30u)|(slots[g]>>24u))&1u)!=0u {values[1]+=min(f32(slots[g]&0xffffffu)/4096.0,2.0);}
  }
  let pr=abs(pressure[i]); if pr>0.5 && pr<100000.0 {values[4]+=min(pr,8.0);}
  if id>>30u==1u {
   let v=bitcast<i32>(motion[i].x);
   let speed2=length(vec2<f32>(vec2<i32>(extractBits(v,0u,16u),extractBits(v,16u,16u))))/1024.0;
   if speed2>0.08 {values[5]+=min(speed2,2.0);}
  }
 }
 for(var k=0u;k<6u;k++){sums[k][lane]=values[k];}
 workgroupBarrier();
 for(var stride=32u;stride>0u;stride/=2u){
  if lane<stride {for(var k=0u;k<6u;k++){sums[k][lane]+=sums[k][lane+stride];}}
  workgroupBarrier();
 }
 if lane==0u {for(var k=0u;k<6u;k++){results[(group.y*p.grid.y+group.x)*6u+k]=sums[k][0]*f32(stride*stride);}}
}
