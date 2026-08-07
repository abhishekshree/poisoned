# poisoned

Fail-fast for poisoned `std` locks.

When a thread panics while holding a [`std::sync::Mutex`] or
[`std::sync::RwLock`] guard, the lock becomes poisoned and every subsequent
`lock()`, `read()`, or `write()` returns a `PoisonError`. This crate's
`or_panic()` turns that into a panic: explicit, greppable, and clippy-clean.

Unlike `.unwrap()`, `or_panic()` also handles the unwinding case. If a
poisoned lock is locked from a `Drop` impl while the stack is already
unwinding, a naive panic triggers a double panic that aborts the process.
`or_panic()` detects this via `std::thread::panicking()` and recovers the
guard instead.

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
