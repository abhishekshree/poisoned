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

Unlike `.unwrap()`, `or_panic()` also handles the unwinding case. This
matters if any `Drop` impl in your codebase locks a shared `Mutex`/`RwLock`
while unwinding, e.g. releasing a pooled connection or flushing metrics on
cleanup. If that lock is poisoned, a naive panic triggers a double panic
that aborts the process. `or_panic()` detects this via
`std::thread::panicking()` and recovers the guard instead.

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

The double-panic case, e.g. flushing metrics from a `Drop` impl while
unwinding. The `unwrap()` version aborts the process if the lock is
poisoned:

```rust
impl Drop for FlushOnDrop {
    fn drop(&mut self) {
        // If this lock is poisoned mid-unwind, this panics again and the
        // process aborts with a double panic.
        self.metrics.lock().unwrap().push(7);
    }
}
```

With `or_panic()` the guard is recovered instead:

```rust
use poisoned::LockExt;

impl Drop for FlushOnDrop {
    fn drop(&mut self) {
        self.metrics.lock().or_panic().push(7);
    }
}
```

## How it behaves

| Lock state                | Thread panicking? | Result                                            |
| ------------------------- | ----------------- | ------------------------------------------------- |
| Not poisoned              | any               | returns the guard, same as `Ok(...)`              |
| Poisoned                  | no                | panics with the message (fail-fast)               |
| Poisoned                  | yes (unwinding)   | recovers the guard via `PoisonError::into_inner()` |

## Why it exists

Tracking issue [rust-lang/rust#149359](https://github.com/rust-lang/rust/issues/149359)
proposes making `std::sync` locks panic on poison by default, possibly at the
2027 edition boundary. Until that lands, this crate is the explicit fail-fast
shim.

## License

MIT
