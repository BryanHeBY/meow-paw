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

struct ProviderControls(Rc<RefCell<usize>>);

impl Render for ProviderControls {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let activations = self.0.clone();
        div()
            .size_full()
            .tab_group()
            .child(
                provider_refresh_button(Locale::En, "ready", false, false)
                    .on_click(move |_, _, _| *activations.borrow_mut() += 1),
            )
            .child(provider_refresh_button(Locale::ZhCn, "current", true, true))
            .child(provider_refresh_button(Locale::ZhCn, "queued", false, true))
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
fn proxy_disclosures_preserve_settings_and_do_not_refresh_or_probe(cx: &mut TestAppContext) {
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
        assert_eq!(control.label().unwrap(), "Show providers (1)");
        assert!(!paw.read(cx).proxy_providers_open);
        window.click("proxy-providers-toggle", cx);
        window.render_frame(cx);
        assert!(paw.read(cx).proxy_providers_open);
        assert_eq!(
            window.find("proxy-providers-toggle").label().unwrap(),
            "Hide providers (1)"
        );
        assert_eq!(
            window
                .find("provider-refresh:Sample Provider")
                .label()
                .unwrap(),
            "Refresh: Sample Provider"
        );
        window.click("proxy-providers-toggle", cx);
        assert!(!paw.read(cx).proxy_providers_open);
        assert_eq!(paw.read(cx).error.as_deref(), Some("unchanged"));
        assert!(paw.read(cx).snapshot.refreshing_provider.is_none());
        assert!(paw.read(cx).snapshot.group_probe.is_none());
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
        let desktop = Desktop::new(binary).unwrap();
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
    let (status, body) = if method == "PATCH" && path == "/configs" {
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
    } else if path == "/connections" {
        ("200 OK", r#"{"connections":[]}"#.to_owned())
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
