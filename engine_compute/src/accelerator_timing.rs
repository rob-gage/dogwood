// Copyright Rob Gage 2026

#[cfg(debug_assertions)]
use super::{
    accelerator_timing_readback::AcceleratorTimingReadback,
    accelerator_timing_record::AcceleratorTimingRecord,
    accelerator_timing_state::AcceleratorTimingState,
};
#[cfg(debug_assertions)]
use std::{
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

/// Debug-only WGPU timestamp-query state shared by an accelerator.
pub(crate) struct AcceleratorTiming {
    #[cfg(debug_assertions)]
    query_set: Option<wgpu::QuerySet>,
    #[cfg(debug_assertions)]
    resolve_buffer: Option<wgpu::Buffer>,
    #[cfg(debug_assertions)]
    timestamp_period: f64,
    #[cfg(debug_assertions)]
    recording: AtomicBool,
    #[cfg(debug_assertions)]
    state: Mutex<AcceleratorTimingState>,
}

impl AcceleratorTiming {
    #[cfg(debug_assertions)]
    const QUERY_CAPACITY: u32 = wgpu::QUERY_SET_MAX_QUERIES;
    #[cfg(debug_assertions)]
    const READBACK_COUNT: usize = 3;
    #[cfg(debug_assertions)]
    const SAMPLE_INTERVAL: Duration = Duration::from_secs(1);

    pub(crate) fn new(_device: &wgpu::Device, _queue: &wgpu::Queue, _is_available: bool) -> Self {
        #[cfg(debug_assertions)]
        {
            let byte_capacity: u64 = u64::from(Self::QUERY_CAPACITY) * u64::from(wgpu::QUERY_SIZE);
            let query_set: Option<wgpu::QuerySet> = _is_available.then(|| {
                _device.create_query_set(&wgpu::QuerySetDescriptor {
                    label: Some("Accelerator timing queries"),
                    ty: wgpu::QueryType::Timestamp,
                    count: Self::QUERY_CAPACITY,
                })
            });
            let resolve_buffer: Option<wgpu::Buffer> = _is_available.then(|| {
                _device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("Accelerator timing query resolve"),
                    size: byte_capacity,
                    usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
                    mapped_at_creation: false,
                })
            });
            let readbacks: Vec<AcceleratorTimingReadback> = if _is_available {
                (0..Self::READBACK_COUNT)
                    .map(|index| AcceleratorTimingReadback::new(_device, byte_capacity, index))
                    .collect()
            } else {
                Vec::new()
            };
            Self {
                query_set,
                resolve_buffer,
                timestamp_period: f64::from(_queue.get_timestamp_period()),
                recording: AtomicBool::new(false),
                state: Mutex::new(AcceleratorTimingState::new(readbacks)),
            }
        }
        #[cfg(not(debug_assertions))]
        Self {}
    }

    #[cfg(debug_assertions)]
    pub(crate) fn begin_sample(&self) {
        #[cfg(debug_assertions)]
        {
            if self.query_set.is_none()
                || !tracing::enabled!(
                    target: "dogwood_gpu",
                    tracing::Level::DEBUG
                )
            {
                return;
            }
            let now: Instant = Instant::now();
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            if state.active_slot.is_some()
                || state.last_sample.is_some_and(|last_sample| {
                    now.duration_since(last_sample) < Self::SAMPLE_INTERVAL
                })
            {
                return;
            }
            state.last_sample = Some(now);
            let Some(slot) = state.readbacks.iter().position(|readback| {
                readback.status.load(Ordering::Acquire) == AcceleratorTimingReadback::IDLE
            }) else {
                tracing::debug!(target: "dogwood_gpu", "skipped Accelerator timing sample; readback busy");
                return;
            };
            state.readbacks[slot]
                .status
                .store(AcceleratorTimingReadback::RECORDING, Ordering::Release);
            state.active_slot = Some(slot);
            state.capacity_exhausted = false;
            state.query_count = 0;
            state.records.clear();
            self.recording.store(true, Ordering::Release);
        }
    }

    #[inline]
    pub(crate) fn compute_timestamp_writes(
        &self,
        label: &str,
    ) -> Option<wgpu::ComputePassTimestampWrites<'_>> {
        #[cfg(debug_assertions)]
        {
            let (begin, end) = self.allocate_queries("compute", label)?;
            Some(wgpu::ComputePassTimestampWrites {
                query_set: self.query_set.as_ref()?,
                beginning_of_pass_write_index: Some(begin),
                end_of_pass_write_index: Some(end),
            })
        }
        #[cfg(not(debug_assertions))]
        {
            let _ = label;
            None
        }
    }

    #[inline]
    pub(crate) fn render_timestamp_writes(
        &self,
        label: &str,
    ) -> Option<wgpu::RenderPassTimestampWrites<'_>> {
        #[cfg(debug_assertions)]
        {
            let (begin, end) = self.allocate_queries("render", label)?;
            Some(wgpu::RenderPassTimestampWrites {
                query_set: self.query_set.as_ref()?,
                beginning_of_pass_write_index: Some(begin),
                end_of_pass_write_index: Some(end),
            })
        }
        #[cfg(not(debug_assertions))]
        {
            let _ = label;
            None
        }
    }

    #[inline]
    pub(crate) fn resolve_sample(&self, encoder: &mut wgpu::CommandEncoder) {
        #[cfg(debug_assertions)]
        {
            self.recording.store(false, Ordering::Release);
            let (Some(query_set), Some(resolve_buffer)) =
                (self.query_set.as_ref(), self.resolve_buffer.as_ref())
            else {
                return;
            };
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let Some(slot) = state.active_slot.take() else {
                return;
            };
            let query_count: u32 = state.query_count;
            if query_count == 0 {
                state.readbacks[slot]
                    .status
                    .store(AcceleratorTimingReadback::IDLE, Ordering::Release);
                return;
            }
            let byte_count: u64 = u64::from(query_count) * u64::from(wgpu::QUERY_SIZE);
            encoder.resolve_query_set(query_set, 0..query_count, resolve_buffer, 0);
            encoder.copy_buffer_to_buffer(
                resolve_buffer,
                0,
                &state.readbacks[slot].buffer,
                0,
                byte_count,
            );
            state.readbacks[slot].sample = state.next_sample;
            state.next_sample = state.next_sample.wrapping_add(1);
            state.readbacks[slot].query_count = query_count;
            state.readbacks[slot].records = std::mem::take(&mut state.records);
            state.readbacks[slot]
                .status
                .store(AcceleratorTimingReadback::READY_TO_MAP, Ordering::Release);
        }
        #[cfg(not(debug_assertions))]
        let _ = encoder;
    }

    #[inline]
    pub(crate) fn map_sample(&self) {
        #[cfg(debug_assertions)]
        {
            let pending: Vec<(
                wgpu::Buffer,
                std::sync::Arc<std::sync::atomic::AtomicU8>,
                u64,
            )> = {
                let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
                state
                    .readbacks
                    .iter()
                    .filter_map(|readback| {
                        if readback.status.load(Ordering::Acquire)
                            != AcceleratorTimingReadback::READY_TO_MAP
                        {
                            return None;
                        }
                        readback
                            .status
                            .store(AcceleratorTimingReadback::MAPPING, Ordering::Release);
                        Some((
                            readback.buffer.clone(),
                            readback.status.clone(),
                            u64::from(readback.query_count) * u64::from(wgpu::QUERY_SIZE),
                        ))
                    })
                    .collect()
            };
            for (buffer, status, byte_count) in pending {
                buffer.map_async(wgpu::MapMode::Read, 0..byte_count, move |result| {
                    status.store(
                        if result.is_ok() {
                            AcceleratorTimingReadback::READY
                        } else {
                            AcceleratorTimingReadback::FAILED
                        },
                        Ordering::Release,
                    );
                });
            }
        }
    }

    pub(crate) fn collect(&self) {
        #[cfg(debug_assertions)]
        {
            let ready: Vec<usize> = {
                let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
                state
                    .readbacks
                    .iter()
                    .enumerate()
                    .filter_map(|(index, readback)| {
                        matches!(
                            readback.status.load(Ordering::Acquire),
                            AcceleratorTimingReadback::READY | AcceleratorTimingReadback::FAILED
                        )
                        .then_some(index)
                    })
                    .collect()
            };
            for index in ready {
                self.collect_readback(index);
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn is_available(&self) -> bool {
        self.query_set.is_some()
    }

    #[cfg(test)]
    pub(crate) fn is_idle(&self) -> bool {
        let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        state.active_slot.is_none()
            && state.readbacks.iter().all(|readback| {
                readback.status.load(Ordering::Acquire) == AcceleratorTimingReadback::IDLE
            })
    }

    #[cfg(debug_assertions)]
    fn allocate_queries(&self, kind: &'static str, label: &str) -> Option<(u32, u32)> {
        if !self.recording.load(Ordering::Acquire) {
            return None;
        }
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        if state.query_count + 2 > Self::QUERY_CAPACITY {
            self.recording.store(false, Ordering::Release);
            if !state.capacity_exhausted {
                state.capacity_exhausted = true;
                tracing::warn!(
                    target: "dogwood_gpu",
                    sample = state.next_sample,
                    capacity = Self::QUERY_CAPACITY,
                    "Accelerator timing query capacity exhausted"
                );
            }
            return None;
        }
        let occurrence: u32 = state
            .records
            .iter()
            .filter(|record| record.kind == kind && record.label == label)
            .count() as u32
            + 1;
        let begin: u32 = state.query_count;
        state.query_count += 2;
        state.records.push(AcceleratorTimingRecord {
            kind,
            label: label.to_owned(),
            occurrence,
        });
        Some((begin, begin + 1))
    }

    #[cfg(debug_assertions)]
    fn collect_readback(&self, index: usize) {
        let (sample, records, timestamps, failed) = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let readback: &mut AcceleratorTimingReadback = &mut state.readbacks[index];
            let failed: bool =
                readback.status.load(Ordering::Acquire) == AcceleratorTimingReadback::FAILED;
            let timestamps: Vec<u64> = if failed {
                Vec::new()
            } else {
                let byte_count: u64 = u64::from(readback.query_count) * u64::from(wgpu::QUERY_SIZE);
                let mapped = readback
                    .buffer
                    .get_mapped_range(0..byte_count)
                    .expect("mapped Accelerator timestamp readback must remain accessible");
                let timestamps: Vec<u64> = mapped
                    .chunks_exact(8)
                    .map(|bytes| {
                        u64::from_le_bytes(
                            bytes.try_into().expect("timestamp occupies eight bytes"),
                        )
                    })
                    .collect();
                drop(mapped);
                readback.buffer.unmap();
                timestamps
            };
            let sample: u64 = readback.sample;
            let records: Vec<AcceleratorTimingRecord> = std::mem::take(&mut readback.records);
            readback.query_count = 0;
            readback
                .status
                .store(AcceleratorTimingReadback::IDLE, Ordering::Release);
            (sample, records, timestamps, failed)
        };
        if failed {
            tracing::warn!(target: "dogwood_gpu", sample, "Accelerator timing readback failed");
            return;
        }
        self.report(sample, records, &timestamps);
    }

    #[cfg(debug_assertions)]
    fn report(&self, sample: u64, records: Vec<AcceleratorTimingRecord>, timestamps: &[u64]) {
        let mut compute_passes: u32 = 0;
        let mut render_passes: u32 = 0;
        let mut compute_gpu_ns: u64 = 0;
        let mut render_gpu_ns: u64 = 0;
        for (index, record) in records.into_iter().enumerate() {
            let Some(ticks) = timestamps[index * 2 + 1].checked_sub(timestamps[index * 2]) else {
                tracing::warn!(
                    target: "dogwood_gpu",
                    sample,
                    pass = %record.label,
                    "discarded wrapped Accelerator timestamp"
                );
                continue;
            };
            let gpu_ns: u64 = (ticks as f64 * self.timestamp_period).round() as u64;
            let gpu_ms: f64 = gpu_ns as f64 / 1_000_000.0;
            if record.kind == "compute" {
                compute_passes += 1;
                compute_gpu_ns = compute_gpu_ns.saturating_add(gpu_ns);
            } else {
                render_passes += 1;
                render_gpu_ns = render_gpu_ns.saturating_add(gpu_ns);
            }
            tracing::debug!(
                target: "dogwood_gpu",
                sample,
                kind = record.kind,
                pass = %record.label,
                occurrence = record.occurrence,
                gpu_ns,
                gpu_ms,
                "Accelerator pass"
            );
        }
        let measured_gpu_ns: u64 = compute_gpu_ns.saturating_add(render_gpu_ns);
        tracing::debug!(
            target: "dogwood_gpu",
            sample,
            compute_passes,
            render_passes,
            compute_gpu_ms = compute_gpu_ns as f64 / 1_000_000.0,
            render_gpu_ms = render_gpu_ns as f64 / 1_000_000.0,
            measured_gpu_ms = measured_gpu_ns as f64 / 1_000_000.0,
            "Accelerator measured-pass sample"
        );
    }
}
