use crate::preferences::Preferences;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use std::path::PathBuf;
use std::time::Duration;

gpui_kit::assets::icon_assets!(pub PawAssets, [
    LayoutDashboard, ArrowLeftRight, FolderOpen, Activity, Settings, Clipboard, Cat,
]);

const BG: u32 = 0xeff6ff;
const SURFACE: u32 = 0xfafcff;
const INK: u32 = 0x10243d;
const FAINT: u32 = 0x536783;
const LINE: u32 = 0xdce7f6;
const BLUE: u32 = 0x2d62db;
const SOFT_BLUE: u32 = 0xe7efff;
const YELLOW: u32 = 0xffdd78;
const STREAM_DOWN: u32 = 0x2c77d2;
const STREAM_UP: u32 = 0xb97019;
#[cfg(any(target_os = "linux", windows))]
const GREEN: u32 = 0x168460;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Overview,
    Proxies,
    Profiles,
    Connections,
    Settings,
}

impl Page {
    const ALL: [Self; 5] = [
        Self::Overview,
        Self::Proxies,
        Self::Profiles,
        Self::Connections,
        Self::Settings,
    ];

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|p| p.title().eq_ignore_ascii_case(name))
    }
    fn title(self) -> &'static str {
        match self {
            Self::Overview => "Overview",
            Self::Proxies => "Proxies",
            Self::Profiles => "Profiles",
            Self::Connections => "Connections",
            Self::Settings => "Settings",
        }
    }
    fn icon(self) -> &'static str {
        match self {
            Self::Overview => "icons/layout-dashboard.svg",
            Self::Proxies => "icons/arrow-left-right.svg",
            Self::Profiles => "icons/folder-open.svg",
            Self::Connections => "icons/activity.svg",
            Self::Settings => "icons/settings.svg",
        }
    }
    fn subtitle(self) -> &'static str {
        match self {
            Self::Overview => "Your proxy session at a glance.",
            Self::Proxies => "Inspect groups and pick the route to use.",
            Self::Profiles => "A local YAML file powers each isolated session.",
            Self::Connections => "See the requests currently handled by Meow.",
            Self::Settings => "Review the boundaries of this desktop build.",
        }
    }
}

pub struct Paw {
    page: Page,
    profile: Option<PathBuf>,
    binary: Option<PathBuf>,
    error: Option<String>,
    copied_listener: Option<u16>,
    preferences: Preferences,
    preference_warning: Option<String>,
    search: Entity<InputState>,
    _search_changes: Subscription,
    #[cfg(any(target_os = "linux", windows))]
    desktop: Option<meow_paw::app::Desktop>,
    #[cfg(any(target_os = "linux", windows))]
    proxy_group: Option<String>,
    #[cfg(any(target_os = "linux", windows))]
    snapshot: meow_paw::app::Snapshot,
}

pub fn open(
    binary: Option<PathBuf>,
    profile: Option<PathBuf>,
    page: Page,
    start: bool,
    (preferences, warning): (Preferences, Option<&str>),
    window: &mut Window,
    cx: &mut App,
) -> Entity<Paw> {
    let search = cx.new(|cx| InputState::new(window, cx));
    cx.new(|cx| {
        let search_changes = cx.subscribe(&search, |_: &mut Paw, _, event, cx| {
            if matches!(event, InputEvent::Change) {
                cx.notify();
            }
        });
        let mut paw = Paw {
            page,
            profile,
            #[cfg(any(target_os = "linux", windows))]
            desktop: binary
                .as_ref()
                .and_then(|path| meow_paw::app::Desktop::new(path.clone()).ok()),
            binary,
            #[cfg(any(target_os = "linux", windows))]
            proxy_group: None,
            error: None,
            copied_listener: None,
            preferences,
            preference_warning: warning.map(str::to_owned),
            search,
            _search_changes: search_changes,
            #[cfg(any(target_os = "linux", windows))]
            snapshot: Default::default(),
        };
        if start {
            paw.start();
        }
        cx.spawn(async move |weak, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(300))
                    .await;
                if weak
                    .update(cx, |paw: &mut Paw, cx| {
                        if paw.refresh() {
                            cx.notify();
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        paw
    })
}

impl Paw {
    fn start(&mut self) {
        self.error = None;
        self.copied_listener = None;
        #[cfg(any(target_os = "linux", windows))]
        {
            let Some(desktop) = self.desktop.as_ref() else {
                self.error = Some("Choose a Meow executable in Settings before starting.".into());
                return;
            };
            let Some(profile) = self.profile.clone() else {
                self.error = Some("Choose a local YAML file in Profiles before starting.".into());
                return;
            };
            self.error = desktop.start(profile).err().map(|e| e.to_string());
        }
        #[cfg(not(any(target_os = "linux", windows)))]
        {
            self.error =
                Some("Managed engine sessions are not enabled on this platform yet.".into());
        }
    }
    fn stop(&mut self) {
        #[cfg(any(target_os = "linux", windows))]
        if let Some(desktop) = &self.desktop {
            desktop.stop();
        }
    }

    fn refresh(&mut self) -> bool {
        #[cfg(any(target_os = "linux", windows))]
        if let Some(desktop) = &self.desktop {
            let snap = desktop.snapshot();
            if self.snapshot.revision != snap.revision {
                self.snapshot = snap;
                return true;
            }
        }
        false
    }

    fn state(&self) -> (&'static str, u32, Option<u16>) {
        #[cfg(any(target_os = "linux", windows))]
        {
            use meow_paw::app::EngineStatus;
            match &self.snapshot.engine {
                EngineStatus::Running { mixed_port } => ("Running", GREEN, Some(*mixed_port)),
                EngineStatus::Starting => ("Starting", BLUE, None),
                EngineStatus::Stopping => ("Stopping", BLUE, None),
                EngineStatus::Error(_) => ("Needs attention", 0xa83a33, None),
                EngineStatus::Stopped => ("Stopped", FAINT, None),
            }
        }
        #[cfg(not(any(target_os = "linux", windows)))]
        {
            ("Not enabled", FAINT, None)
        }
    }

    fn issue(&self) -> Option<String> {
        #[cfg(any(target_os = "linux", windows))]
        {
            if let meow_paw::app::EngineStatus::Error(error) = &self.snapshot.engine {
                return Some(error.clone());
            }
            if self.snapshot.action_error.is_some() {
                return self.snapshot.action_error.clone();
            }
        }
        self.error.clone()
    }

    fn sidebar(&self, compact: bool, cx: &mut Context<Self>) -> Div {
        let nav = Self::pages(self.page, compact, cx);
        div()
            .w(px(if compact { 58. } else { 216. }))
            .h_full()
            .flex_shrink_0()
            .bg(rgb(SURFACE))
            .border_r_1()
            .border_color(rgb(LINE))
            .px(if compact { px(8.) } else { px(20.) })
            .py_6()
            .flex()
            .flex_col()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(paw_badge(if compact { 40. } else { 42. }, YELLOW, INK))
                    .when(!compact, |brand| {
                        brand.child(
                            div()
                                .flex()
                                .flex_col()
                                .child(
                                    div()
                                        .text_lg()
                                        .font_weight(FontWeight::BOLD)
                                        .child("Meow Paw"),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(rgb(FAINT))
                                        .child("DESKTOP CLIENT"),
                                ),
                        )
                    }),
            )
            .when(!compact, |bar| {
                bar.child(
                    div()
                        .mt_8()
                        .mb_2()
                        .text_xs()
                        .text_color(rgb(FAINT))
                        .child("WORKSPACE"),
                )
            })
            .when(compact, |bar| bar.child(div().h_8()))
            .children(nav)
            .child(div().flex_grow(1.0))
            .when(!compact, |bar| {
                bar.child(
                    div()
                        .border_t_1()
                        .border_color(rgb(LINE))
                        .pt_4()
                        .text_xs()
                        .text_color(rgb(FAINT))
                        .child(format!("Paw 0.1  ·  {}", std::env::consts::OS)),
                )
            })
    }

    fn pages(active: Page, compact: bool, cx: &mut Context<Self>) -> Vec<Stateful<Div>> {
        Page::ALL
            .into_iter()
            .enumerate()
            .map(|(index, page)| {
                let on = active == page;
                div()
                    .id(("paw-nav", index))
                    .flex()
                    .items_center()
                    .gap_3()
                    .px_3()
                    .py_2()
                    .my_1()
                    .rounded_md()
                    .cursor_pointer()
                    .bg(rgb(if on { SOFT_BLUE } else { SURFACE }))
                    .text_color(rgb(if on { BLUE } else { INK }))
                    .hover(|s| s.bg(rgb(SOFT_BLUE)))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.page = page;
                        cx.notify();
                    }))
                    .child(line_icon(page.icon(), if on { BLUE } else { FAINT }, 18.))
                    .when(!compact, |row| {
                        row.child(div().font_weight(FontWeight::SEMIBOLD).child(page.title()))
                    })
                    .tooltip(move |window, cx| {
                        gpui_kit::component::tooltip::Tooltip::new(page.title()).build(window, cx)
                    })
            })
            .collect()
    }

    fn actions(&self, cx: &mut Context<Self>) -> Div {
        let (phase, _, _) = self.state();
        let ready = self.desktop_available();
        let supported = cfg!(any(target_os = "linux", windows));
        let enabled = supported && phase != "Stopping";
        let running = phase == "Running" || phase == "Starting";
        let action = div()
            .id("session-control")
            .rounded_md()
            .px_4()
            .py_2()
            .bg(rgb(if enabled { BLUE } else { LINE }))
            .text_color(rgb(if enabled { 0xffffff } else { FAINT }))
            .font_weight(FontWeight::SEMIBOLD)
            .child(if running {
                "Stop session"
            } else if phase == "Stopping" {
                "Stopping…"
            } else if supported && !ready {
                "Choose engine"
            } else if supported && self.profile.is_none() {
                "Choose profile"
            } else {
                "Start session"
            })
            .when(enabled, |e| {
                e.cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if running {
                            this.stop();
                        } else if !ready {
                            this.page = Page::Settings;
                        } else if this.profile.is_none() {
                            this.page = Page::Profiles;
                        } else {
                            this.start();
                        }
                        cx.notify();
                    }))
            });
        div().child(action)
    }

    fn desktop_available(&self) -> bool {
        #[cfg(any(target_os = "linux", windows))]
        {
            self.desktop.is_some()
        }
        #[cfg(not(any(target_os = "linux", windows)))]
        {
            false
        }
    }

    fn overview(&self, cx: &mut Context<Self>) -> Div {
        let (phase, color, port) = self.state();
        let (up, down, total) = self.traffic();
        div()
            .flex()
            .flex_col()
            .gap_4()
            .child(
                div()
                    .rounded_xl()
                    .border_1()
                    .border_color(rgb(0xf6e5b3))
                    .bg(rgb(0xfff8e2))
                    .px_6()
                    .py_5()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_5()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .min_w_0()
                            .child(
                                div()
                                    .text_xs()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(rgb(0x76571b))
                                    .child("LOCAL SESSION"),
                            )
                            .child(div().text_2xl().font_weight(FontWeight::BOLD).child(phase))
                            .child(
                                div()
                                    .flex()
                                    .flex_wrap()
                                    .items_center()
                                    .gap_2()
                                    .child(div().text_sm().text_color(rgb(FAINT)).child(
                                        port.map_or(
                                            "No local listener is active.".into(),
                                            |port| format!("127.0.0.1:{port}"),
                                        ),
                                    ))
                                    .when_some(port, |row, port| {
                                        row.child(
                                            div()
                                                .id("copy-listener")
                                                .cursor_pointer()
                                                .rounded_md()
                                                .flex()
                                                .items_center()
                                                .gap_1()
                                                .text_xs()
                                                .text_color(rgb(BLUE))
                                                .px_2()
                                                .py_1()
                                                .hover(|s| s.bg(rgb(SOFT_BLUE)))
                                                .child(line_icon("icons/clipboard.svg", BLUE, 13.))
                                                .child(if self.copied_listener == Some(port) {
                                                    "Copied"
                                                } else {
                                                    "Copy address"
                                                })
                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                    cx.write_to_clipboard(
                                                        ClipboardItem::new_string(format!(
                                                            "127.0.0.1:{port}"
                                                        )),
                                                    );
                                                    this.copied_listener = Some(port);
                                                    cx.notify();
                                                })),
                                        )
                                    }),
                            ),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .rounded_xl()
                            .bg(rgb(0xffefbd))
                            .p_3()
                            .child(if port.is_some() {
                                paw_badge(52., 0xffefbd, color)
                            } else {
                                div()
                                    .size(px(52.))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child(line_icon("icons/cat.svg", 0x76571b, 38.))
                            }),
                    ),
            )
            .child(
                div()
                    .flex()
                    .gap_4()
                    .child(metric("UPLOAD", up))
                    .child(metric("DOWNLOAD", down))
                    .child(metric("SESSION TOTAL", total)),
            )
            .child(self.traffic_timeline())
            .child(
                card()
                    .flex()
                    .gap_5()
                    .flex_wrap()
                    .items_center()
                    .justify_between()
                    .child(
                        div().min_w_0().child(section_label("PROFILE")).child(
                            div()
                                .mt_1()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(self.overview_profile_label()),
                        ),
                    )
                    .child(
                        div().min_w_0().child(section_label("SYSTEM PROXY")).child(
                            div()
                                .mt_1()
                                .text_sm()
                                .text_color(rgb(FAINT))
                                .child("Unchanged · local apps opt in"),
                        ),
                    ),
            )
    }

    fn traffic_timeline(&self) -> Div {
        let label = div()
            .flex()
            .justify_between()
            .items_center()
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Traffic activity"),
            )
            .child(
                div()
                    .flex()
                    .gap_4()
                    .child(traffic_legend("Download", STREAM_DOWN))
                    .child(traffic_legend("Upload", STREAM_UP)),
            );
        let frame = card().child(label);
        #[cfg(any(target_os = "linux", windows))]
        {
            use meow_paw::app::TRAFFIC_WINDOW_SECS;
            let samples: Vec<_> = self.snapshot.traffic_history.recent().collect();
            if samples.iter().any(Option::is_some) {
                let peak = samples
                    .iter()
                    .flatten()
                    .map(|s| s.up.max(s.down))
                    .max()
                    .unwrap_or(0);
                let pad = TRAFFIC_WINDOW_SECS.saturating_sub(samples.len());
                let columns = std::iter::repeat_n(None, pad).chain(samples).map(|sample| {
                    let (up, down) = sample.map_or((0., 0.), |s| {
                        (traffic_bar(s.up, peak), traffic_bar(s.down, peak))
                    });
                    div()
                        .flex_1()
                        .flex()
                        .items_end()
                        .gap(px(1.))
                        .h_full()
                        .child(div().flex_1().rounded_sm().h(px(down)).bg(rgb(STREAM_DOWN)))
                        .child(div().flex_1().rounded_sm().h(px(up)).bg(rgb(STREAM_UP)))
                });
                return frame
                    .child(
                        div()
                            .flex()
                            .justify_between()
                            .mt_3()
                            .text_xs()
                            .text_color(rgb(FAINT))
                            .child("Shared scale")
                            .child(format!("{}/s peak", bytes(peak))),
                    )
                    .child(
                        div()
                            .mt_1()
                            .h(px(61.))
                            .flex()
                            .items_end()
                            .gap(px(2.))
                            .border_b_1()
                            .border_color(rgb(LINE))
                            .children(columns),
                    )
                    .child(
                        div()
                            .flex()
                            .justify_between()
                            .mt_2()
                            .text_xs()
                            .text_color(rgb(FAINT))
                            .child("39 seconds ago")
                            .child("Now"),
                    );
            }
        }
        frame.child(
            div()
                .mt_4()
                .h(px(70.))
                .rounded_lg()
                .bg(rgb(SOFT_BLUE))
                .flex()
                .items_center()
                .justify_center()
                .text_sm()
                .text_color(rgb(FAINT))
                .child("Live activity appears when a session sends its first sample."),
        )
    }

    fn traffic(&self) -> (String, String, String) {
        #[cfg(any(target_os = "linux", windows))]
        if let Some(t) = self.snapshot.traffic.as_ref() {
            return (
                format!("{}/s", bytes(t.up)),
                format!("{}/s", bytes(t.down)),
                bytes(t.up_total.saturating_add(t.down_total)),
            );
        }
        ("—".into(), "—".into(), "—".into())
    }

    fn profile_label(&self) -> String {
        self.profile
            .as_ref()
            .map(|p| {
                p.file_name()
                    .unwrap_or(p.as_os_str())
                    .to_string_lossy()
                    .into_owned()
            })
            .unwrap_or_else(|| "No profile selected".into())
    }

    fn overview_profile_label(&self) -> String {
        #[cfg(any(target_os = "linux", windows))]
        if self.state().2.is_some()
            && let Some(path) = &self.snapshot.profile
        {
            return path
                .file_name()
                .unwrap_or(path.as_os_str())
                .to_string_lossy()
                .into_owned();
        }
        self.profile_label()
    }

    fn profiles(&self, cx: &mut Context<Self>) -> Div {
        let box_ = card()
            .child(section_label("NEXT SESSION"))
            .child(
                div()
                    .mt_2()
                    .text_xl()
                    .font_weight(FontWeight::BOLD)
                    .child(self.profile_label()),
            )
            .child(div().mt_1().text_sm().text_color(rgb(FAINT)).child(
                "Paw reads a local YAML file and prepares a separate configuration. Your source file is not edited.",
            ))
            .child(div().mt_4().flex().child(
                secondary_button("select-profile", if self.profile.is_some() { "Choose another YAML" } else { "Choose YAML file" })
                    .on_click(cx.listener(|this, _, _, cx| this.choose_file(false, cx))),
            ));
        let page = div().flex().flex_col().gap_4().child(box_);
        #[cfg(any(target_os = "linux", windows))]
        if !self.snapshot.diagnostics.is_empty() {
            return page.child(
                card()
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Diagnostics from the last start attempt"),
                    )
                    .children(self.snapshot.diagnostics.iter().take(40).map(|d| {
                        div()
                            .mt_2()
                            .text_sm()
                            .child(
                                div()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(d.path.clone()),
                            )
                            .child(div().text_color(rgb(FAINT)).child(d.message))
                    })),
            );
        }
        page
    }

    fn choose_file(&mut self, binary: bool, cx: &mut Context<Self>) {
        let selection = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(
                if binary {
                    "Choose Meow executable"
                } else {
                    "Choose Meow YAML"
                }
                .into(),
            ),
        });
        cx.spawn(async move |weak, cx| {
            let path = selection
                .await
                .ok()
                .and_then(Result::ok)
                .flatten()
                .and_then(|paths| paths.into_iter().next());
            if let Some(path) = path {
                let _ = weak.update(cx, |paw: &mut Paw, cx| {
                    if binary {
                        paw.choose_binary(path);
                    } else if path.is_file() {
                        paw.remember_selection(false, &path);
                        paw.profile = Some(path);
                        paw.error = None;
                    } else {
                        paw.error = Some("Choose an existing local YAML file.".into());
                    }
                    cx.notify();
                });
            }
        })
        .detach();
    }

    fn proxies(&self, cx: &mut Context<Self>) -> Div {
        #[cfg(any(target_os = "linux", windows))]
        if let Some(inventory) = self.snapshot.inventory.as_ref() {
            let value = self.search.read(cx).value();
            let query = value.trim().to_lowercase();
            let total = inventory
                .groups
                .values()
                .filter(|group| !group.hidden)
                .count();
            let visible = inventory.groups.iter().filter(|(name, group)| {
                !group.hidden
                    && (route_matches(name, &query)
                        || group.all.as_ref().is_some_and(|members| {
                            members.iter().any(|member| route_matches(member, &query))
                        }))
            });
            let count = visible.clone().count();
            let search = card()
                .child(section_label(if query.is_empty() {
                    format!("{total} ROUTE GROUPS")
                } else {
                    format!("{count} OF {total} ROUTE GROUPS")
                }))
                .child(
                    div()
                        .mt_2()
                        .text_sm()
                        .text_color(rgb(FAINT))
                        .child("Filter by group or route"),
                )
                .child(div().mt_2().child(Input::new(&self.search).cleanable(true)));

            let current = visible
                .clone()
                .find(|(name, _)| self.proxy_group.as_ref() == Some(name))
                .or_else(|| visible.clone().next());
            if let Some((name, group)) = current {
                let groups = search.child(div().mt_3().flex().flex_wrap().gap_2().children(
                    visible.map(|(key, _)| {
                        let key = key.clone();
                        let chip = div()
                            .id(SharedString::from(format!("group:{key}")))
                            .border_1()
                            .border_color(rgb(if key == *name { BLUE } else { LINE }))
                            .rounded_md()
                            .px_3()
                            .py_1()
                            .bg(rgb(if key == *name { SOFT_BLUE } else { SURFACE }))
                            .text_color(rgb(if key == *name { BLUE } else { INK }))
                            .cursor_pointer()
                            .child(key.clone());
                        chip.on_click(cx.listener(move |paw, _, _, cx| {
                            paw.proxy_group = Some(key.clone());
                            cx.notify();
                        }))
                    }),
                ));
                let mutable = group.kind.eq_ignore_ascii_case("selector");
                let all_members = group.all.as_deref().unwrap_or_default();
                let show_all = route_matches(name, &query);
                let members: Vec<_> = all_members
                    .iter()
                    .enumerate()
                    .filter(|(_, member)| show_all || route_matches(member, &query))
                    .collect();
                let routes = if members.len() == all_members.len() {
                    format!("{} ROUTES", all_members.len())
                } else {
                    format!("{} OF {} ROUTES", members.len(), all_members.len())
                };
                let mut detail = card()
                    .child(section_label(format!(
                        "{} · {routes}",
                        group.kind.to_uppercase(),
                    )))
                    .child(
                        div()
                            .mt_1()
                            .text_xl()
                            .font_weight(FontWeight::BOLD)
                            .child(name.clone()),
                    )
                    .child(
                        div()
                            .mt_1()
                            .text_sm()
                            .text_color(rgb(FAINT))
                            .child(if mutable {
                                "Choose a route for new connections."
                            } else {
                                "Meow automatically chooses the active route for this group."
                            }),
                    );
                if members.is_empty() {
                    detail = detail.child(
                        div()
                            .mt_3()
                            .text_sm()
                            .text_color(rgb(FAINT))
                            .child("This group has no candidate routes."),
                    );
                } else {
                    detail = detail.child(div().mt_3().flex().flex_wrap().gap_2().children(
                        members.into_iter().map(|(index, member)| {
                            let (g, n) = (name.clone(), member.clone());
                            let selected = group.now.as_deref() == Some(member.as_str());
                            div()
                                .id(SharedString::from(format!("proxy:{name}:{index}")))
                                .min_w(px(142.))
                                .flex_1()
                                .px_3()
                                .py_2()
                                .gap_2()
                                .flex()
                                .items_center()
                                .rounded_md()
                                .border_1()
                                .border_color(rgb(if selected { 0x9cbafb } else { LINE }))
                                .bg(rgb(if selected { SOFT_BLUE } else { SURFACE }))
                                .child(
                                    div()
                                        .size(px(7.))
                                        .flex_shrink_0()
                                        .rounded_full()
                                        .bg(rgb(if selected { BLUE } else { LINE })),
                                )
                                .child(div().min_w_0().flex_1().child(member.clone()))
                                .when(selected, |el| el.child(section_label("ACTIVE")))
                                .when(mutable && !selected, |el| {
                                    el.cursor_pointer()
                                        .hover(|style| style.bg(rgb(SOFT_BLUE)))
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.error = this.desktop.as_ref().and_then(|d| {
                                                d.select(g.clone(), n.clone())
                                                    .err()
                                                    .map(|e| e.to_string())
                                            });
                                            cx.notify();
                                        }))
                                })
                        }),
                    ));
                }
                return div().flex().flex_col().gap_4().child(groups).child(detail);
            }
            let empty = if total == 0 {
                empty_state(
                    "No visible groups",
                    "The active profile has no visible route groups.",
                )
            } else {
                empty_state(
                    "No matching routes",
                    "Clear the filter or try part of a group or route name.",
                )
            };
            return div().flex().flex_col().gap_4().child(search).child(empty);
        }
        let _ = cx;
        empty_state(
            "No proxy groups yet",
            "Start a local profile to see its route groups and active choices.",
        )
    }

    fn connections(&self) -> Div {
        #[cfg(any(target_os = "linux", windows))]
        if let Some(list) = self.snapshot.connections.as_ref().filter(|c| !c.is_empty()) {
            return card()
                .child(section_label(format!("{} ACTIVE CONNECTIONS", list.len())))
                .child(
                    connection_row(
                        section_label("DESTINATION"),
                        section_label("ROUTE"),
                        section_label("MATCHED RULE"),
                    )
                    .mt_3()
                    .bg(rgb(SOFT_BLUE))
                    .rounded_md(),
                )
                .children(list.iter().map(|c| {
                    let target = if c.metadata.host.is_empty() {
                        &c.metadata.destination_ip
                    } else {
                        &c.metadata.host
                    };
                    let dest = destination_label(target, &c.metadata.destination_port);
                    let route = if c.chains.is_empty() {
                        "—".into()
                    } else {
                        c.chains.join(" / ")
                    };
                    connection_row(
                        div().font_weight(FontWeight::MEDIUM).child(dest),
                        div().text_color(rgb(FAINT)).child(route),
                        div().text_color(rgb(FAINT)).child(if c.rule.is_empty() {
                            "—".into()
                        } else {
                            c.rule.clone()
                        }),
                    )
                    .border_b_1()
                    .border_color(rgb(LINE))
                }));
        }
        let running = self.state().2.is_some();
        empty_state(
            "No active connections",
            if running {
                "Once an app sends a request through Paw’s local listener, its destination and matched rule appear here."
            } else {
                "Start a local session, then send an app’s traffic through its listener to see connections here."
            },
        )
    }

    fn choose_binary(&mut self, path: PathBuf) {
        #[cfg(any(target_os = "linux", windows))]
        {
            if !self.can_change_binary() {
                self.error = Some("Stop the current session before changing the engine.".into());
                return;
            }
            if !path.is_file() {
                self.error = Some("Select an existing local Meow executable.".into());
                return;
            }
            match meow_paw::app::Desktop::new(path.clone()) {
                Ok(desktop) => {
                    self.desktop = Some(desktop);
                    self.remember_selection(true, &path);
                    self.binary = Some(path);
                    self.snapshot = Default::default();
                    self.error = None;
                }
                Err(error) => {
                    self.error = Some(format!("Could not prepare the Meow worker: {error}"))
                }
            }
        }
        #[cfg(not(any(target_os = "linux", windows)))]
        let _ = path;
    }

    fn remember_selection(&mut self, engine: bool, path: &std::path::Path) {
        if let Err(err) = self.preferences.remember(engine, path) {
            self.preference_warning = Some(format!(
                "Could not save this choice. It will be used for this run only: {err}"
            ));
        }
    }

    fn can_change_binary(&self) -> bool {
        let phase = self.state().0;
        cfg!(any(target_os = "linux", windows))
            && !matches!(phase, "Starting" | "Running" | "Stopping")
    }

    fn settings(&self, cx: &mut Context<Self>) -> Div {
        let binary = self
            .binary
            .as_ref()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|| "No executable selected.".into());
        div()
            .flex()
            .flex_col()
            .gap_4()
            .child(
                card()
                    .child(section_label("ENGINE"))
                    .child(div().mt_2().text_lg().font_weight(FontWeight::SEMIBOLD).child("Meow executable"))
                    .child(div().mt_1().text_sm().text_color(rgb(FAINT)).child(binary))
                    .when(self.can_change_binary(), |el| {
                        el.child(div().mt_4().flex().child(
                            secondary_button("select-binary", "Choose executable")
                                .on_click(cx.listener(|this, _, _, cx| this.choose_file(true, cx))),
                        ))
                    }),
            )
            .child(
                card()
                    .child(section_label("NETWORK BOUNDARIES"))
                    .child(div().mt_2().text_lg().font_weight(FontWeight::SEMIBOLD).child("Local listener only"))
                    .child(div().mt_1().text_sm().text_color(rgb(FAINT)).child(
                        "Paw leaves system proxy and routes untouched. Point individual apps at the local listener shown on Overview.",
                    ))
                    .child(div().mt_3().flex().gap_3().flex_wrap()
                        .child(section_label("NO SYSTEM PROXY"))
                        .child(section_label("NO TUN"))
                        .child(section_label("NO ADMINISTRATOR ACCESS"))),
            )
    }
}

impl Render for Paw {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let compact = window.viewport_size().width < px(820.);
        let body = match self.page {
            Page::Overview => self.overview(cx),
            Page::Proxies => self.proxies(cx),
            Page::Profiles => self.profiles(cx),
            Page::Connections => self.connections(),
            Page::Settings => self.settings(cx),
        };
        div()
            .size_full()
            .flex()
            .bg(rgb(BG))
            .text_color(rgb(INK))
            .child(self.sidebar(compact, cx))
            .child(
                div()
                    .id("paw-page")
                    .size_full()
                    .flex_1()
                    .overflow_y_scroll()
                    .px(if compact { px(20.) } else { px(32.) })
                    .py_6()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap_3()
                            .mb_5()
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .child(
                                        div()
                                            .text_2xl()
                                            .font_weight(FontWeight::BOLD)
                                            .child(self.page.title()),
                                    )
                                    .child(
                                        div()
                                            .text_sm()
                                            .text_color(rgb(FAINT))
                                            .child(self.page.subtitle()),
                                    ),
                            )
                            .child(self.actions(cx)),
                    )
                    .when_some(self.issue(), |e, issue| {
                        e.child(
                            div()
                                .p_3()
                                .mb_4()
                                .rounded_md()
                                .bg(rgb(0xffece9))
                                .text_color(rgb(0xa83a33))
                                .child(issue),
                        )
                    })
                    .when_some(self.preference_warning.as_ref(), |pane, warning| {
                        pane.child(
                            div()
                                .p_3()
                                .mb_4()
                                .rounded_md()
                                .bg(rgb(0xfff4d5))
                                .text_color(rgb(0x76571b))
                                .child(warning.clone()),
                        )
                    })
                    .child(body),
            )
    }
}

fn card() -> Div {
    div()
        .rounded_xl()
        .border_1()
        .border_color(rgb(LINE))
        .bg(rgb(SURFACE))
        .p_5()
}

fn line_icon(path: &'static str, color: u32, side: f32) -> impl IntoElement {
    svg()
        .path(path)
        .size(px(side))
        .text_color(rgb(color))
        .flex_shrink_0()
}

fn secondary_button(id: &'static str, label: &'static str) -> Stateful<Div> {
    div()
        .id(id)
        .rounded_md()
        .border_1()
        .border_color(rgb(0xb5cafa))
        .bg(rgb(SOFT_BLUE))
        .text_color(rgb(BLUE))
        .font_weight(FontWeight::SEMIBOLD)
        .cursor_pointer()
        .hover(|style| style.bg(rgb(0xd8e7ff)))
        .px_4()
        .py_2()
        .child(label)
}

fn section_label(label: impl Into<SharedString>) -> Div {
    div()
        .text_xs()
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(rgb(FAINT))
        .child(label.into())
}

fn empty_state(title: &'static str, detail: &'static str) -> Div {
    card()
        .min_h(px(174.))
        .flex()
        .flex_col()
        .justify_center()
        .items_start()
        .gap_2()
        .child(paw_badge(42., SOFT_BLUE, BLUE))
        .child(
            div()
                .text_lg()
                .font_weight(FontWeight::SEMIBOLD)
                .child(title),
        )
        .child(div().text_sm().text_color(rgb(FAINT)).child(detail))
}

#[cfg(any(target_os = "linux", windows))]
fn route_matches(name: &str, normalized_query: &str) -> bool {
    normalized_query.is_empty() || name.to_lowercase().contains(normalized_query)
}

#[cfg(any(target_os = "linux", windows))]
fn connection_row(destination: Div, route: Div, rule: Div) -> Div {
    div()
        .flex()
        .gap_3()
        .px_3()
        .py_2()
        .items_center()
        .text_sm()
        .child(destination.flex_1().min_w_0())
        .child(route.w(px(140.)).min_w_0().flex_shrink(1.))
        .child(rule.w(px(112.)).min_w_0().flex_shrink(1.))
}

#[cfg(any(target_os = "linux", windows))]
fn destination_label(host: &str, port: &serde_json::Value) -> String {
    if host.is_empty() {
        return "—".into();
    }
    let port = port
        .as_u64()
        .and_then(|p| u16::try_from(p).ok())
        .or_else(|| port.as_str()?.parse::<u16>().ok())
        .filter(|&p| p != 0);
    match port {
        Some(p) if host.contains(':') => format!("[{host}]:{p}"),
        Some(p) => format!("{host}:{p}"),
        None => host.to_owned(),
    }
}

fn metric(label: &'static str, value: String) -> Div {
    card()
        .flex_1()
        .child(
            div()
                .text_xs()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(FAINT))
                .child(label),
        )
        .child(
            div()
                .mt_2()
                .text_xl()
                .font_weight(FontWeight::BOLD)
                .child(value),
        )
}

fn traffic_legend(label: &'static str, color: u32) -> Div {
    div()
        .flex()
        .items_center()
        .gap_1()
        .text_xs()
        .text_color(rgb(FAINT))
        .child(div().size(px(7.)).rounded_full().bg(rgb(color)))
        .child(label)
}

#[cfg(any(target_os = "linux", windows))]
fn traffic_bar(value: u64, peak: u64) -> f32 {
    if peak == 0 || value == 0 {
        return 0.;
    }
    (60. * (value.min(peak) as f64 / peak as f64) as f32).max(1.)
}

#[cfg(any(target_os = "linux", windows))]
fn bytes(value: u64) -> String {
    if value < 1024 {
        return format!("{value} B");
    }
    let mut amount = value as f64 / 1024.;
    for unit in ["KiB", "MiB", "GiB", "TiB"] {
        if amount < 1024. || unit == "TiB" {
            return format!("{amount:.1} {unit}");
        }
        amount /= 1024.;
    }
    unreachable!()
}

// A native geometric mark, independent of emoji fonts and platform glyph widths.
fn paw_badge(size: f32, background: u32, foreground: u32) -> Div {
    let dot = |x: f32, y: f32, w: f32, h: f32| {
        div()
            .absolute()
            .left(px(x * size))
            .top(px(y * size))
            .w(px(w * size))
            .h(px(h * size))
            .rounded_full()
            .bg(rgb(foreground))
    };
    div()
        .relative()
        .flex_shrink_0()
        .size(px(size))
        .rounded_xl()
        .bg(rgb(background))
        .child(dot(0.220, 0.315, 0.120, 0.145))
        .child(dot(0.360, 0.210, 0.125, 0.170))
        .child(dot(0.515, 0.210, 0.125, 0.170))
        .child(dot(0.665, 0.315, 0.120, 0.145))
        .child(dot(0.312, 0.446, 0.376, 0.295))
}

#[cfg(test)]
mod tests {
    use super::Page;

    #[test]
    #[cfg(any(target_os = "linux", windows))]
    fn connection_destinations_keep_ports_and_disambiguate_ipv6() {
        use super::destination_label;
        use serde_json::json;
        assert_eq!(
            destination_label("test.local", &json!("443")),
            "test.local:443"
        );
        assert_eq!(destination_label("::1", &json!(80)), "[::1]:80");
        assert_eq!(destination_label("127.0.0.1", &json!(null)), "127.0.0.1");
        assert_eq!(destination_label("", &json!(123)), "—");
    }
    #[test]
    #[cfg(any(target_os = "linux", windows))]
    fn route_names_support_non_ascii_and_case_insensitive_filters() {
        use super::route_matches;
        assert!(route_matches("南京 A-01", "南京"));
        assert!(route_matches("Oberland DIRECT", "direct"));
        assert!(route_matches("ÉTÉ", "été"));
        assert!(route_matches("名称", ""));
        assert!(!route_matches("上海", "南京"));
    }

    #[test]
    fn every_nav_and_workspace_icon_is_embedded() {
        use gpui_kit::AssetSource;

        let assets = super::PawAssets;
        let paths = Page::ALL
            .into_iter()
            .map(Page::icon)
            .chain(["icons/clipboard.svg", "icons/cat.svg"]);
        for path in paths {
            assert!(assets.load(path).unwrap().is_some(), "missing SVG: {path}");
        }
    }

    #[test]
    fn startup_pages_match_the_nav() {
        assert_eq!(Page::ALL.len(), 5);
        for expected in Page::ALL {
            assert!(Page::from_name(expected.title()) == Some(expected));
        }
        assert!(Page::from_name("not-a-page").is_none());
    }
}
