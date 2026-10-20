---
name: rust-testing
description: Write and run Rust tests — unit tests in `#[cfg(test)]` modules, integration tests, doc tests, property/fuzz/snapshot testing, and cargo test / nextest / llvm-cov workflows. Use when adding tests or diagnosing test runs.
---

# Rust Testing

## Layout

| Kind | Location | Access |
|---|---|---|
| Unit | `#[cfg(test)] mod tests` in the source file | private items |
| Integration | `tests/*.rs` (each file is its own crate) | public API only |
| Doc | `///` code blocks on items | public API, compiled as examples |
| Bench | `benches/*.rs` (criterion) | public API |

```rust
pub fn add(a: i32, b: i32) -> i32 { a + b }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adds_positive() {
        assert_eq!(add(2, 3), 5);
    }

    #[test]
    #[should_panic(expected = "divide by zero")]
    fn panics_on_zero() {
        divide(1, 0);
    }

    #[test]
    fn returns_result() -> Result<(), Box<dyn std::error::Error>> {
        let v: i32 = "42".parse()?;    // `?` works in tests returning Result
        assert_eq!(v, 42);
        Ok(())
    }
}
```

## Running

```bash
cargo test                       # unit + integration + doc
cargo test --workspace --all-features
cargo test --doc                 # doc tests only
cargo test name_substring        # filter by name
cargo test -- --nocapture        # show stdout/stderr
cargo test -- --test-threads=1   # serialize (shared resources)
cargo test -- --ignored          # run #[ignore] tests
cargo test --release             # catch debug-only assumptions / timing
```

Doc tests:

````rust
/// Adds two numbers.
///
/// ```
/// assert_eq!(mytool::add(2, 3), 5);
/// ```
///
/// ```no_run        // compiles but is not executed
/// let _ = mytool::connect("x");
/// ```
````

`no_run` still compiles — use it for network/IO examples. Hidden setup lines start with `#`.

## Faster runs — nextest

```bash
cargo install cargo-nextest
cargo nextest run --workspace --all-features
```

Per-test process isolation (catches cross-test state leakage), retries, and machine-readable
output. It does **not** run doc tests — run `cargo test --doc` separately.

## Property & fuzz

```toml
[dev-dependencies]
proptest = "1"
```

```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn roundtrip(v: Vec<u8>) {
        let encoded = encode(&v);
        prop_assert_eq!(decode(&encoded).unwrap(), v);
    }
}
```

```bash
cargo install cargo-fuzz
cargo fuzz init && cargo fuzz run target_name     # libFuzzer, needs nightly
```

## Snapshots — insta

```toml
[dev-dependencies]
insta = "1"
```

```rust
insta::assert_snapshot!(render());
insta::assert_debug_snapshot!(parse(input));
```

Review with `cargo insta review`; committed `.snap` files make diffs reviewable.

## Coverage

```bash
cargo install cargo-llvm-cov
cargo llvm-cov --workspace --all-features --html
cargo llvm-cov --lcov --output-path lcov.info    # for CI upload
```

## Rules

- Tests must be deterministic and isolated: no fixed ports, no wall-clock assumptions, no shared
  temp paths (`tempfile` for scratch dirs).
- One behavioral assertion per test concept; name tests by the behavior, not the implementation.
- `#[ignore]` for tests requiring external services; run them in a dedicated CI job.
- Prefer `pretty_assertions` for readable diffs on large structs.

## Verify

```bash
cargo test --workspace --all-features && cargo test --doc
```
