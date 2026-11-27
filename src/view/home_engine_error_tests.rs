use super::{Page, open};
use crate::preferences::Preferences;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{ClipboardItem, TestAppContext};
use meow_paw::{app::EngineStatus, i18n::Locale};
use std::{cell::RefCell, rc::Rc};

extern crate gpui_kit as gpui;

#[gpui_kit::test]
fn home_error_details_copy_with_keyboard_and_restore_trigger_focus(cx: &mut TestAppContext) {
    const ERROR: &str = "Synthetic engine error: fixture did not start.";
    const TRIGGER: &str = "home-engine-error-details";
    const COPY: &str = "home-engine-error-copy";
    cx.update(gpui_kit::init);
    let holder = Rc::new(RefCell::new(None));
    let capture = holder.clone();
    let (_, ui) = cx.add_window_view(move |window, cx| {
        let paw = open(
            None,
            None,
            (Page::Overview, Locale::ZhCn),
            false,
            (Preferences::discover(false).0, None),
            window,
            cx,
        );
        paw.update(cx, |paw, _| {
            paw.snapshot.engine = EngineStatus::Error(ERROR.into())
        });
        *capture.borrow_mut() = Some(paw.clone());
        gpui_kit::base::Root::new(paw, window, cx)
    });
    let paw = holder.borrow_mut().take().expect("test-owned Home");
    ui.update(|window, cx| {
        window.activate_window();
        window.render_frame(cx);
        let trigger = window.find(TRIGGER);
        assert_eq!(trigger.role(), Some(gpui_kit::Role::Button));
        assert_eq!(trigger.label(), Some("核心错误详情"));
        assert!(window.try_find(COPY).is_none());
        let trigger_focus = paw.read(cx).home_engine_details_focus.clone();
        trigger_focus.focus(window, cx);
        window.press("enter", cx);
        window.render_frame(cx);
        assert!(window.try_find(COPY).is_some());

        for _ in 0..3 {
            if window.find(COPY).focused() == Some(true) {
                break;
            }
            window.press("tab", cx);
            window.render_frame(cx);
        }
        assert_eq!(window.find(COPY).focused(), Some(true));
        for key in ["enter", "space"] {
            cx.write_to_clipboard(ClipboardItem::new_string("not copied".into()));
            window.press(key, cx);
            window.render_frame(cx);
            assert_eq!(
                cx.read_from_clipboard()
                    .and_then(|item| item.text())
                    .as_deref(),
                Some(ERROR),
                "{key} must copy the exact diagnosis"
            );
            assert_eq!(window.find(COPY).focused(), Some(true));
        }

        window.press("escape", cx);
        window.render_frame(cx);
        assert!(window.try_find(COPY).is_none());
        assert_eq!(window.find(TRIGGER).focused(), Some(true));
    });
}
