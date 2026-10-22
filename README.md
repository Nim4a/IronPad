# notepad-rs

[![CI](https://github.com/Nim4a/notepad-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/Nim4a/notepad-rs/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/Nim4a/notepad-rs)](https://github.com/Nim4a/notepad-rs/releases)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

A small, modern notepad for Windows written in Rust — [eframe](https://github.com/emilk/egui)/egui + glow, no webview, no runtime deps.

![notepad-rs](docs/screenshot.png)

## Features

- Dark theme, monospace editor
- Open / Save / Save As with native Windows file dialogs
- `Ctrl+N` new · `Ctrl+O` open · `Ctrl+S` save · `Ctrl+Shift+S` save as
- Live line / word / character count
- Warns before closing with unsaved changes
- Persian/Arabic text renders correctly (Segoe UI fallback)
- Single self-contained `.exe`, no installer
- `notepad.exe path.txt` opens a file from the command line

## Download

Grab the latest `notepad.exe` from [Releases](https://github.com/Nim4a/notepad-rs/releases).

## Build from source

```bash
cargo build --release
# -> target/release/notepad.exe
```

Requires the `x86_64-pc-windows-gnu` target — no Visual Studio needed. On a
machine without MSVC, copy `.cargo/config.toml.example` to `.cargo/config.toml`
and fix the two paths.

## Run

```bash
cargo run --release
```

## License

MIT — see [LICENSE](LICENSE).
