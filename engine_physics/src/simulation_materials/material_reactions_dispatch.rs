// Copyright Rob Gage 2026

use engine_compute::Accelerator;

use super::MaterialReactions;

impl MaterialReactions {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn encode(&self, accelerator: &Accelerator, encoder: &mut wgpu::CommandEncoder) {
        if self.reaction_count == 0 {
            return;
        }
        let mut chemistry_compute_pass: wgpu::ComputePass<'_> =
            accelerator.begin_compute_pass(encoder, "chemistry clear");
        chemistry_compute_pass.set_pipeline(&self.clear_pipeline);
        chemistry_compute_pass.set_bind_group(0, &self.bind_group, &[]);
        chemistry_compute_pass.dispatch_workgroups(self.clear_count.div_ceil(64), 1, 1);
        drop(chemistry_compute_pass);
        let mut chemistry_compute_pass: wgpu::ComputePass<'_> =
            accelerator.begin_compute_pass(encoder, "chemistry discovery");
        chemistry_compute_pass.set_pipeline(&self.discover_pipeline);
        chemistry_compute_pass.set_bind_group(0, &self.bind_group, &[]);
        chemistry_compute_pass.dispatch_workgroups(self.cell_count.div_ceil(64), 1, 1);
        drop(chemistry_compute_pass);
        let mut chemistry_compute_pass: wgpu::ComputePass<'_> =
            accelerator.begin_compute_pass(encoder, "chemistry candidate compaction");
        chemistry_compute_pass.set_pipeline(&self.compact_pipeline);
        chemistry_compute_pass.set_bind_group(0, &self.bind_group, &[]);
        chemistry_compute_pass.dispatch_workgroups(self.cell_count.div_ceil(64), 1, 1);
        drop(chemistry_compute_pass);
        let mut chemistry_compute_pass: wgpu::ComputePass<'_> =
            accelerator.begin_compute_pass(encoder, "chemistry sort dispatch preparation");
        chemistry_compute_pass.set_pipeline(&self.prepare_sort_pipeline);
        chemistry_compute_pass.set_bind_group(0, &self.prepare_sort_bind_group, &[]);
        chemistry_compute_pass.dispatch_workgroups(1, 1, 1);
        drop(chemistry_compute_pass);
        for step in 0..self.sort_step_count {
            encoder.copy_buffer_to_buffer(
                self.sort_steps.wgpu_buffer(),
                step as u64 * 8,
                &self.sort_parameters,
                0,
                8,
            );
            let mut chemistry_compute_pass: wgpu::ComputePass<'_> =
                accelerator.begin_compute_pass(encoder, "chemistry candidate sort");
            chemistry_compute_pass.set_pipeline(&self.sort_pipeline);
            chemistry_compute_pass.set_bind_group(0, &self.sort_bind_group, &[]);
            chemistry_compute_pass.dispatch_workgroups_indirect(&self.sort_indirect, 0);
            drop(chemistry_compute_pass);
        }
        let mut chemistry_compute_pass: wgpu::ComputePass<'_> =
            accelerator.begin_compute_pass(encoder, "chemistry arbitration");
        chemistry_compute_pass.set_pipeline(&self.reserve_pipeline);
        chemistry_compute_pass.set_bind_group(0, &self.bind_group, &[]);
        // one invocation performs the ordered arbitration. It is intentionally
        // serialized: reservation order is a correctness rule, not a race.
        chemistry_compute_pass.dispatch_workgroups(1, 1, 1);
        drop(chemistry_compute_pass);
        let mut chemistry_compute_pass: wgpu::ComputePass<'_> =
            accelerator.begin_compute_pass(encoder, "chemistry apply");
        chemistry_compute_pass.set_pipeline(&self.apply_pipeline);
        chemistry_compute_pass.set_bind_group(0, &self.bind_group, &[]);
        chemistry_compute_pass.dispatch_workgroups(self.cell_count.div_ceil(64), 1, 1);
    }
}
