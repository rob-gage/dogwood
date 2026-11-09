use engine_host::acceleration::Accelerator;

// Run the actual old and new WGSL traversal against a stateful contact handler. It consumes
// existing units and creates later units, exposing invalid caching or reordered contacts.
const HARNESS: &str = r#"
struct Uniforms { unit_count:u32, block_cell_count:u32, face_count:u32, loaded_cell_count:u32, neighbor_axis_count:u32, block_offset:u32 }
struct Tiles { active_tiles:array<u32> }
struct Cell { index:u32, flags:array<u32,7> }
@group(0) @binding(0) var<storage,read> tiles:Tiles;
@group(0) @binding(1) var<uniform> uniforms:Uniforms;
@group(0) @binding(2) var<storage,read_write> output:array<u32>;
@group(0) @binding(3) var<storage,read_write> thermal_awake:array<atomic<u32>>;
const CELL_OUTSIDE:u32=0xffffffffu;
const FLAG_UNARY:u32=1u;
const FLAG_PAIRED:u32=2u;
const THERMAL_TILE_AWAKE:u32=1u;
const TILES_PER_WORKGROUP:u32=4u;
var<private> cells:array<Cell,12>;
var<private> is_reacting:bool;
var<private> trace_base:u32;
var<private> trace_count:u32;
fn listed_tile(t:u32)->vec2<i32>{ return vec2<i32>(i32(t)*8,0); }
fn load_cell(slot:u32,p:vec2<i32>){
 cells[slot].index=select(slot,CELL_OUTSIDE,(u32(p.x)*7u+slot*13u)%17u==0u);
 for(var u=0u;u<7u;u++){cells[slot].flags[u]=(u32(p.x)*1597u+u32(p.y)*28411u+slot*31u+u*7919u)&3u;}
}
fn load_cell_heat(slot:u32){}
fn block_cell_position(p:vec2<i32>,slot:u32)->vec2<i32>{return p+vec2<i32>(i32(slot%4u),i32(slot/4u));}
fn unit_flags(slot:u32,unit:u32)->u32{if cells[slot].index==CELL_OUTSIDE{return 0u;}return cells[slot].flags[unit];}
fn cells_can_pair(a:u32,b:u32)->bool{
 var first=false;var second=false;
 for(var u=0u;u<uniforms.unit_count;u++){first=first || (unit_flags(a,u)&FLAG_PAIRED)!=0u;second=second || (unit_flags(b,u)&FLAG_PAIRED)!=0u;}
 return first && second;
}
fn react_contact(a:u32,au:u32,b:u32,bu:u32,scale:f32){
 let paired=b!=CELL_OUTSIDE;
 let required=select(FLAG_UNARY,FLAG_PAIRED,paired);
 if (unit_flags(a,au)&required)==0u{return;}
 if paired {if (unit_flags(b,bu)&FLAG_PAIRED)==0u{return;}}
 let code=a|(au<<4u)|((b&15u)<<8u)|(bu<<12u)|(u32(scale)<<16u);
 output[trace_base+1u+trace_count]=code;
 trace_count++;
 cells[a].flags[au]=(cells[a].flags[au]+trace_count+bu)&3u;
 let target_unit=(au+1u+trace_count%3u)%uniforms.unit_count;
 cells[a].flags[target_unit]=(trace_count>>2u)&3u;
 if paired && b<4u{cells[b].flags[bu]=(cells[b].flags[bu]+1u+(trace_count&1u))&3u;}
 is_reacting=true;
}
fn store_cell(slot:u32){
 output[trace_base]=trace_count;
 for(var u=0u;u<7u;u++){output[trace_base+701u+slot*7u+u]=cells[slot].flags[u];}
}
"#;

#[test]
#[ignore = "requires GPU; verifies actual WGSL traversal order with changing reactants"]
fn sparse_contacts_preserve_order_and_new_products() {
    let a=Accelerator::default();
    let current=include_str!("chemistry_cellular_tile.wgsl");
    let marker="@compute @workgroup_size(64)\nfn react_tiles(";
    let current=&current[current.find(marker).unwrap()..];
    let reference=include_str!("chemistry_contact_reference.wgsl");
    for units in 1u32..=7 {
        let mut traces=Vec::new();
        for body in [reference,current] {
            let body=body.replace("    let local_block: u32 = local_index % 16u;", "    let local_block: u32 = local_index % 16u;\n    trace_base=(listed*16u+local_block)*736u; trace_count=0u;");
            let source=format!("{HARNESS}\n{body}");
            traces.push(run(&a,&source,units));
        }
        assert_eq!(traces[0],traces[1],"contact sequence or products changed with {units} units");
    }
}
fn run(a:&Accelerator,source:&str,units:u32)->Vec<u32>{
    let d=a.device();
    let module=d.create_shader_module(wgpu::ShaderModuleDescriptor{label:Some("contact traversal regression"),source:wgpu::ShaderSource::Wgsl(source.into())});
    let pipeline=d.create_compute_pipeline(&wgpu::ComputePipelineDescriptor{label:None,layout:None,module:&module,entry_point:Some("react_tiles"),compilation_options:Default::default(),cache:None});
    let buffer=|size,usage|d.create_buffer(&wgpu::BufferDescriptor{label:None,size,usage,mapped_at_creation:false});
    let tiles=buffer(32,wgpu::BufferUsages::STORAGE|wgpu::BufferUsages::COPY_DST);
    let tile_words=[0u32,65535,1,65535,2,65535,3,65535];
    a.queue().write_buffer(&tiles,0,&tile_words.iter().flat_map(|v|v.to_le_bytes()).collect::<Vec<_>>());
    let uniforms=buffer(32,wgpu::BufferUsages::UNIFORM|wgpu::BufferUsages::COPY_DST);
    a.queue().write_buffer(&uniforms,0,&[units,4,4,12,2,1,0,0].iter().flat_map(|v|v.to_le_bytes()).collect::<Vec<_>>());
    let size=64*736*4;
    let output=buffer(size,wgpu::BufferUsages::STORAGE|wgpu::BufferUsages::COPY_SRC);
    let awake=buffer(16,wgpu::BufferUsages::STORAGE);
    let read=buffer(size,wgpu::BufferUsages::MAP_READ|wgpu::BufferUsages::COPY_DST);
    let group=d.create_bind_group(&wgpu::BindGroupDescriptor{label:None,layout:&pipeline.get_bind_group_layout(0),entries:&[
        wgpu::BindGroupEntry{binding:0,resource:tiles.as_entire_binding()},wgpu::BindGroupEntry{binding:1,resource:uniforms.as_entire_binding()},wgpu::BindGroupEntry{binding:2,resource:output.as_entire_binding()},wgpu::BindGroupEntry{binding:3,resource:awake.as_entire_binding()}]});
    let mut e=d.create_command_encoder(&Default::default());
    {let mut pass=e.begin_compute_pass(&Default::default());pass.set_pipeline(&pipeline);pass.set_bind_group(0,&group,&[]);pass.dispatch_workgroups(1,1,1);}
    e.copy_buffer_to_buffer(&output,0,&read,0,size);a.queue().submit([e.finish()]);
    read.slice(..).map_async(wgpu::MapMode::Read,|r|r.unwrap());d.poll(wgpu::PollType::Wait{submission_index:None,timeout:None}).unwrap();
    let bytes=read.slice(..).get_mapped_range().unwrap();bytes.chunks_exact(4).map(|v|u32::from_le_bytes(v.try_into().unwrap())).collect()
}

fn shader_function(source: &str, name: &str) -> String {
    let start = source.find(&format!("fn {name}(" )).unwrap();
    let brace = start + source[start..].find('{').unwrap();
    let mut depth = 0;
    for (offset, ch) in source[brace..].char_indices() {
        if ch == '{' { depth += 1; }
        if ch == '}' { depth -= 1; if depth == 0 { return source[start..brace + offset + 1].to_owned(); } }
    }
    panic!("unterminated function")
}

#[test]
#[ignore = "requires GPU; verifies cached paired units with stateful same-tick products"]
fn paired_unit_cache_preserves_contact_order_and_products() {
    let a = Accelerator::default();
    let shader = include_str!("chemistry_cellular_tile.wgsl");
    let marker = "@compute @workgroup_size(64)\nfn react_tiles(";
    let traversal = shader[shader.find(marker).unwrap()..].split("// The private masks").next().unwrap();
    let prefix = "var<private> paired_units_cache:array<u32,12>; var<private> paired_units_valid:u32;";
    let helpers = format!("{}\n{}", shader_function(shader,"cached_paired_units"), shader_function(shader,"invalidate_paired_units"));
    let old_pair = shader_function(HARNESS,"cells_can_pair");
    let cached_pair = "fn cells_can_pair(a:u32,b:u32)->bool { return cached_paired_units(a)!=0u && cached_paired_units(b)!=0u; }";
    let cached = HARNESS.replace(&old_pair,cached_pair).replace("cells[a].flags[au]=", "invalidate_paired_units(); cells[a].flags[au]=");
    for units in 1u32..=7 {
        let mut results = Vec::new();
        for harness in [HARNESS, cached.as_str()] {
            let body = traversal.replace("    let local_block: u32 = local_index % 16u;", "    let local_block: u32 = local_index % 16u;\n    trace_base=(listed*16u+local_block)*736u; trace_count=0u;");
            results.push(run(&a,&format!("{harness}\n{prefix}\n{helpers}\n{body}"),units));
        }
        assert_eq!(results[0],results[1],"cached contact order or products changed for {units} units");
    }
}
