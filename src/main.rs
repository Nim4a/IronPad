#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! A modern notepad for Windows.
//!
//! The chrome (toolbar, tabs, status bar) is egui; the text area is the native
//! Win32 RichEdit control, because egui cannot shape or order RTL text
//! (Persian/Arabic) and cannot colour parts of a text either. The OS control
//! does shaping, bidi, RTL paragraphs, IME — and `EM_FORMATRANGE` lets us paint
//! syntax-highlighted ranges.
//!
//! egui is pinned to 0.31 on purpose: 0.35+ ships a new renderer (vello) that
//! wedges the OpenGL driver on this machine.

mod win_edit;

use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use eframe::egui;
use syntect::easy::HighlightLines;
use syntect::highlighting::{Color, ThemeSet};
use syntect::parsing::{SyntaxReference, SyntaxSet};
use syntect::util::LinesWithEndings;
use win_edit::Edit;

const HIGHLIGHT_DELAY: Duration = Duration::from_millis(300);

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("IronPad")
            .with_inner_size([960.0, 640.0])
            .with_min_inner_size([480.0, 320.0]),
        ..Default::default()
    };
    // `notepad.exe path.txt` opens that file at startup.
    let startup_file = std::env::args().nth(1).map(PathBuf::from);
    eframe::run_native(
        "IronPad",
        options,
        Box::new(move |cc| {
            let mut app = IronPad::new(cc);
            match startup_file {
                Some(path) => app.open_path(path),
                None => {
                    app.tabs.push(Tab::default());
                    app.active = 0;
                }
            }
            Ok(Box::new(app))
        }),
    )
}

#[derive(Default)]
struct Tab {
    path: Option<PathBuf>,
    text: String,
    dirty: bool,
}

impl Tab {
    fn label(&self) -> String {
        let name = self
            .path
            .as_deref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Untitled".into());
        format!("{name}{}", if self.dirty { " *" } else { "" })
    }
}

enum TabAction {
    Switch(usize),
    Close(usize),
    New,
}

struct IronPad {
    tabs: Vec<Tab>,
    active: usize,
    error: Option<String>,
    last_title: String,
    themed: bool,
    edit: Option<Edit>,
    parent_hwnd: Option<windows_sys::Win32::Foundation::HWND>,
    pending_editor: Option<String>,
    /// Syntax definitions, loaded in the background (they are slow to parse).
    syntax_rx: Option<mpsc::Receiver<(SyntaxSet, ThemeSet)>>,
    syntaxes: Option<(SyntaxSet, ThemeSet)>,
    highlight_dirty: bool,
    last_edit: Instant,
}

impl IronPad {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        cc.egui_ctx.set_theme(egui::ThemePreference::Dark);
        setup_fonts(&cc.egui_ctx);
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let ps = SyntaxSet::load_defaults_newlines();
            let ts = ThemeSet::load_defaults();
            let _ = tx.send((ps, ts));
        });
        Self {
            tabs: Vec::new(),
            active: 0,
            error: None,
            last_title: String::new(),
            themed: false,
            edit: None,
            parent_hwnd: None,
            pending_editor: None,
            syntax_rx: Some(rx),
            syntaxes: None,
            highlight_dirty: false,
            last_edit: Instant::now(),
        }
    }

    fn tab(&self) -> &Tab {
        &self.tabs[self.active]
    }

    /// Put text into the native editor (next frame, once it exists).
    fn push_to_editor(&mut self, text: String) {
        self.pending_editor = Some(text);
        self.highlight_dirty = true;
    }

    /// Pull the latest text out of the native editor.
    fn sync_from_editor(&mut self) {
        // Poll periodically even without focus, so the counters stay correct.
        let focused = self.edit.as_ref().map(|e| e.has_focus()).unwrap_or(false);
        if !focused && self.last_edit.elapsed() < Duration::from_millis(400) {
            return;
        }
        if let Some(edit) = self.edit.as_ref() {
            let t = edit.get_text().replace("\r\n", "\n").replace('\r', "\n");
            let tab = &mut self.tabs[self.active];
            if t != tab.text {
                tab.text = t;
                tab.dirty = true;
                self.highlight_dirty = true;
                self.last_edit = Instant::now();
            }
        }
    }

    fn open_path(&mut self, path: PathBuf) {
        // Already open? Just switch to it.
        if let Some(i) = self
            .tabs
            .iter()
            .position(|t| t.path.as_deref() == Some(&path))
        {
            self.switch_to(i);
            return;
        }
        match std::fs::read_to_string(&path) {
            Ok(s) => {
                self.tabs.push(Tab {
                    path: Some(path),
                    text: s,
                    dirty: false,
                });
                let idx = self.tabs.len() - 1;
                self.active = idx;
                let text = self.tabs[idx].text.clone();
                self.push_to_editor(text);
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
            self.open_path(path);
        }
    }

    fn switch_to(&mut self, idx: usize) {
        if idx >= self.tabs.len() {
            return;
        }
        self.sync_from_editor();
        if idx == self.active {
            return;
        }
        self.active = idx;
        let text = self.tabs[idx].text.clone();
        self.push_to_editor(text);
    }

    fn new_tab(&mut self) {
        self.sync_from_editor();
        self.tabs.push(Tab::default());
        self.active = self.tabs.len() - 1;
        self.push_to_editor(String::new());
    }

    fn close_tab(&mut self, idx: usize, parent: windows_sys::Win32::Foundation::HWND) {
        if idx >= self.tabs.len() {
            return;
        }
        if idx == self.active {
            self.sync_from_editor();
        }
        if self.tabs[idx].dirty {
            match Edit::confirm_discard(parent) {
                win_edit::Confirm::Save => {
                    if !self.save_tab(idx, false) {
                        return;
                    }
                }
                win_edit::Confirm::Discard => {}
                win_edit::Confirm::Cancel => return,
            }
        }
        self.tabs.remove(idx);
        if self.tabs.is_empty() {
            self.tabs.push(Tab::default());
        }
        if self.active >= self.tabs.len() {
            self.active = self.tabs.len() - 1;
        }
        let text = self.tabs[self.active].text.clone();
        self.push_to_editor(text);
    }

    fn save_tab(&mut self, idx: usize, save_as: bool) -> bool {
        if idx >= self.tabs.len() {
            return false;
        }
        if idx == self.active {
            self.sync_from_editor();
        }
        let tab = &mut self.tabs[idx];
        let path = if save_as || tab.path.is_none() {
            let name = tab
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
            tab.path.clone().unwrap()
        };
        match std::fs::write(&path, &tab.text) {
            Ok(()) => {
                self.tabs[idx].path = Some(path);
                self.tabs[idx].dirty = false;
                self.error = None;
                true
            }
            Err(e) => {
                self.error = Some(format!("Save failed: {e}"));
                false
            }
        }
    }

    /// Save every dirty tab (for quitting).
    fn save_all_dirty(&mut self) -> bool {
        for i in 0..self.tabs.len() {
            if self.tabs[i].dirty && !self.save_tab(i, false) {
                return false;
            }
        }
        true
    }

    fn any_dirty(&self) -> bool {
        self.tabs.iter().any(|t| t.dirty)
    }

    /// Syntax highlighting for the active tab, applied to the native editor.
    fn highlight(&mut self) -> bool {
        if self.syntaxes.is_none() {
            if let Some(rx) = self.syntax_rx.as_ref() {
                if let Ok(msg) = rx.try_recv() {
                    self.syntaxes = Some(msg);
                }
            }
        }
        let (ps, ts) = match self.syntaxes.as_ref() {
            Some(s) => s,
            None => return false, // syntaxes still loading
        };
        let theme = &ts.themes["base16-ocean.dark"];
        let syntax: &SyntaxReference = match self.tab().path.as_deref() {
            Some(p) => ps
                .find_syntax_by_extension(p.extension().and_then(|e| e.to_str()).unwrap_or(""))
                .or_else(|| {
                    ps.find_syntax_by_first_line(self.tab().text.lines().next().unwrap_or(""))
                })
                .unwrap_or_else(|| ps.find_syntax_plain_text()),
            None => ps.find_syntax_plain_text(),
        };
        if syntax.name == "Plain Text" {
            return true;
        }

        let text = self.tab().text.clone();
        let mut hl = HighlightLines::new(syntax, theme);
        let mut ranges: Vec<(i32, i32, u32)> = Vec::new();
        let mut base = 0usize; // UTF-16 offset of the current line
        for line in LinesWithEndings::from(&text) {
            let ranges_here = match hl.highlight_line(line, ps) {
                Ok(r) => r,
                Err(_) => break,
            };
            let mut off = base;
            for (style, piece) in ranges_here {
                let len = piece.encode_utf16().count();
                if len > 0 {
                    let color = style.foreground;
                    if color.a > 0 {
                        ranges.push((off as i32, (off + len) as i32, colorref(color)));
                    }
                }
                off += len;
            }
            base += line.encode_utf16().count();
        }

        if let Some(edit) = self.edit.as_ref() {
            for (start, end, color) in ranges {
                edit.color_range(start, end, color);
            }
        }
        true
    }
}

/// syntect colour -> Win32 COLORREF (0x00BBGGRR).
fn colorref(c: Color) -> u32 {
    ((c.b as u32) << 16) | ((c.g as u32) << 8) | c.r as u32
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

impl eframe::App for IronPad {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // eframe applies the system theme after App::new, so force dark on frame 1.
        if !self.themed {
            ctx.set_theme(egui::ThemePreference::Dark);
            // Give panels (toolbar, tabs) a distinct fill so the chrome reads
            // separately from the editor surface.
            ctx.style_mut(|st| {
                st.visuals.panel_fill = egui::Color32::from_rgb(0x23, 0x26, 0x2c);
                st.visuals.window_fill = egui::Color32::from_rgb(0x1b, 0x1d, 0x22);
            });
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

        if self.last_title.is_empty() {}
        if self.edit.is_none() && !parent.is_null() {
            if let Some(edit) = Edit::new(parent) {
                self.edit = Some(edit);
            }
        }
        // Push pending text into the editor once it exists.
        if let Some(text) = self.pending_editor.take() {
            if let Some(edit) = self.edit.as_ref() {
                edit.set_text(&text);
                self.highlight_dirty = true;
            } else {
                self.pending_editor = Some(text);
            }
        }

        // Shortcuts pressed inside the native editor arrive through the subclass.
        let mut quit = false;
        match win_edit::ACTION.swap(0, Ordering::Relaxed) {
            win_edit::ACT_NEW => self.new_tab(),
            win_edit::ACT_OPEN => self.open_file(),
            win_edit::ACT_SAVE => {
                self.save_tab(self.active, false);
            }
            win_edit::ACT_SAVE_AS => {
                self.save_tab(self.active, true);
            }
            win_edit::ACT_QUIT => quit = true,
            _ => {}
        }

        // Same shortcuts when the egui part has the keyboard focus.
        let ctrl = egui::Modifiers::COMMAND;
        if shortcut(ctx, ctrl, egui::Key::N) {
            self.new_tab();
        }
        if shortcut(ctx, ctrl, egui::Key::O) {
            self.open_file();
        }
        if shortcut(ctx, ctrl, egui::Key::S) {
            self.save_tab(self.active, false);
        }
        if shortcut(ctx, ctrl | egui::Modifiers::SHIFT, egui::Key::S) {
            self.save_tab(self.active, true);
        }
        if shortcut(ctx, ctrl, egui::Key::W) {
            let idx = self.active;
            self.close_tab(idx, parent);
        }

        let mut tab_action: Option<TabAction> = None;

        let _top = egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("New").clicked() {
                    tab_action = Some(TabAction::New);
                }
                if ui.button("Open").clicked() {
                    self.open_file();
                }
                if ui.button("Save").clicked() {
                    self.save_tab(self.active, false);
                }
                if ui.button("Save As").clicked() {
                    self.save_tab(self.active, true);
                }
                ui.separator();
                if ui.button("Quit").clicked() {
                    quit = true;
                }
                let name = self.tab().label();
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.weak(name);
                });
            });
        });

        let labels: Vec<String> = self.tabs.iter().map(|t| t.label()).collect();
        let active = self.active;
        let _tabs = egui::TopBottomPanel::top("tabs").show(ctx, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.add_space(4.0);
                for (i, label) in labels.iter().enumerate() {
                    let active_tab = i == active;
                    let galley = ui.painter().layout_no_wrap(
                        label.clone(),
                        egui::TextStyle::Body.resolve(ui.style()),
                        egui::Color32::WHITE,
                    );
                    let (rect, resp) = ui.allocate_exact_size(
                        egui::vec2(galley.mesh_bounds.width() + 34.0, 26.0),
                        egui::Sense::click(),
                    );
                    let fill = if active_tab {
                        egui::Color32::from_rgb(0x35, 0x3a, 0x44)
                    } else {
                        egui::Color32::from_rgb(0x1b, 0x1d, 0x22)
                    };
                    ui.painter().rect_filled(rect, 4.0, fill);
                    ui.painter().galley(
                        rect.left_center() + egui::vec2(8.0, -galley.size().y / 2.0),
                        galley,
                        if active_tab {
                            egui::Color32::WHITE
                        } else {
                            egui::Color32::from_rgb(0xaa, 0xb0, 0xba)
                        },
                    );
                    // close x
                    let xpos = rect.right_center() - egui::vec2(12.0, 0.0);
                    ui.painter().text(
                        xpos,
                        egui::Align2::CENTER_CENTER,
                        "x",
                        egui::TextStyle::Body.resolve(ui.style()),
                        egui::Color32::from_rgb(0x88, 0x8e, 0x98),
                    );
                    if resp.clicked() {
                        // click on right 20px closes, otherwise switches
                        if resp
                            .interact_pointer_pos()
                            .map(|p| p.x > xpos.x - 8.0)
                            .unwrap_or(false)
                        {
                            tab_action = Some(TabAction::Close(i));
                        } else {
                            tab_action = Some(TabAction::Switch(i));
                        }
                    }
                }
                if ui.small_button("+").clicked() {
                    tab_action = Some(TabAction::New);
                }
            });
            ui.add_space(2.0);
        });

        match tab_action {
            Some(TabAction::Switch(i)) => self.switch_to(i),
            Some(TabAction::Close(i)) => self.close_tab(i, parent),
            Some(TabAction::New) => self.new_tab(),
            None => {}
        }

        let bottom = egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| match &self.error {
                Some(err) => {
                    ui.colored_label(egui::Color32::from_rgb(255, 90, 90), err);
                }
                None => {
                    let text = &self.tab().text;
                    let lines = text.lines().count();
                    let words = text.split_whitespace().count();
                    let chars = text.chars().count();
                    let n_tabs = self.tabs.len();
                    ui.label(format!(
                        "{lines} lines · {words} words · {chars} chars · {n_tabs} tabs"
                    ));
                }
            });
        });

        // The native editor covers the area below the toolbar and the tab strip.
        // egui has to paint that area itself: the Rich Edit only paints its own
        // text extent, the rest of the window would stay white.
        let ppp = ctx.pixels_per_point();
        let mut gutter_w = 0.0f32;
        let rect = egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ctx, |ui| {
                let bg = ctx.style().visuals.extreme_bg_color;
                ui.painter().rect_filled(ui.max_rect(), 0.0, bg);
                // Line-number gutter (egui-drawn; the native editor is shifted right).
                if let Some(edit) = self.edit.as_ref() {
                    let total = edit.line_count();
                    if total > 0 {
                        let font = egui::TextStyle::Monospace.resolve(ui.style());
                        let digits = total.to_string().len();
                        let sample = ui.painter().layout_no_wrap(
                            "8".repeat(digits),
                            font.clone(),
                            egui::Color32::WHITE,
                        );
                        gutter_w = sample.size().x + 16.0;
                        let area = ui.max_rect();
                        ui.painter().rect_filled(
                            egui::Rect::from_min_size(
                                area.min,
                                egui::vec2(gutter_w, area.height()),
                            ),
                            0.0,
                            egui::Color32::from_rgb(0x23, 0x26, 0x2c),
                        );
                        let first = edit.first_visible_line();
                        for line in first..total {
                            let (_, ly) = edit.line_pos(line);
                            if ly < 0 {
                                continue;
                            }
                            let y = area.min.y + ly as f32 / ppp + 2.0;
                            if y > area.bottom() {
                                break;
                            }
                            ui.painter().text(
                                egui::pos2(area.min.x + gutter_w - 8.0, y),
                                egui::Align2::RIGHT_TOP,
                                (line + 1).to_string(),
                                font.clone(),
                                egui::Color32::from_rgb(0x88, 0x8e, 0x98),
                            );
                        }
                    }
                }
            })
            .response
            .rect;
        if let Some(edit) = self.edit.as_ref() {
            edit.set_rect(
                ((rect.min.x + gutter_w) * ppp).round() as i32,
                (rect.min.y * ppp).round() as i32,
                ((rect.width() - gutter_w) * ppp).round() as i32,
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

        // Keep painting until the native editor exists (it needs our HWND).
        if self.edit.is_none() {
            ctx.request_repaint();
        }

        // Re-highlight a moment after the last edit.
        if self.highlight_dirty {
            if self.last_edit.elapsed() > HIGHLIGHT_DELAY {
                if self.highlight() {
                    self.highlight_dirty = false;
                } else {
                    // syntaxes not loaded yet: try again shortly
                    ctx.request_repaint_after(Duration::from_millis(200));
                }
            } else {
                ctx.request_repaint_after(HIGHLIGHT_DELAY);
            }
        } else {
            // Keep the counters in sync with the editor.
            ctx.request_repaint_after(Duration::from_millis(400));
        }

        // Close handling.
        if ctx.input(|i| i.viewport().close_requested()) {
            if self.any_dirty() {
                match Edit::confirm_discard(parent) {
                    win_edit::Confirm::Save => {
                        if self.save_all_dirty() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        } else {
                            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                        }
                    }
                    win_edit::Confirm::Discard => {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    win_edit::Confirm::Cancel => {
                        ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                    }
                }
            } else {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        } else if quit {
            if self.any_dirty() {
                match Edit::confirm_discard(parent) {
                    win_edit::Confirm::Save => {
                        if self.save_all_dirty() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    }
                    win_edit::Confirm::Discard => {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    win_edit::Confirm::Cancel => {}
                }
            } else {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }

        let title = format!("{} - IronPad", self.tab().label());
        if title != self.last_title {
            self.last_title = title.clone();
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));
        }
    }
}
