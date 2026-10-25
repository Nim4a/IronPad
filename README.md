# IronPad

[![CI](https://github.com/Nim4a/IronPad/actions/workflows/ci.yml/badge.svg)](https://github.com/Nim4a/IronPad/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/Nim4a/IronPad)](https://github.com/Nim4a/IronPad/releases)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

A small, modern notepad for Windows written in Rust — [eframe](https://github.com/emilk/egui)/egui
for the chrome, the native Win32 `EDIT` control for the text area. No webview, no runtime deps.

![IronPad](docs/screenshot.png)

## Why a native text control?

egui cannot shape or order RTL text: Persian/Arabic comes out with disconnected
letters and the wrong word order. The Windows `EDIT` control uses Uniscribe, so
letter joining, bidi, RTL paragraphs and the Persian IME come for free — and it
also gives us word wrap, undo/redo, selection and clipboard behaviour for free.

The chrome (toolbar, status bar) stays in egui and is themed dark; the editor
itself is a child window of the egui window, so it is positioned over the
central area every frame.

## Features

- Dark theme, monospace editor
- Open / Save / Save As with native Windows file dialogs
- `Ctrl+N` new · `Ctrl+O` open · `Ctrl+S` save · `Ctrl+Shift+S` save as
- Live line / word / character count
- Warns before closing with unsaved changes
- Persian/Arabic: correct letter joining, RTL paragraphs, mixed digits, IME
- Single self-contained `.exe`, no installer
- `ironpad.exe path.txt` opens a file from the command line

## Download

Grab the latest `ironpad.exe` (Windows) and `IronPad-Android-*.apk` (Android)
from [Releases](https://github.com/Nim4a/IronPad/releases).

## Android

A native Kotlin port (`android/`) with the same IronPad look: dark/light theme,
multi-tab editing, syntax highlighting, line numbers, and Persian/RTL support
via the platform's text engine.

- Min SDK 24, target SDK 34
- Open / Save / Save As / Share text / Share as file
- Tap a `.txt` file in any file manager → "Open with IronPad"
- Built by CI (`.github/workflows/android.yml`); APK is signed with the debug
  keystore for sideloading

Build locally:

```bash
cd android && ./gradlew assembleRelease
```


## Build from source

```bash
cargo build --release
# -> target/release/ironpad.exe
```

Requires the `x86_64-pc-windows-gnu` target — no Visual Studio needed. On a
machine without MSVC, copy `.cargo/config.toml.example` to `.cargo/config.toml`
and fix the two paths.

egui is pinned to `0.31`: 0.35+ ships a new renderer (vello) that wedges the
OpenGL driver on older GPUs.

## Run

```bash
cargo run --release
```

## License

MIT — see [LICENSE](LICENSE).
