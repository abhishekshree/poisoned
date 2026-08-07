# lock-ext

Fail-fast for poisoned `std` locks.

When a thread panics while holding a [`std::sync::Mutex`] or
[`std::sync::RwLock`] guard, the lock becomes *poisoned*: every subsequent
`lock()` / `read()` / `write()` returns a `PoisonError`. In practice that
usually means the world is already broken, so you want to fail fast — but the
plain `Result`/`unwrap()` dance makes that intent invisible, and it gets the
double-panic case wrong.

This crate provides `or_panic()` — an explicit, greppable, clippy-clean
fail-fast that also handles the unwinding case correctly.

## Usage

```rust
use std::sync::{Arc, Mutex};
use lock_ext::LockExt;

let shared = Arc::new(Mutex::new(0_i32));
*shared.lock().or_panic() += 1;
```

A custom message, built lazily:

```rust
use std::sync::Mutex;
use lock_ext::LockExt;

let name = "config".to_string();
let cache = Mutex::new(vec![1, 2, 3]);
let first = cache.lock().or_panic_with(|| format!("cache lock poisoned for {name}"));
```

The trait is implemented for every `Result<T, PoisonError<T>>`, so it covers:

- `Mutex::lock()`, `RwLock::read()`, `RwLock::write()`
- `Mutex::into_inner()`, `RwLock::into_inner()`
- the `get_mut()` variants

## How it behaves

| Lock state                  | Thread panicking? | Result                                             |
| --------------------------- | ----------------- | -------------------------------------------------- |
| Not poisoned                | any               | returns the guard, same as `Ok(...)`               |
| Poisoned                    | no                | `panic!` with the message (fail-fast)              |
| Poisoned                    | yes (unwinding)   | recovers the guard via `PoisonError::into_inner()` |

The last row is the reason this is not just `.unwrap()` with a fancy name: if a
poisoned lock is locked from a `Drop` implementation while the stack is already
unwinding, a naive panic triggers a **double panic**, which aborts the whole
process. `or_panic()` detects that via `std::thread::panicking()` and recovers
the guard instead.

## Why it exists

Tracking issue [rust-lang/rust#149359](https://github.com/rust-lang/rust/issues/149359)
proposes making `std::sync` locks panic on poison by default, possibly at the
2027 edition boundary. Until that lands, this crate is the explicit fail-fast
shim.

## License

MIT
