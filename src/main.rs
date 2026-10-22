#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;

use eframe::egui;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Notepad")
            .with_inner_size([960.0, 640.0])
            .with_min_inner_size([480.0, 320.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Notepad",
        options,
        Box::new(|cc| Ok(Box::new(Notepad::new(cc)))),
    )
}

#[derive(Clone, Copy, PartialEq)]
enum Action {
    Close,
    New,
}

struct Notepad {
    text: String,
    path: Option<PathBuf>,
    dirty: bool,
    pending: Option<Action>,
    error: Option<String>,
    last_title: String,
}

impl Notepad {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        cc.egui_ctx.set_visuals(egui::Visuals::dark());
        setup_fonts(&cc.egui_ctx);
        Self {
            text: String::new(),
            path: None,
            dirty: false,
            pending: None,
            error: None,
            last_title: String::new(),
        }
    }

    fn open(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter(
                "Text files",
                &["txt", "md", "rs", "toml", "json", "csv", "xml", "html", "css", "js", "log"],
            )
            .pick_file()
        {
            match std::fs::read_to_string(&path) {
                Ok(s) => {
                    self.text = s;
                    self.path = Some(path);
                    self.dirty = false;
                    self.error = None;
                }
                Err(e) => self.error = Some(format!("Open failed: {e}")),
            }
        }
    }

    fn save(&mut self, save_as: bool) -> bool {
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

    fn request(&mut self, ctx: &egui::Context, action: Action) {
        if self.dirty {
            self.pending = Some(action);
        } else {
            self.do_action(ctx, action);
        }
    }

    fn do_action(&mut self, ctx: &egui::Context, action: Action) {
        match action {
            Action::Close => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            Action::New => {
                self.text.clear();
                self.path = None;
                self.dirty = false;
                self.error = None;
            }
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
        let ctrl = egui::Modifiers::COMMAND; // Ctrl on Windows
        let new_pressed = shortcut(ctx, ctrl, egui::Key::N);
        let open_pressed = shortcut(ctx, ctrl, egui::Key::O);
        let save_pressed = shortcut(ctx, ctrl, egui::Key::S);
        let save_as_pressed = shortcut(ctx, ctrl | egui::Modifiers::SHIFT, egui::Key::S);

        // Intercept window close while dirty.
        if ctx.input(|i| i.viewport().close_requested()) {
            if self.dirty {
                self.pending = Some(Action::Close);
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            }
        }

        let mut open = false;
        let mut save = false;
        let mut save_as = false;

        egui::TopBottomPanel::top("menu").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("New\tCtrl+N").clicked() {
                        self.request(ctx, Action::New);
                        ui.close_menu();
                    }
                    if ui.button("Open…\tCtrl+O").clicked() {
                        open = true;
                        ui.close_menu();
                    }
                    if ui.button("Save\tCtrl+S").clicked() {
                        save = true;
                        ui.close_menu();
                    }
                    if ui.button("Save As…\tCtrl+Shift+S").clicked() {
                        save_as = true;
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Quit").clicked() {
                        ui.close_menu();
                        self.request(ctx, Action::Close);
                    }
                });
                if self.path.is_some() {
                    let name = self
                        .path
                        .as_deref()
                        .and_then(|p| p.file_name())
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.weak(format!("{name}{}", if self.dirty { " *" } else { "" }));
                    });
                }
            });
        });

        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                match &self.error {
                    Some(err) => {
                        ui.colored_label(egui::Color32::from_rgb(255, 90, 90), err);
                    }
                    None => {
                        let lines = self.text.lines().count();
                        let words = self.text.split_whitespace().count();
                        let chars = self.text.chars().count();
                        ui.label(format!("{lines} lines · {words} words · {chars} chars"));
                    }
                }
            });
        });

        if let Some(action) = self.pending {
            let mut choice: Option<u8> = None;
            egui::Window::new("Unsaved changes")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.label("You have unsaved changes. Save them before continuing?");
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button("Save").clicked() {
                            choice = Some(0);
                        }
                        if ui.button("Discard").clicked() {
                            choice = Some(1);
                        }
                        if ui.button("Cancel").clicked() {
                            choice = Some(2);
                        }
                    });
                });
            match choice {
                Some(0) => {
                    if self.save(false) {
                        self.pending = None;
                        self.do_action(ctx, action);
                    }
                }
                Some(1) => {
                    self.dirty = false;
                    self.pending = None;
                    self.do_action(ctx, action);
                }
                Some(2) => self.pending = None,
                _ => {}
            }
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            let response = ui.add_sized(
                [ui.available_width(), ui.available_height()],
                egui::TextEdit::multiline(&mut self.text)
                    .font(egui::TextStyle::Monospace)
                    .desired_width(f32::INFINITY)
                    .hint_text("Start typing…"),
            );
            if response.changed() {
                self.dirty = true;
            }
        });

        if new_pressed {
            self.request(ctx, Action::New);
        }
        if open_pressed {
            self.open();
        }
        if save_pressed {
            self.save(false);
        }
        if save_as_pressed {
            self.save(true);
        }
        if open {
            self.open();
        }
        if save {
            self.save(false);
        }
        if save_as {
            self.save(true);
        }

        let name = self
            .path
            .as_deref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Untitled".into());
        let title = format!(
            "{}{} - Notepad",
            if self.dirty { "*" } else { "" },
            name
        );
        if title != self.last_title {
            self.last_title = title.clone();
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));
        }
    }
}