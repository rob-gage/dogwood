// Copyright Rob Gage 2026

use std::sync::{Mutex, MutexGuard, OnceLock};

static ACCELERATOR_TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

pub(crate) fn acquire_accelerator_test_lock() -> MutexGuard<'static, ()> {
    ACCELERATOR_TEST_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
