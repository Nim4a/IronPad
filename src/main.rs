#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! A modern notepad for Windows.
//!
//! The chrome (toolbar, status bar) is egui; the text area is the native
//! Win32 EDIT control, because egui cannot shape or order RTL text
//! (Persian/Arabic) — the OS control does shaping, bidi and IME for free.
//!
//! egui is pinned to 0.31 on purpose: 0.35+ (new panel API / vello renderer)
//! wedges the OpenGL driver on this machine.

mod win_edit;

use std::path::PathBuf;
use std::sync::atomic::Ordering;

use eframe::egui;
use win_edit::Edit;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Notepad")
            .with_inner_size([960.0, 640.0])
            .with_min_inner_size([480.0, 320.0]),
        ..Default::default()
    };
    // `notepad.exe path.txt` opens that file at startup.
    let startup_file = std::env::args().nth(1).map(PathBuf::from);
    eframe::run_native(
        "Notepad",
        options,
        Box::new(move |cc| {
            let mut app = Notepad::new(cc);
            if let Some(path) = startup_file {
                app.load(path);
            }
            Ok(Box::new(app))
        }),
    )
}

struct Notepad {
    text: String,
    path: Option<PathBuf>,
    dirty: bool,
    error: Option<String>,
    last_title: String,
    themed: bool,
    edit: Option<Edit>,
    pending_text: Option<String>,
    parent_hwnd: Option<windows_sys::Win32::Foundation::HWND>,
}

impl Notepad {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        cc.egui_ctx.set_theme(egui::ThemePreference::Dark);
        setup_fonts(&cc.egui_ctx);
        Self {
            text: String::new(),
            path: None,
            dirty: false,
            error: None,
            last_title: String::new(),
            themed: false,
            edit: None,
            pending_text: None,
            parent_hwnd: None,
        }
    }

    /// Put text into the native editor (next frame, once it exists).
    fn set_editor_text(&mut self, text: String) {
        self.text = text.clone();
        match self.edit.as_ref() {
            Some(edit) => edit.set_text(&text),
            None => self.pending_text = Some(text),
        }
    }

    /// Pull the latest text out of the native editor.
    fn sync_from_editor(&mut self) {
        if let Some(edit) = self.edit.as_ref() {
            if edit.has_focus() {
                let t = edit.get_text();
                if t != self.text {
                    self.text = t;
                    self.dirty = true;
                }
            }
        }
    }

    fn load(&mut self, path: PathBuf) {
        match std::fs::read_to_string(&path) {
            Ok(s) => {
                self.path = Some(path);
                self.dirty = false;
                self.error = None;
                self.set_editor_text(s);
            }
            Err(e) => self.error = Some(format!("Open failed: {e}")),
        }
    }

    fn open_file(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter(
                "Text files",
                &[
                    "txt", "md", "rs", "toml", "json", "csv", "xml", "html", "css", "js", "log",
                ],
            )
            .pick_file()
        {
            self.load(path);
        }
    }

    fn save_file(&mut self, save_as: bool) -> bool {
        self.sync_from_editor();
        let path = if save_as || self.path.is_none() {
            let name = self
                .path
                .as_deref()
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "untitled.txt".into());
            match rfd::FileDialog::new().set_file_name(&name).save_file() {
                Some(p) => p,
                None => return false,
            }
        } else {
            self.path.clone().unwrap()
        };
        match std::fs::write(&path, &self.text) {
            Ok(()) => {
                self.path = Some(path);
                self.dirty = false;
                self.error = None;
                true
            }
            Err(e) => {
                self.error = Some(format!("Save failed: {e}"));
                false
            }
        }
    }

    fn new_file(&mut self) {
        self.path = None;
        self.dirty = false;
        self.error = None;
        self.set_editor_text(String::new());
    }

    /// Ask about unsaved changes. Returns true if we may continue.
    fn confirm_discard(&mut self, parent: windows_sys::Win32::Foundation::HWND) -> bool {
        self.sync_from_editor();
        if !self.dirty {
            return true;
        }
        match Edit::confirm_discard(parent) {
            win_edit::Confirm::Save => self.save_file(false),
            win_edit::Confirm::Discard => {
                self.dirty = false;
                true
            }
            win_edit::Confirm::Cancel => false,
        }
    }
}

fn setup_fonts(ctx: &egui::Context) {
    // Add Segoe UI as fallback so Persian/Arabic text renders instead of boxes.
    if let Ok(data) = std::fs::read("C:\\Windows\\Fonts\\segoeui.ttf") {
        let mut fonts = egui::FontDefinitions::default();
        fonts
            .font_data
            .insert("segoe-ui".into(), egui::FontData::from_owned(data).into());
        for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            fonts
                .families
                .entry(family)
                .or_default()
                .push("segoe-ui".into());
        }
        ctx.set_fonts(fonts);
    }
}

fn shortcut(ctx: &egui::Context, mods: egui::Modifiers, key: egui::Key) -> bool {
    ctx.input_mut(|i| i.consume_shortcut(&egui::KeyboardShortcut::new(mods, key)))
}

impl eframe::App for Notepad {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // eframe applies the system theme after App::new, so force dark on frame 1.
        if !self.themed {
            ctx.set_theme(egui::ThemePreference::Dark);
            ctx.request_repaint();
            self.themed = true;
        }
        // eframe does not expose the HWND in 0.31, so look it up ourselves.
        if self.parent_hwnd.is_none() {
            let hwnd = win_edit::find_main_window();
            if !hwnd.is_null() {
                self.parent_hwnd = Some(hwnd);
            }
        }
        let parent = self.parent_hwnd.unwrap_or(std::ptr::null_mut());

        if self.edit.is_none() && !parent.is_null() {
            if let Some(edit) = Edit::new(parent) {
                self.edit = Some(edit);
                if let Some(t) = self.pending_text.take() {
                    self.edit.as_ref().unwrap().set_text(&t);
                }
            }
        }

        // Shortcuts pressed inside the native editor arrive through the subclass.
        let mut quit = false;
        match win_edit::ACTION.swap(0, Ordering::Relaxed) {
            win_edit::ACT_NEW => {
                if self.confirm_discard(parent) {
                    self.new_file();
                }
            }
            win_edit::ACT_OPEN => self.open_file(),
            win_edit::ACT_SAVE => {
                self.save_file(false);
            }
            win_edit::ACT_SAVE_AS => {
                self.save_file(true);
            }
            win_edit::ACT_QUIT => quit = true,
            _ => {}
        }

        // Same shortcuts when the egui part has the keyboard focus.
        let ctrl = egui::Modifiers::COMMAND;
        if shortcut(ctx, ctrl, egui::Key::N) && self.confirm_discard(parent) {
            self.new_file();
        }
        if shortcut(ctx, ctrl, egui::Key::O) {
            self.open_file();
        }
        if shortcut(ctx, ctrl, egui::Key::S) {
            self.save_file(false);
        }
        if shortcut(ctx, ctrl | egui::Modifiers::SHIFT, egui::Key::S) {
            self.save_file(true);
        }

        let top = egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("New").clicked() && self.confirm_discard(parent) {
                    self.new_file();
                }
                if ui.button("Open").clicked() {
                    self.open_file();
                }
                if ui.button("Save").clicked() {
                    self.save_file(false);
                }
                if ui.button("Save As").clicked() {
                    self.save_file(true);
                }
                ui.separator();
                if ui.button("Quit").clicked() {
                    quit = true;
                }
                let name = self
                    .path
                    .as_deref()
                    .and_then(|p| p.file_name())
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "Untitled".into());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.weak(format!("{name}{}", if self.dirty { " *" } else { "" }));
                });
            });
        });

        let bottom = egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| match &self.error {
                Some(err) => {
                    ui.colored_label(egui::Color32::from_rgb(255, 90, 90), err);
                }
                None => {
                    let lines = self.text.lines().count();
                    let words = self.text.split_whitespace().count();
                    let chars = self.text.chars().count();
                    ui.label(format!("{lines} lines · {words} words · {chars} chars"));
                }
            });
        });

        // The native editor covers the area between the toolbar and the status bar.
        let full = ctx.screen_rect();
        let top_h = top.response.rect.height().max(0.0);
        let bottom_h = bottom.response.rect.height().max(0.0);
        let rect = egui::Rect::from_min_max(
            egui::pos2(full.min.x, full.min.y + top_h),
            egui::pos2(full.max.x, full.max.y - bottom_h),
        );
        if let Some(edit) = self.edit.as_ref() {
            let ppp = ctx.pixels_per_point();
            edit.set_rect(
                (rect.min.x * ppp).round() as i32,
                (rect.min.y * ppp).round() as i32,
                (rect.width() * ppp).round() as i32,
                (rect.height() * ppp).round() as i32,
            );
        }

        self.sync_from_editor();

        // Keep the keyboard in the editor even after clicking the toolbar.
        if let Some(edit) = self.edit.as_ref() {
            if !edit.has_focus() {
                edit.focus();
            }
        }

        // Close handling.
        if ctx.input(|i| i.viewport().close_requested()) {
            if self.confirm_discard(parent) {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            } else {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            }
        } else if quit && self.confirm_discard(parent) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }

        let name = self
            .path
            .as_deref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Untitled".into());
        let title = format!("{}{} - Notepad", if self.dirty { "*" } else { "" }, name);
        if title != self.last_title {
            self.last_title = title.clone();
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));
        }
    }
}
