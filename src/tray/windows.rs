//! Shell notification icons, backed by a hidden top-level Win32 window.
use super::{Signals, paw_pixel};
use meow_paw::i18n::Locale;
use std::{
    cell::Cell,
    mem::size_of,
    ptr::{null, null_mut},
    sync::{Arc, atomic::Ordering},
    time::{Duration, Instant},
};
use windows_sys::{
    Win32::{
        Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            Shell::{
                NIF_ICON, NIF_MESSAGE, NIF_SHOWTIP, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_SETFOCUS,
                NIM_SETVERSION, NIN_SELECT, NINF_KEY, NOTIFYICON_VERSION_4, NOTIFYICONDATAW,
                NOTIFYICONIDENTIFIER, Shell_NotifyIconGetRect, Shell_NotifyIconW,
            },
            WindowsAndMessaging::{
                AppendMenuW, CREATESTRUCTW, CreateIcon, CreatePopupMenu, CreateWindowExW,
                DefWindowProcW, DestroyIcon, DestroyMenu, DispatchMessageW, EndMenu, FindWindowW,
                GWLP_USERDATA, GetCursorPos, GetForegroundWindow, GetMessageW, GetSystemMetrics,
                GetWindowLongPtrW, HICON, HMENU, MF_STRING, MSG, PostMessageW, RegisterClassW,
                RegisterWindowMessageW, SM_MENUDROPALIGNMENT, SetForegroundWindow, SetTimer,
                SetWindowLongPtrW, TPM_LEFTALIGN, TPM_NONOTIFY, TPM_RETURNCMD, TPM_RIGHTALIGN,
                TPM_RIGHTBUTTON, TPMPARAMS, TrackPopupMenuEx, TranslateMessage, UnregisterClassW,
                WM_APP, WM_CONTEXTMENU, WM_DESTROY, WM_LBUTTONUP, WM_NCCREATE, WM_NULL,
                WM_RBUTTONUP, WM_TIMER, WNDCLASSW,
            },
        },
    },
    core::w,
};

const NIN_KEYSELECT: u32 = NIN_SELECT | NINF_KEY;
const CALLBACK: u32 = WM_APP + 3;
const ID: u32 = 1;
const MENU_SHOW: i32 = 1;
const MENU_QUIT: i32 = 2;
// The shell can report one mouse selection in both raw and semantic forms.
// Pair nearby opposite reports, not individual physical gestures; Windows
// can deliver either form alone. Same-kind and keyboard input stays distinct.
// The application separately coalesces pending Show signals until its poll.
const PRIMARY_PAIR_WINDOW: Duration = Duration::from_millis(160);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PrimaryActivation {
    MouseButton,
    MouseSelect,
    Keyboard,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CallbackAction {
    Show(PrimaryActivation),
    Menu,
}

fn callback_action(version_4: bool, wparam: WPARAM, lparam: LPARAM) -> Option<CallbackAction> {
    // V4 packs the icon ID in HIWORD(lParam). Older shells use wParam for the ID.
    match version_4 {
        true if (lparam as u32 >> 16) == ID => match lparam as u32 & 0xffff {
            NIN_SELECT => Some(CallbackAction::Show(PrimaryActivation::MouseSelect)),
            NIN_KEYSELECT => Some(CallbackAction::Show(PrimaryActivation::Keyboard)),
            WM_LBUTTONUP => Some(CallbackAction::Show(PrimaryActivation::MouseButton)),
            // Handle the semantic callback only, not raw right-button events too.
            WM_CONTEXTMENU => Some(CallbackAction::Menu),
            _ => None,
        },
        false if wparam == ID as WPARAM => match lparam as u32 {
            WM_LBUTTONUP => Some(CallbackAction::Show(PrimaryActivation::MouseButton)),
            WM_RBUTTONUP => Some(CallbackAction::Menu),
            _ => None,
        },
        _ => None,
    }
}

struct PopupMenu(HMENU);

impl PopupMenu {
    fn new(locale: Locale) -> Option<Self> {
        let handle = unsafe { CreatePopupMenu() };
        if handle.is_null() {
            return None;
        }
        let menu = Self(handle);
        let (show, quit) = match locale {
            Locale::En => (w!("Show"), w!("Quit")),
            Locale::ZhCn => (w!("显示"), w!("退出")),
        };
        // Strings are copied into the menu; ownership of the HMENU remains here.
        let populated = unsafe {
            AppendMenuW(menu.0, MF_STRING, MENU_SHOW as usize, show) != 0
                && AppendMenuW(menu.0, MF_STRING, MENU_QUIT as usize, quit) != 0
        };
        populated.then_some(menu)
    }
}

impl Drop for PopupMenu {
    fn drop(&mut self) {
        unsafe { DestroyMenu(self.0) };
    }
}

fn icon_rectangle(window: HWND) -> Option<RECT> {
    let icon = NOTIFYICONIDENTIFIER {
        cbSize: size_of::<NOTIFYICONIDENTIFIER>() as u32,
        hWnd: window,
        uID: ID,
        ..Default::default()
    };
    let mut rect = RECT::default();
    (unsafe { Shell_NotifyIconGetRect(&icon, &mut rect) } >= 0
        && rect.left < rect.right
        && rect.top < rect.bottom)
        .then_some(rect)
}

fn popup_position(rect: RECT, right: bool) -> (POINT, TPMPARAMS) {
    (
        POINT {
            x: if right { rect.right } else { rect.left },
            y: rect.bottom,
        },
        TPMPARAMS {
            cbSize: size_of::<TPMPARAMS>() as u32,
            rcExclude: rect,
        },
    )
}

fn track_if_foreground(
    acquired: bool,
    already_foreground: impl FnOnce() -> bool,
    track: impl FnOnce() -> i32,
) -> Option<i32> {
    (acquired || already_foreground()).then(track)
}

fn dismiss_tracked_popup() {
    // EndMenu only ends the calling thread's active menu. Production callers
    // reach this on the tray thread while our own popup is tracking.
    unsafe { EndMenu() };
}

struct TrayState {
    shell: Cell<HWND>,
    icon: HICON,
    signals: Arc<Signals>,
    taskbar_created: u32,
    locale: Locale,
    version_4: Cell<bool>,
    retrying: Cell<bool>,
    menu_open: Cell<bool>,
    menu_current: Cell<bool>,
    menu_tracking: Cell<bool>,
    last_mouse: Cell<Option<(PrimaryActivation, Instant)>>,
}

impl TrayState {
    fn new(signals: Arc<Signals>, locale: Locale, taskbar_created: u32) -> Self {
        Self {
            shell: Cell::new(null_mut()),
            icon: null_mut(),
            signals,
            taskbar_created,
            locale,
            version_4: Cell::new(false),
            retrying: Cell::new(false),
            menu_open: Cell::new(false),
            menu_current: Cell::new(false),
            menu_tracking: Cell::new(false),
            last_mouse: Cell::new(None),
        }
    }

    fn accept_primary(&self, activation: PrimaryActivation, now: Instant) -> bool {
        if activation == PrimaryActivation::Keyboard {
            return true;
        }
        let paired = self.last_mouse.get().is_some_and(|(previous, at)| {
            previous != activation && now.saturating_duration_since(at) <= PRIMARY_PAIR_WINDOW
        });
        self.last_mouse.set((!paired).then_some((activation, now)));
        !paired
    }

    fn mark_unready(&self, dismiss: impl FnOnce()) {
        // Don't restore this latch on a successful NIM_ADD (the active popup
        // still belongs to the old icon). A later popup sets its own latch.
        let dismiss_current = self.menu_current.replace(false) && self.menu_tracking.get();
        self.last_mouse.set(None);
        self.signals.ready.store(false, Ordering::Release);
        if dismiss_current {
            dismiss();
        }
    }

    fn menu_is_current(&self) -> bool {
        self.menu_current.get() && self.signals.ready()
    }

    fn track_owned_popup(&self, track: impl FnOnce() -> i32) -> i32 {
        self.menu_tracking.set(true);
        let command = track();
        self.menu_tracking.set(false);
        command
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
        // Dismissing an old modal can dispatch another retry on this thread.
        if self.retrying.replace(true) {
            return;
        }
        self.retry_registration(window, forced);
        self.retrying.set(false);
    }

    fn owns_window(&self, window: HWND) -> bool {
        unsafe { GetWindowLongPtrW(window, GWLP_USERDATA) == std::ptr::from_ref(self) as isize }
    }

    fn retry_registration(&self, window: HWND, forced: bool) {
        let shell = unsafe { FindWindowW(w!("Shell_TrayWnd"), null()) };
        if !forced && self.shell.get() == shell && self.signals.ready() {
            return;
        }
        self.mark_unready(dismiss_tracked_popup);
        self.version_4.set(false);
        self.shell.set(shell);
        if shell.is_null() || self.icon.is_null() || !self.owns_window(window) {
            return;
        }
        let mut data = self.data(window);
        if unsafe { Shell_NotifyIconW(NIM_ADD, &data) } != 0 && self.owns_window(window) {
            data.Anonymous.uVersion = NOTIFYICON_VERSION_4;
            let version_4 = unsafe { Shell_NotifyIconW(NIM_SETVERSION, &data) } != 0;
            if self.owns_window(window) {
                self.version_4.set(version_4);
                self.signals.ready.store(true, Ordering::Release);
            }
        }
    }

    fn run_menu(&self, open: impl FnOnce() -> Option<i32>) {
        // TrackPopupMenuEx pumps messages on this same thread. A nested or
        // duplicate context event must not start a second modal loop.
        if !self.signals.ready() || self.menu_open.replace(true) {
            return;
        }
        self.menu_current.set(true);
        let command = open();
        self.menu_open.set(false);
        let current = self.menu_is_current();
        self.menu_current.set(false);
        if !current {
            return;
        }
        match command {
            Some(MENU_SHOW) => self.signals.show.store(true, Ordering::Release),
            Some(MENU_QUIT) => self.signals.quit.store(true, Ordering::Release),
            _ => {}
        }
    }

    fn show_menu(&self, window: HWND) {
        self.run_menu(|| {
            let menu = PopupMenu::new(self.locale)?;
            let right = unsafe { GetSystemMetrics(SM_MENUDROPALIGNMENT) } != 0;
            let mut anchor = POINT::default();
            // A keyboard request may arrive with the cursor on another display.
            // Use our icon rectangle, not screen coordinates from the callback.
            let exclude = icon_rectangle(window).map(|rect| {
                let (position, params) = popup_position(rect, right);
                anchor = position;
                params
            });
            if exclude.is_none() && unsafe { GetCursorPos(&mut anchor) } == 0 {
                return None;
            }
            let params = exclude.as_ref().map_or(null(), std::ptr::from_ref);
            let alignment = if right { TPM_RIGHTALIGN } else { TPM_LEFTALIGN };
            if !self.menu_is_current() {
                return None;
            }
            let acquired = unsafe { SetForegroundWindow(window) } != 0;
            if !self.menu_is_current() {
                return None;
            }
            // Windows can deny foreground changes. An already-foreground
            // owner is fine, but don't open a popup that can't dismiss itself.
            // Only a return value from this owned menu can generate Quit.
            let command = track_if_foreground(
                acquired,
                || unsafe { GetForegroundWindow() == window },
                || {
                    self.track_owned_popup(|| unsafe {
                        TrackPopupMenuEx(
                            menu.0,
                            alignment | TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON,
                            anchor.x,
                            anchor.y,
                            window,
                            params,
                        )
                    })
                },
            )?;
            if self.menu_is_current() {
                unsafe {
                    // Follow the notification-area dismissal protocol before another open.
                    PostMessageW(window, WM_NULL, 0, 0);
                    if command == 0 {
                        Shell_NotifyIconW(NIM_SETFOCUS, &self.data(window));
                    }
                }
            }
            Some(command)
        });
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
        if message == CALLBACK {
            if state.signals.ready() {
                match callback_action(state.version_4.get(), wparam, lparam) {
                    Some(CallbackAction::Show(activation))
                        if !state.menu_open.get()
                            && state.accept_primary(activation, Instant::now()) =>
                    {
                        state.signals.show.store(true, Ordering::Release);
                    }
                    Some(CallbackAction::Menu) => state.show_menu(window),
                    _ => {}
                }
            }
            return 0;
        }
        if message == WM_TIMER || (state.taskbar_created != 0 && message == state.taskbar_created) {
            state.retry(window, message != WM_TIMER);
            return 0;
        }
        if message == WM_DESTROY {
            // Clear callback state before either dismissal or shell deletion
            // can reenter the hidden window's procedure.
            unsafe { SetWindowLongPtrW(window, GWLP_USERDATA, 0) };
            state.mark_unready(dismiss_tracked_popup);
            unsafe {
                Shell_NotifyIconW(NIM_DELETE, &state.data(window));
                windows_sys::Win32::UI::WindowsAndMessaging::PostQuitMessage(0);
            }
            return 0;
        }
    }
    unsafe { DefWindowProcW(window, message, wparam, lparam) }
}

pub(super) fn start(signals: Arc<Signals>, locale: Locale) {
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
            let taskbar_created = unsafe { RegisterWindowMessageW(w!("TaskbarCreated")) };
            let mut state = Box::new(TrayState::new(signals, locale, taskbar_created));
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

#[cfg(test)]
mod tests {
    use super::*;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetMenuItemCount, GetMenuItemID, GetMenuStringW, MF_BYPOSITION, WM_COMMAND,
    };

    fn v4(event: u32, icon: u32) -> LPARAM {
        ((icon << 16) | event) as LPARAM
    }

    #[test]
    fn callback_respects_version_and_icon_ownership() {
        for (event, activation) in [
            (NIN_SELECT, PrimaryActivation::MouseSelect),
            (NIN_KEYSELECT, PrimaryActivation::Keyboard),
            (WM_LBUTTONUP, PrimaryActivation::MouseButton),
        ] {
            assert_eq!(
                callback_action(true, 0, v4(event, ID)),
                Some(CallbackAction::Show(activation))
            );
        }
        assert_eq!(
            callback_action(false, ID as WPARAM, WM_LBUTTONUP as LPARAM),
            Some(CallbackAction::Show(PrimaryActivation::MouseButton))
        );
        assert_eq!(
            callback_action(true, 0, v4(WM_CONTEXTMENU, ID)),
            Some(CallbackAction::Menu)
        );
        assert_eq!(
            callback_action(false, ID as WPARAM, WM_RBUTTONUP as LPARAM),
            Some(CallbackAction::Menu)
        );
        for event in [WM_RBUTTONUP, WM_COMMAND] {
            assert_eq!(callback_action(true, 0, v4(event, ID)), None);
        }
        assert_eq!(callback_action(true, 0, v4(WM_CONTEXTMENU, ID + 1)), None);
        assert_eq!(
            callback_action(false, (ID + 1) as WPARAM, WM_RBUTTONUP as LPARAM),
            None
        );
    }

    #[test]
    fn primary_activation_pairs_only_adjacent_raw_and_semantic_mouse_reports() {
        use PrimaryActivation::*;
        let now = Instant::now();
        for (first, second) in [(MouseButton, MouseSelect), (MouseSelect, MouseButton)] {
            let state = TrayState::new(Arc::new(Signals::default()), Locale::En, 0);
            assert!(state.accept_primary(first, now));
            assert!(state.accept_primary(Keyboard, now));
            assert!(!state.accept_primary(second, now + Duration::from_millis(10)));
            // A new callback after consuming the pair remains observable.
            assert!(state.accept_primary(first, now + Duration::from_millis(20)));
            // Raw-only or semantic-only repeated clicks must not be dropped.
            assert!(state.accept_primary(first, now + Duration::from_millis(30)));
            assert!(state.accept_primary(second, now + Duration::from_secs(1)));
        }
    }

    #[test]
    fn menu_cancel_reopen_and_nested_events_do_not_duplicate_actions() {
        let signals = Arc::new(Signals::default());
        let state = TrayState::new(signals.clone(), Locale::En, 0);
        signals.ready.store(true, Ordering::Release);
        state.run_menu(|| {
            state.run_menu(|| panic!("nested popup must not start"));
            Some(0)
        });
        assert!(!signals.take_show());
        assert!(!signals.take_quit());
        state.run_menu(|| Some(MENU_SHOW));
        assert!(signals.take_show());
        assert!(!signals.take_show());
        assert!(!signals.take_quit());
        state.run_menu(|| Some(1234));
        assert!(!signals.take_show());
        assert!(!signals.take_quit());
        state.run_menu(|| Some(MENU_QUIT));
        assert!(signals.take_quit());
        assert!(!signals.take_quit());
        assert!(!signals.take_show());
    }

    #[test]
    fn denied_foreground_never_tracks_or_signals_then_allows_reopen() {
        let signals = Arc::new(Signals::default());
        let state = TrayState::new(signals.clone(), Locale::En, 0);
        signals.ready.store(true, Ordering::Release);
        state.run_menu(|| {
            track_if_foreground(false, || false, || panic!("denied menu must not track"))
        });
        assert!(!signals.take_show());
        assert!(!signals.take_quit());
        state.run_menu(|| track_if_foreground(false, || true, || MENU_SHOW));
        assert!(signals.take_show());
        assert!(!signals.take_quit());
        state.run_menu(|| track_if_foreground(true, || panic!("no fallback needed"), || 0));
        assert!(!signals.take_show());
        assert!(!signals.take_quit());
    }

    #[test]
    fn commands_returned_after_reentrant_destroy_are_ignored() {
        for command in [MENU_SHOW, MENU_QUIT] {
            let signals = Arc::new(Signals::default());
            let state = TrayState::new(signals.clone(), Locale::En, 0);
            signals.ready.store(true, Ordering::Release);
            let dismissed = Cell::new(false);
            state.run_menu(|| {
                Some(state.track_owned_popup(|| {
                    // The same readiness transition as WM_DESTROY in the modal loop.
                    state.mark_unready(|| dismissed.set(true));
                    assert!(!state.menu_is_current());
                    command
                }))
            });
            assert!(dismissed.get());
            assert!(!signals.take_show());
            assert!(!signals.take_quit());
            assert!(!state.menu_open.get());
            state.run_menu(|| panic!("destroyed tray must not track"));
        }
    }

    #[test]
    fn explorer_renewal_dismisses_an_old_modal_only_once_then_allows_reopen() {
        let signals = Arc::new(Signals::default());
        let state = TrayState::new(signals.clone(), Locale::En, 0);
        signals.ready.store(true, Ordering::Release);
        let dismissed = Cell::new(false);
        state.run_menu(|| {
            Some(state.track_owned_popup(|| {
                assert!(state.menu_is_current());
                state.mark_unready(|| dismissed.set(true));
                state.mark_unready(|| panic!("don't dismiss the same menu twice"));
                signals.ready.store(true, Ordering::Release);
                assert!(!state.menu_is_current());
                state.run_menu(|| panic!("the obsolete modal is still unwinding"));
                MENU_QUIT
            }))
        });
        assert!(dismissed.get());
        assert!(!signals.take_show());
        assert!(!signals.take_quit());
        state.run_menu(|| {
            Some(state.track_owned_popup(|| {
                assert!(state.menu_is_current());
                MENU_SHOW
            }))
        });
        assert!(signals.take_show());
        assert!(!signals.take_quit());
    }

    #[test]
    fn invalidation_outside_native_tracking_does_not_end_another_menu() {
        let signals = Arc::new(Signals::default());
        let state = TrayState::new(signals.clone(), Locale::En, 0);
        for after_tracking in [false, true] {
            signals.ready.store(true, Ordering::Release);
            state.run_menu(|| {
                if after_tracking {
                    assert_eq!(state.track_owned_popup(|| 0), 0);
                }
                state.mark_unready(|| panic!("no owned popup is tracking"));
                Some(MENU_QUIT)
            });
            assert!(!signals.take_show());
            assert!(!signals.take_quit());
        }
    }

    #[test]
    fn popup_anchor_keeps_signed_icon_coordinates_and_excludes_icon() {
        let icon = RECT {
            left: -80,
            top: -40,
            right: -60,
            bottom: -20,
        };
        for (right, expected_x) in [(false, -80), (true, -60)] {
            let (point, params) = popup_position(icon, right);
            assert_eq!((point.x, point.y), (expected_x, -20));
            let excluded = params.rcExclude;
            assert_eq!(
                (excluded.left, excluded.top, excluded.right, excluded.bottom),
                (-80, -40, -60, -20)
            );
        }
    }

    #[test]
    fn native_popup_contains_only_the_two_localized_actions() {
        for (locale, labels) in [
            (Locale::En, ["Show", "Quit"]),
            (Locale::ZhCn, ["显示", "退出"]),
        ] {
            let menu = PopupMenu::new(locale).expect("allocate an unattached Win32 menu");
            assert_eq!(unsafe { GetMenuItemCount(menu.0) }, 2);
            for (position, (id, label)) in
                [MENU_SHOW, MENU_QUIT].into_iter().zip(labels).enumerate()
            {
                assert_eq!(unsafe { GetMenuItemID(menu.0, position as i32) }, id as u32);
                let mut buffer = [0; 16];
                let length = unsafe {
                    GetMenuStringW(
                        menu.0,
                        position as u32,
                        buffer.as_mut_ptr(),
                        buffer.len() as i32,
                        MF_BYPOSITION,
                    )
                };
                assert!(length > 0);
                assert_eq!(String::from_utf16_lossy(&buffer[..length as usize]), label);
            }
        }
    }
}
