//! Headless routing controls, backed only by a test-owned fake controller.
use super::{Page, Paw, open, provider_refresh_button};
use crate::preferences::Preferences;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AppContext, Context, Entity, InteractiveElement, IntoElement, ParentElement, Render, Styled,
    TestAppContext, Window, div,
};
use meow_paw::{
    app::{Desktop, EngineStatus},
    engine::controller::RoutingMode,
    i18n::{Locale, Message},
};
use std::{
    cell::RefCell,
    fs::{self, OpenOptions},
    io::{BufRead, BufReader, Read, Write},
    net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream},
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    rc::Rc,
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

extern crate gpui_kit as gpui;

struct Header(Entity<Paw>);
impl Render for Header {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let header = self.0.update(cx, |paw, cx| paw.proxy_header(cx));
        div().size_full().tab_group().child(header)
    }
}

struct Connections(Entity<Paw>);
impl Render for Connections {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let rows = self.0.update(cx, |paw, cx| {
            paw.connections(window.viewport_size().width, cx)
        });
        div().size_full().tab_group().child(rows)
    }
}

struct ProviderControls(Rc<RefCell<usize>>);

impl Render for ProviderControls {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let activations = self.0.clone();
        div()
            .size_full()
            .tab_group()
            .child(
                provider_refresh_button(Locale::En, "ready", false, false, false)
                    .on_click(move |_, _, _| *activations.borrow_mut() += 1),
            )
            .child(provider_refresh_button(
                Locale::ZhCn,
                "current",
                true,
                true,
                false,
            ))
            .child(provider_refresh_button(
                Locale::ZhCn,
                "queued",
                false,
                true,
                false,
            ))
    }
}

#[gpui_kit::test]
fn provider_refresh_exposes_keyboard_action_and_busy_labels(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let activations = Rc::new(RefCell::new(0));
    let counter = activations.clone();
    let (_, ui) = cx.add_window_view(move |window, cx| {
        let controls = cx.new(|_| ProviderControls(counter));
        gpui_kit::base::Root::new(controls, window, cx)
    });
    ui.update(|window, _| window.activate_window());
    ui.update(|window, cx| {
        window.render_frame(cx);
        for (name, locale, message) in [
            ("ready", Locale::En, Message::Refresh),
            ("current", Locale::ZhCn, Message::ProviderRefreshing),
            ("queued", Locale::ZhCn, Message::ProviderRefreshBusy),
        ] {
            let id = gpui_kit::SharedString::from(format!("provider-refresh:{name}"));
            let button = window.find(id);
            assert_eq!(button.role(), Some(gpui_kit::Role::Button));
            assert_eq!(
                button.label().unwrap(),
                format!("{}: {name}", locale.message(message))
            );
        }
        window.press("tab", cx);
        let focus = window
            .focused(cx)
            .expect("refresh button can receive focus");
        window.press("enter", cx);
        window.press("tab", cx);
        assert_eq!(window.focused(cx).as_ref(), Some(&focus));
        window.press("space", cx);
    });
    assert_eq!(*activations.borrow(), 2);
}

#[gpui_kit::test]
fn proxy_panel_preserves_settings_and_does_not_refresh_or_probe(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        cx.set_reduce_motion(true);
    });
    let holder = Rc::new(RefCell::new(None));
    let holder2 = holder.clone();
    let (_, ui) = cx.add_window_view(move |window, cx| {
        let paw = open(
            None,
            None,
            (Page::Proxies, Locale::En),
            false,
            (Preferences::discover(false).0, None),
            window,
            cx,
        );
        super::seed_proxy_fixture(&paw, cx);
        paw.update(cx, |paw, _| paw.error = Some("unchanged".into()));
        *holder2.borrow_mut() = Some(paw.clone());
        gpui_kit::base::Root::new(paw, window, cx)
    });
    let paw = holder.borrow_mut().take().unwrap();
    ui.update(|window, cx| {
        window.activate_window();
        window.render_frame(cx);
        let test_settings = "proxy-probe-settings-toggle";
        assert_eq!(
            window.find(test_settings).label().unwrap(),
            "Set latency test URL"
        );
        assert!(!paw.read(cx).proxy_probe_settings_open);
        window.click(test_settings, cx);
        window.render_frame(cx);
        assert!(paw.read(cx).proxy_probe_settings_open);
        assert_eq!(
            window.find(test_settings).label().unwrap(),
            "Hide latency test settings"
        );
        let target = "https://example.invalid/check";
        let input = paw.read(cx).probe_url.clone();
        input.update(cx, |input, cx| input.set_value(target, window, cx));
        window.click(test_settings, cx);
        window.render_frame(cx);
        assert!(!paw.read(cx).proxy_probe_settings_open);
        assert_eq!(
            window.find(test_settings).label().unwrap(),
            "Latency test settings"
        );
        window.click(test_settings, cx);
        window.render_frame(cx);
        assert_eq!(input.read(cx).value().as_str(), target);
        window.click(test_settings, cx);
        window.render_frame(cx);
        let control = window.find("proxy-providers-toggle");
        assert_eq!(control.role(), Some(gpui_kit::Role::Button));
        assert_eq!(control.label().unwrap(), "Manage providers (1)");
        paw.update(cx, |paw, cx| {
            let inventory = Arc::make_mut(paw.snapshot.inventory.as_mut().unwrap());
            let group = inventory.groups["Work"].clone();
            for index in 0..20 {
                inventory
                    .groups
                    .insert(format!("Extra {index:02}"), group.clone());
            }
            cx.notify();
        });
        window.render_frame(cx);
        let handle = paw.read(cx).proxy_group_scroll.clone();
        handle.set_offset(gpui_kit::point(gpui_kit::px(0.), gpui_kit::px(-120.)));
        window.render_frame(cx);
        assert!(handle.offset().y < gpui_kit::px(0.));
        let selection = paw.read(cx).snapshot.inventory.as_ref().unwrap().groups["Daily Proxy"]
            .now
            .clone();
        let expanded = paw.read(cx).expanded_groups.clone();
        let scroll = paw.read(cx).proxy_group_scroll.offset();
        let trigger_focus = paw.read(cx).proxy_providers_focus.clone();
        trigger_focus.focus(window, cx);
        window.press("enter", cx);
        window.render_frame(cx);
        assert!(window.try_find("provider-panel-content").is_some());
        assert_eq!(handle.offset(), scroll);
        assert_eq!(
            window
                .find("provider-refresh:Sample Provider")
                .label()
                .unwrap(),
            "Refresh: Sample Provider"
        );
        window.press("escape", cx);
        window.render_frame(cx);
        assert!(window.find("proxy-providers-toggle").focused().unwrap());
        window.press("space", cx);
        window.render_frame(cx);
        assert!(window.try_find("provider-panel-content").is_some());
        window.click("provider-panel-close", cx);
        window.render_frame(cx);
        assert!(window.find("proxy-providers-toggle").focused().unwrap());
        assert_eq!(
            paw.read(cx).snapshot.inventory.as_ref().unwrap().groups["Daily Proxy"].now,
            selection
        );
        assert_eq!(paw.read(cx).expanded_groups, expanded);
        assert_eq!(paw.read(cx).proxy_group_scroll.offset(), scroll);
        assert_eq!(paw.read(cx).error.as_deref(), Some("unchanged"));
        assert!(paw.read(cx).snapshot.refreshing_provider.is_none());
        assert!(paw.read(cx).snapshot.group_probe.is_none());
    });
}

#[gpui_kit::test]
fn provider_panel_tracks_stale_empty_and_busy_inventory(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        cx.set_reduce_motion(true);
    });
    let holder = Rc::new(RefCell::new(None));
    let result = holder.clone();
    let (_, ui) = cx.add_window_view(move |window, cx| {
        let paw = open(
            None,
            None,
            (Page::Proxies, Locale::ZhCn),
            false,
            (Preferences::discover(false).0, None),
            window,
            cx,
        );
        super::seed_proxy_fixture(&paw, cx);
        paw.update(cx, |paw, _| {
            use meow_paw::engine::controller::{ProviderStatus, ProviderUsage};
            let inventory = Arc::make_mut(paw.snapshot.inventory.as_mut().unwrap());
            let provider = inventory.providers.get_mut("示例供应商").unwrap();
            provider.updated_at = Some("2026-11-27".into());
            provider.subscription_info = Some(ProviderUsage {
                upload: 10,
                download: 20,
                total: 100,
                expire: 0,
            });
            inventory.providers.insert(
                "本地".into(),
                ProviderStatus {
                    vehicle_type: "File".into(),
                    updated_at: None,
                    subscription_info: None,
                },
            );
        });
        *result.borrow_mut() = Some(paw.clone());
        gpui_kit::base::Root::new(paw, window, cx)
    });
    let paw = holder.borrow_mut().take().unwrap();
    ui.update(|window, cx| {
        window.activate_window();
        window.render_frame(cx);
        assert_eq!(
            window.find("proxy-providers-toggle").label(),
            Some("管理提供者 (2)")
        );
        window.click("proxy-providers-toggle", cx);
        window.render_frame(cx);
        let id = "provider-refresh:示例供应商";
        let bulk = "provider-refresh-all";
        assert_eq!(window.find(bulk).label(), Some("刷新所有 HTTP 代理集合"));
        assert_ne!(window.find(id).disabled(), Some(true));
        assert!(window.try_find("provider-refresh:本地").is_none());
        let texts = gpui_kit::base::test_support::texts(window);
        assert!(texts.iter().any(|text| text.text().contains("2026-11-27")));
        assert!(texts.iter().any(|text| text.text().contains("10 B")
            && text.text().contains("20 B")
            && text.text().contains("100 B")));
        paw.update(cx, |paw, cx| {
            paw.error = Some("sentinel".into());
            paw.snapshot.engine = meow_paw::app::EngineStatus::Running { mixed_port: 7890 };
            cx.notify();
        });
        window.render_frame(cx);
        assert_ne!(window.find(bulk).disabled(), Some(true));
        window.press("tab", cx);
        assert_eq!(window.find(bulk).focused(), Some(true));
        window.press("tab", cx);
        assert_eq!(window.find(id).focused(), Some(true));
        window.press("enter", cx);
        assert_eq!(paw.read(cx).error, None);
        paw.update(cx, |paw, cx| {
            paw.snapshot.refreshing_provider = Some("示例供应商".into());
            cx.notify();
        });
        window.render_frame(cx);
        assert_eq!(window.find(bulk).disabled(), Some(true));
        assert_eq!(window.find(id).disabled(), Some(true));
        assert_eq!(window.find(id).label(), Some("正在刷新…: 示例供应商"));
        paw.update(cx, |paw, cx| {
            paw.snapshot.refreshing_provider = None;
            paw.snapshot.inventory_stale = true;
            cx.notify();
        });
        window.render_frame(cx);
        assert!(window.try_find("provider-panel-warning").is_some());
        assert_eq!(window.find(bulk).disabled(), Some(true));
        assert_eq!(window.find(id).disabled(), Some(true));
        assert_eq!(window.find(id).label(), Some("刷新不可用: 示例供应商"));
        let texts = gpui_kit::base::test_support::texts(window);
        assert!(texts.iter().any(|text| text.text().contains("可能已过期")));
        assert!(!texts.iter().any(|text| text.text().contains("自动更新")));
        paw.update(cx, |paw, cx| {
            paw.snapshot.inventory_stale = false;
            let inventory = Arc::make_mut(paw.snapshot.inventory.as_mut().unwrap());
            inventory.provider_error = Some("private diagnostic sentinel".into());
            inventory.providers.clear();
            cx.notify();
        });
        window.render_frame(cx);
        assert!(window.try_find("provider-panel-warning").is_some());
        assert!(window.try_find("provider-panel-empty").is_none());
        assert_eq!(window.find(bulk).disabled(), Some(true));
        for (locale, copy) in [
            (
                Locale::ZhCn,
                "无法读取提供者。数据恢复后，此面板会自动更新。",
            ),
            (
                Locale::En,
                "Could not read providers. This panel updates automatically",
            ),
        ] {
            paw.update(cx, |paw, cx| {
                paw.locale = locale;
                paw.snapshot.mode = Some(meow_paw::engine::controller::RoutingMode::Direct);
                cx.notify();
            });
            window.render_frame(cx);
            let texts = gpui_kit::base::test_support::texts(window);
            assert!(texts.iter().any(|text| text.text().contains(copy)));
            assert!(!texts.iter().any(|text| {
                text.text().contains("Proxy groups are still available")
                    || text.text().contains("代理组仍可使用")
            }));
            assert!(
                !texts
                    .iter()
                    .any(|text| text.text().contains("private diagnostic sentinel"))
            );
        }
        paw.update(cx, |paw, cx| {
            paw.locale = Locale::ZhCn;
            Arc::make_mut(paw.snapshot.inventory.as_mut().unwrap()).provider_error = None;
            cx.notify();
        });
        window.render_frame(cx);
        assert!(window.try_find("provider-panel-warning").is_none());
        assert!(window.try_find("provider-panel-empty").is_some());
        assert_eq!(window.find(bulk).disabled(), Some(true));
        assert_eq!(window.find("provider-panel-close").focused(), Some(true));
        window.press("escape", cx);
        window.render_frame(cx);
        assert!(window.find("proxy-providers-toggle").focused().unwrap());
    });
}

#[gpui_kit::test]
fn sorting_from_proxy_header_does_not_collapse_group(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let holder = Rc::new(RefCell::new(None));
    let holder2 = holder.clone();
    let (_, ui) = cx.add_window_view(move |window, cx| {
        let paw = open(
            None,
            None,
            (Page::Proxies, Locale::En),
            false,
            (Preferences::discover(false).0, None),
            window,
            cx,
        );
        super::seed_proxy_fixture(&paw, cx);
        *holder2.borrow_mut() = Some(paw.clone());
        gpui_kit::base::Root::new(paw, window, cx)
    });
    let paw = holder.borrow_mut().take().unwrap();
    ui.update(|window, cx| {
        window.activate_window();
        window.render_frame(cx);
        let sort = window.find("proxy-order:Daily Proxy");
        assert_eq!(sort.role(), Some(gpui_kit::Role::Button));
        assert_eq!(
            window.find("group-compare:Daily Proxy").role(),
            Some(gpui_kit::Role::Button)
        );
        window.click("group-compare:Daily Proxy", cx);
        assert!(paw.read(cx).expanded_groups.contains("Daily Proxy"));
        window.click("proxy-order:Daily Proxy", cx);
        let state = paw.read(cx);
        assert!(state.proxy_order.contains_key("Daily Proxy"));
        assert!(state.expanded_groups.contains("Daily Proxy"));
        assert!(state.snapshot.group_probe.is_none());
    });
}

#[gpui_kit::test]
fn proxy_expand_tooltip_tracks_group_with_pointer_still_hovering(cx: &mut TestAppContext) {
    use gpui_kit::{point, px, size};

    cx.update(gpui_kit::init);
    let holder = Rc::new(RefCell::new(None));
    let result = holder.clone();
    let (_, ui) = cx.add_window_view(move |window, cx| {
        let paw = open(
            None,
            None,
            (Page::Proxies, Locale::En),
            false,
            (Preferences::discover(false).0, None),
            window,
            cx,
        );
        super::seed_proxy_fixture(&paw, cx);
        *result.borrow_mut() = Some(paw.clone());
        gpui_kit::base::Root::new(paw, window, cx)
    });
    let paw = holder.borrow_mut().take().unwrap();
    ui.simulate_resize(size(px(1120.), px(900.)));
    let button = "group-expand:Daily Proxy";
    // Complete initial window bounds callbacks before moving the pointer.
    // GPUI's test platform reports (0, 0) when asked during a bounds change.
    ui.update(|window, _| window.activate_window());
    ui.run_until_parked();
    ui.executor().advance_clock(Duration::from_secs(1));
    ui.run_until_parked();
    let at = ui.update(|window, cx| {
        window.render_frame(cx);
        window.find(button).bounds().center()
    });
    ui.simulate_mouse_move(at, None, Default::default());
    ui.run_until_parked();
    ui.executor().advance_clock(Duration::from_secs(1));
    ui.run_until_parked();

    for (expected, stale, toggle) in [
        ("Collapse group", "Expand group", true),
        ("Expand group", "Collapse group", true),
        ("Collapse group", "Expand group", false),
    ] {
        ui.update(|window, cx| {
            window.render_frame(cx);
            let control = window.find(button);
            assert_eq!(control.role(), Some(gpui_kit::Role::Button));
            assert_eq!(control.label(), Some(expected));
            let texts = gpui_kit::base::test_support::texts(window);
            assert_eq!(window.mouse_position(), at, "pointer moved between states");
            let visible = |label| {
                texts
                    .iter()
                    .any(|text| text.visible() && text.text() == label)
            };
            assert!(
                visible(expected),
                "expected visible hover hint {expected:?}"
            );
            assert!(
                !visible(stale),
                "stale hover hint {stale:?} is still visible"
            );
            // An expansion can also originate outside the hovered button.
            // Keep the pointer still so this exercises the already-open popup.
            if toggle {
                paw.update(cx, |paw, cx| {
                    paw.toggle_proxy_group("Daily Proxy");
                    cx.notify();
                });
            }
        });
    }
    ui.simulate_mouse_move(point(px(3.), px(3.)), None, Default::default());
    ui.executor().advance_clock(Duration::from_secs(1));
    ui.run_until_parked();
    ui.update(|window, cx| {
        window.render_frame(cx);
        assert!(
            !gpui_kit::base::test_support::texts(window)
                .iter()
                .any(|text| {
                    text.visible() && matches!(text.text(), "Expand group" | "Collapse group")
                })
        );
    });
}

#[gpui_kit::test]
fn proxy_group_actions_align_across_states_and_apply_locally(cx: &mut TestAppContext) {
    use super::GroupRouteField;
    cx.update(gpui_kit::init);
    let holder = Rc::new(RefCell::new(None));
    let result = holder.clone();
    let (_, ui) = cx.add_window_view(move |window, cx| {
        let paw = open(
            None,
            None,
            (Page::Proxies, Locale::En),
            false,
            (Preferences::discover(false).0, None),
            window,
            cx,
        );
        super::seed_proxy_fixture(&paw, cx);
        *result.borrow_mut() = Some(paw.clone());
        gpui_kit::base::Root::new(paw, window, cx)
    });
    let paw = holder.borrow_mut().take().unwrap();
    for width in [1120., 760., 560.] {
        ui.simulate_resize(gpui_kit::size(gpui_kit::px(width), gpui_kit::px(900.)));
        ui.update(|window, cx| {
            window.activate_window();
            window.render_frame(cx);
            assert!(paw.read(cx).expanded_groups.contains("Daily Proxy"));
            assert!(!paw.read(cx).expanded_groups.contains("Work"));
            let mut prev = gpui_kit::px(0.);
            for prefix in [
                "locate",
                "group-compare",
                "proxy-order",
                "group-probe-url",
                "group-details",
                "group-filter",
                "group-expand",
            ] {
                let find =
                    |name| window.find(gpui_kit::SharedString::from(format!("{prefix}:{name}")));
                let open = find("Daily Proxy");
                let closed = find("Work");
                assert_eq!(open.role(), Some(gpui_kit::Role::Button));
                assert_eq!(closed.role(), Some(gpui_kit::Role::Button));
                assert_eq!(open.bounds().size.width, gpui_kit::px(26.));
                assert_eq!(
                    closed.bounds().origin.x,
                    open.bounds().origin.x,
                    "{prefix} at {width}"
                );
                assert!(open.bounds().origin.x > prev);
                assert!(open.bounds().origin.x + gpui_kit::px(26.) < gpui_kit::px(width));
                prev = open.bounds().origin.x;
            }
        });
    }

    ui.simulate_resize(gpui_kit::size(gpui_kit::px(1120.), gpui_kit::px(900.)));
    ui.update(|window, cx| {
        window.render_frame(cx);
        window.click("group-filter:Work", cx);
        window.render_frame(cx);
        assert!(paw.read(cx).expanded_groups.contains("Work"));
        let filter = paw.read(cx).group_route_tools["Work"]
            .filter
            .as_ref()
            .unwrap()
            .input
            .clone();
        filter.update(cx, |input, cx| input.set_value("DIRECT", window, cx));
        window.render_frame(cx);
        assert!(window.find_all("proxy:Work:1").is_empty());
        assert!(window.find("proxy:Work:0").visible());
        assert!(!window.find_all("proxy:Daily Proxy:1").is_empty());
        window.click("group-details:Work", cx);
        assert!(paw.read(cx).group_route_tools["Work"].hide_details);
        assert!(!paw.read(cx).group_route_tools.contains_key("Daily Proxy"));
        window.click("group-probe-url:Work", cx);
        window.render_frame(cx);
        assert!(paw.read(cx).group_route_tools["Work"].open == Some(GroupRouteField::Url));
        assert!(paw.read(cx).snapshot.group_probe.is_none());
        window.click("group-compare:Daily Proxy", cx);
        window.render_frame(cx);
        assert!(paw.read(cx).group_route_tools["Daily Proxy"].open == Some(GroupRouteField::Url));
        assert!(paw.read(cx).snapshot.group_probe.is_none());
    });
}

#[gpui_kit::test]
fn profile_forget_has_context_and_leaves_the_original_file(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("example.yaml");
    fs::write(&path, "mode: rule\\n").unwrap();
    let profile = path.clone();
    let holder = Rc::new(RefCell::new(None));
    let holder2 = holder.clone();
    let (_, ui) = cx.add_window_view(move |window, cx| {
        let mut preferences = Preferences::discover(false).0;
        preferences.remember(false, &profile).unwrap();
        let paw = open(
            None,
            None,
            (Page::Profiles, Locale::En),
            false,
            (preferences, None),
            window,
            cx,
        );
        *holder2.borrow_mut() = Some(paw.clone());
        gpui_kit::base::Root::new(paw, window, cx)
    });
    let paw = holder.borrow_mut().take().unwrap();
    ui.update(|window, cx| {
        window.activate_window();
        window.render_frame(cx);
        let id = gpui_kit::SharedString::from(format!("profile-forget:{path:?}"));
        let button = window.find(id.clone());
        assert_eq!(button.role(), Some(gpui_kit::Role::Button));
        assert_eq!(
            button.label().unwrap(),
            format!("{}: example.yaml", Locale::En.message(Message::Forget))
        );
        window.click(id, cx);
        assert!(paw.read(cx).preferences.profiles().is_empty());
    });
    assert_eq!(fs::read_to_string(&path).unwrap(), "mode: rule\\n");
}

#[gpui_kit::test]
fn proxy_cards_keep_equal_tracks_and_expose_selected_protocol(cx: &mut TestAppContext) {
    use meow_paw::engine::controller::{Inventory, Node, Origin, Proxy};
    cx.update(gpui_kit::init);
    let (_, ui) = cx.add_window_view(move |window, cx| {
        let paw = open(
            None,
            None,
            (Page::Proxies, Locale::En),
            false,
            (Preferences::discover(false).0, None),
            window,
            cx,
        );
        paw.update(cx, |paw, _| {
            let members = [
                "Selected",
                "A particularly long name that should not change its track width",
                "Third",
                "Tail",
            ];
            let group = Proxy {
                kind: "Selector".into(),
                now: Some(members[0].into()),
                all: Some(members.into_iter().map(str::to_owned).collect()),
                ..Proxy::default()
            };
            paw.snapshot.mode = Some(RoutingMode::Rule);
            paw.expanded_groups.insert("Fixture".into());
            paw.snapshot.inventory = Some(Arc::new(Inventory {
                groups: [("Fixture".into(), group)].into(),
                nodes: members
                    .into_iter()
                    .map(|name| Node {
                        name: name.into(),
                        origin: Origin::Local,
                        proxy: Proxy {
                            kind: "Socks5".into(),
                            ..Proxy::default()
                        },
                    })
                    .collect(),
                ..Inventory::default()
            }));
        });
        gpui_kit::base::Root::new(paw, window, cx)
    });
    let px = gpui_kit::px;
    for (width, columns) in [(1120., 3), (760., 2), (560., 1)] {
        ui.simulate_resize(gpui_kit::size(px(width), px(738.)));
        ui.update(|window, cx| {
            window.render_frame(cx);
            let first = window.find("proxy:Fixture:0");
            assert_eq!(first.checked(), Some(true));
            assert_eq!(first.label(), Some("Selected · Socks5"));
            let first_bounds = first.bounds();
            let tail = window.find("proxy:Fixture:3").bounds();
            for index in 1..=3 {
                let id = gpui_kit::SharedString::from(format!("proxy:Fixture:{index}"));
                let next = window.find(id);
                assert_eq!(next.checked(), Some(false));
                assert!(
                    (next.bounds().size.width - first_bounds.size.width).abs() <= px(1.),
                    "width {width}, card {index}: {next:?} versus {first_bounds:?}",
                );
            }
            assert!(tail.origin.y > first_bounds.origin.y);
            let expected_x = window
                .find(gpui_kit::SharedString::from(format!(
                    "proxy:Fixture:{}",
                    3 % columns
                )))
                .bounds()
                .origin
                .x;
            assert!((tail.origin.x - expected_x).abs() <= px(1.));
        });
    }
}

#[gpui_kit::test]
fn jump_navigation_stays_reachable_and_scrolls_only_on_activation(cx: &mut TestAppContext) {
    use meow_paw::engine::controller::{Inventory, Proxy};

    cx.update(gpui_kit::init);
    let holder = Rc::new(RefCell::new(None));
    let result = holder.clone();
    let (_, ui) = cx.add_window_view(move |window, cx| {
        let paw = open(
            None,
            None,
            (Page::Proxies, Locale::En),
            false,
            (Preferences::discover(false).0, None),
            window,
            cx,
        );
        paw.update(cx, |paw, _| {
            let large = Proxy {
                kind: "Selector".into(),
                all: Some((0..80).map(|i| format!("Node {i}")).collect()),
                ..Proxy::default()
            };
            let small = Proxy {
                kind: "Selector".into(),
                all: Some(vec!["DIRECT".into()]),
                ..Proxy::default()
            };
            paw.snapshot.mode = Some(RoutingMode::Rule);
            paw.snapshot.inventory = Some(Arc::new(Inventory {
                groups: [
                    ("Daily".into(), large),
                    ("Weekend".into(), small.clone()),
                    ("Work".into(), small),
                ]
                .into(),
                ..Inventory::default()
            }));
            paw.expanded_groups.insert("Daily".into());
        });
        *result.borrow_mut() = Some(paw.clone());
        gpui_kit::base::Root::new(paw, window, cx)
    });
    let paw = holder.borrow_mut().take().unwrap();
    let px = gpui_kit::px;
    ui.simulate_resize(gpui_kit::size(px(1120.), px(540.)));
    ui.update(|window, cx| {
        window.activate_window();
        window.render_frame(cx);
        assert!(window.try_find("proxy-jump-menu").is_none());
        let rail = window.find("jump-group:Work");
        assert_eq!(rail.role(), Some(gpui_kit::Role::Button));
        assert_eq!(rail.label(), Some("Jump to group: Work"));
        assert_eq!(
            window.find("jump-group:Weekend").label(),
            Some("Jump to group: Weekend")
        );
        assert!(rail.bounds().origin.x > px(1080.));
        let scroll = paw.read(cx).proxy_group_scroll.clone();
        window.hover("jump-group:Work", cx);
        assert_eq!(
            scroll.offset().y,
            px(0.),
            "hover must not move the main pane"
        );
        window.click("jump-group:Daily", cx);
        window.press("tab", cx);
        window.press("tab", cx);
        assert!(window.find("jump-group:Work").focused().unwrap());
        window.press("enter", cx);
        window.simulate_next_frame(cx);
        window.render_frame(cx);
        assert!(scroll.offset().y < px(-200.));
        assert!(
            window.find("jump-group:Work").visible(),
            "nav rail must not scroll away"
        );
        assert!(
            !paw.read(cx).expanded_groups.contains("Work"),
            "jump is not expansion"
        );
        scroll.set_offset(gpui_kit::point(px(0.), px(0.)));
        window.render_frame(cx);
    });

    ui.simulate_resize(gpui_kit::size(px(760.), px(540.)));
    ui.update(|window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("jump-group:Work").is_none());
        assert!(window.find("proxy-jump-menu").visible());
        window.click("proxy-jump-menu", cx);
        window.press("down", cx);
        window.press("down", cx);
        window.press("down", cx);
        window.press("enter", cx);
        window.simulate_next_frame(cx);
        window.render_frame(cx);
        assert!(paw.read(cx).proxy_group_scroll.offset().y < px(-200.));
        assert!(!paw.read(cx).expanded_groups.contains("Work"));
        let search = paw.read(cx).search.clone();
        search.update(cx, |input, cx| {
            input.set_value("no matching group", window, cx)
        });
        window.render_frame(cx);
        assert!(window.try_find("proxy-jump-menu").is_none());
    });
}

#[gpui_kit::test]
fn locating_the_selected_route_expands_its_group_and_scrolls(cx: &mut TestAppContext) {
    use meow_paw::engine::controller::{Inventory, Proxy};

    cx.update(gpui_kit::init);
    let holder = Rc::new(RefCell::new(None));
    let holder2 = holder.clone();
    let (_, ui) = cx.add_window_view(move |window, cx| {
        let paw = open(
            None,
            None,
            (Page::Proxies, Locale::ZhCn),
            false,
            (Preferences::discover(false).0, None),
            window,
            cx,
        );
        let mut all: Vec<_> = (0..60).map(|i| format!("Node {i:02}")).collect();
        all.extend(["当前节点".into(), "当前节点".into()]);
        let first = Proxy {
            kind: "Selector".into(),
            now: Some("当前节点".into()),
            all: Some(all),
            ..Proxy::default()
        };
        let second = Proxy {
            all: Some(vec!["当前节点".into()]),
            ..first.clone()
        };
        paw.update(cx, |paw, _| {
            paw.snapshot.mode = Some(RoutingMode::Rule);
            paw.snapshot.inventory = Some(Arc::new(Inventory {
                groups: [("Group A".into(), first), ("Group B".into(), second)].into(),
                ..Inventory::default()
            }));
        });
        *holder2.borrow_mut() = Some(paw.clone());
        gpui_kit::base::Root::new(paw, window, cx)
    });
    let paw = holder.borrow_mut().take().unwrap();
    ui.update(|window, cx| {
        window.activate_window();
        window.render_frame(cx);
        let locator = window.find("locate:Group A");
        assert_eq!(locator.role(), Some(gpui_kit::Role::Button));
        assert!(locator.label().unwrap().contains("当前节点"));
        let starting = paw.read(cx).proxy_group_scroll.offset().y;
        window.click("locate:Group A", cx);
        window.render_frame(cx);
        window.simulate_next_frame(cx);
        window.render_frame(cx);
        let paw = paw.read(cx);
        assert!(paw.expanded_groups.contains("Group A"));
        assert!(!paw.expanded_groups.contains("Group B"));
        assert!(
            paw.proxy_group_scroll.offset().y < starting - gpui_kit::px(200.),
            "the selected deep candidate must scroll into view"
        );
        assert!(
            paw.locate_route.is_none(),
            "scroll must not repeat on refresh"
        );
    });
}

#[gpui_kit::test]
fn proxy_groups_follow_mode_transitions_without_losing_selections(cx: &mut TestAppContext) {
    use meow_paw::engine::controller::{Inventory, Proxy};
    cx.update(gpui_kit::init);
    for locale in [Locale::En, Locale::ZhCn] {
        let holder = Rc::new(RefCell::new(None));
        let result = holder.clone();
        let (_, ui) = cx.add_window_view(move |window, cx| {
            let paw = open(
                None,
                None,
                (Page::Proxies, locale),
                false,
                (Preferences::discover(false).0, None),
                window,
                cx,
            );
            let group = |selected: &str, members: &[&str]| Proxy {
                kind: "Selector".into(),
                now: Some(selected.into()),
                all: Some(members.iter().map(|member| (*member).into()).collect()),
                ..Proxy::default()
            };
            paw.update(cx, |paw, _| {
                paw.snapshot.engine = EngineStatus::Running { mixed_port: 1234 };
                paw.snapshot.mode = Some(RoutingMode::Global);
                paw.snapshot.inventory = Some(Arc::new(Inventory {
                    groups: [
                        ("GLOBAL".into(), group("Work", &["Work", "DIRECT"])),
                        ("Work".into(), group("Node B", &["Node A", "Node B"])),
                    ]
                    .into(),
                    ..Inventory::default()
                }));
                paw.expanded_groups.insert("Work".into());
            });
            *result.borrow_mut() = Some(paw.clone());
            gpui_kit::base::Root::new(paw, window, cx)
        });
        let paw = holder.borrow_mut().take().unwrap();
        ui.update(|window, cx| {
            window.activate_window();
            window.render_frame(cx);
            assert!(window.try_find("proxy:GLOBAL:0").is_some());
            assert!(window.try_find("proxy:Work:1").is_some());
        });
        for mode in [
            RoutingMode::Direct,
            RoutingMode::Rule,
            RoutingMode::Direct,
            RoutingMode::Global,
        ] {
            paw.update(ui, |paw, cx| {
                paw.snapshot.mode = Some(mode);
                cx.notify();
            });
            ui.update(|window, cx| {
                window.render_frame(cx);
                assert_eq!(
                    window.try_find("proxy-direct-explanation").is_some(),
                    mode == RoutingMode::Direct
                );
                assert_eq!(
                    window.try_find("proxy:GLOBAL:0").is_some(),
                    mode == RoutingMode::Global
                );
                assert_eq!(
                    window.try_find("proxy:Work:1").is_some(),
                    mode != RoutingMode::Direct
                );
                if mode != RoutingMode::Direct {
                    assert_eq!(window.find("proxy:Work:1").checked(), Some(true));
                    assert_eq!(window.find("proxy:Work:0").checked(), Some(false));
                }
                if mode == RoutingMode::Global {
                    assert_eq!(window.find("proxy:GLOBAL:0").checked(), Some(true));
                }
                assert!(window.try_find("proxy-providers-toggle").is_some());
                assert!(window.try_find(("routing-mode", 0usize)).is_some());
                assert!(window.try_find(("routing-mode", 1usize)).is_some());
                let inventory = paw.read(cx).snapshot.inventory.as_ref().unwrap();
                assert_eq!(inventory.groups["GLOBAL"].now.as_deref(), Some("Work"));
                assert_eq!(inventory.groups["Work"].now.as_deref(), Some("Node B"));
            });
        }
        for (mode, stale, engine) in [
            (
                Some(RoutingMode::Direct),
                true,
                EngineStatus::Running { mixed_port: 1234 },
            ),
            (Some(RoutingMode::Direct), false, EngineStatus::Stopped),
            (None, false, EngineStatus::Running { mixed_port: 1234 }),
        ] {
            paw.update(ui, |paw, cx| {
                paw.snapshot.mode = mode;
                paw.snapshot.inventory_stale = stale;
                paw.snapshot.engine = engine;
                cx.notify();
            });
            ui.update(|window, cx| {
                window.render_frame(cx);
                assert!(window.try_find("proxy-direct-explanation").is_none());
                if mode.is_none() {
                    assert_eq!(window.find("proxy:Work:0").disabled(), Some(true));
                }
            });
        }
        paw.update(ui, |paw, cx| {
            paw.snapshot.engine = EngineStatus::Running { mixed_port: 1234 };
            paw.snapshot.mode = Some(RoutingMode::Direct);
            paw.snapshot.inventory_stale = false;
            paw.snapshot.applying_profile = Some("pending.yaml".into());
            cx.notify();
        });
        ui.update(|window, cx| {
            window.render_frame(cx);
            assert!(window.try_find("proxy-direct-explanation").is_none());
            assert!(window.try_find("proxy-apply-pending").is_some());
            assert!(window.try_find("proxy:GLOBAL:0").is_none());
            assert!(window.try_find("proxy:Work:0").is_none());
        });
    }
}

fn quote(path: &Path) -> String {
    format!("'{}'", path.to_str().unwrap().replace('\'', "'\\''"))
}

fn wait_for(mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !condition() {
        assert!(
            Instant::now() < deadline,
            "timed out waiting for synthetic controller"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

fn requests(dir: &Path) -> Vec<String> {
    fs::read_to_string(dir.join("requests"))
        .unwrap_or_default()
        .lines()
        .map(str::to_owned)
        .collect()
}

#[gpui_kit::test]
fn connection_row_fences_delayed_apply_and_pointer_closes_once(cx: &mut TestAppContext) {
    match TcpListener::bind((Ipv4Addr::LOCALHOST, 0)) {
        Ok(listener) => drop(listener),
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
            eprintln!("SKIP connection row integration: loopback sockets unavailable: {error}");
            return;
        }
        Err(error) => panic!("loopback availability probe failed: {error}"),
    }
    cx.update(gpui_kit::init);
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("connection-fixture"), "").unwrap();
    let binary = dir.path().join("fake-controller");
    let script = format!(
        "#!/bin/sh\nconfig=\nwhile [ $# -gt 0 ]; do\n  case \"$1\" in\n    -t) exit 0 ;;\n    -f) shift; config=$1 ;;\n  esac\n  shift\ndone\nPAW_ROUTING_FAKE_CONFIG=\"$config\" PAW_ROUTING_FAKE_DIR={} exec {} --ignored --exact view::routing_tests::fake_controller_child\n",
        quote(dir.path()),
        quote(&std::env::current_exe().unwrap())
    );
    fs::write(&binary, script).unwrap();
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o700)).unwrap();
    let profile = dir.path().join("synthetic.yaml");
    fs::write(&profile, "mode: direct\nrules: ['MATCH,DIRECT']\n").unwrap();
    let desktop = Desktop::with_mixed_port(binary, 0).unwrap();
    desktop.start(profile.clone()).unwrap();
    wait_for(|| {
        desktop
            .snapshot()
            .connections
            .as_ref()
            .is_some_and(|rows| !rows.is_empty())
    });
    let snapshot = desktop.snapshot();
    let old_epoch = snapshot.probe_epoch;
    let holder = Rc::new(RefCell::new(None));
    let out = holder.clone();
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
            paw.desktop = Some(desktop);
            paw.snapshot = snapshot;
        });
        *out.borrow_mut() = Some(paw.clone());
        let rows = cx.new(|_| Connections(paw));
        gpui_kit::base::Root::new(rows, window, cx)
    });
    let paw = holder.borrow_mut().take().unwrap();
    ui.update(|window, cx| {
        window.activate_window();
        window.render_frame(cx);
        assert_eq!(
            window.find("connection-close:row-id").label(),
            Some("Close connection: example.invalid:443")
        );
    });
    paw.update(ui, |paw, _| {
        paw.desktop
            .as_ref()
            .unwrap()
            .apply_profile(profile)
            .unwrap()
    });
    wait_for(|| {
        requests(dir.path())
            .iter()
            .any(|line| line == "PUT /configs")
    });
    ui.update(|window, cx| {
        window.click("connection-close:row-id", cx);
        assert!(
            paw.read(cx)
                .error
                .as_deref()
                .is_some_and(|error| error.contains("session changed"))
        );
    });
    assert!(
        requests(dir.path())
            .iter()
            .all(|line| !line.starts_with("DELETE "))
    );
    fs::write(dir.path().join("release-apply"), "").unwrap();
    wait_for(|| {
        ui.update(|_, cx| {
            let snapshot = paw.read(cx).desktop.as_ref().unwrap().snapshot();
            snapshot.probe_epoch != old_epoch
                && snapshot.applying_profile.is_none()
                && snapshot
                    .connections
                    .as_ref()
                    .is_some_and(|rows| rows.iter().any(|row| row.id == "row-id"))
        })
    });
    ui.update(|window, cx| {
        window.click("connection-close:row-id", cx);
        assert!(
            paw.read(cx)
                .error
                .as_deref()
                .is_some_and(|error| error.contains("session changed"))
        );
    });
    assert!(
        requests(dir.path())
            .iter()
            .all(|line| !line.starts_with("DELETE "))
    );
    paw.update(ui, |paw, cx| {
        paw.snapshot = paw.desktop.as_ref().unwrap().snapshot();
        cx.notify();
    });
    ui.update(|window, cx| {
        window.render_frame(cx);
        window.click("connection-close:row-id", cx);
    });
    wait_for(|| {
        requests(dir.path())
            .iter()
            .any(|line| line == "DELETE /connections/row-id")
    });
    paw.update(ui, |paw, _| drop(paw.desktop.take()));
    assert_eq!(
        requests(dir.path())
            .iter()
            .filter(|line| line.starts_with("DELETE "))
            .collect::<Vec<_>>(),
        ["DELETE /connections/row-id"]
    );
}

#[gpui_kit::test]
fn routing_segments_are_focusable_and_activate_without_a_controller(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    for locale in [Locale::En, Locale::ZhCn] {
        let holder = Rc::new(RefCell::new(None));
        let holder2 = holder.clone();
        let (_, ui) = cx.add_window_view(move |window, cx| {
            let paw = open(
                None,
                None,
                (Page::Proxies, locale),
                false,
                (Preferences::discover(false).0, None),
                window,
                cx,
            );
            paw.update(cx, |paw, _| {
                paw.snapshot.mode = Some(RoutingMode::Direct);
                paw.error = Some("sentinel".into());
            });
            *holder2.borrow_mut() = Some(paw.clone());
            let header = cx.new(|_| Header(paw));
            gpui_kit::base::Root::new(header, window, cx)
        });
        let paw = holder.borrow_mut().take().unwrap();
        ui.update(|window, _| window.activate_window());
        ui.update(|window, cx| {
            window.render_frame(cx);
            for (index, message) in [Message::ModeRule, Message::ModeGlobal, Message::ModeDirect]
                .into_iter()
                .enumerate()
            {
                let segment = window.find(("routing-mode", index));
                assert_eq!(segment.role(), Some(gpui_kit::Role::Button));
                assert_eq!(segment.checked(), Some(index == 2));
                let label = segment.label().unwrap();
                assert!(label.contains(locale.message(message)));
                assert_eq!(
                    label.contains(locale.message(Message::ActiveTab)),
                    index == 2
                );
            }
        });
        let mut focuses = Vec::new();
        for _ in 0..3 {
            ui.update(|window, cx| {
                window.press("tab", cx);
                focuses.push(window.focused(cx).unwrap());
            });
        }
        assert_ne!(focuses[0], focuses[1]);
        assert_ne!(focuses[0], focuses[2]);
        assert_ne!(focuses[1], focuses[2]);
        ui.update(|window, cx| {
            window.press("enter", cx);
            window.press("space", cx);
            window.click(("routing-mode", 2usize), cx);
        });
        ui.update(|_, cx| assert_eq!(paw.read(cx).error.as_deref(), Some("sentinel")));
        for (index, key) in [(1usize, "space"), (0, "enter")] {
            ui.update(|window, cx| {
                window.press("shift-tab", cx);
                assert_eq!(window.focused(cx).as_ref(), Some(&focuses[index]));
                window.press(key, cx);
            });
            ui.update(|_, cx| assert_eq!(paw.read(cx).error, None));
            paw.update(ui, |paw, _| paw.error = Some("sentinel".into()));
        }
        ui.update(|window, cx| window.click(("routing-mode", 0usize), cx));
        ui.update(|_, cx| assert_eq!(paw.read(cx).error, None));
    }
}

#[gpui_kit::test]
fn routing_keyboard_activates_once_and_selected_segments_are_inert(cx: &mut TestAppContext) {
    match TcpListener::bind((Ipv4Addr::LOCALHOST, 0)) {
        Ok(listener) => drop(listener),
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
            eprintln!(
                "SKIP synthetic controller integration: loopback sockets unavailable: {error}"
            );
            return;
        }
        Err(error) => panic!("loopback availability probe failed: {error}"),
    }
    cx.update(gpui_kit::init);
    for (key, target_tabs, target, locale) in [
        ("enter", 1, RoutingMode::Rule, Locale::En),
        ("space", 2, RoutingMode::Global, Locale::ZhCn),
        ("click", 1, RoutingMode::Rule, Locale::En),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let executable = std::env::current_exe().unwrap();
        let binary = dir.path().join("fake-controller");
        let script = format!(
            "#!/bin/sh\nconfig=\nwhile [ $# -gt 0 ]; do\n  case \"$1\" in\n    -t) exit 0 ;;\n    -f) shift; config=$1 ;;\n  esac\n  shift\ndone\nPAW_ROUTING_FAKE_CONFIG=\"$config\" PAW_ROUTING_FAKE_DIR={} exec {} --ignored --exact view::routing_tests::fake_controller_child\n",
            quote(dir.path()),
            quote(&executable)
        );
        fs::write(&binary, script).unwrap();
        fs::set_permissions(&binary, fs::Permissions::from_mode(0o700)).unwrap();
        let profile = dir.path().join("synthetic.yaml");
        fs::write(&profile, "mode: direct\nrules: ['MATCH,DIRECT']\n").unwrap();
        let desktop = Desktop::with_mixed_port(binary, 0).unwrap();
        desktop.start(profile).unwrap();
        wait_for(|| {
            let snapshot = desktop.snapshot();
            assert!(
                !matches!(snapshot.engine, EngineStatus::Error(_)),
                "{snapshot:?}"
            );
            snapshot.mode == Some(RoutingMode::Direct)
        });
        let snapshot = desktop.snapshot();
        let holder = Rc::new(RefCell::new(None));
        let holder2 = holder.clone();
        let (_, ui) = cx.add_window_view(move |window, cx| {
            let paw = open(
                None,
                None,
                (Page::Proxies, locale),
                false,
                (Preferences::discover(false).0, None),
                window,
                cx,
            );
            paw.update(cx, |paw, _| {
                paw.desktop = Some(desktop);
                paw.snapshot = snapshot;
            });
            *holder2.borrow_mut() = Some(paw.clone());
            let header = cx.new(|_| Header(paw));
            gpui_kit::base::Root::new(header, window, cx)
        });
        let paw = holder.borrow_mut().take().unwrap();
        ui.update(|window, _| window.activate_window());
        ui.run_until_parked();
        ui.update(|window, cx| {
            window.render_frame(cx);
            for (index, mode_message) in
                [Message::ModeRule, Message::ModeGlobal, Message::ModeDirect]
                    .into_iter()
                    .enumerate()
            {
                let segment = window.find(("routing-mode", index));
                assert_eq!(segment.role(), Some(gpui_kit::Role::Button));
                assert_eq!(segment.checked(), Some(index == 2));
                assert!(
                    segment
                        .label()
                        .unwrap()
                        .contains(locale.message(mode_message))
                );
                assert_eq!(
                    segment
                        .label()
                        .unwrap()
                        .contains(locale.message(Message::ActiveTab)),
                    index == 2
                );
            }
        });
        // Establish the actual three separate stops, including the current segment.
        let mut focuses = Vec::new();
        for _ in 0..3 {
            ui.update(|window, cx| {
                window.press("tab", cx);
                focuses.push(window.focused(cx).unwrap());
            });
        }
        assert_ne!(focuses[0], focuses[1]);
        assert_ne!(focuses[0], focuses[2]);
        assert_ne!(focuses[1], focuses[2]);
        // Direct is selected. Both native keyboard activations must do nothing.
        ui.update(|window, cx| {
            window.press("enter", cx);
            window.press("space", cx);
        });
        assert!(
            requests(dir.path())
                .iter()
                .all(|line| !line.starts_with("PATCH "))
        );
        // Return to the requested target using normal reverse Tab traversal.
        for _ in target_tabs..3 {
            ui.update(|window, cx| window.press("shift-tab", cx));
        }
        ui.update(|window, cx| {
            window.draw(cx).clear(cx);
            assert_eq!(window.focused(cx).as_ref(), Some(&focuses[target_tabs - 1]));
        });
        if key == "click" {
            ui.update(|window, cx| window.click(("routing-mode", target_tabs - 1), cx));
        } else {
            ui.update(|window, cx| window.press(key, cx));
        }
        wait_for(|| {
            ui.update(|_, cx| {
                paw.read(cx).desktop.as_ref().unwrap().snapshot().mode == Some(target)
            })
        });
        paw.update(ui, |paw, cx| {
            paw.snapshot = paw.desktop.as_ref().unwrap().snapshot();
            cx.notify();
        });
        ui.update(|window, cx| {
            window.render_frame(cx);
            let segment = window.find(("routing-mode", target_tabs - 1));
            assert_eq!(segment.checked(), Some(true));
            assert!(
                segment
                    .label()
                    .unwrap()
                    .contains(locale.message(Message::ActiveTab))
            );
        });
        ui.update(|window, cx| {
            window.press("enter", cx);
            window.press("space", cx);
            window.render_frame(cx);
            window.click(("routing-mode", target_tabs - 1), cx);
        });
        // Wait for the worker to process any accidental duplicate commands.
        thread::sleep(Duration::from_millis(300));
        paw.update(ui, |paw, _| drop(paw.desktop.take()));
        let writes: Vec<_> = requests(dir.path())
            .into_iter()
            .filter(|line| {
                line.starts_with("PATCH ")
                    || line.starts_with("PUT ")
                    || line.starts_with("DELETE ")
            })
            .collect();
        let expected = match target {
            RoutingMode::Rule => "rule",
            RoutingMode::Global => "global",
            RoutingMode::Direct => unreachable!(),
        };
        assert_eq!(writes, [format!("PATCH /configs {expected}")]);
    }
}

#[test]
#[ignore = "test-owned synthetic controller child"]
fn fake_controller_child() {
    let Some(config) = std::env::var_os("PAW_ROUTING_FAKE_CONFIG") else {
        return;
    };
    let dir = PathBuf::from(std::env::var_os("PAW_ROUTING_FAKE_DIR").unwrap());
    let config: serde_json::Value =
        serde_saphyr::from_str(&fs::read_to_string(config).unwrap()).unwrap();
    let address: SocketAddr = config["external-controller"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    assert!(address.ip().is_loopback());
    assert_eq!(config["allow-lan"], false);
    assert_eq!(config["tun"]["enable"], false);
    assert!(
        config
            .get("dns")
            .and_then(|dns| dns.get("listen"))
            .is_none()
    );
    assert_eq!(config["mode"], "direct");
    assert_eq!(config["rules"], serde_json::json!(["MATCH,DIRECT"]));
    let listener = TcpListener::bind(address).unwrap();
    let mixed = TcpListener::bind((
        Ipv4Addr::LOCALHOST,
        config["mixed-port"].as_u64().unwrap() as u16,
    ))
    .unwrap();
    thread::spawn(move || {
        for connection in mixed.incoming() {
            drop(connection.unwrap());
        }
    });
    let secret = config["secret"].as_str().unwrap().to_owned();
    let state = Arc::new(Mutex::new("direct".to_owned()));
    let log = Arc::new(Mutex::new(()));
    for connection in listener.incoming() {
        let (stream, secret, dir, state, log) = (
            connection.unwrap(),
            secret.clone(),
            dir.clone(),
            state.clone(),
            log.clone(),
        );
        thread::spawn(move || serve(stream, &secret, &dir, &state, &log));
    }
}

fn serve(mut stream: TcpStream, secret: &str, dir: &Path, state: &Mutex<String>, log: &Mutex<()>) {
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let mut reader = BufReader::new(&mut stream);
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    let request = line.trim_end().to_owned();
    let mut authorized = false;
    let mut length = 0;
    loop {
        line.clear();
        if reader.read_line(&mut line).unwrap() == 0 || line == "\r\n" {
            break;
        }
        authorized |= line
            .trim()
            .eq_ignore_ascii_case(&format!("authorization: Bearer {secret}"));
        if let Some((name, value)) = line.split_once(':')
            && name.eq_ignore_ascii_case("content-length")
        {
            length = value.trim().parse().unwrap();
        }
    }
    assert!(authorized, "{request}");
    let mut body = vec![0; length];
    reader.read_exact(&mut body).unwrap();
    let method = request.split_whitespace().next().unwrap();
    let path = request.split_whitespace().nth(1).unwrap();
    let mode = if method == "PATCH" && path == "/configs" {
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body.as_object().unwrap().len(), 1);
        let mode = body["mode"].as_str().unwrap().to_owned();
        *state.lock().unwrap() = mode.clone();
        format!(" {mode}")
    } else {
        String::new()
    };
    {
        let _guard = log.lock().unwrap();
        writeln!(
            OpenOptions::new()
                .create(true)
                .append(true)
                .open(dir.join("requests"))
                .unwrap(),
            "{method} {path}{mode}"
        )
        .unwrap();
    }
    let (status, body) = if method == "PUT"
        && path == "/configs"
        && dir.join("connection-fixture").exists()
    {
        wait_for(|| dir.join("release-apply").exists());
        ("204 No Content", String::new())
    } else if method == "PATCH" && path == "/configs" {
        ("204 No Content", String::new())
    } else if path == "/" {
        ("200 OK", r#"{"hello":"meow"}"#.to_owned())
    } else if path == "/configs" {
        (
            "200 OK",
            serde_json::json!({"mode": *state.lock().unwrap()}).to_string(),
        )
    } else if path == "/proxies" {
        ("200 OK", r#"{"proxies":{}}"#.to_owned())
    } else if path == "/providers/proxies" {
        ("200 OK", r#"{"providers":{}}"#.to_owned())
    } else if method == "DELETE" && path == "/connections/row-id" {
        ("204 No Content", String::new())
    } else if path == "/connections" {
        let body = if dir.join("connection-fixture").exists() {
            r#"{"connections":[{"id":"row-id","metadata":{"host":"example.invalid","destinationPort":"443"}}]}"#
        } else {
            r#"{"connections":[]}"#
        };
        ("200 OK", body.to_owned())
    } else if path == "/traffic" {
        ("200 OK", r#"{"up":0,"down":0}"#.to_owned())
    } else {
        ("404 Not Found", "{}".to_owned())
    };
    let _ = write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    );
}

struct SecondaryAction(Rc<RefCell<usize>>);

impl Render for SecondaryAction {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let activations = self.0.clone();
        div().size_full().tab_group().child(
            super::secondary_button("secondary-action", "本地操作")
                .on_click(move |_, _, _| *activations.borrow_mut() += 1),
        )
    }
}

#[gpui_kit::test]
fn secondary_action_is_named_and_activates_with_tab_enter_and_space(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let activations = Rc::new(RefCell::new(0));
    let counter = activations.clone();
    let (_, ui) = cx.add_window_view(move |window, cx| {
        let controls = cx.new(|_| SecondaryAction(counter));
        gpui_kit::base::Root::new(controls, window, cx)
    });
    ui.update(|window, cx| {
        window.activate_window();
        window.render_frame(cx);
        let button = window.find("secondary-action");
        assert_eq!(button.role(), Some(gpui_kit::Role::Button));
        assert_eq!(button.label(), Some("本地操作"));
        window.press("tab", cx);
        assert!(window.focused(cx).is_some());
        window.press("enter", cx);
        window.press("space", cx);
    });
    assert_eq!(*activations.borrow(), 2);
}

#[gpui_kit::test]
fn profile_file_picker_and_pending_rule_read_publish_native_semantics(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let holder = Rc::new(RefCell::new(None));
    let result = holder.clone();
    let (_, ui) = cx.add_window_view(move |window, cx| {
        let paw = open(
            None,
            None,
            (Page::Profiles, Locale::ZhCn),
            false,
            (Preferences::discover(false).0, None),
            window,
            cx,
        );
        *result.borrow_mut() = Some(paw.clone());
        gpui_kit::base::Root::new(paw, window, cx)
    });
    let paw = holder.borrow_mut().take().unwrap();
    ui.update(|window, cx| {
        window.activate_window();
        window.render_frame(cx);
        let browse = window.find("select-profile");
        assert_eq!(browse.role(), Some(gpui_kit::Role::Button));
        assert_eq!(
            browse.label(),
            Some(Locale::ZhCn.message(Message::ImportLocalYaml))
        );

        paw.update(cx, |paw, cx| {
            paw.page = Page::Rules;
            paw.snapshot.engine = EngineStatus::Running { mixed_port: 7898 };
            paw.rule_sets_open = true;
            cx.notify();
        });
        window.render_frame(cx);
        let ready = window.find("read-rule-sets");
        assert_eq!(ready.role(), Some(gpui_kit::Role::Button));
        assert_ne!(ready.disabled(), Some(true));
        let focus = paw.read(cx).rule_sets_read_focus.clone();
        focus.focus(window, cx);
        window.render_frame(cx);
        assert!(focus.is_focused(window));

        paw.update(cx, |paw, cx| {
            paw.snapshot.rule_provider_pending = true;
            cx.notify();
        });
        window.render_frame(cx);
        let pending = window.find("read-rule-sets");
        assert_eq!(pending.disabled(), Some(true));
        assert_eq!(
            pending.label(),
            Some(Locale::ZhCn.message(Message::ReadingRuleSets))
        );
        window.press("enter", cx);
        assert!(!paw.read(cx).rule_sets_read_failed);
        window.press("shift-tab", cx);
        assert!(!focus.is_focused(window));
    });
}
