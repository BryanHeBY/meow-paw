use super::{Page, open};
use crate::preferences::Preferences;
use gpui_kit::{TestAppContext, test::TestWindowExt};
use meow_paw::{
    app::EngineStatus,
    i18n::{Locale, Message},
};
use std::{cell::RefCell, rc::Rc};

extern crate gpui_kit as gpui;

#[gpui_kit::test]
fn staging_a_profile_does_not_change_the_running_profile(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let files = tempfile::tempdir().unwrap();
    let current = files.path().join("Current.yaml");
    let next = files.path().join("Next.yaml");
    for path in [&current, &next] {
        std::fs::write(path, "rules: []\n").unwrap();
    }

    for locale in [Locale::En, Locale::ZhCn] {
        let mut preferences = Preferences::discover(false).0;
        preferences.remember(false, &next).unwrap();
        preferences.remember(false, &current).unwrap();
        let holder = Rc::new(RefCell::new(None));
        let capture = holder.clone();
        let running_profile = current.clone();
        let (_, ui) = cx.add_window_view(move |window, cx| {
            let paw = open(
                None,
                Some(running_profile.clone()),
                (Page::Profiles, locale),
                false,
                (preferences, None),
                window,
                cx,
            );
            paw.update(cx, |paw, _| {
                paw.snapshot.engine = EngineStatus::Running { mixed_port: 17890 };
                paw.snapshot.profile = Some(running_profile);
            });
            *capture.borrow_mut() = Some(paw.clone());
            gpui_kit::base::Root::new(paw, window, cx)
        });
        let paw = holder.borrow_mut().take().expect("test-owned Profiles");
        let stage = format!("profile-use:{next:?}");
        let apply = format!("profile-apply:{next:?}");
        let reload = format!("profile-apply:{current:?}");
        ui.update(|window, cx| {
            window.activate_window();
            window.render_frame(cx);
            macro_rules! check_label {
                ($id:expr, $action:expr, $name:expr) => {
                    assert_eq!(
                        window
                            .find(gpui_kit::SharedString::from($id.clone()))
                            .label(),
                        Some(locale.profile_action_label($action, $name).as_str())
                    );
                };
            }
            check_label!(stage, Message::UseOnNextStart, "Next.yaml");
            check_label!(apply, Message::ApplyLive, "Next.yaml");
            check_label!(reload, Message::Reload, "Current.yaml");
            window.click(gpui_kit::SharedString::from(stage.clone()), cx);
            window.render_frame(cx);
            assert!(
                window
                    .try_find(gpui_kit::SharedString::from(stage.clone()))
                    .is_none()
            );
            assert_eq!(paw.read(cx).profile.as_ref(), Some(&next));
            assert_eq!(paw.read(cx).snapshot.profile.as_ref(), Some(&current));
            assert!(paw.read(cx).snapshot.applying_profile.is_none());
            check_label!(apply, Message::ApplyLive, "Next.yaml");
            check_label!(reload, Message::Reload, "Current.yaml");
        });
    }
}
