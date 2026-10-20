---
name: rust-performance
description: Profile and optimize Rust for speed and size — flamegraph/perf profiling, criterion benchmarks, release-profile and RUSTFLAGS tuning, allocation reduction, and binary-size analysis (cargo bloat). Use when a workload is slow/big or a performance claim needs evidence.
---

# Rust Performance

Rule zero: **measure before changing anything.** A guess-driven optimization is a regression
waiting to happen.

## Measure

```bash
cargo install flamegraph
cargo flamegraph --release -- <args>          # needs perf on Linux, works on macOS via dtrace
cargo build --release && perf stat ./target/release/app
cargo install cargo-bloat
cargo bloat --release --crates                 # which crates dominate binary size
cargo bloat --release -n 20                    # biggest functions
cargo install cargo-llvm-lines
cargo llvm-lines --release | head              # monomorphization cost by generic
```

- Profile **release** builds with debug symbols (`[profile.release] debug = "line-tables-only"`).
- Compare percentages (relative), not absolute times, across machines.
- Use `cargo bench` for micro-level, an end-to-end wall-clock run for macro-level.

## Benchmarks — criterion

```toml
[dev-dependencies]
criterion = { version = "0.5", features = ["html_reports"] }

[[bench]]
name = "parse"
harness = false
```

```rust
use criterion::{criterion_group, criterion_main, black_box, Criterion};

fn bench_parse(c: &mut Criterion) {
    let input = std::fs::read_to_string("fixtures/large.json").unwrap();
    c.bench_function("json parse", |b| {
        b.iter(|| serde_json::from_str::<serde_json::Value>(black_box(&input)))
    });
}

criterion_group!(benches, bench_parse);
criterion_main!(benches);
```

- `black_box` prevents the optimizer from deleting the measured work.
- Criterion detects statistical noise and reports change vs. the previous baseline.
- Nightly-only `#[bench]` is fine for internal code; use criterion for anything shared.

## Release profile / RUSTFLAGS

```toml
[profile.release]
lto = "fat"              # thin first; fat only with a measured win
codegen-units = 1
panic = "abort"          # smaller/faster, but disables unwinding and catch_unwind
strip = "symbols"
opt-level = 3
```

```bash
RUSTFLAGS="-C target-cpu=native" cargo build --release   # local only; NOT portable binaries
```

- `target-cpu=native` can unlock SIMD but breaks portability — never ship it in release artifacts.
- PGO: build with `-Cprofile-generate`, run a representative workload, rebuild with
  `-Cprofile-use`. Meaningful for hot loops, overkill elsewhere.
- `-Z build-std` (nightly) for size; usually not worth the fragility.

## Code-level

- **Allocation** dominates most hot paths. Prefer `&str` over `String`, `&[T]` over `Vec<T>`,
  `Cow<str>` for sometimes-owned, `SmallVec`/`ArrayVec` for small bounded collections.
- `Vec::with_capacity(n)` when the size is known; `reserve` when growing in a loop.
- Iterator chains (`iter().filter().map().sum()`) compile to tight loops — prefer them over manual
  indexing; add `.filter_map` instead of branching in a map.
- Reuse buffers across calls instead of allocating per call (`Write` into a shared `String`/`Vec`).
- Avoid `clone()` in loops; pass by reference; use `Arc` for shared ownership instead of deep copies.
- `#[inline]` on tiny cross-crate hot functions; `#[cold]` on error paths; `#[inline(never)]` to keep
  large cold code out of the hot path.
- Hashing: `ahash`/`rustc-hash` for non-cryptographic maps (default `SipHash` is DoS-resistant but slow).
- Formatting is expensive: avoid `format!` in hot loops; write into a reusable buffer.
- Parallelism (`rayon::par_iter`) often beats micro-tuning — but only after the algorithm is right.

## Algorithmic first

10x-100x wins come from better complexity or fewer passes, not from `lto`. Before micro-tuning,
check: redundant work, repeated parsing, N+1 queries, O(n^2) loops, unnecessary sorts/allocations,
caching opportunities.

## Verify

```bash
cargo bench                       # before/after comparison, not a single run
cargo bloat --release --crates
```
