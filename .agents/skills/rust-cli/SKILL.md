---
name: rust-cli
description: Build production Rust command-line tools — clap derive argument parsing, exit codes, tracing logging, config loading, progress output, and cross-compilation/release. Use when adding subcommands/flags, structured logging, or CLI error/exit behavior.
---

# Rust CLI

```toml
[dependencies]
clap = { version = "4", features = ["derive", "env", "wrap_help"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
anyhow = "1"
```

## Argument parsing — clap derive

```rust
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "mytool", version, about, propagate_version = true)]
struct Cli {
    /// Increase log verbosity (-v, -vv)
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    verbose: u8,

    /// Config file path
    #[arg(short, long, env = "MYTOOL_CONFIG")]
    config: Option<std::path::PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run the job
    Run {
        #[arg(default_value = ".")]
        path: std::path::PathBuf,
        /// Worker count
        #[arg(short, long, default_value_t = num_cpus::get())]
        jobs: usize,
    },
    /// Print resolved configuration
    Show,
}

fn main() -> std::process::ExitCode {
    let cli = Cli::parse();          // exits with usage error (code 2) on bad input
    // ...
    std::process::ExitCode::SUCCESS
}
```

- `version` / `about` come from `Cargo.toml`; keep them there, not duplicated in attributes.
- `env = "..."` reads a fallback from the environment; CLI args always override env.
- `value_enum` for `enum` args; `ValueEnum` derives accepted spellings.
- Subcommand-specific help: `mytool run --help`. Parent `--help` lists subcommands.

## Exit codes

- Return `std::process::ExitCode` from `main` (portable, no `std::process::exit` early-exit).
- `0` success; `1` generic runtime error; `2` usage error (clap handles this automatically).
- Reserve codes < 126; document any custom ones. Avoid `std::process::exit` in library code — it
  skips destructors and flushes.

```rust
fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e:#}");
            std::process::ExitCode::FAILURE
        }
    }
}
```

## Logging — tracing

```rust
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

fn init_logging(verbose: u8) {
    let level = match verbose {
        0 => "warn", 1 => "info", 2 => "debug", _ => "trace",
    };
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(level));
    tracing_subscriber::registry()
        .with(fmt::layer().with_writer(std::io::stderr))   // logs to stderr, data to stdout
        .with(filter)
        .init();
}
```

```rust
tracing::info!(path = %p.display(), jobs, "starting run");
tracing::debug!(?config, "resolved configuration");
```

- **Logs to stderr**, machine-readable output to stdout — keeps `mytool | jq` clean.
- `RUST_LOG=mycrate=debug` overrides with `EnvFilter`.
- `?v` = `Debug`, `%v` = `Display` field capture.

## Progress and interactivity

- `indicatif` for progress bars; only when `std::io::stderr().is_terminal()` (use
  `std::io::IsTerminal`) to avoid garbling piped output.
- Detect TTY before emitting ANSI colors; respect `NO_COLOR` and `CLICOLOR_FORCE`.

## Config

- Precedence: CLI flag > env var > config file > built-in default. Implement explicitly and document.
- `figment` merges many sources; or `serde` + `toml` + `envy` hand-rolled.
- Validate once after merging; reject unknown keys in strict mode (`deny_unknown_fields`).

## Distribution

```bash
cargo build --release
cargo install --path . --locked
cargo dist init          # cargo-dist: multi-target archives, installers, GitHub releases
```

- Commit `Cargo.lock` for binaries; `--locked` in CI/installs for reproducibility.
- Cross-compile with `cross` (Docker-based musl targets) or a per-target toolchain.
- Ship man pages via `clap_mangen`.

## Verify

```bash
cargo run -- --help
cargo run -- run --jobs 4
cargo run -- bad-subcommand; echo "exit=$?"    # expect 2
```
