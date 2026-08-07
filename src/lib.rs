//! Fail-fast handling for poisoned standard-library locks.
//!
//! The [`LockExt`] extension trait converts a [`PoisonError`] from the
//! locking methods of [`std::sync::Mutex`] and [`std::sync::RwLock`] into a
//! panic. A poisoned lock means a previous holder panicked mid critical
//! section, so the guarded data may be in an inconsistent state — in most
//! programs that is a hard failure you want to surface immediately rather
//! than limp along on.
//!
//! # Why not just `.unwrap()`?
//!
//! `Result::unwrap` on a poisoned lock also panics, but:
//!
//! - It is indistinguishable from the many unrelated `.unwrap()` calls in a
//!   codebase. `or_panic()` makes the fail-fast intent explicit and greppable.
//! - It ignores [`std::thread::panicking()`]. If a poisoned lock is locked
//!   from a [`Drop`] implementation while the stack is already unwinding,
//!   `.unwrap()` triggers a *double panic*, which aborts the process.
//!   `or_panic()` detects that case and recovers the guard via
//!   [`PoisonError::into_inner`] instead.
//!
//! # Relationship to upstream Rust
//!
//! Tracking issue
//! [rust-lang/rust#149359](https://github.com/rust-lang/rust/issues/149359)
//! proposes making `std::sync` locks panic on poison by default (and adding
//! non-poisoning variants), possibly at the 2027 edition boundary. Until that
//! lands, this crate is the explicit fail-fast shim.
//!
//! # Examples
//!
//! ```
//! use std::sync::{Arc, Mutex};
//! use lock_ext::LockExt;
//!
//! let shared = Arc::new(Mutex::new(0_i32));
//! *shared.lock().or_panic() += 1;
//! ```
//!
//! A custom message, built lazily:
//!
//! ```
//! use std::sync::Mutex;
//! use lock_ext::LockExt;
//!
//! let name = "config".to_string();
//! let cache = Mutex::new(vec![1, 2, 3]);
//! let first = cache.lock().or_panic_with(|| format!("cache lock poisoned for {name}"));
//! assert_eq!(first[0], 1);
//! ```

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

/// Extension trait for fail-fast handling of poisoned locks.
///
/// Implemented for every `Result<T, PoisonError<T>>`, which is what
/// [`Mutex::lock`](std::sync::Mutex::lock),
/// [`Mutex::into_inner`](std::sync::Mutex::into_inner),
/// [`RwLock::read`](std::sync::RwLock::read) and
/// [`RwLock::write`](std::sync::RwLock::write) return on error.
///
/// This trait is *sealed*: it cannot be implemented outside this crate.
pub trait LockExt<T>: private::Sealed {
    /// Return the inner value, or panic if the lock is poisoned.
    ///
    /// # Panics
    ///
    /// Panics if the lock is poisoned and the current thread is not already
    /// panicking. While unwinding, the guard is recovered instead, so the
    /// panic is not turned into a process-aborting double panic.
    #[track_caller]
    #[must_use]
    fn or_panic(self) -> T;

    /// Like [`or_panic`](Self::or_panic), with a custom, lazily built message.
    ///
    /// # Panics
    ///
    /// Panics with `message()` if the lock is poisoned and the current thread
    /// is not already panicking. While unwinding, the guard is recovered
    /// instead, so the panic is not turned into a process-aborting double
    /// panic.
    #[track_caller]
    #[must_use]
    fn or_panic_with<M: Display>(self, message: impl FnOnce() -> M) -> T;
}

impl<T> LockExt<T> for Result<T, PoisonError<T>> {
    #[track_caller]
    #[inline]
    fn or_panic(self) -> T {
        self.or_panic_with(|| DEFAULT_PANIC_MESSAGE)
    }

    #[track_caller]
    #[inline]
    fn or_panic_with<M: Display>(self, message: impl FnOnce() -> M) -> T {
        match self {
            Ok(value) => value,
            Err(poisoned) => {
                if std::thread::panicking() {
                    poisoned.into_inner()
                } else {
                    let message = message();
                    panic!("{message}");
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::panic::catch_unwind;
    use std::sync::{Arc, Mutex, PoisonError, RwLock};

    use super::{DEFAULT_PANIC_MESSAGE, LockExt};

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

        let payload = catch_unwind(|| mutex.lock().or_panic_with(|| "custom fail-fast".to_owned()))
            .expect_err("should panic");
        let message = payload
            .downcast_ref::<String>()
            .expect("panic payload should be a String");
        assert_eq!(message, "custom fail-fast");
    }

    #[test]
    fn message_is_built_lazily() {
        let mutex = Mutex::new(0);

        let guard = mutex.lock().or_panic_with(|| {
            panic!("message closure must not run when the lock is healthy");
        });
        assert_eq!(*guard, 0);
    }

    #[test]
    fn recovers_during_unwind_instead_of_aborting() {
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
        let result = catch_unwind(move || {
            let _held = accessor;
            panic!("unwind here");
        });

        assert!(result.is_err());
        assert_eq!(*shared.lock().unwrap_or_else(PoisonError::into_inner), 7);
    }
}
