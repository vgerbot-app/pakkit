---
name: rust-async
description: Async Rust with tokio — runtime setup, concurrency (spawn/join/select), cancellation, channels, timeouts, and Send/'static bounds across await points. Use when writing or debugging async code, tasks, or concurrent I/O.
---

# Async Rust (tokio)

```toml
[dependencies]
tokio = { version = "1", features = ["rt-multi-thread", "macros", "time", "sync", "fs", "signal"] }
```

## Runtime

```rust
#[tokio::main]                       // multi-thread by default
async fn main() -> anyhow::Result<()> { run().await }

// or explicit, current-thread (cheap, single core, good for tests/small CLIs):
fn main() -> anyhow::Result<()> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(run())
}
```

- Never create a runtime inside async code; never call `block_on` on a worker thread (it panics).
- Only one multithreaded runtime per process is typical; libraries must **not** spawn runtimes —
  they are runtime-agnostic and assume the caller owns one.

## Concurrency primitives

```rust
use tokio::{join, try_join, select, spawn, time::{timeout, sleep, Duration}};

let (a, b) = join!(fetch_a(), fetch_b());          // run concurrently, wait for both
let (a, b) = try_join!(fetch_a(), fetch_b())?;     // propagate first error, others keep running
```

`tokio::spawn` requires `Future + Send + 'static` and returns a `JoinHandle` whose `Result` also
carries a `JoinError` (panic or cancellation):

```rust
let h = spawn(async move { work(id).await });
let out = h.await?;                 // ? unwraps JoinError
```

`select!` races futures; **losing branches are dropped (cancelled)**:

```rust
select! {
    _ = sleep(Duration::from_secs(1)) => eprintln!("timeout"),
    msg = rx.recv() => handle(msg),
    _ = token.cancelled() => return,
}
```

All `select!` branches must be cancel-safe (no partial reads lost on drop) — check the tokio docs'
cancel-safety note per method.

## Cancellation

- Dropping a future cancels it at the next `.await`. There is **no** implicit async cleanup —
  do cleanup in `Drop` or by not canceling mid-transaction.
- Structured cancellation: `tokio_util::sync::CancellationToken`; clone it into tasks, call
  `.cancel()`, and `select!` on `.cancelled()`.
- `JoinHandle::abort()` cancels a spawned task; awaiting the handle returns `JoinError::is_cancelled`.
- Graceful shutdown: `tokio::signal::ctrl_c()` or `signal(SignalKind::terminate())`, then cancel.

## Don't block the executor

- CPU-bound or blocking-syscall work => `tokio::task::spawn_blocking(move || { ... }).await?`.
- Blocking locks (`std::sync::Mutex`) held across `.await` stall the worker: hold them only in short
  scopes, or use `tokio::sync::Mutex` when the lock must span an await.
- `std::fs` in async hot paths blocks; use `tokio::fs` (which itself uses blocking threads).

## Channels

| Channel | Use |
|---|---|
| `tokio::sync::mpsc` | many producers -> one consumer (backpressure via bounded capacity) |
| `tokio::sync::oneshot` | single response / request-reply |
| `tokio::sync::broadcast` | fan-out to all current subscribers |
| `tokio::sync::watch` | latest-value state; receivers see newest value |
| `tokio::sync::Semaphore` | bound concurrent permits |

Prefer bounded `mpsc` with `.send().await` for backpressure; unbounded channels hide overload.

## `Send` / `'static` across await

A task is `Send` only if every value held across an `.await` is `Send`.

- Non-`Send` types: `Rc`, `RefCell` guards, `std::sync::MutexGuard` (pre-1.62 era caveats aside,
  the guard is `!Send`), some `dyn Trait` objects.
- Fixes: scope the non-`Send` value inside a block ending before the await; switch to `Arc`/`Mutex`;
  or use a current-thread runtime + `spawn_local` when truly single-threaded.
- Compiler error `future cannot be sent between threads safely` + `held across an await point`
  names the exact offending line.

## Timeouts and retries

```rust
let res = timeout(Duration::from_secs(5), request()).await
    .map_err(|_| anyhow::anyhow!("request timed out"))??;
```

Use `tokio-retry` or `backoff` for retry with exponential backoff + jitter; never tight-loop on
failure.

## Testing

```rust
#[tokio::test]                 // needs `macros` + `rt` features
async fn works() { assert!(connect().await.is_ok()); }

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn concurrent() { /* ... */ }
```

Use `tokio::time::pause()` + `advance()` to test timeouts deterministically without real sleeping.

## Verify

```bash
cargo test --all-features
cargo clippy --all-targets -- -D warnings   # catches some blocking-in-async misuse
```
