---
name: rust-quality-gate
description: Enforce Rust code quality — rustfmt, clippy lint configuration via `[lints]`, cargo-deny/cargo-audit for licenses and advisories, undocumented-unsafe and missing-docs lints, and a CI workflow. Use when setting up lint/format gates or reviewing CI failures.
---

# Rust Quality Gate

Run these before every hand-off; wire the same commands into CI.

```bash
cargo fmt --all                                     # apply formatting
cargo fmt --all -- --check                          # CI: fail on drift
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo deny check                                    # licenses + advisories + banned crates
```

- `--all-targets` covers tests/benches/examples — lints there matter too.
- `--all-features` catches feature-gated breakage; also test `--no-default-features` separately.
- `-D warnings` turns Clippy warnings into errors so CI cannot rot.

## Lint configuration — Cargo.toml

```toml
[lints.rust]
unsafe_code = "forbid"          # or "deny"; use forbid when truly no unsafe
missing_docs = "warn"           # public API documented
unused_crate_dependencies = "warn"
rust_2018_idioms = "warn"

[lints.clippy]
all = { level = "warn", priority = -1 }
pedantic = { level = "warn", priority = -1 }
nursery = { level = "warn", priority = -1 }   # opt in per-crate, can be noisy
unwrap_used = "warn"            # libraries: prefer expect()/Result
todo = "warn"
dbg_macro = "warn"

[lints.clippy.module_name_repetitions]   # commonly allowed
level = "allow"

[lints.clippy.missing_errors_doc]
level = "allow"
```

Workspace inheritance: define `[workspace.lints]` in the root and `[lints] workspace = true` in each
member.

- Set `priority = -1` on the lint **groups** so individual overrides (priority 0 default) win.
- `pedantic`/`nursery` are opt-in advice; enable, then allow the specific lints you disagree with —
  don't blanket-disable the group.

Crate-level attributes remain valid for invariants:

```rust
#![forbid(unsafe_code)]
#![deny(missing_debug_implementations)]
#![deny(unsafe_op_in_unsafe_fn)]   // require explicit unsafe blocks inside unsafe fns (edition 2024)
```

## Supply chain — cargo-deny

```toml
# deny.toml (minimal)
[advisories]
version = 2
yanked = "deny"

[licenses]
allow = ["MIT", "Apache-2.0", "Apache-2.0 WITH LLVM-exception", "BSD-2-Clause", "BSD-3-Clause", "ISC", "Unicode-3.0", "Zlib"]

[bans]
multiple-versions = "warn"
wildcards = "deny"

[sources]
unknown-registry = "deny"
unknown-git = "deny"
```

```bash
cargo install cargo-deny
cargo deny init      # generates deny.toml
cargo deny check
```

Repo-local advisories database: merge with `[advisories] ignore` entries that cite a reason and an
expiry in a comment, never silent ignores.

## Doc-as-test

```bash
cargo test --doc                 # examples in `///` are compiled and run
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
```

Broken intra-doc links are errors with `-D warnings`; catches stale docs during refactors.

## Publishing checks

```bash
cargo publish --dry-run          # packages, verifies, does not upload
cargo package --list             # what ships (respects `include`/`exclude`)
```

`Cargo.toml` fields required to publish cleanly: `license` (or `license-file`), `description`,
`repository`, `readme`, `keywords`, `categories`.

## CI workflow

```yaml
# .github/workflows/ci.yml
name: ci
on: { push: { branches: [main] }, pull_request: {} }
jobs:
  ci:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with: { components: rustfmt, clippy }
      - run: cargo fmt --all -- --check
      - run: cargo clippy --workspace --all-targets --all-features -- -D warnings
      - run: cargo test --workspace --all-features
      - run: cargo test --doc
```

Add caching (`Swatinem/rust-cache@v2`) for build time and a separate `cargo deny check` job.

## Verify

```bash
cargo fmt --all -- --check && cargo clippy --workspace --all-targets --all-features -- -D warnings
```
