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

#[gpui_kit::test]
fn local_path_disclosure_is_compact_reversible_and_owned_by_exact_profile(cx: &mut TestAppContext) {
    use gpui_kit::{SharedString, px, size};

    cx.update(gpui_kit::init);
    let files = tempfile::tempdir().unwrap();
    let left = files
        .path()
        .join("first-private-directory-for-duplicate-path")
        .join("Same.yaml");
    let right = files
        .path()
        .join("second-private-directory-for-duplicate-path")
        .join("Same.yaml");
    for path in [&left, &right] {
        std::fs::create_dir(path.parent().unwrap()).unwrap();
        std::fs::write(path, "mode: rule\n").unwrap();
    }
    let mut preferences = Preferences::discover(false).0;
    for path in [&left, &right] {
        preferences.remember(false, path).unwrap();
    }
    let fixture = (left.clone(), right.clone());
    let holder = Rc::new(RefCell::new(None));
    let capture = holder.clone();
    let (_, ui) = cx.add_window_view(move |window, cx| {
        let paw = open(
            None,
            Some(fixture.1.clone()),
            (Page::Profiles, Locale::En),
            false,
            (preferences, None),
            window,
            cx,
        );
        paw.update(cx, |paw, _| {
            paw.snapshot.engine = EngineStatus::Running { mixed_port: 17890 };
            paw.snapshot.profile = Some(fixture.0);
        });
        *capture.borrow_mut() = Some(paw.clone());
        gpui_kit::base::Root::new(paw, window, cx)
    });
    let paw = holder.borrow_mut().take().expect("test-owned Profiles");
    ui.simulate_resize(size(px(1120.), px(738.)));
    ui.update(|window, cx| {
        window.activate_window();
        window.render_frame(cx);
        let id = |prefix, path: &std::path::Path| SharedString::from(format!("{prefix}:{path:?}"));
        let left_card = id("profile-card", &left);
        let right_card = id("profile-card", &right);
        let left_toggle = id("profile-path-toggle", &left);
        let right_toggle = id("profile-path-toggle", &right);
        let left_panel = id("profile-path-details", &left);
        let right_panel = id("profile-path-details", &right);
        let show = Locale::En.profile_action_label(Message::ShowProfilePath, "Same.yaml");
        let hide = Locale::En.profile_action_label(Message::HideProfilePath, "Same.yaml");

        let path_painted = |window: &gpui_kit::Window, path: &std::path::Path| {
            gpui_kit::base::test_support::texts(window)
                .iter()
                .any(|text| text.visible() && text.text() == path.to_string_lossy())
        };
        for (path, toggle, panel) in [
            (&left, &left_toggle, &left_panel),
            (&right, &right_toggle, &right_panel),
        ] {
            assert_eq!(window.find(toggle.clone()).label(), Some(show.as_str()));
            assert!(window.try_find(panel.clone()).is_none());
            assert!(!path_painted(window, path));
        }
        let collapsed = window.find(left_card.clone()).bounds().size.height;
        assert!(window.find("profiles-selected-label").visible());
        assert!(
            gpui_kit::base::test_support::texts(window)
                .iter()
                .any(|text| text.visible() && text.text() == "Same.yaml")
        );

        let reload = id("profile-apply", &left);
        let apply = id("profile-apply", &right);
        assert_eq!(
            window.find(reload.clone()).label(),
            Some(
                Locale::En
                    .profile_action_label(Message::Reload, "Same.yaml")
                    .as_str()
            )
        );
        assert_eq!(
            window.find(apply.clone()).label(),
            Some(
                Locale::En
                    .profile_action_label(Message::ApplyLive, "Same.yaml")
                    .as_str()
            )
        );
        assert!(window.try_find(id("profile-use", &right)).is_none());

        window.click(left_toggle.clone(), cx);
        window.render_frame(cx);
        assert_eq!(
            window.find(left_toggle.clone()).label(),
            Some(hide.as_str())
        );
        assert!(window.find(left_panel.clone()).visible());
        assert!(window.try_find(right_panel.clone()).is_none());
        assert!(path_painted(window, &left) && !path_painted(window, &right));
        assert!(window.find(left_card.clone()).bounds().size.height > collapsed);

        window.click(right_toggle.clone(), cx);
        window.render_frame(cx);
        assert_eq!(
            window.find(right_toggle.clone()).label(),
            Some(hide.as_str())
        );
        assert!(window.try_find(left_panel.clone()).is_none());
        assert!(window.find(right_panel.clone()).visible());
        assert!(!path_painted(window, &left) && path_painted(window, &right));

        window.click("copy-path", cx);
        assert_eq!(
            cx.read_from_clipboard()
                .and_then(|item| item.text())
                .as_deref(),
            right.to_str()
        );
        window.click(right_toggle.clone(), cx);
        window.render_frame(cx);
        assert_eq!(
            window.find(right_toggle.clone()).label(),
            Some(show.as_str())
        );
        assert!(window.try_find(right_panel.clone()).is_none());
        assert!(!path_painted(window, &right));
        assert_eq!(window.find(left_card).bounds().size.height, collapsed);

        window.click(right_toggle, cx);
        paw.update(cx, |paw, cx| {
            paw.navigate_to(Page::Overview);
            paw.navigate_to(Page::Profiles);
            cx.notify();
        });
        window.render_frame(cx);
        assert!(window.try_find(right_panel).is_none());
        assert!(window.find(right_card).visible());
        assert!(window.find(reload).visible() && window.find(apply).visible());
        assert_eq!(paw.read(cx).snapshot.profile.as_ref(), Some(&left));
        assert_eq!(paw.read(cx).profile.as_ref(), Some(&right));
    });
}
