# poisoned

[![Crates.io](https://img.shields.io/crates/v/poisoned)](https://crates.io/crates/poisoned)
[![Docs.rs](https://docs.rs/poisoned/badge.svg)](https://docs.rs/poisoned)
[![CI](https://github.com/abhishekshree/poisoned/actions/workflows/ci.yml/badge.svg)](https://github.com/abhishekshree/poisoned/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/crates/l/poisoned)](LICENSE)

Fail-fast for poisoned `std` locks.

When a thread panics while holding a [`std::sync::Mutex`] or
[`std::sync::RwLock`] guard, the lock becomes poisoned and every subsequent
`lock()`, `read()`, or `write()` returns a `PoisonError`. This crate's
`or_panic()` turns that into a panic: explicit, greppable, and clippy-clean.

`or_panic()` does **not** special-case unwinding. Calling it on a poisoned
lock from a `Drop` impl while the stack is already unwinding panics like any
other panic, which aborts the process with a double panic — the same as
`.unwrap()`. Recovery via `std::thread::panicking()` was removed in 1.1.0; if
a `Drop` impl must tolerate a poisoned lock, catch the panic explicitly with
`std::panic::catch_unwind`.

## Usage

```rust
use std::sync::{Arc, Mutex};
use poisoned::LockExt;

let shared = Arc::new(Mutex::new(0_i32));
*shared.lock().or_panic() += 1;
```

A custom message, built lazily:

```rust
use std::sync::Mutex;
use poisoned::LockExt;

let name = "config".to_string();
let cache = Mutex::new(vec![1, 2, 3]);
let first = cache.lock().or_panic_with(|| format!("cache lock poisoned for {name}"));
```

The trait is implemented for every `Result<T, PoisonError<T>>`, so it covers:

- `Mutex::lock()`, `RwLock::read()`, `RwLock::write()`
- `Mutex::into_inner()`, `RwLock::into_inner()`
- the `get_mut()` variants

The double-panic trap, e.g. flushing metrics from a `Drop` impl while
unwinding. Both the `.unwrap()` version and the `or_panic()` version abort
the process if the lock is poisoned:

```rust
impl Drop for FlushOnDrop {
    fn drop(&mut self) {
        // If this lock is poisoned mid-unwind, this panics again and the
        // process aborts with a double panic.
        self.metrics.lock().unwrap().push(7);
    }
}
```

To keep the process alive, catch the panic explicitly. The guard is dropped
normally if the lock is healthy, so only the poisoned case panics:

```rust
use std::panic::catch_unwind;

impl Drop for FlushOnDrop {
    fn drop(&mut self) {
        let _ = catch_unwind(|| {
            self.metrics.lock().or_panic().push(7);
        });
    }
}
```

## How it behaves

| Lock state                | Result                                            |
| ------------------------- | ------------------------------------------------- |
| Not poisoned              | returns the guard, same as `Ok(...)`              |
| Poisoned                  | panics with the message (fail-fast)               |

## Why it exists

Tracking issue [rust-lang/rust#149359](https://github.com/rust-lang/rust/issues/149359)
proposes making `std::sync` locks panic on poison by default, possibly at the
2027 edition boundary. Until that lands, this crate is the explicit fail-fast
shim.

## License

MIT
