// Copyright Rob Gage 2026

use std::io;
use std::sync::MutexGuard;

use super::CellularPressure;
use crate::simulation::RigidGranularReactionBatch;
use crate::simulation::RigidGranularReadbackStatus;

impl CellularPressure {
    /// Collects completed mappings and returns every now-contiguous ordered batch.
    pub(crate) fn collect_rigid_reactions(
        &mut self,
    ) -> Result<Vec<RigidGranularReactionBatch>, io::Error> {
        for readback_slot in &self.rigid_reaction_readback_slots {
            let mut readback_status: MutexGuard<'_, RigidGranularReadbackStatus> = readback_slot
                .status
                .lock()
                .map_err(|_| io::Error::other("Rigid granular readback state is unavailable"))?;
            if matches!(*readback_status, RigidGranularReadbackStatus::Complete(_)) {
                let RigidGranularReadbackStatus::Complete(result) = std::mem::replace(
                    &mut *readback_status,
                    RigidGranularReadbackStatus::Available,
                ) else {
                    unreachable!()
                };
                let reaction_batch: RigidGranularReactionBatch =
                    result.map_err(io::Error::other)?;
                self.rigid_reaction_completed
                    .insert(reaction_batch.sequence, reaction_batch);
            }
        }
        self.rigid_reaction_completed.retain(|_, reaction_batch| {
            reaction_batch.topology_revision == self.rigid_topology_revision
        });
        let mut ordered_reaction_batches: Vec<RigidGranularReactionBatch> = Vec::new();
        while let Some(reaction_batch) = self
            .rigid_reaction_completed
            .remove(&self.rigid_reaction_sequence_apply_next)
        {
            ordered_reaction_batches.push(reaction_batch);
            self.rigid_reaction_sequence_apply_next =
                self.rigid_reaction_sequence_apply_next.wrapping_add(1);
        }
        Ok(ordered_reaction_batches)
    }
}
