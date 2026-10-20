---
name: rust-project-setup
description: Scaffold and configure Rust crates and Cargo workspaces — edition, resolver, release profiles, MSRV, rust-toolchain.toml, feature flags, and shared workspace dependencies. Use when creating a crate/workspace or tuning Cargo.toml.
---

# Rust Project Setup

## Scaffold

```bash
cargo new --lib  crates/core           # library
cargo new --bin  apps/cli              # binary
cargo new --lib  crates/foo && cargo new --bin apps/foo
```

Prefer a workspace from day one once a second crate appears; a single crate should stay a plain
crate (no `[workspace]` boilerplate).

## Cargo.toml baseline

```toml
[package]
name = "mytool"
version = "0.1.0"
edition = "2021"          # "2024" requires toolchain >= 1.85
rust-version = "1.75"     # MSRV; enforced by `cargo msrv` / CI
license = "MIT OR Apache-2.0"
description = "One line."
repository = "https://github.com/org/mytool"

[dependencies]
```

- `edition` implies the resolver: 2021 => `resolver = "2"`, 2024 => `"3"`. Only set `resolver`
  explicitly on a workspace root whose members use different editions.
- Set `rust-version` and keep CI on that toolchain; don't claim an MSRV you never build with.

## Workspace layout

```toml
# root Cargo.toml
[workspace]
resolver = "2"
members = ["crates/*", "apps/*"]

[workspace.package]        # inherit per-crate
edition = "2021"
license = "MIT OR Apache-2.0"

[workspace.dependencies]   # single source of truth for versions
serde = { version = "1", features = ["derive"] }
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
anyhow = "1"
```

Members opt in explicitly:

```toml
[package]
name = "core"
edition.workspace = true
license.workspace = true

[dependencies]
serde = { workspace = true, features = ["rc"] }  # add features per crate
```

Rules: put versions **only** in `[workspace.dependencies]`; members add features, never versions.
Run workspace-wide commands with `--workspace` (`cargo test --workspace --all-features`).

## Profiles

```toml
[profile.dev]
opt-level = 1              # dependencies noticeably faster; thin debug builds

[profile.release]
lto = "thin"
codegen-units = 1
panic = "abort"            # ONLY if you never catch_unwind / no panic-leaking FFI
strip = "symbols"          # release artifacts
debug = false

[profile.bench]
lto = "thin"
```

- `lto = "fat"` only if a measured win justifies the build time; start with `"thin"`.
- `panic = "abort"` breaks tests that expect unwinding and any `catch_unwind`; leave it off unless
  binary size/startup is the actual goal.

## Toolchain pin

```toml
# rust-toolchain.toml
[toolchain]
channel = "stable"                 # or "1.75.0" for an exact pin
components = ["rustfmt", "clippy"]
targets = ["x86_64-unknown-linux-musl"]
profile = "minimal"
```

Pinning `channel = "1.75.0"` makes builds reproducible; `"stable"` tracks updates. Commit this file.

## Feature flags

- Features are **additive** — enabling more must never remove functionality or change behavior
  incompatibly.
- Gate optional deps: `dep:serde` syntax (`["dep:serde"]`) so the implicit feature is not exposed.
- Provide an `std`-vs-`no_std` split and a `full` convenience feature.
- Test both extremes: `cargo test --no-default-features` and `--all-features`.

## Dependency hygiene

```bash
cargo add serde --features derive       # edit Cargo.toml correctly
cargo tree -d                           # duplicate versions
cargo tree -e features -i serde         # why is serde in the graph
cargo update -p some-crate              # targeted upgrade
cargo outdated                          # (cargo-outdated) stale deps
cargo machete                           # (cargo-machete) unused deps
```

## Verify

```bash
cargo check --workspace --all-targets
cargo build --release
```
