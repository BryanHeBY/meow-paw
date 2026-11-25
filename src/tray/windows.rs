//! Shell notification icons, backed by a hidden top-level Win32 window.
use super::{Signals, paw_pixel};
use std::{
    cell::Cell,
    mem::size_of,
    ptr::{null, null_mut},
    sync::{Arc, atomic::Ordering},
};
use windows_sys::{
    Win32::{
        Foundation::{HWND, LPARAM, LRESULT, WPARAM},
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            Shell::{
                NIF_ICON, NIF_MESSAGE, NIF_SHOWTIP, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_SETVERSION,
                NIN_SELECT, NINF_KEY, NOTIFYICON_VERSION_4, NOTIFYICONDATAW, Shell_NotifyIconW,
            },
            WindowsAndMessaging::{
                CREATESTRUCTW, CreateIcon, CreateWindowExW, DefWindowProcW, DestroyIcon,
                DispatchMessageW, FindWindowW, GWLP_USERDATA, GetMessageW, GetWindowLongPtrW,
                HICON, MSG, RegisterClassW, RegisterWindowMessageW, SetTimer, SetWindowLongPtrW,
                TranslateMessage, UnregisterClassW, WM_APP, WM_CONTEXTMENU, WM_DESTROY,
                WM_LBUTTONUP, WM_NCCREATE, WM_TIMER, WNDCLASSW,
            },
        },
    },
    core::w,
};

const NIN_KEYSELECT: u32 = NIN_SELECT | NINF_KEY;
const CALLBACK: u32 = WM_APP + 3;
const ID: u32 = 1;

struct TrayState {
    shell: Cell<HWND>,
    icon: HICON,
    signals: Arc<Signals>,
    taskbar_created: u32,
}

impl TrayState {
    fn new(signals: Arc<Signals>) -> Self {
        Self {
            shell: Cell::new(null_mut()),
            icon: null_mut(),
            signals,
            taskbar_created: unsafe { RegisterWindowMessageW(w!("TaskbarCreated")) },
        }
    }

    fn data(&self, window: HWND) -> NOTIFYICONDATAW {
        let mut data = NOTIFYICONDATAW {
            cbSize: size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: window,
            hIcon: self.icon,
            uID: ID,
            uFlags: NIF_ICON | NIF_MESSAGE | NIF_SHOWTIP | NIF_TIP,
            uCallbackMessage: CALLBACK,
            ..Default::default()
        };
        data.szTip[..8].copy_from_slice(&[77, 101, 111, 119, 32, 80, 97, 119]);
        data
    }

    fn retry(&self, window: HWND, forced: bool) {
        let shell = unsafe { FindWindowW(w!("Shell_TrayWnd"), null()) };
        if !forced && self.shell.get() == shell && self.signals.ready() {
            return;
        }
        self.signals.ready.store(false, Ordering::Release);
        self.shell.set(shell);
        if shell.is_null() || self.icon.is_null() {
            return;
        }
        let mut data = self.data(window);
        if unsafe { Shell_NotifyIconW(NIM_ADD, &data) } != 0 {
            data.Anonymous.uVersion = NOTIFYICON_VERSION_4;
            unsafe { Shell_NotifyIconW(NIM_SETVERSION, &data) };
            self.signals.ready.store(true, Ordering::Release);
        }
    }
}

unsafe extern "system" fn window_proc(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_NCCREATE {
        let creation = unsafe { &*(lparam as *const CREATESTRUCTW) };
        unsafe { SetWindowLongPtrW(window, GWLP_USERDATA, creation.lpCreateParams as isize) };
        return 1;
    }
    let address = unsafe { GetWindowLongPtrW(window, GWLP_USERDATA) };
    if address != 0 {
        // The message-loop thread owns the Box until the window is destroyed.
        let state = unsafe { &*(address as *const TrayState) };
        if message == CALLBACK
            && matches!(
                lparam as u32 & 0xffff,
                NIN_SELECT | NIN_KEYSELECT | WM_LBUTTONUP | WM_CONTEXTMENU
            )
        {
            state.signals.show.store(true, Ordering::Release);
            return 0;
        }
        if message == WM_TIMER || message == state.taskbar_created {
            state.retry(window, message != WM_TIMER);
            return 0;
        }
        if message == WM_DESTROY {
            state.signals.ready.store(false, Ordering::Release);
            unsafe {
                Shell_NotifyIconW(NIM_DELETE, &state.data(window));
                SetWindowLongPtrW(window, GWLP_USERDATA, 0);
                windows_sys::Win32::UI::WindowsAndMessaging::PostQuitMessage(0);
            }
            return 0;
        }
    }
    unsafe { DefWindowProcW(window, message, wparam, lparam) }
}

pub(super) fn start(signals: Arc<Signals>) {
    let _ = std::thread::Builder::new()
        .name("paw-notification-icon".into())
        .spawn(move || {
            let module = unsafe { GetModuleHandleW(null()) };
            let class = w!("MeowPaw.NotificationIcon");
            let definition = WNDCLASSW {
                lpfnWndProc: Some(window_proc),
                hInstance: module,
                lpszClassName: class,
                ..Default::default()
            };
            if unsafe { RegisterClassW(&definition) } == 0 {
                return;
            }
            let mut state = Box::new(TrayState::new(signals));
            state.icon = {
                let mut and_mask = [0_u8; 128];
                let mut pixels = Vec::with_capacity(4096);
                for y in 0..32 {
                    for x in 0..32 {
                        let [a, r, g, b] = paw_pixel(x, y, 32);
                        pixels.extend([b, g, r, a]);
                        if a == 0 {
                            and_mask[(y * 4 + x / 8) as usize] |= 0x80 >> (x % 8);
                        }
                    }
                }
                unsafe { CreateIcon(module, 32, 32, 1, 32, and_mask.as_ptr(), pixels.as_ptr()) }
            };
            let window = unsafe {
                CreateWindowExW(
                    0,
                    class,
                    class,
                    0,
                    0,
                    0,
                    0,
                    0,
                    null_mut(),
                    null_mut(),
                    module,
                    (&raw mut *state).cast(),
                )
            };
            if !window.is_null() {
                state.retry(window, false);
                unsafe { SetTimer(window, 1, 900, None) };
                let mut message = MSG::default();
                while unsafe { GetMessageW(&mut message, null_mut(), 0, 0) } > 0 {
                    unsafe {
                        TranslateMessage(&message);
                        DispatchMessageW(&message);
                    }
                }
                unsafe { windows_sys::Win32::UI::WindowsAndMessaging::DestroyWindow(window) };
            }
            if !state.icon.is_null() {
                unsafe { DestroyIcon(state.icon) };
            }
            unsafe { UnregisterClassW(class, module) };
        });
}
