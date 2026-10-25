// Copyright Rob Gage 2026

use super::{Accelerator, AcceleratorReadbackError};
use std::sync::mpsc::{channel, Receiver, Sender};

/// A ring of asynchronous GPU readback buffers
///
/// A per-tick readback copied during tick N is read during tick N+1. Reading waits only for that
/// copy's own submission if it has not finished, never for later work, so feedback is exactly one
/// tick late and never later.
pub struct AcceleratorReadback<T> {
    /// Buffers and their current readback state
    slots: Vec<AcceleratorReadbackSlot<T>>,
    /// Sender for asynchronous mapping results
    response_sender: Sender<(usize, Result<(), wgpu::BufferAsyncError>)>,
    /// Receiver for asynchronous mapping results
    response_receiver: Receiver<(usize, Result<(), wgpu::BufferAsyncError>)>,
    /// Sequence assigned to the next copied slot
    next_sequence: u64,
}

/// One readback buffer and its state
struct AcceleratorReadbackSlot<T> {
    /// GPU buffer used for the readback
    buffer: wgpu::Buffer,
    /// Current copy or mapping metadata
    state: AcceleratorReadbackSlotState<T>,
}

/// State of one readback buffer
enum AcceleratorReadbackSlotState<T> {
    /// The buffer is available for another copy
    Free,
    /// The buffer has a copy recorded in an encoder
    Copied { metadata: T, size: u64 },
    /// The buffer is being mapped after a queue submission
    Mapping {
        metadata: T,
        size: u64,
        sequence: u64,
        submission_index: wgpu::SubmissionIndex,
        ready: bool,
    },
}

impl<T> AcceleratorReadback<T> {

    /// Creates a ring of fixed-size mappable readback buffers
    pub fn new(accelerator: &Accelerator, label: &'static str, size: u64, count: usize) -> Self {
        let (response_sender, response_receiver): (
            Sender<(usize, Result<(), wgpu::BufferAsyncError>)>,
            Receiver<(usize, Result<(), wgpu::BufferAsyncError>)>,
        ) = channel();
        let slots: Vec<AcceleratorReadbackSlot<T>> = (0..count).map(|_| AcceleratorReadbackSlot {
            buffer: accelerator.device().create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            state: AcceleratorReadbackSlotState::Free,
        }).collect();
        Self { slots, response_sender, response_receiver, next_sequence: 0 }
    }

    /// Records a copy into a free slot, returning false when every slot is busy
    pub fn copy(
        &mut self,
        command_encoder: &mut wgpu::CommandEncoder,
        source: &wgpu::Buffer,
        source_offset: u64,
        size: u64,
        metadata: T,
    ) -> bool {
        let Some(slot): Option<usize> = self.slots.iter()
            .position(|slot| matches!(slot.state, AcceleratorReadbackSlotState::Free))
        else { return false; };
        command_encoder.copy_buffer_to_buffer(source, source_offset, &self.slots[slot].buffer, 0,
            size);
        self.slots[slot].state = AcceleratorReadbackSlotState::Copied { metadata, size };
        true
    }

    /// Starts mapping every copied slot after the submission containing its copy
    pub fn map(&mut self, submission_index: wgpu::SubmissionIndex) {
        for (slot, readback) in self.slots.iter_mut().enumerate() {
            // A submission may contain no new copy for a still-mapping slot. Keep its
            // ownership until read() drops the mapped view and unmaps the buffer.
            if !matches!(readback.state, AcceleratorReadbackSlotState::Copied { .. }) {
                continue;
            }
            let AcceleratorReadbackSlotState::Copied { metadata, size } =
                std::mem::replace(&mut readback.state, AcceleratorReadbackSlotState::Free)
            else { continue; };
            readback.state = AcceleratorReadbackSlotState::Mapping {
                metadata,
                size,
                sequence: self.next_sequence,
                submission_index: submission_index.clone(),
                ready: false,
            };
            self.next_sequence += 1;
            let response_sender: Sender<(usize, Result<(), wgpu::BufferAsyncError>)> =
                self.response_sender.clone();
            readback.buffer.slice(..size).map_async(
                wgpu::MapMode::Read,
                move |result| { let _ = response_sender.send((slot, result)); },
            );
        }
    }

    /// Visits every completed readback in submission order and frees its slot, first waiting for
    /// the newest mapping submission when waiting
    pub fn read(
        &mut self,
        accelerator: &Accelerator,
        is_waiting: bool,
        mut visit: impl FnMut(T, &[u8]),
    ) -> Result<(), AcceleratorReadbackError> {
        let newest_submission: Option<wgpu::SubmissionIndex> = self.slots.iter()
            .filter_map(|slot| match &slot.state {
                AcceleratorReadbackSlotState::Mapping { sequence, submission_index, .. } =>
                    Some((*sequence, submission_index.clone())),
                AcceleratorReadbackSlotState::Free | AcceleratorReadbackSlotState::Copied { .. } => None,
            })
            .max_by_key(|(sequence, _)| *sequence)
            .map(|(_, submission_index)| submission_index);
        let poll_type: wgpu::PollType = match newest_submission.filter(|_| is_waiting) {
            Some(submission_index) => wgpu::PollType::Wait {
                submission_index: Some(submission_index),
                timeout: None,
            },
            None => wgpu::PollType::Poll,
        };
        accelerator.device().poll(poll_type).map_err(AcceleratorReadbackError::Poll)?;
        let mut first_error: Option<AcceleratorReadbackError> = None;
        while let Ok((slot, result)) = self.response_receiver.try_recv() {
            let AcceleratorReadbackSlotState::Mapping { ready, .. } = &mut self.slots[slot].state else {
                continue;
            };
            match result {
                Ok(()) => *ready = true,
                Err(error) => {
                    self.slots[slot].state = AcceleratorReadbackSlotState::Free;
                    first_error.get_or_insert(AcceleratorReadbackError::BufferAsync(error));
                }
            }
        }
        let mut ready_slots: Vec<(u64, usize)> = self.slots.iter().enumerate()
            .filter_map(|(slot, readback)| match &readback.state {
                AcceleratorReadbackSlotState::Mapping { sequence, ready: true, .. } => Some((*sequence, slot)),
                AcceleratorReadbackSlotState::Free | AcceleratorReadbackSlotState::Copied { .. }
                | AcceleratorReadbackSlotState::Mapping { ready: false, .. } => None,
            })
            .collect();
        ready_slots.sort_by_key(|(sequence, _)| *sequence);
        for (_, slot) in ready_slots {
            let AcceleratorReadbackSlotState::Mapping { metadata, size, .. } =
                std::mem::replace(&mut self.slots[slot].state, AcceleratorReadbackSlotState::Free)
            else { continue; };
            {
                let mapped_bytes: wgpu::BufferView = self.slots[slot].buffer
                    .get_mapped_range(..size)
                    .expect("mapped accelerator readback should be readable");
                visit(metadata, &mapped_bytes);
            }
            self.slots[slot].buffer.unmap();
        }
        match first_error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

}
