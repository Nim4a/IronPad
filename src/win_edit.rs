//! Native Win32 RichEdit control hosted inside the egui window.
//!
//! Why native: egui cannot shape or order RTL text (Persian/Arabic), and it
//! cannot colour parts of a text either. The Win32 RichEdit control (Uniscribe)
//! gives us shaping, bidi, RTL paragraphs, IME, word wrap, undo/redo — and
//! `EM_FORMATRANGE` lets us paint syntax-highlighted ranges.

#![cfg(windows)]

use std::ffi::c_void;
use std::sync::atomic::{AtomicPtr, AtomicU32, AtomicU8, Ordering};

use windows_sys::Win32::Foundation::{BOOL, HWND, LPARAM, LRESULT, POINTL, RECT, WPARAM};
use windows_sys::Win32::Graphics::Dwm::DwmSetWindowAttribute;
use windows_sys::Win32::Graphics::Gdi::{
    CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, CreateFontW, CreateSolidBrush, DEFAULT_CHARSET,
    DeleteObject, FF_DONTCARE, FIXED_PITCH, FillRect, HDC, InvalidateRect, OUT_TT_PRECIS, RedrawWindow,
    SetBkColor, SetSysColors, SetTextColor, UpdateWindow,
};
use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleW, LoadLibraryW};
use windows_sys::Win32::UI::Controls::SetWindowTheme;
use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetFocus, GetKeyState, SetFocus, VK_CONTROL, VK_F, VK_N, VK_O, VK_S, VK_SHIFT,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallWindowProcW, CreateWindowExW, ES_AUTOHSCROLL, ES_AUTOVSCROLL, ES_MULTILINE, ES_NOHIDESEL,
    ES_WANTRETURN, EnumWindows, GWLP_WNDPROC, GetClientRect, GetWindowTextLengthW,
    GetWindowThreadProcessId, HWND_TOP, IsWindowVisible, MB_ICONWARNING, MB_YESNOCANCEL,
    MessageBoxW, SWP_NOACTIVATE, SendMessageW, SetClassLongPtrW, SetWindowLongPtrW, SetWindowPos,
    WM_ERASEBKGND, WM_KEYDOWN, WM_SETFONT, WS_CHILD, WS_TABSTOP, WS_VISIBLE,
};

/// Actions the native editor asks the app to perform.
pub const ACT_NEW: u8 = 1;
pub const ACT_OPEN: u8 = 2;
pub const ACT_SAVE: u8 = 3;
pub const ACT_SAVE_AS: u8 = 4;
pub const ACT_QUIT: u8 = 5;
pub const ACT_FIND: u8 = 6;

pub static ACTION: AtomicU8 = AtomicU8::new(0);

static ORIG_PROC: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static ORIG_PARENT_PROC: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static DARK_BRUSH: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static MSFTEDIT: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());

const WM_CTLCOLOREDIT: u32 = 0x0133;
const EDIT_BG: u32 = 0x0018_1818; // COLORREF (BGR)
const EDIT_FG: u32 = 0x00E0_E0E0;

static CURRENT_BG: AtomicU32 = AtomicU32::new(EDIT_BG);
static CURRENT_FG: AtomicU32 = AtomicU32::new(EDIT_FG);

// Plain edit control messages (winuser.h).
const EM_GETSEL: u32 = 0x00B0;
const EM_SETSEL: u32 = 0x00B1;
const EM_REPLACESEL: u32 = 0x00C2;
const EM_SETLIMITTEXT: u32 = 0x00C5;

// Rich edit messages (riched.h).
const EM_SETWORDBREAK: u32 = 0x040B; // WM_USER + 11
const EM_EXSETSEL: u32 = 0x0437; // WM_USER + 55
const EM_SETBKGNDCOLOR: u32 = 0x0443; // WM_USER + 67
const EM_SETCHARFORMAT: u32 = 0x0444; // WM_USER + 68
const EM_GETTEXTRANGE: u32 = 0x044B; // WM_USER + 75
const EM_POSFROMCHAR: u32 = 0x0436; // WM_USER + 54 (Rich Edit form: POINTL out)
const EM_LINEINDEX: u32 = 0x00BB;
const EM_GETFIRSTVISIBLELINE: u32 = 0x00CE;
const EM_GETLINECOUNT: u32 = 0x00BA;

const CFM_COLOR: u32 = 0x4000_0000;
const CFM_BACKCOLOR: u32 = 0x0400_0000;
const SCF_DEFAULT: usize = 0x0000;
const SCF_SELECTION: usize = 0x0001;
const WB_WRAP: usize = 0;

#[repr(C)]
struct CharRange {
    cp_min: i32,
    cp_max: i32,
}

/// CHARFORMAT2W as defined by the Windows SDK (116 bytes).
#[repr(C)]
struct CharFormat {
    cb_size: u32,
    dw_mask: u32,
    dw_effects: u32,
    y_height: i32,
    y_offset: i32,
    cr_text_color: u32,
    b_char_set: u8,
    b_pitch_and_family: u8,
    sz_face_name: [u16; 32],
    w_weight: u16,
    s_spacing: i16,
    cr_back_color: u32,
    lcid: u32,
    dw_reserved: u32,
    s_style: i16,
    w_kerning: u16,
    b_underline_type: u8,
    b_animation: u8,
    b_rev_author: u8,
    b_underline_color: u8,
}

impl Default for CharFormat {
    fn default() -> Self {
        Self {
            cb_size: std::mem::size_of::<CharFormat>() as u32,
            dw_mask: 0,
            dw_effects: 0,
            y_height: 0,
            y_offset: 0,
            cr_text_color: 0,
            b_char_set: 0,
            b_pitch_and_family: 0,
            sz_face_name: [0; 32],
            w_weight: 400,
            s_spacing: 0,
            cr_back_color: 0,
            lcid: 0,
            dw_reserved: 0,
            s_style: 0,
            w_kerning: 0,
            b_underline_type: 0,
            b_animation: 0,
            b_rev_author: 0,
            b_underline_color: 0,
        }
    }
}

#[repr(C)]
struct TextRange {
    chrg: CharRange,
    lpstr_text: *mut u16,
}

/// Find our own top-level window (eframe does not expose the HWND in 0.31).
pub fn find_main_window() -> HWND {
    unsafe extern "system" fn cb(hwnd: HWND, lparam: LPARAM) -> BOOL {
        unsafe {
            let mut pid: u32 = 0;
            GetWindowThreadProcessId(hwnd, &mut pid);
            let vis = IsWindowVisible(hwnd);
            let len = GetWindowTextLengthW(hwnd);
            if pid == std::process::id() && vis != 0 && len > 0 {
                *(lparam as *mut HWND) = hwnd;
                return 0; // stop enumeration
            }
            1 // keep going
        }
    }
    let mut hwnd: HWND = std::ptr::null_mut();
    unsafe {
        EnumWindows(Some(cb), &mut hwnd as *mut HWND as LPARAM);
    }
    hwnd
}

pub struct Edit {
    hwnd: HWND,
}

impl Edit {
    pub fn new(parent: HWND) -> Option<Edit> {
        unsafe {
            // The RichEdit50W class lives in Msftedit.dll.
            if MSFTEDIT.load(Ordering::Relaxed).is_null() {
                let dll: Vec<u16> = "Msftedit.dll\0".encode_utf16().collect();
                let h = LoadLibraryW(dll.as_ptr());
                MSFTEDIT.store(h as *mut c_void, Ordering::Relaxed);
            }

            let hinst = GetModuleHandleW(std::ptr::null());
            let class: Vec<u16> = "RICHEDIT50W\0".encode_utf16().collect();
            // No ES_AUTOHSCROLL => word wrap.
            let style = (WS_CHILD | WS_VISIBLE | WS_TABSTOP)
                | ES_MULTILINE as u32
                | ES_AUTOVSCROLL as u32
                | ES_AUTOHSCROLL as u32 // no word wrap: line numbers stay aligned
                | ES_WANTRETURN as u32
                | ES_NOHIDESEL as u32;
            let hwnd = CreateWindowExW(
                0,
                class.as_ptr(),
                std::ptr::null(),
                style,
                0,
                0,
                100,
                100,
                parent,
                std::ptr::null_mut(),
                hinst,
                std::ptr::null(),
            );
            if hwnd.is_null() {
                return None;
            }
            SendMessageW(hwnd, EM_SETLIMITTEXT, 0x7FFF_FFFF, 0);
            SendMessageW(hwnd, EM_SETWORDBREAK, WB_WRAP, 0);
            SendMessageW(hwnd, EM_SETBKGNDCOLOR, EDIT_BG as usize, 0);

            // The Rich Edit paints the area outside its text with COLOR_WINDOW.
            let idx = [5i32]; // COLOR_WINDOW
            let sys = [EDIT_BG];
            SetSysColors(1, idx.as_ptr(), sys.as_ptr());

            // Dark RichEdit: Explorer's dark theme + Windows 11 immersive dark mode.
            {
                // Rich Edit 2 ignores EM_SETBKGNDCOLOR: the document background is the
                // default character format's background colour.
                let dflt = CharFormat {
                    dw_mask: CFM_COLOR | CFM_BACKCOLOR,
                    cr_text_color: EDIT_FG,
                    cr_back_color: EDIT_BG,
                    ..Default::default()
                };
                SendMessageW(
                    hwnd,
                    EM_SETCHARFORMAT,
                    SCF_DEFAULT,
                    &dflt as *const CharFormat as LPARAM,
                );

                let dark: i32 = 1;
                let theme: Vec<u16> = "DarkMode_Explorer ".encode_utf16().collect();
                SetWindowTheme(hwnd, theme.as_ptr(), std::ptr::null());
                DwmSetWindowAttribute(
                    hwnd,
                    20, // DWMWA_USE_IMMERSIVE_DARK_MODE
                    &dark as *const i32 as *const c_void,
                    std::mem::size_of::<i32>() as u32,
                );
            }

            let dpi = GetDpiForWindow(parent).max(96);
            let height = -((16 * dpi as i32) / 72);
            let face: Vec<u16> = "Consolas\0".encode_utf16().collect();
            let font = CreateFontW(
                height,
                0,
                0,
                0,
                400,
                0,
                0,
                0,
                DEFAULT_CHARSET as u32,
                OUT_TT_PRECIS as u32,
                CLIP_DEFAULT_PRECIS as u32,
                CLEARTYPE_QUALITY as u32,
                FIXED_PITCH as u32 | FF_DONTCARE as u32,
                face.as_ptr(),
            );
            SendMessageW(hwnd, WM_SETFONT, font as usize as WPARAM, 1);

            // Subclass so Ctrl+N/O/S reach the app (the child has the keyboard focus).
            let prev = SetWindowLongPtrW(hwnd, GWLP_WNDPROC, edit_proc as *const c_void as isize);
            ORIG_PROC.store(prev as *mut c_void, Ordering::Relaxed);

            // Subclass the parent to answer WM_CTLCOLOREDIT => dark editor.
            let brush = CreateSolidBrush(EDIT_BG);
            DARK_BRUSH.store(brush as *mut c_void, Ordering::Relaxed);
            let prev_parent =
                SetWindowLongPtrW(parent, GWLP_WNDPROC, parent_proc as *const c_void as isize);
            ORIG_PARENT_PROC.store(prev_parent as *mut c_void, Ordering::Relaxed);

            // Windows 11 immersive dark mode: dark title bar + dark RichEdit.
            let dark: i32 = 1;
            DwmSetWindowAttribute(
                parent,
                20, // DWMWA_USE_IMMERSIVE_DARK_MODE
                &dark as *const i32 as *const c_void,
                std::mem::size_of::<i32>() as u32,
            );

            // Repaint with the new (dark) background.
            unsafe {
                // The class brush paints the area outside the text extent.
                SetClassLongPtrW(hwnd, -10, brush as isize); // GCLP_HBRBACKGROUND
                InvalidateRect(hwnd, std::ptr::null(), 1);
                RedrawWindow(
                    hwnd,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    0x0001 | 0x0004 | 0x0400,
                );
                UpdateWindow(hwnd);
            }
            SetFocus(hwnd);
            Some(Edit { hwnd })
        }
    }

    pub fn set_rect(&self, x: i32, y: i32, w: i32, h: i32) {
        unsafe {
            SetWindowPos(self.hwnd, HWND_TOP, x, y, w, h, SWP_NOACTIVATE);
        }
    }

    pub fn set_text(&self, text: &str) {
        unsafe {
            // RichEdit wants CR line breaks.
            let wide: Vec<u16> = text
                .replace("\r\n", "\n")
                .replace('\n', "\r")
                .encode_utf16()
                .collect();
            let mut buf = wide;
            buf.push(0);
            SendMessageW(self.hwnd, EM_SETSEL, 0, -1);
            SendMessageW(self.hwnd, EM_REPLACESEL, 1, buf.as_ptr() as LPARAM);
            SendMessageW(self.hwnd, EM_SETSEL, 0, 0);
        }
    }

    pub fn get_text(&self) -> String {
        unsafe {
            let mut cap = 4096usize;
            loop {
                let mut buf = vec![0u16; cap];
                let mut tr = TextRange {
                    chrg: CharRange {
                        cp_min: 0,
                        cp_max: -1,
                    },
                    lpstr_text: buf.as_mut_ptr(),
                };
                let n = SendMessageW(
                    self.hwnd,
                    EM_GETTEXTRANGE,
                    0,
                    &mut tr as *mut TextRange as LPARAM,
                );
                if n <= 0 {
                    // variant B: -1..-1
                    let mut tr2 = TextRange {
                        chrg: CharRange {
                            cp_min: -1,
                            cp_max: -1,
                        },
                        lpstr_text: buf.as_mut_ptr(),
                    };
                    let _n2 = SendMessageW(
                        self.hwnd,
                        EM_GETTEXTRANGE,
                        0,
                        &mut tr2 as *mut TextRange as LPARAM,
                    );
                }
                if n <= 0 {
                    return String::new();
                }
                let n = n as usize;
                if n < cap {
                    buf.truncate(n);
                    return String::from_utf16_lossy(&buf)
                        .replace("\r\n", "\n")
                        .replace('\r', "\n");
                }
                cap *= 2;
                if cap > 64 * 1024 * 1024 {
                    return String::new();
                }
            }
        }
    }

    /// Paint a range of characters in `color` (COLORREF / BGR).
    pub fn color_range(&self, start: i32, end: i32, color: u32) {
        if end <= start {
            return;
        }
        unsafe {
            // Remember the caret.
            let mut sel = [0i32; 2];
            SendMessageW(
                self.hwnd,
                EM_GETSEL,
                sel.as_mut_ptr() as WPARAM,
                (sel.as_mut_ptr() as *mut i32).add(1) as LPARAM,
            );
            // Select the range, colour it, then restore the caret.
            let range = CharRange {
                cp_min: start,
                cp_max: end,
            };
            SendMessageW(
                self.hwnd,
                EM_EXSETSEL,
                0,
                &range as *const CharRange as LPARAM,
            );
            let fmt = CharFormat {
                cb_size: std::mem::size_of::<CharFormat>() as u32,
                dw_mask: CFM_COLOR | CFM_BACKCOLOR,
                cr_text_color: color,
                cr_back_color: EDIT_BG,
                ..Default::default()
            };
            let _r = SendMessageW(
                self.hwnd,
                EM_SETCHARFORMAT,
                SCF_SELECTION,
                &fmt as *const CharFormat as LPARAM,
            );
            let restore = CharRange {
                cp_min: sel[0],
                cp_max: sel[1],
            };
            SendMessageW(
                self.hwnd,
                EM_EXSETSEL,
                0,
                &restore as *const CharRange as LPARAM,
            );
        }
    }

    /// Current selection (start, end) in characters.
    pub fn get_selection(&self) -> (i32, i32) {
        unsafe {
            let mut sel = [0i32; 2];
            SendMessageW(
                self.hwnd,
                EM_GETSEL,
                sel.as_mut_ptr() as WPARAM,
                (sel.as_mut_ptr() as *mut i32).add(1) as LPARAM,
            );
            (sel[0], sel[1])
        }
    }

    /// Set selection to [start, end).
    pub fn set_selection(&self, start: i32, end: i32) {
        unsafe {
            let range = CharRange {
                cp_min: start,
                cp_max: end,
            };
            SendMessageW(
                self.hwnd,
                EM_EXSETSEL,
                0,
                &range as *const CharRange as LPARAM,
            );
        }
    }

    /// Change editor colours at runtime (theme switch).
    pub fn set_colors(&self, bg: u32, fg: u32) {
        CURRENT_BG.store(bg, Ordering::Relaxed);
        CURRENT_FG.store(fg, Ordering::Relaxed);
        unsafe {
            // EM_SETBKGNDCOLOR: wParam=0 (use lParam color), lParam=COLORREF.
            SendMessageW(self.hwnd, EM_SETBKGNDCOLOR, 0, bg as LPARAM);
            let dflt = CharFormat {
                dw_mask: CFM_COLOR | CFM_BACKCOLOR,
                cr_text_color: fg,
                cr_back_color: bg,
                ..Default::default()
            };
            SendMessageW(
                self.hwnd,
                EM_SETCHARFORMAT,
                SCF_DEFAULT,
                &dflt as *const CharFormat as LPARAM,
            );
            // Recreate the background brush used by WM_ERASEBKGND / WM_CTLCOLOREDIT.
            let brush = CreateSolidBrush(bg);
            let old = DARK_BRUSH.swap(brush as *mut c_void, Ordering::Relaxed);
            if !old.is_null() {
                DeleteObject(old);
            }
            // The class brush paints the area outside the text extent.
            SetClassLongPtrW(self.hwnd, -10, brush as isize); // GCLP_HBRBACKGROUND
            // COLOR_WINDOW is global; point it at the new bg too.
            let idx = [5i32];
            let sys = [bg];
            SetSysColors(1, idx.as_ptr(), sys.as_ptr());
            InvalidateRect(self.hwnd, std::ptr::null(), 1);
            RedrawWindow(
                self.hwnd,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                0x0001 | 0x0004 | 0x0400,
            );
            UpdateWindow(self.hwnd);
        }
    }

    pub fn first_visible_line(&self) -> usize {
        unsafe { SendMessageW(self.hwnd, EM_GETFIRSTVISIBLELINE, 0, 0) as usize }
    }

    pub fn line_count(&self) -> usize {
        unsafe { SendMessageW(self.hwnd, EM_GETLINECOUNT, 0, 0) as usize }
    }

    /// Client-area pixel position of the start of `line` (0-based).
    pub fn line_pos(&self, line: usize) -> (i32, i32) {
        unsafe {
            let idx = SendMessageW(self.hwnd, EM_LINEINDEX, line, 0);
            if idx < 0 {
                return (0, 0);
            }
            let mut pt = POINTL { x: 0, y: 0 };
            SendMessageW(
                self.hwnd,
                EM_POSFROMCHAR,
                idx as usize,
                &mut pt as *mut POINTL as isize,
            );
            (pt.x, pt.y)
        }
    }

    pub fn has_focus(&self) -> bool {
        unsafe { GetFocus() == self.hwnd }
    }

    pub fn focus(&self) {
        unsafe {
            SetFocus(self.hwnd);
        }
    }

    /// Native "you have unsaved changes" prompt: Save / Don't save / Cancel.
    pub fn confirm_discard(parent: HWND) -> Confirm {
        unsafe {
            let text: Vec<u16> = "Save changes before closing?\0".encode_utf16().collect();
            let caption: Vec<u16> = "Notepad\0".encode_utf16().collect();
            match MessageBoxW(
                parent,
                text.as_ptr(),
                caption.as_ptr(),
                MB_YESNOCANCEL | MB_ICONWARNING,
            ) {
                6 => Confirm::Save,    // IDYES
                7 => Confirm::Discard, // IDNO
                _ => Confirm::Cancel,  // IDCANCEL
            }
        }
    }
}

pub enum Confirm {
    Save,
    Discard,
    Cancel,
}

unsafe extern "system" fn parent_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if msg == WM_CTLCOLOREDIT {
        unsafe {
            let hdc = wparam as windows_sys::Win32::Graphics::Gdi::HDC;
            SetTextColor(hdc, CURRENT_FG.load(Ordering::Relaxed));
            SetBkColor(hdc, CURRENT_BG.load(Ordering::Relaxed));
            return DARK_BRUSH.load(Ordering::Relaxed) as LRESULT;
        }
    }
    let orig = ORIG_PARENT_PROC.load(Ordering::Relaxed);
    unsafe {
        CallWindowProcW(
            Some(std::mem::transmute::<
                *mut c_void,
                unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT,
            >(orig)),
            hwnd,
            msg,
            wparam,
            lparam,
        )
    }
}

unsafe extern "system" fn edit_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if msg == WM_ERASEBKGND {
        // Paint the editor background dark (Rich Edit ignores EM_SETBKGNDCOLOR).
        let brush = DARK_BRUSH.load(Ordering::Relaxed);
        if !brush.is_null() {
            unsafe {
                let mut rect = RECT {
                    left: 0,
                    top: 0,
                    right: 0,
                    bottom: 0,
                };
                if lparam != 0 {
                    rect = *(lparam as *const RECT);
                } else {
                    GetClientRect(hwnd, &mut rect);
                }
                FillRect(wparam as HDC, &rect, brush);
            }
            return 1;
        }
    }
    if msg == WM_KEYDOWN {
        let (ctrl, shift) = unsafe {
            (
                GetKeyState(VK_CONTROL as i32) < 0,
                GetKeyState(VK_SHIFT as i32) < 0,
            )
        };
        if ctrl {
            let action = match wparam as u16 {
                x if x == VK_F => ACT_FIND,
                x if x == VK_S => {
                    if shift {
                        ACT_SAVE_AS
                    } else {
                        ACT_SAVE
                    }
                }
                x if x == VK_O => ACT_OPEN,
                x if x == VK_N => ACT_NEW,
                _ => 0,
            };
            if action != 0 {
                ACTION.store(action, Ordering::Relaxed);
                return 0;
            }
        }
    }
    let orig = ORIG_PROC.load(Ordering::Relaxed);
    CallWindowProcW(
        Some(std::mem::transmute::<
            *mut c_void,
            unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT,
        >(orig)),
        hwnd,
        msg,
        wparam,
        lparam,
    )
}
