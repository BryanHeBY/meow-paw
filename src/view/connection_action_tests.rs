//! Synthetic controls only: no desktop, controller, or profile is started.
use super::{Page, Paw, connection_close_button, open};
use crate::preferences::Preferences;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AppContext, Context, Entity, InteractiveElement, IntoElement, ParentElement, Render, Styled,
    TestAppContext, Window, div,
};
use meow_paw::i18n::Locale;
use std::{cell::RefCell, rc::Rc, sync::Arc};

extern crate gpui_kit as gpui;

struct CloseControls(Rc<RefCell<Vec<String>>>);

struct ConnectionRows(Entity<Paw>);

impl Render for ConnectionRows {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let rows = self.0.update(cx, |paw, cx| {
            paw.connections(window.viewport_size().width, cx)
        });
        div().size_full().tab_group().child(rows)
    }
}

#[gpui_kit::test]
fn actual_connection_row_exposes_disabled_labeled_close_without_desktop(cx: &mut TestAppContext) {
    use meow_paw::{
        app::EngineStatus,
        engine::controller::{Connection, Metadata},
    };
    cx.update(gpui_kit::init);
    let (_, ui) = cx.add_window_view(move |window, cx| {
        let paw = open(
            None,
            None,
            (Page::Connections, Locale::En),
            false,
            (Preferences::discover(false).0, None),
            window,
            cx,
        );
        paw.update(cx, |paw, _| {
            paw.snapshot.engine = EngineStatus::Running { mixed_port: 1234 };
            paw.snapshot.connections = Some(Arc::new(vec![Connection {
                id: "row".into(),
                metadata: Metadata {
                    host: "example.invalid".into(),
                    destination_port: "443".into(),
                    ..Default::default()
                },
                ..Default::default()
            }]));
        });
        let rows = cx.new(|_| ConnectionRows(paw));
        gpui_kit::base::Root::new(rows, window, cx)
    });
    ui.update(|window, cx| {
        window.activate_window();
        window.render_frame(cx);
        let close = window.find("connection-close:row");
        assert_eq!(close.role(), Some(gpui_kit::Role::Button));
        assert_eq!(close.label(), Some("Close connection: example.invalid:443"));
        assert_eq!(close.disabled(), Some(true));
        window.click("connection-close:row", cx);
    });
}

impl Render for CloseControls {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let count = self.0.clone();
        div()
            .size_full()
            .tab_group()
            .child(connection_close_button(
                "first",
                "example.invalid:443",
                Locale::En,
                false,
                move |_, _, _| {
                    count.borrow_mut().push("first".to_string());
                },
            ))
            .child(connection_close_button(
                "disabled",
                "测试.invalid:443",
                Locale::ZhCn,
                true,
                |_, _, _| panic!("disabled action must not run"),
            ))
    }
}

#[gpui_kit::test]
fn close_control_is_labeled_focusable_and_activates_once_per_key(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let count = Rc::new(RefCell::new(Vec::new()));
    let fixture_count = count.clone();
    let (_, ui) = cx.add_window_view(move |window, cx| {
        let controls = cx.new(|_| CloseControls(fixture_count));
        gpui_kit::base::Root::new(controls, window, cx)
    });
    ui.update(|window, cx| {
        window.activate_window();
        window.render_frame(cx);
        let close = window.find("connection-close:first");
        assert_eq!(close.role(), Some(gpui_kit::Role::Button));
        assert_eq!(close.label(), Some("Close connection: example.invalid:443"));
        let disabled = window.find("connection-close:disabled");
        assert_eq!(disabled.role(), Some(gpui_kit::Role::Button));
        assert_eq!(disabled.label(), Some("关闭连接: 测试.invalid:443"));
        assert_eq!(disabled.disabled(), Some(true));

        window.press("tab", cx);
        let focus = window.focused(cx).expect("close receives keyboard focus");
        window.press("enter", cx);
        assert_eq!(&*count.borrow(), &["first"]);
        window.press("tab", cx);
        assert_eq!(window.focused(cx).as_ref(), Some(&focus));
        window.press("space", cx);
        assert_eq!(&*count.borrow(), &["first", "first"]);
        window.click("connection-close:first", cx);
        assert_eq!(&*count.borrow(), &["first", "first", "first"]);
        window.click("connection-close:disabled", cx);
        assert_eq!(&*count.borrow(), &["first", "first", "first"]);
    });
}

#[gpui_kit::test]
fn actual_connection_order_is_keyboard_accessible_and_keeps_the_selected_row(
    cx: &mut TestAppContext,
) {
    use super::ConnectionDetailSelection;
    use meow_paw::{
        app::EngineStatus,
        engine::controller::{Connection, Metadata},
    };
    cx.update(gpui_kit::init);
    let holder = Rc::new(RefCell::new(None));
    let holder_in = holder.clone();
    let (_, ui) = cx.add_window_view(move |window, cx| {
        let paw = open(
            None,
            None,
            (Page::Connections, Locale::En),
            false,
            (Preferences::discover(false).0, None),
            window,
            cx,
        );
        let row = |host: &str, network: &str, upload| Connection {
            id: "same-controller-id".into(),
            metadata: Metadata {
                host: host.into(),
                network: Some(network.into()),
                ..Default::default()
            },
            upload: Some(upload),
            download: Some(0),
            ..Default::default()
        };
        paw.update(cx, |paw, _| {
            paw.snapshot.engine = EngineStatus::Running { mixed_port: 1234 };
            paw.snapshot.connections = Some(Arc::new(vec![
                row("Zulu", "tcp", 20),
                row("Alpha", "udp", 10),
            ]));
        });
        holder_in.replace(Some(paw.clone()));
        let rows = cx.new(|_| ConnectionRows(paw));
        gpui_kit::base::Root::new(rows, window, cx)
    });
    let paw = holder.take().unwrap();
    ui.update(|window, cx| {
        window.activate_window();
        window.render_frame(cx);
        let order = window.find("connection-order");
        assert_eq!(order.role(), Some(gpui_kit::Role::Button));
        assert_eq!(order.label(), Some("Sort: Original order"));
        let first_row = ("connection-details", 0usize);
        let selected_row = ("connection-details", 1usize);
        assert!(
            window.find(first_row).bounds().origin.y < window.find(selected_row).bounds().origin.y
        );
        window.click(selected_row, cx);

        let order_focus = paw.read(cx).connection_order_focus.clone();
        order_focus.focus(window, cx);
        window.press("enter", cx);
        window.render_frame(cx);
        assert_eq!(
            window.find("connection-order").label(),
            Some("Sort: Destination")
        );
        assert_eq!(window.focused(cx).as_ref(), Some(&order_focus));
        assert!(
            window.find(selected_row).bounds().origin.y < window.find(first_row).bounds().origin.y
        );
        assert!(matches!(
            paw.read(cx).connection_detail,
            Some(ConnectionDetailSelection::Active { index: 1, .. })
        ));

        window.press("space", cx);
        window.render_frame(cx);
        assert_eq!(
            window.find("connection-order").label(),
            Some("Sort: Most data")
        );
        assert!(
            window.find(first_row).bounds().origin.y < window.find(selected_row).bounds().origin.y
        );
        let filter = paw.read(cx).connection_filter.clone();
        filter.update(cx, |input, cx| input.set_value("UDP", window, cx));
        window.render_frame(cx);
        assert!(matches!(
            paw.read(cx).connection_detail,
            Some(ConnectionDetailSelection::Active { index: 1, .. })
        ));
        let selection = paw.read(cx).connection_detail.as_ref().unwrap();
        let ConnectionDetailSelection::Active { sample, index: 1 } = selection else {
            panic!("details must still refer to the second source row");
        };
        assert_eq!(sample[1].metadata.host, "Alpha");
    });
}

#[gpui_kit::test]
fn actual_connection_table_shows_truthful_transfer_totals_in_both_layouts(cx: &mut TestAppContext) {
    use gpui_kit::{px, size};
    use meow_paw::{
        app::EngineStatus,
        engine::controller::{Connection, Metadata},
    };

    cx.update(gpui_kit::init);
    let held = Rc::new(RefCell::new(None));
    let state = held.clone();
    let (_, ui) = cx.add_window_view(move |window, cx| {
        let paw = open(
            None,
            None,
            (Page::Connections, Locale::En),
            false,
            (Preferences::discover(false).0, None),
            window,
            cx,
        );
        paw.update(cx, |paw, _| {
            paw.snapshot.engine = EngineStatus::Running { mixed_port: 1234 };
            paw.snapshot.connections = Some(Arc::new(vec![
                Connection {
                    id: "first".into(),
                    download: Some(12_288),
                    upload: None,
                    metadata: Metadata {
                        host: "arbitrarily-long-hostname-that-must-not-expel-row-actions.example.invalid".into(),
                        ..Default::default()
                    },
                    chains: vec!["A very long selection path".into(), "DIRECT".into()],
                    ..Default::default()
                },
                Connection {
                    id: "second".into(),
                    download: None,
                    upload: Some(0),
                    ..Default::default()
                },
            ]));
        });
        state.replace(Some(paw.clone()));
        let rows = cx.new(|_| ConnectionRows(paw));
        gpui_kit::base::Root::new(rows, window, cx)
    });
    let paw = held.take().unwrap();
    for (width, wide) in [(1120., true), (960., true), (959., false), (760., false)] {
        ui.simulate_resize(size(px(width), px(738.)));
        ui.update(|window, cx| {
            window.activate_window();
            window.render_frame(cx);
            assert_eq!(
                window.try_find("connections-column-headings").is_some(),
                wide
            );
            let painted = gpui_kit::base::test_support::texts(window);
            for (key, index, text) in [
                ("connection-row-download", 0usize, "↓ 12.0 KiB"),
                ("connection-row-upload", 0usize, "↑ —"),
                ("connection-row-download", 1usize, "↓ —"),
                ("connection-row-upload", 1usize, "↑ 0 B"),
            ] {
                let observed = window.find((key, index));
                assert!(
                    painted
                        .iter()
                        .any(|item| item.visible() && item.text() == text),
                    "missing counter {text} at {width}"
                );
                let bounds = observed.bounds();
                assert!(bounds.origin.x + bounds.size.width <= px(width));
            }
            for index in 0..2usize {
                let details = window.find(("connection-details", index));
                assert!(details.bounds().origin.x + details.bounds().size.width <= px(width));
            }
        });
    }
    ui.update(|window, cx| {
        window.click(("connection-details", 0usize), cx);
        window.render_frame(cx);
        assert!(window.try_find("connection-details-close").is_some());
        paw.update(cx, |paw, cx| {
            paw.snapshot.connections = None;
            cx.notify();
        });
        window.render_frame(cx);
        assert!(
            window
                .try_find(("connection-row-download", 0usize))
                .is_none()
        );
        assert!(window.try_find(("connection-row-upload", 1usize)).is_none());
    });
}
