<div align="center"><img src="./assets/logo.svg" alt="Pakkit logo" width="96" height="96"></div>

# Pakkit

<div align="center"><img alt="CI" src="https://github.com/vgerbot-app/pakkit/actions/workflows/ci.yml/badge.svg"> <img alt="License" src="https://img.shields.io/badge/license-MIT-blue.svg"> <img alt="Version" src="https://img.shields.io/badge/version-0.1.0-green.svg"> <img alt="Rust" src="https://img.shields.io/badge/rust-1.92%2B-orange.svg"> <img alt="Platform" src="https://img.shields.io/badge/platform-Windows-0078D6.svg"></div>

<p align="center">
🇺🇸 <a href="./README.md">English</a> | 🇨🇳 <a href="./README.zh.md">简体中文</a>
</p>

**Click to install, commands in the open.**

A native graphical client for your package managers — winget, apt and flatpak — built with Rust + [gpui-kit](https://crates.io/crates/gpui-kit) 0.8. A real native window, no Electron / WebView.

Every operation is a transparent wrapper around the backend CLI: the command line shown in the UI is exactly the command line that gets executed — nothing hidden, nothing rewritten.

![Preview](./assets/preview.png)

## Table of Contents

- [Features](#features)
- [Installation](#installation)
- [Usage](#usage)
- [Localization](#localization)
- [Project Structure](#project-structure)
- [Contributing](#contributing)
- [License](#license)

## Features

- **Seven views**: Discover, Categories, All Packages, Installed, Updates, Sources, Settings
- **Queue drawer**: installs / upgrades run serially with configurable concurrency (1–3), showing the command line and progress for each item
- **Package details**: sourced from `winget show --id --exact`, fetched in the background and cached — multi-line description, release notes, tags, publisher, license, homepage, installer type, release date, and more
- **Package icons**: favicons fetched automatically and cached to disk
- **Source management**: add / edit / remove / reset / update sources, with source export
- **Manifest import & export**: built on `winget export` / `winget import`
- **Themes**: dark / light / follow-system, with 5 accent colors
- **Multilingual UI**: five languages with instant switching and persistence; follows the OS language on first launch
- **Friendly error messages**: winget exit codes mapped to readable descriptions; benign codes treated as success

## Installation

### Download a prebuilt release

Grab the latest Windows build from [GitHub Releases](https://github.com/vgerbot-app/pakkit/releases).

### Build from source

Requirements: Rust 1.92+ (see `rust-toolchain.toml`), Windows.

```bash
git clone https://github.com/vgerbot-app/pakkit.git
cd pakkit
cargo build --release
```

> [!NOTE]
> Running the app requires [winget](https://learn.microsoft.com/windows/package-manager/winget/) to be installed (usually built into Windows 10 1809+ / Windows 11).

## Usage

```bash
cargo run --release
```

Or run `target/release/pakkit.exe` directly.

- Search packages in "Discover" / "All Packages"; clicking install enqueues the task into the queue drawer, which executes serially
- Upgrade individual or all packages in the "Updates" view
- Add private sources in "Sources" (requires administrator privileges)
- Switch theme, accent color, and language in "Settings → Appearance"; settings persist to `settings.json`

## Localization

| Language | Code | Status |
| --- | --- | --- |
| 简体中文 (Simplified Chinese) | `zh-CN` | ✅ |
| 繁體中文 (Traditional Chinese) | `zh-TW` | ✅ |
| English | `en` | ✅ |
| 日本語 (Japanese) | `ja` | ✅ |
| 한국어 (Korean) | `ko` | ✅ |

Strings are embedded at compile time (rust-i18n) and maintained in `locales/*.yml`.

## Project Structure

| Module | Responsibility |
| --- | --- |
| `src/winget/` | Pure-function winget command builders, output parsers, exit-code mapping (no UI dependencies, independently testable) |
| `src/model.rs` / `src/state.rs` | Data model and application state |
| `src/queue.rs` | Install / upgrade task queue |
| `src/shell.rs` | Main window view `AppView`, the single entry point for state mutation |
| `src/views.rs` / `src/widgets.rs` / `src/dialogs.rs` | Views, widgets, and dialogs (read-only state rendering) |
| `src/icons.rs` / `src/cache.rs` | Favicon fetching and disk caching |
| `src/theme.rs` / `themes/` | Themes and accent colors |
| `locales/` | Strings for the five languages |

See [docs/architecture.md](./docs/architecture.md) for more detail.

## Contributing

Issues and pull requests are welcome. Before submitting, please run:

```bash
cargo fmt
cargo clippy
cargo test
```

## License

[MIT](./LICENSE) © 2026 y1j2x34
