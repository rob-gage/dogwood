// Copyright Rob Gage 2026

use std::sync::Mutex;
use std::sync::MutexGuard;
use std::sync::OnceLock;

use engine_compute::Accelerator;

static ACCELERATOR_TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

pub(crate) fn acquire_accelerator_test_lock() -> MutexGuard<'static, ()> {
    ACCELERATOR_TEST_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub(crate) fn new_accelerator_test() -> (MutexGuard<'static, ()>, Accelerator) {
    let accelerator_test_lock: MutexGuard<'static, ()> = acquire_accelerator_test_lock();
    let accelerator: Accelerator = Accelerator::new().unwrap();
    (accelerator_test_lock, accelerator)
}
