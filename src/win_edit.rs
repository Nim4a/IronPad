//! Native Win32 multiline EDIT control hosted inside the egui window.
//!
//! Why: egui has no bidi (and until 0.35 no shaping), so Persian/Arabic text
//! renders with disconnected letters in the wrong order. The Windows EDIT
//! control does shaping, bidi, RTL paragraphs and IME for free.

#![cfg(windows)]

use std::ffi::c_void;
use std::sync::atomic::{AtomicPtr, AtomicU8, Ordering};

use windows_sys::Win32::Foundation::{BOOL, HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, CreateFontW, CreateSolidBrush, DEFAULT_CHARSET,
    FF_DONTCARE, FIXED_PITCH, OUT_TT_PRECIS, SetBkColor, SetTextColor,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetFocus, GetKeyState, SetFocus, VK_CONTROL, VK_N, VK_O, VK_S, VK_SHIFT,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallWindowProcW, CreateWindowExW, ES_AUTOVSCROLL, ES_MULTILINE, ES_NOHIDESEL, ES_WANTRETURN,
    EnumWindows, GWLP_WNDPROC, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId,
    HWND_TOP, IsWindowVisible, MB_ICONWARNING, MB_YESNOCANCEL, MessageBoxW, SWP_NOACTIVATE,
    SendMessageW, SetWindowLongPtrW, SetWindowPos, WM_KEYDOWN, WM_SETFONT, WM_SETTEXT, WS_CHILD,
    WS_TABSTOP, WS_VISIBLE,
};

/// Actions the native editor asks the app to perform.
pub const ACT_NEW: u8 = 1;
pub const ACT_OPEN: u8 = 2;
pub const ACT_SAVE: u8 = 3;
pub const ACT_SAVE_AS: u8 = 4;
pub const ACT_QUIT: u8 = 5;

pub static ACTION: AtomicU8 = AtomicU8::new(0);

static ORIG_PROC: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static ORIG_PARENT_PROC: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static DARK_BRUSH: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());

const WM_CTLCOLOREDIT: u32 = 0x0133;
const EDIT_BG: u32 = 0x0018_1818; // COLORREF BGR
const EDIT_FG: u32 = 0x00E0_E0E0;

const EM_SETBKGNDCOLOR: u32 = 0x0401;
const EM_SETTEXTCOLOR: u32 = 0x0402;
const EM_SETLIMITTEXT: u32 = 0x04C5;

/// Find our own top-level window (eframe does not expose the HWND in 0.31).
pub fn find_main_window() -> HWND {
    unsafe extern "system" fn cb(hwnd: HWND, lparam: LPARAM) -> BOOL {
        unsafe {
            let mut pid: u32 = 0;
            GetWindowThreadProcessId(hwnd, &mut pid);
            if pid == std::process::id()
                && IsWindowVisible(hwnd) != 0
                && GetWindowTextLengthW(hwnd) > 0
            {
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
            let hinst = GetModuleHandleW(std::ptr::null());
            let class: Vec<u16> = "EDIT\0".encode_utf16().collect();
            // No ES_AUTOHSCROLL => the control word-wraps like Notepad.
            let style = (WS_CHILD | WS_VISIBLE | WS_TABSTOP)
                | ES_MULTILINE as u32
                | ES_AUTOVSCROLL as u32
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
            SendMessageW(hwnd, EM_SETBKGNDCOLOR, 0x0018_1818, 0); // BGR dark gray
            SendMessageW(hwnd, EM_SETTEXTCOLOR, 0x00E0_E0E0, 0); // BGR light gray

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
            let wide: Vec<u16> = text
                .replace("\r\n", "\n")
                .replace('\n', "\r\n")
                .encode_utf16()
                .collect();
            let mut buf = wide;
            buf.push(0);
            SendMessageW(self.hwnd, WM_SETTEXT, 0, buf.as_ptr() as LPARAM);
        }
    }

    pub fn get_text(&self) -> String {
        unsafe {
            let len = GetWindowTextLengthW(self.hwnd);
            if len <= 0 {
                return String::new();
            }
            let mut buf = vec![0u16; len as usize + 1];
            let n = GetWindowTextW(self.hwnd, buf.as_mut_ptr(), buf.len() as i32);
            if n <= 0 {
                return String::new();
            }
            buf.truncate(n as usize);
            String::from_utf16_lossy(&buf).replace("\r\n", "\n")
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
            SetTextColor(hdc, EDIT_FG);
            SetBkColor(hdc, EDIT_BG);
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
    if msg == WM_KEYDOWN {
        let (ctrl, shift) = unsafe {
            (
                GetKeyState(VK_CONTROL as i32) < 0,
                GetKeyState(VK_SHIFT as i32) < 0,
            )
        };
        if ctrl {
            let action = match wparam as u16 {
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
