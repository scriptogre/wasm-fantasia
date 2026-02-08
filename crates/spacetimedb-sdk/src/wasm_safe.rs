//! WASM-safe mutex locking.
//!
//! On WASM with atomics, `Mutex::lock()` uses `Atomics.wait`, which is
//! forbidden on the browser's main thread. Since WASM is single-threaded,
//! the lock is never contended, so `try_lock()` always succeeds.

use std::sync::{Mutex, MutexGuard};

pub(crate) trait MutexExt<T> {
    fn wasm_lock(&self) -> MutexGuard<'_, T>;
}

impl<T> MutexExt<T> for Mutex<T> {
    #[inline]
    fn wasm_lock(&self) -> MutexGuard<'_, T> {
        #[cfg(not(feature = "web"))]
        {
            self.lock().unwrap()
        }
        #[cfg(feature = "web")]
        {
            self.try_lock()
                .expect("unexpected mutex contention on WASM main thread")
        }
    }
}
