// Copyright Rob Gage 2026

use std::sync::mpsc::sync_channel;

use engine_compute::Accelerator;

use super::MaterialReactions;
use crate::simulation::simulation_constants::RIGID_REMOVAL_EVENT_SIZE;
use crate::simulation::simulation_constants::RIGID_REMOVAL_EVENTS_OFFSET;

impl MaterialReactions {
    pub(crate) fn rigid_removal_readback_pending(&self) -> bool {
        self.rigid_removal_readback_result.is_some()
    }

    pub(crate) fn submit_rigid_removal_readback(&mut self, accelerator: &Accelerator) {
        if self.rigid_removal_readback_result.is_some() {
            return;
        }
        let initial_rigid_removal_readback_length: u64 = RIGID_REMOVAL_EVENTS_OFFSET;
        let mut encoder: wgpu::CommandEncoder =
            accelerator
                .wgpu_device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("rigid chemistry removal readback"),
                });
        encoder.copy_buffer_to_buffer(
            self.rigid_removal_count.wgpu_buffer(),
            0,
            &self.rigid_removal_readback,
            0,
            4,
        );
        accelerator.wgpu_queue().submit(Some(encoder.finish()));
        self.rigid_removal_readback_len = initial_rigid_removal_readback_length;
        self.rigid_removal_readback_capacity = 0;
        let (sender, receiver) = sync_channel(1);
        self.rigid_removal_readback
            .slice(0..initial_rigid_removal_readback_length)
            .map_async(wgpu::MapMode::Read, move |readback_result| {
                sender.send(readback_result).ok();
            });
        self.rigid_removal_readback_result = Some(receiver);
    }
    pub(crate) fn take_rigid_removal_events(
        &mut self,
        accelerator: &Accelerator,
    ) -> Option<Vec<[u32; 6]>> {
        let readback_result: Result<(), wgpu::BufferAsyncError> = self
            .rigid_removal_readback_result
            .as_ref()?
            .try_recv()
            .ok()?;
        self.rigid_removal_readback_result = None;
        if readback_result.is_err() {
            return Some(Vec::new());
        }
        let rigid_removal_readback_bytes: wgpu::BufferView = match self
            .rigid_removal_readback
            .slice(0..self.rigid_removal_readback_len)
            .get_mapped_range()
        {
            Ok(bytes) => bytes,
            Err(_) => {
                self.rigid_removal_readback.unmap();
                return Some(Vec::new());
            }
        };
        let rigid_removal_event_count: usize =
            u32::from_le_bytes(rigid_removal_readback_bytes[..4].try_into().ok()?)
                .min(self.cell_count) as usize;
        if self.rigid_removal_readback_capacity == 0 {
            drop(rigid_removal_readback_bytes);
            self.rigid_removal_readback.unmap();
            if rigid_removal_event_count == 0 {
                return Some(Vec::new());
            }
            let event_readback_length: u64 = RIGID_REMOVAL_EVENTS_OFFSET
                + rigid_removal_event_count as u64 * RIGID_REMOVAL_EVENT_SIZE;
            let mut encoder: wgpu::CommandEncoder = accelerator
                .wgpu_device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("rigid chemistry event readback"),
                });
            encoder.copy_buffer_to_buffer(
                self.rigid_removal_events.wgpu_buffer(),
                0,
                &self.rigid_removal_readback,
                RIGID_REMOVAL_EVENTS_OFFSET,
                rigid_removal_event_count as u64 * RIGID_REMOVAL_EVENT_SIZE,
            );
            accelerator.wgpu_queue().submit(Some(encoder.finish()));
            self.rigid_removal_readback_len = event_readback_length;
            self.rigid_removal_readback_capacity = rigid_removal_event_count as u32;
            let (sender, receiver) = sync_channel(1);
            self.rigid_removal_readback
                .slice(0..event_readback_length)
                .map_async(wgpu::MapMode::Read, move |readback_result| {
                    sender.send(readback_result).ok();
                });
            self.rigid_removal_readback_result = Some(receiver);
            return None;
        }
        let events: Vec<[u32; 6]> = rigid_removal_readback_bytes
            [usize::try_from(RIGID_REMOVAL_EVENTS_OFFSET).unwrap()..]
            .as_chunks::<32>()
            .0
            .iter()
            .take(rigid_removal_event_count)
            .map(|event_bytes| {
                let mut rigid_removal_event: [u32; 6] = [0; 6];
                for (event_word, event_word_bytes) in rigid_removal_event
                    .iter_mut()
                    .zip(event_bytes.as_chunks::<4>().0)
                {
                    *event_word = u32::from_le_bytes(*event_word_bytes);
                }
                rigid_removal_event
            })
            .collect();
        drop(rigid_removal_readback_bytes);
        self.rigid_removal_readback.unmap();
        Some(events)
    }
}
