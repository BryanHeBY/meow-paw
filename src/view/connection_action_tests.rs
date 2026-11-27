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
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let rows = self.0.update(cx, |paw, cx| paw.connections(cx));
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
