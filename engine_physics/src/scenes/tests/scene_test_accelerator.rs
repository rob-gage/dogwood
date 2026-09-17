use engine_compute::Accelerator;
use std::sync::{Arc, MutexGuard};

pub(crate) fn new_scene_test_accelerator() -> (MutexGuard<'static, ()>, Arc<Accelerator>) {
    let (accelerator_test_lock, accelerator) = crate::simulation::tests::new_accelerator_test();
    (accelerator_test_lock, Arc::new(accelerator))
}
