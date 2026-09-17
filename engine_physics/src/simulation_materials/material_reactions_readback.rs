// Copyright Rob Gage 2026

use super::*;

impl MaterialReactions {
    pub(crate) fn submit_rigid_removal_readback(&mut self, accelerator: &Accelerator) {
        if self.rigid_removal_readback_result.is_some() {
            return;
        }
        let len = RIGID_REMOVAL_EVENTS_OFFSET;
        let mut encoder =
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
        self.rigid_removal_readback_len = len;
        self.rigid_removal_readback_capacity = 0;
        let (sender, receiver) = sync_channel(1);
        self.rigid_removal_readback
            .slice(0..len)
            .map_async(wgpu::MapMode::Read, move |result| {
                let _ = sender.send(result);
            });
        self.rigid_removal_readback_result = Some(receiver);
    }
    pub(crate) fn take_rigid_removal_events(
        &mut self,
        accelerator: &Accelerator,
    ) -> Option<Vec<[u32; 6]>> {
        let result = self
            .rigid_removal_readback_result
            .as_ref()?
            .try_recv()
            .ok()?;
        self.rigid_removal_readback_result = None;
        if result.is_err() {
            self.rigid_removal_readback.unmap();
            return Some(Vec::new());
        }
        let bytes = self
            .rigid_removal_readback
            .slice(0..self.rigid_removal_readback_len)
            .get_mapped_range()
            .ok()?;
        let count = u32::from_le_bytes(bytes[..4].try_into().ok()?).min(self.cell_count) as usize;
        if self.rigid_removal_readback_capacity == 0 {
            drop(bytes);
            self.rigid_removal_readback.unmap();
            if count == 0 {
                return Some(Vec::new());
            }
            let len = RIGID_REMOVAL_EVENTS_OFFSET + count as u64 * RIGID_REMOVAL_EVENT_SIZE;
            let mut encoder =
                accelerator
                    .wgpu_device()
                    .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                        label: Some("rigid chemistry event readback"),
                    });
            encoder.copy_buffer_to_buffer(
                self.rigid_removal_events.wgpu_buffer(),
                0,
                &self.rigid_removal_readback,
                RIGID_REMOVAL_EVENTS_OFFSET,
                count as u64 * RIGID_REMOVAL_EVENT_SIZE,
            );
            accelerator.wgpu_queue().submit(Some(encoder.finish()));
            self.rigid_removal_readback_len = len;
            self.rigid_removal_readback_capacity = count as u32;
            let (sender, receiver) = sync_channel(1);
            self.rigid_removal_readback.slice(0..len).map_async(
                wgpu::MapMode::Read,
                move |result| {
                    let _ = sender.send(result);
                },
            );
            self.rigid_removal_readback_result = Some(receiver);
            return None;
        }
        let events = bytes[usize::try_from(RIGID_REMOVAL_EVENTS_OFFSET).unwrap()..]
            .as_chunks::<32>()
            .0
            .iter()
            .take(count)
            .map(|b| {
                let mut event = [0; 6];
                for (word, value) in event.iter_mut().zip(b.as_chunks::<4>().0) {
                    *word = u32::from_le_bytes(*value);
                }
                event
            })
            .collect();
        drop(bytes);
        self.rigid_removal_readback.unmap();
        Some(events)
    }
}
