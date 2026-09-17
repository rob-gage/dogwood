// Copyright Rob Gage 2026

use super::*;

impl MaterialReactions {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn encode(&self, accelerator: &Accelerator, encoder: &mut wgpu::CommandEncoder) {
        if self.reaction_count == 0 {
            return;
        }
        let mut pass = accelerator.begin_compute_pass(encoder, "chemistry clear");
        pass.set_pipeline(&self.clear_pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.dispatch_workgroups(self.clear_count.div_ceil(64), 1, 1);
        drop(pass);
        let mut pass = accelerator.begin_compute_pass(encoder, "chemistry discovery");
        pass.set_pipeline(&self.discover_pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.dispatch_workgroups(self.cell_count.div_ceil(64), 1, 1);
        drop(pass);
        let mut pass = accelerator.begin_compute_pass(encoder, "chemistry candidate compaction");
        pass.set_pipeline(&self.compact_pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.dispatch_workgroups(self.cell_count.div_ceil(64), 1, 1);
        drop(pass);
        let mut pass =
            accelerator.begin_compute_pass(encoder, "chemistry sort dispatch preparation");
        pass.set_pipeline(&self.prepare_sort_pipeline);
        pass.set_bind_group(0, &self.prepare_sort_bind_group, &[]);
        pass.dispatch_workgroups(1, 1, 1);
        drop(pass);
        for step in 0..self.sort_step_count {
            encoder.copy_buffer_to_buffer(
                self.sort_steps.wgpu_buffer(),
                step as u64 * 8,
                &self.sort_parameters,
                0,
                8,
            );
            let mut pass = accelerator.begin_compute_pass(encoder, "chemistry candidate sort");
            pass.set_pipeline(&self.sort_pipeline);
            pass.set_bind_group(0, &self.sort_bind_group, &[]);
            pass.dispatch_workgroups_indirect(&self.sort_indirect, 0);
            drop(pass);
        }
        let mut pass = accelerator.begin_compute_pass(encoder, "chemistry arbitration");
        pass.set_pipeline(&self.reserve_pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        // One invocation performs the ordered arbitration. It is intentionally
        // serialized: reservation order is a correctness rule, not a race.
        pass.dispatch_workgroups(1, 1, 1);
        drop(pass);
        let mut pass = accelerator.begin_compute_pass(encoder, "chemistry apply");
        pass.set_pipeline(&self.apply_pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.dispatch_workgroups(self.cell_count.div_ceil(64), 1, 1);
    }
}
