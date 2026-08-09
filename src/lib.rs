//! Fail-fast handling for poisoned [`std::sync`] locks.
//!
//! When a thread panics while holding a [`Mutex`] or [`RwLock`] guard, the
//! lock is poisoned and every subsequent `lock`/`read`/`write` returns a
//! [`PoisonError`]. [`LockExt::or_panic`] turns that error into a panic.
//!
//! # Examples
//!
//! ```
//! use std::sync::{Arc, Mutex};
//! use poisoned::LockExt;
//!
//! let shared = Arc::new(Mutex::new(0_i32));
//! *shared.lock().or_panic() += 1;
//! ```
//!
//! A custom message, built lazily:
//!
//! ```
//! use std::sync::Mutex;
//! use poisoned::LockExt;
//!
//! let name = "config".to_string();
//! let cache = Mutex::new(vec![1, 2, 3]);
//! let first = cache.lock().or_panic_with(|_| format!("cache lock poisoned for {name}"));
//! assert_eq!(first[0], 1);
//! ```
//!
//! [`Mutex`]: std::sync::Mutex
//! [`RwLock`]: std::sync::RwLock

#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![warn(clippy::all, clippy::pedantic)]

use std::fmt::Display;
use std::sync::PoisonError;

/// The default panic message used by [`LockExt::or_panic`].
const DEFAULT_PANIC_MESSAGE: &str =
    "lock is poisoned: a previous holder panicked while holding the guard";

mod private {
    /// Restricts [`LockExt`](crate::LockExt) to the blanket implementation in
    /// this crate so downstream crates cannot implement it for their own types.
    #[doc(hidden)]
    pub trait Sealed {}
}

impl<T> private::Sealed for Result<T, PoisonError<T>> {}

/// Extension trait that converts a poisoned lock result into a panic.
///
/// Implemented for every `Result<T, PoisonError<T>>`, which is what the
/// fallible methods of [`Mutex`] and [`RwLock`] return: `lock`, `read`,
/// `write`, `get_mut`, and `into_inner`.
///
/// The `try_*` methods are not covered: they return a [`TryLockError`] whose
/// `WouldBlock` variant is not a poison condition.
///
/// Sealed: cannot be implemented outside this crate.
///
/// [`Mutex`]: std::sync::Mutex
/// [`RwLock`]: std::sync::RwLock
/// [`TryLockError`]: std::sync::TryLockError
pub trait LockExt<T>: private::Sealed {
    /// Returns the inner value, or panics if the lock is poisoned.
    ///
    /// # Panics
    ///
    /// Panics if the lock is poisoned.
    #[track_caller]
    #[must_use]
    fn or_panic(self) -> T;

    /// Like [`or_panic`](Self::or_panic), with a custom message built from
    /// the [`PoisonError`].
    ///
    /// `message` receives the [`PoisonError`], so it can inspect the recovered
    /// data via [`PoisonError::get_ref`]. It is evaluated only if the lock is
    /// poisoned.
    ///
    /// # Panics
    ///
    /// Panics with `message(&error)` if the lock is poisoned.
    #[track_caller]
    #[must_use]
    fn or_panic_with<M: Display>(self, message: impl FnOnce(&PoisonError<T>) -> M) -> T;

    /// Returns the inner value, repairing it first if the lock is poisoned.
    ///
    /// On a poisoned lock, the value is recovered via [`PoisonError::into_inner`]
    /// and `repair` is called on it before it is returned, so the caller can
    /// restore it to a consistent state. A healthy lock is returned as-is and
    /// `repair` is not called.
    ///
    /// Recovery is per-acquisition: the lock stays poisoned and later calls
    /// still return `Err`. A `lock()` guard derefs to its
    /// [`Mutex`](std::sync::Mutex), so `repair` can clear the flag with
    /// [`Mutex::clear_poison`](std::sync::Mutex::clear_poison) to resume
    /// normal use.
    #[track_caller]
    #[must_use]
    fn or_recover(self, repair: impl FnOnce(&mut T)) -> T;
}

impl<T> LockExt<T> for Result<T, PoisonError<T>> {
    #[track_caller]
    #[inline]
    fn or_panic(self) -> T {
        self.or_panic_with(|_| DEFAULT_PANIC_MESSAGE)
    }

    #[track_caller]
    #[inline]
    fn or_panic_with<M: Display>(self, message: impl FnOnce(&PoisonError<T>) -> M) -> T {
        match self {
            Ok(value) => value,
            Err(error) => {
                let message = message(&error);
                panic!("{message}");
            }
        }
    }

    #[track_caller]
    #[inline]
    fn or_recover(self, repair: impl FnOnce(&mut T)) -> T {
        match self {
            Ok(value) => value,
            Err(error) => {
                let mut value = error.into_inner();
                repair(&mut value);
                value
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::panic::catch_unwind;
    use std::sync::{Arc, Mutex, PoisonError, RwLock};

    use super::*;

    fn poison(mutex: &Mutex<i32>) {
        let _ = catch_unwind(|| {
            let _guard = mutex.lock().unwrap();
            panic!("poison it");
        });
    }

    #[test]
    fn returns_value_when_not_poisoned() {
        let mutex = Mutex::new(7);
        let mut guard = mutex.lock().or_panic();
        *guard += 1;
        assert_eq!(*guard, 8);
    }

    #[test]
    fn works_for_rwlock() {
        let lock = RwLock::new(5);

        let read = lock.read().or_panic();
        assert_eq!(*read, 5);
        drop(read);

        let mut write = lock.write().or_panic();
        *write += 1;
        drop(write);

        assert_eq!(*lock.read().or_panic(), 6);
    }

    #[test]
    fn works_for_into_inner() {
        let mutex = Mutex::new("hello");
        let value = mutex.into_inner().or_panic();
        assert_eq!(value, "hello");
    }

    #[test]
    fn panics_on_poison() {
        let mutex = Mutex::new(7);
        poison(&mutex);

        let payload = catch_unwind(|| mutex.lock().or_panic()).expect_err("should panic");
        let message = payload
            .downcast_ref::<String>()
            .expect("panic payload should be a String");
        assert_eq!(message, DEFAULT_PANIC_MESSAGE);
    }

    #[test]
    fn uses_custom_message() {
        let mutex = Mutex::new(0);
        poison(&mutex);

        let payload = catch_unwind(|| {
            mutex
                .lock()
                .or_panic_with(|_| "custom fail-fast".to_owned())
        })
        .expect_err("should panic");
        let message = payload
            .downcast_ref::<String>()
            .expect("panic payload should be a String");
        assert_eq!(message, "custom fail-fast");
    }

    #[test]
    fn message_can_read_data_from_error() {
        let mutex = Mutex::new(7);
        poison(&mutex);

        let payload = catch_unwind(|| {
            mutex
                .lock()
                .or_panic_with(|error| format!("data was {}", **error.get_ref()))
        })
        .expect_err("should panic");
        let message = payload
            .downcast_ref::<String>()
            .expect("panic payload should be a String");
        assert_eq!(message, "data was 7");
    }

    #[test]
    fn message_is_built_lazily() {
        let mutex = Mutex::new(0);

        let guard = mutex.lock().or_panic_with(|_| {
            panic!("message closure must not run when the lock is healthy");
        });
        assert_eq!(*guard, 0);
    }

    #[test]
    fn or_recover_skips_repair_when_healthy() {
        let mutex = Mutex::new(7);

        let guard = mutex.lock().or_recover(|_| {
            panic!("repair must not run when the lock is healthy");
        });
        assert_eq!(*guard, 7);
    }

    #[test]
    fn or_recover_repairs_poisoned_guard() {
        let mutex = Mutex::new(7);
        poison(&mutex);

        let guard = mutex.lock().or_recover(|guard| **guard = 0);
        assert_eq!(*guard, 0);
    }

    #[test]
    fn or_recover_repairs_poisoned_value() {
        let mutex = Mutex::new(7);
        poison(&mutex);

        let value = mutex.into_inner().or_recover(|value| *value = 9);
        assert_eq!(value, 9);
    }

    #[test]
    fn or_recover_repairs_poisoned_rwlock() {
        let lock = RwLock::new(vec![1, 2, 3]);
        let _ = catch_unwind(|| {
            let _guard = lock.write().unwrap();
            panic!("poison it");
        });
        assert!(lock.is_poisoned());

        let guard = lock.write().or_recover(|guard| guard.clear());
        assert!(guard.is_empty());
    }

    #[test]
    fn or_recover_keeps_lock_poisoned() {
        let mutex = Mutex::new(7);
        poison(&mutex);

        let guard = mutex.lock().or_recover(|guard| **guard = 0);
        assert_eq!(*guard, 0);
        drop(guard);

        assert!(mutex.is_poisoned());
        assert!(mutex.lock().is_err());
    }

    #[test]
    fn panics_during_unwind() {
        struct LockOnDrop(Arc<Mutex<i32>>);
        impl Drop for LockOnDrop {
            fn drop(&mut self) {
                let caught = catch_unwind(|| {
                    let _guard = self.0.lock().or_panic();
                });
                assert!(caught.is_err(), "or_panic must panic during unwinding");
            }
        }

        let shared = Arc::new(Mutex::new(7));

        let shared_2 = Arc::clone(&shared);
        let _ = catch_unwind(move || {
            let _guard = shared_2.lock().unwrap();
            panic!("poison it");
        });
        assert!(shared.is_poisoned());

        let accessor = LockOnDrop(Arc::clone(&shared));
        let result = catch_unwind(move || {
            let _held = accessor;
            panic!("unwind here");
        });

        assert!(result.is_err());
        assert_eq!(*shared.lock().unwrap_or_else(PoisonError::into_inner), 7);
    }

    #[test]
    fn or_panic_during_unwind_aborts() {
        const HELPER_ENV: &str = "POISONED_DOUBLE_PANIC_HELPER";
        const CHILD_ENV: &str = "POISONED_DOUBLE_PANIC_CHILD";

        if std::env::var_os(HELPER_ENV).is_some() && std::env::var_os(CHILD_ENV).is_some() {
            struct LockOnDrop(Arc<Mutex<i32>>);
            impl Drop for LockOnDrop {
                fn drop(&mut self) {
                    let _guard = self.0.lock().or_panic();
                }
            }

            let shared = Arc::new(Mutex::new(7));
            let shared_2 = Arc::clone(&shared);
            let _ = catch_unwind(move || {
                let _guard = shared_2.lock().unwrap();
                panic!("poison it");
            });
            assert!(shared.is_poisoned());

            let accessor = LockOnDrop(Arc::clone(&shared));
            let _ = catch_unwind(move || {
                let _held = accessor;
                panic!("unwind here");
            });

            // The `or_panic` in `Drop` runs while unwinding and must abort the
            // process before this point. Exiting normally means it did not.
            std::process::exit(0);
        }

        let output = std::process::Command::new(std::env::current_exe().expect("test binary"))
            .arg("--exact")
            .arg("tests::or_panic_during_unwind_aborts")
            .env(HELPER_ENV, "1")
            .env(CHILD_ENV, "1")
            .output()
            .expect("run double-panic helper");

        // The helper exits 0 only if it survived without aborting, and the only
        // non-success exit path is the double-panic abort, so this holds on
        // both Unix (signal death) and Windows (error exit code).
        assert!(
            !output.status.success(),
            "or_panic in a Drop while unwinding must abort the process, got {:?}",
            output.status
        );
    }
}
