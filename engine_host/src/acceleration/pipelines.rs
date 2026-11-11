//! Device-lifetime pipeline reuse and compilation telemetry.
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex, OnceLock, Weak,
        atomic::{AtomicU64, Ordering},
    },
    time::Instant,
};
static CREATED: AtomicU64 = AtomicU64::new(0);
static DEVICES: OnceLock<Mutex<HashMap<wgpu::Device, Weak<PipelineStore>>>> = OnceLock::new();

#[derive(Default)]
pub(crate) struct PipelineStore(Mutex<Cache>);
#[derive(Default)]
struct Cache {
    modules: HashMap<String, wgpu::ShaderModule>,
    bindings: HashMap<Vec<wgpu::BindGroupLayoutEntry>, wgpu::BindGroupLayout>,
    layouts: HashMap<LayoutKey, wgpu::PipelineLayout>,
    compute: HashMap<ComputeKey, wgpu::ComputePipeline>,
    render: HashMap<RenderKey, wgpu::RenderPipeline>,
}
#[derive(Hash, PartialEq, Eq)]
struct LayoutKey {
    bindings: Vec<Option<wgpu::BindGroupLayout>>,
    immediate_size: u32,
}
#[derive(Hash, PartialEq, Eq)]
struct StageKey {
    module: wgpu::ShaderModule,
    entry: Option<String>,
    constants: Vec<(String, u64)>,
    zero_initialize_workgroup_memory: bool,
}
impl StageKey {
    fn new(
        module: &wgpu::ShaderModule,
        entry: Option<&str>,
        options: &wgpu::PipelineCompilationOptions<'_>,
    ) -> Self {
        Self {
            module: module.clone(),
            entry: entry.map(str::to_owned),
            constants: options
                .constants
                .iter()
                .map(|(name, value)| ((*name).to_owned(), value.to_bits()))
                .collect(),
            zero_initialize_workgroup_memory: options.zero_initialize_workgroup_memory,
        }
    }
}
#[derive(Hash, PartialEq, Eq)]
struct ComputeKey {
    layout: Option<wgpu::PipelineLayout>,
    stage: StageKey,
    cache: Option<wgpu::PipelineCache>,
}
#[derive(Hash, PartialEq, Eq)]
struct VertexBufferKey {
    stride: u64,
    step: wgpu::VertexStepMode,
    attributes: Vec<wgpu::VertexAttribute>,
}
#[derive(Hash, PartialEq, Eq)]
struct RenderKey {
    layout: Option<wgpu::PipelineLayout>,
    vertex: StageKey,
    buffers: Vec<Option<VertexBufferKey>>,
    primitive: wgpu::PrimitiveState,
    depth: Option<wgpu::DepthStencilState>,
    multisample: wgpu::MultisampleState,
    fragment: Option<(StageKey, Vec<Option<wgpu::ColorTargetState>>)>,
    multiview_mask: Option<std::num::NonZeroU32>,
    cache: Option<wgpu::PipelineCache>,
}
fn devices() -> &'static Mutex<HashMap<wgpu::Device, Weak<PipelineStore>>> {
    DEVICES.get_or_init(Mutex::default)
}
pub(crate) fn register(device: &wgpu::Device, store: &Arc<PipelineStore>) {
    devices()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(device.clone(), Arc::downgrade(store));
}
fn store(device: &wgpu::Device) -> Option<Arc<PipelineStore>> {
    devices()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(device)
        .and_then(Weak::upgrade)
}
impl Drop for PipelineStore {
    fn drop(&mut self) {
        // The registry does not own caches. The last Accelerator releases its device entry.
        devices()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .retain(|_, store| store.strong_count() != 0);
    }
}

/// Number of actual engine pipeline creations begun, excluding cache hits.
pub fn pipeline_creation_count() -> u64 {
    CREATED.load(Ordering::Relaxed)
}

/// Reuse a WGSL module with identical source on the same Accelerator device.
pub fn create_shader_module(
    device: &wgpu::Device,
    descriptor: wgpu::ShaderModuleDescriptor<'_>,
) -> wgpu::ShaderModule {
    if let (Some(store), wgpu::ShaderSource::Wgsl(source)) = (store(device), &descriptor.source) {
        let mut cache = store.0.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(module) = cache.modules.get(source.as_ref()) {
            return module.clone();
        }
        let key = source.to_string();
        let module = device.create_shader_module(descriptor);
        cache.modules.insert(key, module.clone());
        return module;
    }
    device.create_shader_module(descriptor)
}

/// Reuse a layout with identical binding entries on the same Accelerator device.
pub fn create_bind_group_layout(
    device: &wgpu::Device,
    descriptor: &wgpu::BindGroupLayoutDescriptor<'_>,
) -> wgpu::BindGroupLayout {
    if let Some(store) = store(device) {
        let mut cache = store.0.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(layout) = cache.bindings.get(descriptor.entries) {
            return layout.clone();
        }
        let layout = device.create_bind_group_layout(descriptor);
        cache
            .bindings
            .insert(descriptor.entries.to_vec(), layout.clone());
        return layout;
    }
    device.create_bind_group_layout(descriptor)
}

/// Reuse a pipeline layout, including its exact bind-group layouts and immediate size.
pub fn create_pipeline_layout(
    device: &wgpu::Device,
    descriptor: &wgpu::PipelineLayoutDescriptor<'_>,
) -> wgpu::PipelineLayout {
    if let Some(store) = store(device) {
        let key = LayoutKey {
            bindings: descriptor
                .bind_group_layouts
                .iter()
                .map(|layout| layout.map(Clone::clone))
                .collect(),
            immediate_size: descriptor.immediate_size,
        };
        let mut cache = store.0.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(layout) = cache.layouts.get(&key) {
            return layout.clone();
        }
        let layout = device.create_pipeline_layout(descriptor);
        cache.layouts.insert(key, layout.clone());
        return layout;
    }
    device.create_pipeline_layout(descriptor)
}

/// Reuse a compute pipeline with the same module, layout, entry and specialization options.
pub fn create_compute_pipeline(
    device: &wgpu::Device,
    descriptor: &wgpu::ComputePipelineDescriptor<'_>,
) -> wgpu::ComputePipeline {
    let store = store(device);
    let mut cache = store
        .as_ref()
        .map(|store| store.0.lock().unwrap_or_else(|e| e.into_inner()));
    let key = ComputeKey {
        layout: descriptor.layout.cloned(),
        stage: StageKey::new(
            descriptor.module,
            descriptor.entry_point,
            &descriptor.compilation_options,
        ),
        cache: descriptor.cache.cloned(),
    };
    if let Some(pipeline) = cache.as_ref().and_then(|cache| cache.compute.get(&key)) {
        return pipeline.clone();
    }
    let id = CREATED.fetch_add(1, Ordering::Relaxed) + 1;
    let began = Instant::now();
    eprintln!(
        "GPU pipeline {id} begin: {} / {}",
        descriptor.label.unwrap_or("compute"),
        descriptor.entry_point.unwrap_or("default")
    );
    let result = device.create_compute_pipeline(descriptor);
    eprintln!(
        "GPU pipeline {id} ready in {} ms",
        began.elapsed().as_millis()
    );
    if let Some(cache) = &mut cache {
        cache.compute.insert(key, result.clone());
    }
    result
}

/// Reuse a render pipeline with identical shader stages, layouts and render state.
pub fn create_render_pipeline(
    device: &wgpu::Device,
    descriptor: &wgpu::RenderPipelineDescriptor<'_>,
) -> wgpu::RenderPipeline {
    let store = store(device);
    let mut cache = store
        .as_ref()
        .map(|store| store.0.lock().unwrap_or_else(|e| e.into_inner()));
    let key = RenderKey {
        layout: descriptor.layout.cloned(),
        vertex: StageKey::new(
            descriptor.vertex.module,
            descriptor.vertex.entry_point,
            &descriptor.vertex.compilation_options,
        ),
        buffers: descriptor
            .vertex
            .buffers
            .iter()
            .map(|buffer| {
                buffer.as_ref().map(|buffer| VertexBufferKey {
                    stride: buffer.array_stride,
                    step: buffer.step_mode,
                    attributes: buffer.attributes.to_vec(),
                })
            })
            .collect(),
        primitive: descriptor.primitive,
        depth: descriptor.depth_stencil.clone(),
        multisample: descriptor.multisample,
        fragment: descriptor.fragment.as_ref().map(|fragment| {
            (
                StageKey::new(
                    fragment.module,
                    fragment.entry_point,
                    &fragment.compilation_options,
                ),
                fragment.targets.to_vec(),
            )
        }),
        multiview_mask: descriptor.multiview_mask,
        cache: descriptor.cache.cloned(),
    };
    if let Some(pipeline) = cache.as_ref().and_then(|cache| cache.render.get(&key)) {
        return pipeline.clone();
    }
    let id = CREATED.fetch_add(1, Ordering::Relaxed) + 1;
    let began = Instant::now();
    eprintln!(
        "GPU pipeline {id} begin: {}",
        descriptor.label.unwrap_or("render")
    );
    let result = device.create_render_pipeline(descriptor);
    eprintln!(
        "GPU pipeline {id} ready in {} ms",
        began.elapsed().as_millis()
    );
    if let Some(cache) = &mut cache {
        cache.render.insert(key, result.clone());
    }
    result
}
