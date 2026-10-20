---
name: rust-error-handling
description: Idiomatic Rust error handling — thiserror for typed library errors, anyhow for applications, error propagation with `?`, `#[from]`/`#[source]` conversions, and when panicking is acceptable. Use when designing error types or wiring Result propagation.
---

# Rust Error Handling

Decision rule:

- **Library / public API** => typed error enum with `thiserror`. Callers must be able to match variants.
- **Binary / application glue** => `anyhow::Result<T>` + `.context(...)`. Erase types; keep the chain.
- **Never** return `Box<dyn Error>` from a public API: it is not `Send + Sync`, cannot be matched, and
  forces dynamic dispatch.

## Library errors — thiserror

```rust
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("failed to read config at {path}")]
    Read {
        path: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("invalid value for `{key}`")]
    Invalid { key: String },

    #[error(transparent)]           // delegate Display + source
    Parse(#[from] toml::de::Error), // `?` auto-converts toml::de::Error
}
```

- `#[from]` gives you `?` conversion; use it once per source type per enum.
- `#[source]` preserves the cause chain without an automatic `From`.
- `#[error(transparent)]` forwards the inner error's `Display`/`source` — use for single-variant wrappers.
- Derive `Debug` always (required by `Error`).

## Application errors — anyhow

```rust
use anyhow::{Context, Result, bail, ensure};

fn load(path: &std::path::Path) -> Result<Config> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("reading {}", path.display()))?;

    ensure!(!text.is_empty(), "config at {} is empty", path.display());

    let cfg: Config = toml::from_str(&text).context("parsing config")?;
    if cfg.workers == 0 {
        bail!("`workers` must be >= 1");
    }
    Ok(cfg)
}
```

- `.context()` / `.with_context(|| ...)` add a layer; the lazy closure avoids formatting on the happy path.
- `bail!` = early `return Err(...)`; `ensure!` = `bail!` on a false condition.
- Print the full chain with `{:#}` (or `{:?}`), not `{}`:

  ```rust
  if let Err(e) = run() {
      eprintln!("error: {e:#}");
      std::process::exit(1);
  }
  ```

## Propagation and conversion

- `?` calls `From::from` on the error. Implement it manually only when `#[from]` cannot apply
  (e.g. two variants of the same source type — then map explicitly with `.map_err(...)`).
- Convert at the boundary: an internal `thiserror` type becomes `anyhow` with `?` (anyhow is
  `From<E: Error + Send + Sync + 'static>`).
- `Option` -> `Result`: `.ok_or(MyError::Missing)?` / `.ok_or_else(|| ...)`; `Option::context` from anyhow.

## Panics

- Panic (`unwrap`/`expect`/`panic!`/index out of range) is for **programmer bugs**, not expected
  failure. Recoverable conditions must be `Result`.
- `unwrap()` is acceptable in tests, examples, and where the invariant is locally proven.
- `expect("why this cannot fail")` documents the invariant; prefer it over bare `unwrap`.
- In `no_std`/embedded, avoid panics entirely; return `Result` or use `panic-never`.
- Never `unwrap()` on I/O, parse, or user input in library paths.

## Patterns

- **Validate once at the boundary**, then use infallible types internally (newtype with a checked
  constructor).
- **Don't `.map_err(|e| ...)` into `String`** — you lose `source()` and the `?` chain.
- **`std::io::Error` is opaque**; wrap it with context rather than matching its `ErrorKind` blindly.
- Group related variants instead of a catch-all `Other(String)` — that defeats typed matching.

## Verify

```bash
cargo clippy --all-targets -- -D clippy::unwrap_used   # where unwrap is banned
```
