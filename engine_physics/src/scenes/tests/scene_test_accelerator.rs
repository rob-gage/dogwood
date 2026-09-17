use engine_compute::Accelerator;
use std::sync::{Arc, MutexGuard};

pub(crate) fn new_scene_test_accelerator() -> (MutexGuard<'static, ()>, Arc<Accelerator>) {
    let accelerator_test_lock = crate::simulation::tests::acquire_accelerator_test_lock();
    let accelerator: Arc<Accelerator> = Arc::new(Accelerator::new().unwrap());
    (accelerator_test_lock, accelerator)
}
