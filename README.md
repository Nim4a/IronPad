# notepad-rs

A small modern notepad for Windows, written in Rust (eframe/egui + glow).

![features](#features)

- Dark theme, monospace editor
- Open / Save / Save As with native Windows file dialogs
- `Ctrl+N` new · `Ctrl+O` open · `Ctrl+S` save · `Ctrl+Shift+S` save as
- Live line / word / character count
- Warns before closing with unsaved changes
- Persian/Arabic text renders correctly (Segoe UI fallback)

## Build

```bash
cargo build --release
# -> target/release/notepad.exe
```

Needs a Rust toolchain with the `x86_64-pc-windows-gnu` target (no MSVC required).
See `.cargo/config.toml.example` if linking on a machine without MSVC.

## Run

```bash
cargo run --release
```
