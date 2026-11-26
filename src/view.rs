use crate::preferences::{LanguagePreference, Preferences};
use gpui_kit::base::{Disableable, Selectable};
use gpui_kit::component::button::Button;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use meow_paw::i18n::{Locale, Message};
use meow_paw::subscription;
use meow_paw::subscription_store::{RefreshPolicy, SubscriptionId, SubscriptionStore};
use std::path::PathBuf;
use std::time::Duration;

const BG: u32 = 0xeff2f6;
const CHROME: u32 = 0xf8faff;
const SURFACE: u32 = 0xffffff;
const INK: u32 = 0x14213a;
const FAINT: u32 = 0x535f73;
const LINE: u32 = 0xdbe2ec;
const BLUE: u32 = 0x155cd7;
const SOFT_BLUE: u32 = 0xe8f1ff;
const STREAM_DOWN: u32 = 0x146cce;
const STREAM_UP: u32 = 0xa06100;
#[cfg(any(target_os = "linux", windows))]
const GREEN: u32 = 0x168460;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Overview,
    Proxies,
    Profiles,
    Connections,
    Rules,
    Logs,
    Unlock,
    Settings,
}

impl Page {
    const ALL: [Self; 8] = [
        Self::Overview,
        Self::Proxies,
        Self::Profiles,
        Self::Connections,
        Self::Rules,
        Self::Logs,
        Self::Unlock,
        Self::Settings,
    ];

    pub fn from_name(name: &str) -> Option<Self> {
        if name.eq_ignore_ascii_case("overview") {
            return Some(Self::Overview);
        }
        Self::ALL
            .into_iter()
            .find(|p| p.cli_name().eq_ignore_ascii_case(name))
    }
    fn cli_name(self) -> &'static str {
        match self {
            Self::Overview => "Home",
            Self::Proxies => "Proxies",
            Self::Profiles => "Profiles",
            Self::Connections => "Connections",
            Self::Rules => "Rules",
            Self::Logs => "Logs",
            Self::Unlock => "Unlock",
            Self::Settings => "Settings",
        }
    }
    fn message(self) -> Message {
        match self {
            Self::Overview => Message::Home,
            Self::Proxies => Message::Proxies,
            Self::Profiles => Message::Profiles,
            Self::Connections => Message::Connections,
            Self::Rules => Message::Rules,
            Self::Logs => Message::Logs,
            Self::Unlock => Message::Unlock,
            Self::Settings => Message::Settings,
        }
    }
    fn icon(self) -> &'static str {
        match self {
            Self::Overview => "icons/house.svg",
            Self::Proxies => "icons/wifi.svg",
            Self::Profiles => "icons/server.svg",
            Self::Connections => "icons/globe.svg",
            Self::Rules => "icons/git-branch.svg",
            Self::Logs => "icons/list.svg",
            Self::Unlock => "icons/lock-open.svg",
            Self::Settings => "icons/settings.svg",
        }
    }
}

fn subscription_error(locale: Locale, error: subscription::DownloadError) -> String {
    use subscription::DownloadError;
    let message = match error {
        DownloadError::InvalidUrl => Message::InvalidSubscriptionUrl,
        DownloadError::Request => Message::SubscriptionDownloadFailed,
        DownloadError::TooLarge => Message::SubscriptionTooLarge,
        DownloadError::InvalidText => Message::SubscriptionInvalidText,
        DownloadError::NoProxies => Message::SubscriptionNoProxies,
        DownloadError::InvalidProfile(error) => return error.to_string(),
    };
    locale.message(message).to_owned()
}

// Mirrors the subscription downloader's URL policy for feedback before starting a job.
// The downloader validates again before making a request.
fn valid_subscription_url(input: &str) -> bool {
    url::Url::parse(input.trim()).is_ok_and(|url| {
        url.scheme() == "https"
            && url.has_host()
            && url.username().is_empty()
            && url.password().is_none()
            && url.fragment().is_none()
    })
}

fn subscription_paths(dir: PathBuf) -> std::io::Result<Vec<PathBuf>> {
    let store = SubscriptionStore::new(dir);
    Ok(store
        .ids()?
        .into_iter()
        .map(|id| store.record_path(id))
        .collect())
}

pub struct Paw {
    page: Page,
    locale: Locale,
    profile: Option<PathBuf>,
    binary: Option<PathBuf>,
    error: Option<String>,
    copied_listener: Option<u16>,
    show_engine_path: bool,
    show_usage: bool,
    preferences: Preferences,
    subscription_paths: Vec<PathBuf>,
    subscription_busy: bool,
    subscription_notice: Option<Message>,
    subscription_entry_open: bool,
    subscription_url: Entity<InputState>,
    _subscription_url_changes: Subscription,
    preference_warning: Option<String>,
    language_save_failed: bool,
    search: Entity<InputState>,
    _search_changes: Subscription,
    connection_filter: Entity<InputState>,
    _connection_filter_changes: Subscription,
    probe_url: Entity<InputState>,
    _probe_url_changes: Subscription,
    #[cfg(any(target_os = "linux", windows))]
    desktop: Option<meow_paw::app::Desktop>,
    #[cfg(any(target_os = "linux", windows))]
    expanded_groups: std::collections::HashSet<String>,
    #[cfg(any(target_os = "linux", windows))]
    proxy_order: std::collections::HashMap<String, ProxyOrder>,
    #[cfg(any(target_os = "linux", windows))]
    snapshot: meow_paw::app::Snapshot,
}

fn subscribe_search(search: &Entity<InputState>, cx: &mut Context<Paw>) -> Subscription {
    cx.subscribe(search, |_: &mut Paw, _, event, cx| {
        if matches!(event, InputEvent::Change) {
            cx.notify();
        }
    })
}

fn subscribe_subscription_url(input: &Entity<InputState>, cx: &mut Context<Paw>) -> Subscription {
    cx.subscribe(input, |paw: &mut Paw, _, event, cx| {
        if matches!(event, InputEvent::Change) {
            if paw.subscription_notice == Some(Message::InvalidSubscriptionUrl) {
                paw.subscription_notice = None;
            }
            cx.notify();
        }
    })
}

fn subscribe_probe_url(input: &Entity<InputState>, cx: &mut Context<Paw>) -> Subscription {
    cx.subscribe(input, |paw: &mut Paw, _, event, cx| {
        if matches!(event, InputEvent::Change) {
            #[cfg(any(target_os = "linux", windows))]
            if let Some(desktop) = &paw.desktop {
                desktop.invalidate_group_probe();
            }
            cx.notify();
        }
    })
}

pub fn rebind_window(paw: &Entity<Paw>, window: &mut Window, cx: &mut App) {
    let query = paw.read(cx).search.read(cx).value();
    let search = cx.new(|cx| InputState::new(window, cx).default_value(query));
    let query = paw.read(cx).connection_filter.read(cx).value();
    let connection_filter = cx.new(|cx| InputState::new(window, cx).default_value(query));
    let query = paw.read(cx).probe_url.read(cx).value();
    let probe_url = cx.new(|cx| InputState::new(window, cx).default_value(query));
    let query = paw.read(cx).subscription_url.read(cx).value();
    let subscription_url = cx.new(|cx| InputState::new(window, cx).default_value(query));
    paw.update(cx, |state, cx| {
        state._subscription_url_changes = subscribe_subscription_url(&subscription_url, cx);
        state.subscription_url = subscription_url;
        state._probe_url_changes = subscribe_probe_url(&probe_url, cx);
        state.probe_url = probe_url;
        state._search_changes = subscribe_search(&search, cx);
        state.search = search;
        state._connection_filter_changes = subscribe_search(&connection_filter, cx);
        state.connection_filter = connection_filter;
        cx.notify();
    });
}

pub fn open(
    binary: Option<PathBuf>,
    profile: Option<PathBuf>,
    (page, locale): (Page, Locale),
    start: bool,
    (preferences, warning): (Preferences, Option<&str>),
    window: &mut Window,
    cx: &mut App,
) -> Entity<Paw> {
    let search = cx.new(|cx| InputState::new(window, cx));
    let connection_filter = cx.new(|cx| InputState::new(window, cx));
    let probe_url = cx.new(|cx| InputState::new(window, cx));
    let subscription_url = cx.new(|cx| InputState::new(window, cx));
    cx.new(|cx| {
        let subscription_url_changes = subscribe_subscription_url(&subscription_url, cx);
        let probe_url_changes = subscribe_probe_url(&probe_url, cx);
        let search_changes = subscribe_search(&search, cx);
        let connection_filter_changes = subscribe_search(&connection_filter, cx);
        let (subscription_paths, subscription_notice) = preferences
            .subscription_dir()
            .map(subscription_paths)
            .map_or((Vec::new(), None), |result| match result {
                Ok(paths) => (paths, None),
                Err(_) => (Vec::new(), Some(Message::ListSubscriptionsFailed)),
            });
        let mut paw = Paw {
            page,
            locale,
            profile,
            #[cfg(any(target_os = "linux", windows))]
            desktop: binary
                .as_ref()
                .and_then(|path| meow_paw::app::Desktop::new(path.clone()).ok()),
            binary,
            #[cfg(any(target_os = "linux", windows))]
            expanded_groups: Default::default(),
            #[cfg(any(target_os = "linux", windows))]
            proxy_order: Default::default(),
            error: None,
            copied_listener: None,
            show_engine_path: false,
            show_usage: false,
            preferences,
            subscription_paths,
            subscription_busy: false,
            subscription_notice,
            subscription_entry_open: false,
            subscription_url,
            _subscription_url_changes: subscription_url_changes,
            preference_warning: warning.map(str::to_owned),
            language_save_failed: false,
            search,
            _search_changes: search_changes,
            connection_filter,
            _connection_filter_changes: connection_filter_changes,
            probe_url,
            _probe_url_changes: probe_url_changes,
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
                self.error = Some("Choose a profile in Profiles before starting.".into());
                return;
            };
            let result = if let Some((store, id)) = self.subscription_for(&profile) {
                desktop.start_subscription(store, id)
            } else {
                desktop.start(profile)
            };
            self.error = result.err().map(|e| e.to_string());
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
            if self.snapshot.provider_error.is_some() {
                return self.snapshot.provider_error.clone();
            }
            if self.snapshot.action_error.is_some() {
                return self.snapshot.action_error.clone();
            }
        }
        self.error.clone()
    }

    fn sidebar(&self, compact: bool, cx: &mut Context<Self>) -> Div {
        let (up, down, _) = self.traffic();
        div()
            .w(px(if compact { 62. } else { 204. }))
            .h_full()
            .flex_shrink_0()
            .bg(rgb(CHROME))
            .border_r_1()
            .border_color(rgb(LINE))
            .px(if compact { px(8.) } else { px(12.) })
            .flex()
            .flex_col()
            .child(
                div()
                    .h(px(74.))
                    .px(if compact { px(3.) } else { px(10.) })
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(paw_badge(40.))
                    .when(!compact, |brand| {
                        brand.child(
                            div()
                                .text_xl()
                                .font_weight(FontWeight::BOLD)
                                .child(self.locale.message(Message::AppTitle)),
                        )
                    }),
            )
            .child(
                div()
                    .id("paw-nav-list")
                    .min_h_0()
                    .flex_1()
                    .overflow_y_scroll()
                    .children(Self::pages(self.page, self.locale, compact, cx)),
            )
            .when(!compact, |bar| {
                bar.child(
                    div()
                        .border_t_1()
                        .border_color(rgb(LINE))
                        .px_2()
                        .py_3()
                        .text_xs()
                        .child(sidebar_rate(
                            "icons/arrow-up-right.svg",
                            self.locale.message(Message::Upload),
                            up,
                            STREAM_UP,
                        ))
                        .child(sidebar_rate(
                            "icons/arrow-down-left.svg",
                            self.locale.message(Message::Download),
                            down,
                            STREAM_DOWN,
                        )),
                )
            })
    }

    fn pages(
        active: Page,
        locale: Locale,
        compact: bool,
        cx: &mut Context<Self>,
    ) -> Vec<Stateful<Div>> {
        Page::ALL
            .into_iter()
            .enumerate()
            .map(|(index, page)| {
                let on = active == page;
                let label = locale.message(page.message());
                div()
                    .id(("paw-nav", index))
                    .flex()
                    .items_center()
                    .gap_3()
                    .h(px(46.))
                    .px_3()
                    .mb_1()
                    .when(compact, |row| row.justify_center())
                    .rounded_md()
                    .cursor_pointer()
                    .bg(rgb(if on { SOFT_BLUE } else { CHROME }))
                    .text_color(rgb(if on { BLUE } else { INK }))
                    .hover(|s| s.bg(rgb(SOFT_BLUE)))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.page = page;
                        if page == Page::Rules {
                            this.read_rules();
                        }
                        cx.notify();
                    }))
                    .child(line_icon(page.icon(), if on { BLUE } else { FAINT }, 18.))
                    .when(!compact, |row| {
                        row.child(
                            div()
                                .min_w_0()
                                .truncate()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(label),
                        )
                    })
                    .when(compact, |el| {
                        el.tooltip(move |window, cx| {
                            gpui_kit::component::tooltip::Tooltip::new(label).build(window, cx)
                        })
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
            .child(self.locale.message(if running {
                Message::StopSession
            } else if phase == "Stopping" {
                Message::Stopping
            } else if supported && !ready {
                Message::ChooseEngine
            } else if supported && self.profile.is_none() {
                Message::ChooseProfile
            } else {
                Message::StartSession
            }))
            .when(enabled, |e| {
                e.cursor_pointer()
                    .hover(|style| style.bg(rgb(0x1149ae)))
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

    fn proxy_header(&self, cx: &mut Context<Self>) -> Div {
        #[cfg(any(target_os = "linux", windows))]
        {
            use meow_paw::engine::controller::RoutingMode;
            if let Some(active) = self.snapshot.mode {
                return div()
                    .flex()
                    .rounded_md()
                    .overflow_hidden()
                    .border_1()
                    .border_color(rgb(0x9eb9ee))
                    .children(
                        RoutingMode::ALL
                            .into_iter()
                            .enumerate()
                            .map(|(index, mode)| {
                                let selected = mode == active;
                                div()
                                    .id(("routing-mode", index))
                                    .px_4()
                                    .py_2()
                                    .text_sm()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(rgb(if selected { SURFACE } else { BLUE }))
                                    .bg(rgb(if selected { BLUE } else { SURFACE }))
                                    .when(index != 0, |el| {
                                        el.border_l_1().border_color(rgb(0x9eb9ee))
                                    })
                                    .child(self.locale.message(match mode {
                                        RoutingMode::Rule => Message::ModeRule,
                                        RoutingMode::Global => Message::ModeGlobal,
                                        RoutingMode::Direct => Message::ModeDirect,
                                    }))
                                    .when(!selected, |el| {
                                        el.cursor_pointer()
                                            .hover(|s| s.bg(rgb(SOFT_BLUE)))
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.error = this
                                                    .desktop
                                                    .as_ref()
                                                    .and_then(|d| d.set_mode(mode).err())
                                                    .map(|e| e.to_string());
                                                cx.notify();
                                            }))
                                    })
                            }),
                    );
            }
        }
        self.actions(cx)
    }

    fn read_rules(&mut self) {
        #[cfg(any(target_os = "linux", windows))]
        {
            self.error = self
                .desktop
                .as_ref()
                .and_then(|d| d.read_rules().err())
                .map(|e| e.to_string());
        }
    }

    fn rules(&self, cx: &mut Context<Self>) -> Div {
        #[cfg(any(target_os = "linux", windows))]
        if self.state().2.is_some() {
            let button =
                secondary_button("refresh-rules", self.locale.message(Message::RefreshRules))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.read_rules();
                        cx.notify();
                    }));
            let page = div()
                .flex()
                .flex_col()
                .gap_3()
                .child(div().flex().justify_end().child(button));
            return match &self.snapshot.rules {
                Some(rules) => page.child(
                    card()
                        .child(section_label(self.locale.rule_count(rules.len())))
                        .child(
                            connection_row(
                                section_label(self.locale.message(Message::RuleTypeColumn)),
                                section_label(self.locale.message(Message::RuleMatchColumn)),
                                section_label(self.locale.message(Message::RulePolicyColumn)),
                            )
                            .mt_2()
                            .bg(rgb(SOFT_BLUE))
                            .rounded_md(),
                        )
                        .children(rules.iter().map(|rule| {
                            connection_row(
                                div()
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(rule.kind.clone()),
                                div()
                                    .text_color(rgb(FAINT))
                                    .child(if rule.payload.is_empty() {
                                        "—".into()
                                    } else {
                                        rule.payload.clone()
                                    }),
                                div().child(rule.proxy.clone()),
                            )
                            .border_b_1()
                            .border_color(rgb(LINE))
                        })),
                ),
                None => page.child(empty_state(
                    self.locale.message(Message::NoRuleSnapshot),
                    self.locale.message(Message::RefreshRulesDetail),
                )),
            };
        }
        let _ = cx;
        empty_state(
            self.locale.message(Message::NoRunningSession),
            self.locale.message(Message::StartToViewRules),
        )
    }

    fn logs(&self, cx: &mut Context<Self>) -> Div {
        #[cfg(any(target_os = "linux", windows))]
        if let Some(history) = self.snapshot.logs.as_ref() {
            let entries = history.recent();
            let clear = history.clone();
            let panel = div().flex().flex_col().gap_3().child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(chip(
                        self.locale.retained_log_count(entries.len()),
                        BLUE,
                        SOFT_BLUE,
                    ))
                    .child(
                        secondary_button("clear-logs", self.locale.message(Message::ClearLogs))
                            .on_click(cx.listener(move |_, _, _, cx| {
                                clear.clear();
                                cx.notify();
                            })),
                    ),
            );
            if entries.is_empty() {
                return panel.child(empty_state(
                    self.locale.message(Message::WaitingForLogs),
                    self.locale.message(Message::WaitingForLogsDetail),
                ));
            }
            return panel.child(card().children(entries.into_iter().map(|entry| {
                let color = match entry.level.as_str() {
                    "error" => 0xa83a33,
                    "warn" => STREAM_UP,
                    _ => BLUE,
                };
                div()
                    .flex()
                    .items_start()
                    .gap_3()
                    .py_2()
                    .border_b_1()
                    .border_color(rgb(LINE))
                    .child(
                        div()
                            .w(px(70.))
                            .flex_shrink_0()
                            .text_sm()
                            .text_color(rgb(FAINT))
                            .child(entry.time),
                    )
                    .child(
                        div()
                            .w(px(56.))
                            .flex_shrink_0()
                            .child(chip(entry.level, color, BG)),
                    )
                    .child(div().min_w_0().flex_1().text_sm().child(entry.message))
            })));
        }
        let _ = cx;
        empty_state(
            self.locale.message(Message::NoRunningSession),
            self.locale.message(Message::StartToViewLogs),
        )
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

    fn overview(&self, wide: bool, cx: &mut Context<Self>) -> Div {
        let (phase, color, port) = self.state();
        let metric_row = || div().flex().gap_3().when(!wide, |el| el.flex_col());
        let width = |card: Div| card.w_full().min_w_0().when(wide, |el| el.flex_1());
        let session_state = chip(phase, color, if port.is_some() { 0xe2f6ec } else { BG });
        let address = port.map_or_else(|| "No listener".into(), |p| format!("127.0.0.1:{p}"));
        let copy = if let Some(port) = port {
            secondary_button(
                "copy-listener",
                if self.copied_listener == Some(port) {
                    "Copied"
                } else {
                    self.locale.message(Message::Copy)
                },
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.write_to_clipboard(ClipboardItem::new_string(format!("127.0.0.1:{port}")));
                this.copied_listener = Some(port);
                cx.notify();
            }))
            .into_any_element()
        } else {
            chip("Loopback", BLUE, SOFT_BLUE).into_any_element()
        };
        let profile =
            form_card(self.locale.message(Message::ActiveProfile))
                .child(form_row(
                    self.locale.message(Message::Configuration),
                    self.overview_profile_label(),
                    secondary_button("home-profiles", self.locale.message(Message::Profiles))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.page = Page::Profiles;
                            cx.notify();
                        })),
                ))
                .child(form_row(
                    self.locale.message(Message::ProxyGroups),
                    self.locale.message(Message::SelectedRoutes),
                    secondary_button("home-proxies", self.locale.message(Message::Proxies))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.page = Page::Proxies;
                            cx.notify();
                        })),
                ));
        let network = form_card(self.locale.message(Message::Network))
            .child(form_row(
                self.locale.message(Message::MeowEngine),
                self.locale.message(Message::ManagedSession),
                session_state,
            ))
            .child(form_row(
                self.locale.message(Message::LocalListener),
                address,
                copy,
            ))
            .child(form_row(
                self.locale.message(Message::SystemProxy),
                self.locale.message(Message::ExistingSystemProxy),
                chip("Unmanaged", FAINT, BG),
            ));
        let usage_toggle = secondary_button(
            "home-toggle-usage",
            if self.show_usage {
                self.locale.message(Message::HideDetails)
            } else {
                self.locale.message(Message::ShowDetails)
            },
        )
        .on_click(cx.listener(|this, _, _, cx| {
            this.show_usage = !this.show_usage;
            cx.notify();
        }));
        let (up, down, total) = self.traffic();
        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(metric_row().child(width(profile)).child(width(network)))
            .child(
                card()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap_3()
                            .child(
                                div()
                                    .font_weight(FontWeight::BOLD)
                                    .child(self.locale.message(Message::Traffic)),
                            )
                            .child(usage_toggle),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_3()
                            .child(traffic_metric(
                                self.locale.message(Message::Download),
                                down,
                                STREAM_DOWN,
                            ))
                            .child(traffic_metric(
                                self.locale.message(Message::Upload),
                                up,
                                STREAM_UP,
                            ))
                            .child(traffic_metric(
                                self.locale.message(Message::SessionTotal),
                                total,
                                INK,
                            )),
                    )
                    .child(self.traffic_chart(28.)),
            )
            .when(self.show_usage, |body| body.child(self.traffic_timeline()))
    }

    fn traffic_timeline(&self) -> Div {
        card()
            .child(
                div()
                    .flex()
                    .justify_between()
                    .items_center()
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(self.locale.message(Message::TrafficActivity)),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_4()
                            .child(traffic_legend(
                                self.locale.message(Message::Download),
                                STREAM_DOWN,
                            ))
                            .child(traffic_legend(
                                self.locale.message(Message::Upload),
                                STREAM_UP,
                            )),
                    ),
            )
            .child(self.traffic_chart(60.))
    }

    fn traffic_chart(&self, height: f32) -> Div {
        #[cfg(any(target_os = "linux", windows))]
        {
            use meow_paw::app::TRAFFIC_WINDOW_SECS;
            let samples: Vec<_> = self.snapshot.traffic_history.recent().collect();
            if let Some((peak, bars)) = traffic_bars(&samples, height) {
                let pad = TRAFFIC_WINDOW_SECS.saturating_sub(bars.len());
                let columns = std::iter::repeat_n(None, pad).chain(bars).map(|sample| {
                    let column = div().flex_1().flex().items_end().gap(px(1.)).h_full();
                    match sample {
                        Some((0., 0.)) => {
                            column.child(div().flex_1().h(px(2.)).rounded_sm().bg(rgb(LINE)))
                        }
                        Some((up, down)) => column
                            .child(div().flex_1().rounded_sm().h(px(down)).bg(rgb(STREAM_DOWN)))
                            .child(div().flex_1().rounded_sm().h(px(up)).bg(rgb(STREAM_UP))),
                        None => column,
                    }
                });
                return div()
                    .child(
                        div()
                            .mt_2()
                            .h(px(height + 1.))
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
                            .mt_1()
                            .text_xs()
                            .text_color(rgb(FAINT))
                            .child("39s ago · gaps unavailable · grey zero")
                            .child(format!("{}/s shared peak · now", bytes(peak))),
                    );
            }
        }
        let _ = height;
        div()
            .mt_2()
            .text_xs()
            .text_color(rgb(FAINT))
            .child("No traffic samples available yet.")
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

    fn display_profile_name(&self, path: &std::path::Path) -> String {
        if let Some((_, id)) = self.subscription_for(path) {
            return format!(
                "{} {}",
                self.locale.message(Message::Subscription),
                &id.to_string()[..8]
            );
        }
        path.file_name()
            .unwrap_or(path.as_os_str())
            .to_string_lossy()
            .into_owned()
    }

    fn profile_label(&self) -> String {
        self.profile.as_deref().map_or_else(
            || self.locale.message(Message::NoProfileSelected).into(),
            |path| self.display_profile_name(path),
        )
    }

    fn overview_profile_label(&self) -> String {
        #[cfg(any(target_os = "linux", windows))]
        if self.state().2.is_some() {
            return self.snapshot.profile.as_deref().map_or_else(
                || "Unconfirmed live profile".into(),
                |path| self.display_profile_name(path),
            );
        }
        self.profile_label()
    }

    fn profiles(&self, cx: &mut Context<Self>) -> Div {
        let path = self.profile.as_ref().map_or_else(
            || self.locale.message(Message::NoProfileSelected).into(),
            |path| {
                if self.subscription_for(path).is_some() {
                    self.display_profile_name(path)
                } else {
                    path.display().to_string()
                }
            },
        );
        let browse = secondary_button(
            "select-profile",
            self.locale.message(Message::ImportLocalYaml),
        )
        .on_click(cx.listener(|this, _, _, cx| this.choose_file(false, cx)));
        let import = div()
            .flex()
            .gap_2()
            .items_center()
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .px_3()
                    .py_2()
                    .rounded_md()
                    .border_1()
                    .bg(rgb(SURFACE))
                    .border_color(rgb(LINE))
                    .text_sm()
                    .text_color(rgb(FAINT))
                    .truncate()
                    .child(path),
            )
            .child(div().flex_shrink_0().child(browse));
        let selected = self.profile.as_ref();
        let mut profiles = self.preferences.profiles().iter().collect::<Vec<_>>();
        if let Some(path) = selected
            && !profiles.contains(&path)
        {
            profiles.insert(0, path);
        }
        for path in &self.subscription_paths {
            if !profiles.contains(&path) {
                profiles.push(path);
            }
        }
        let mut page = div().flex().flex_col().gap_3().child(import);
        if self.preferences.subscription_dir().is_some() && !self.subscription_busy {
            if self.subscription_entry_open {
                page = page.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(section_label(self.locale.message(Message::SubscriptionUrl)))
                        .child(Input::new(&self.subscription_url).cleanable(true))
                        .child(
                            div()
                                .flex()
                                .flex_wrap()
                                .gap_2()
                                .child(
                                    secondary_button(
                                        "confirm-subscription",
                                        self.locale.message(Message::ImportSubscription),
                                    )
                                    .on_click(cx.listener(
                                        |paw, _, window, cx| paw.import_subscription(window, cx),
                                    )),
                                )
                                .child(
                                    secondary_button(
                                        "cancel-subscription",
                                        self.locale.message(Message::CancelSubscription),
                                    )
                                    .on_click(cx.listener(
                                        |paw, _, window, cx| paw.cancel_subscription(window, cx),
                                    )),
                                ),
                        ),
                );
            } else {
                page = page.child(
                    secondary_button(
                        "import-subscription",
                        self.locale.message(Message::AddSubscription),
                    )
                    .on_click(cx.listener(|paw, _, _, cx| {
                        paw.subscription_entry_open = true;
                        paw.subscription_notice = None;
                        cx.notify();
                    })),
                );
            }
        }
        if self.subscription_busy || self.subscription_notice.is_some() {
            page = page.child(div().text_sm().text_color(rgb(FAINT)).child(
                if self.subscription_busy {
                    self.locale.message(Message::DownloadingSubscription)
                } else {
                    self.subscription_notice
                        .map(|message| self.locale.message(message))
                        .unwrap_or_default()
                },
            ));
        }
        if self.state().2.is_some() {
            page = page.child(
                div()
                    .text_sm()
                    .text_color(rgb(FAINT))
                    .child(self.locale.message(Message::ProfileSelectionDetail))
                    .child(" ")
                    .child(self.locale.message(Message::LiveApplyWarning)),
            );
        }
        if profiles.is_empty() {
            page = page.child(empty_state(
                self.locale.message(Message::NoLocalProfiles),
                self.locale.message(Message::ImportYamlDetail),
            ));
        } else {
            page = page.children(profiles.into_iter().map(|profile| {
                let next = selected == Some(profile);
                #[cfg(any(target_os = "linux", windows))]
                let active =
                    self.state().2.is_some() && self.snapshot.profile.as_ref() == Some(profile);
                #[cfg(not(any(target_os = "linux", windows)))]
                let active = false;
                #[cfg(any(target_os = "linux", windows))]
                let (live, applying) = (
                    self.state().2.is_some(),
                    self.snapshot.applying_profile.as_ref(),
                );
                #[cfg(not(any(target_os = "linux", windows)))]
                let (live, applying): (bool, Option<&PathBuf>) = (false, None);
                let apply_path = profile.clone();
                let use_path = profile.clone();
                let forget_path = profile.clone();
                let subscription = self.subscription_for(profile).map(|(_, id)| id);
                let name = self.display_profile_name(profile);
                let action_id = |action: &str| SharedString::from(format!("{action}:{profile:?}"));
                let details = if subscription.is_some() {
                    self.locale.message(Message::SubscriptionDetail).to_owned()
                } else {
                    profile.display().to_string()
                };
                card()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .child(div().font_weight(FontWeight::BOLD).truncate().child(name))
                            .child(
                                div()
                                    .mt_1()
                                    .text_xs()
                                    .text_color(rgb(FAINT))
                                    .truncate()
                                    .child(details),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .gap_2()
                            .when(active, |el| {
                                el.child(chip(
                                    self.locale.message(Message::Running),
                                    0x168460,
                                    0xe2f6ec,
                                ))
                            })
                            .when(next, |el| {
                                el.child(chip(
                                    self.locale.message(Message::NextSession),
                                    BLUE,
                                    SOFT_BLUE,
                                ))
                            })
                            .when(applying == Some(profile), |el| {
                                el.child(chip(
                                    self.locale.message(Message::Applying),
                                    BLUE,
                                    SOFT_BLUE,
                                ))
                            })
                            .when(live && applying.is_none(), |el| {
                                el.child(
                                    secondary_button(
                                        action_id("profile-apply"),
                                        self.locale.message(if active {
                                            Message::Reload
                                        } else {
                                            Message::ApplyLive
                                        }),
                                    )
                                    .on_click(cx.listener(
                                        move |paw, _, _, cx| {
                                            paw.apply_local_profile(apply_path.clone());
                                            cx.notify();
                                        },
                                    )),
                                )
                            })
                            .when_some(
                                subscription.filter(|_| !self.subscription_busy),
                                |el, id| {
                                    el.child(
                                        secondary_button(
                                            action_id("profile-refresh"),
                                            self.locale.message(Message::Refresh),
                                        )
                                        .on_click(
                                            cx.listener(move |paw, _, _, cx| {
                                                paw.refresh_subscription(id, cx);
                                            }),
                                        ),
                                    )
                                },
                            )
                            .when(!next, |el| {
                                el.child(
                                    secondary_button(
                                        action_id("profile-use"),
                                        self.locale.message(Message::Use),
                                    )
                                    .on_click(cx.listener(
                                        move |paw, _, _, cx| {
                                            paw.select_local_profile(use_path.clone());
                                            cx.notify();
                                        },
                                    )),
                                )
                            })
                            .when(
                                subscription.is_none()
                                    && !active
                                    && applying != Some(profile)
                                    && self.preferences.profiles().contains(profile),
                                |el| {
                                    el.child(
                                        secondary_button(
                                            action_id("profile-forget"),
                                            self.locale.message(Message::Forget),
                                        )
                                        .on_click(
                                            cx.listener(move |paw, _, _, cx| {
                                                match paw.preferences.forget_profile(&forget_path) {
                                                    Ok(()) => {
                                                        if paw.profile.as_ref()
                                                            == Some(&forget_path)
                                                        {
                                                            paw.profile = None;
                                                        }
                                                        paw.error = None;
                                                    }
                                                    Err(error) => {
                                                        paw.error = Some(error.to_string())
                                                    }
                                                }
                                                cx.notify();
                                            }),
                                        ),
                                    )
                                },
                            ),
                    )
            }));
        }
        #[cfg(any(target_os = "linux", windows))]
        if !self.snapshot.diagnostics.is_empty() {
            return page.child(
                card()
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(self.locale.message(Message::LastStartDiagnostics)),
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
                    self.locale.message(Message::ChooseYaml)
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
                    } else {
                        paw.select_local_profile(path);
                    }
                    cx.notify();
                });
            }
        })
        .detach();
    }

    fn subscription_for(
        &self,
        path: &std::path::Path,
    ) -> Option<(SubscriptionStore, SubscriptionId)> {
        let store = SubscriptionStore::new(self.preferences.subscription_dir()?);
        let id = store.id_for_record(path)?;
        Some((store, id))
    }

    fn cancel_subscription(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.subscription_url
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.subscription_entry_open = false;
        self.subscription_notice = None;
        cx.notify();
    }

    fn import_subscription(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.subscription_busy || self.preferences.subscription_dir().is_none() {
            return;
        }
        let url = self.subscription_url.read(cx).value().to_string();
        if !valid_subscription_url(&url) {
            self.subscription_notice = Some(Message::InvalidSubscriptionUrl);
            cx.notify();
            return;
        }
        self.subscription_url
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.subscription_entry_open = false;
        let locale = self.locale;
        self.subscription_job(cx, move |store| {
            let candidate =
                subscription::fetch(&url).map_err(|err| subscription_error(locale, err))?;
            let id = store
                .import(&url, candidate, RefreshPolicy::default())
                .map_err(|_| locale.message(Message::SubscriptionSaveFailed).to_owned())?;
            Ok(Some(store.record_path(id)))
        });
    }

    fn refresh_subscription(&mut self, id: SubscriptionId, cx: &mut Context<Self>) {
        let locale = self.locale;
        self.subscription_job(cx, move |store| {
            let prior = store
                .read(id)
                .map_err(|_| locale.message(Message::SubscriptionReadFailed).to_owned())?;
            let candidate = prior
                .fetch_latest()
                .map_err(|err| subscription_error(locale, err))?;
            store.refresh(id, candidate).map_err(|_| {
                locale
                    .message(Message::SubscriptionRefreshSaveFailed)
                    .to_owned()
            })?;
            Ok(None)
        });
    }

    fn subscription_job(
        &mut self,
        cx: &mut Context<Self>,
        job: impl FnOnce(&mut SubscriptionStore) -> Result<Option<PathBuf>, String> + Send + 'static,
    ) {
        let Some(dir) = self.preferences.subscription_dir() else {
            return;
        };
        if self.subscription_busy {
            return;
        }
        self.subscription_busy = true;
        self.subscription_notice = None;
        self.error = None;
        cx.notify();
        cx.spawn(async move |weak, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { job(&mut SubscriptionStore::new(dir)) })
                .await;
            let _ = weak.update(cx, |paw, cx| {
                paw.subscription_busy = false;
                match result {
                    Ok(Some(path)) => {
                        if !paw.subscription_paths.contains(&path) {
                            paw.subscription_paths.push(path);
                        }
                        paw.subscription_notice = Some(Message::SubscriptionSaved);
                    }
                    Ok(None) => paw.subscription_notice = Some(Message::SubscriptionRefreshed),
                    Err(error) => paw.error = Some(error),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn apply_local_profile(&mut self, path: PathBuf) {
        #[cfg(any(target_os = "linux", windows))]
        if let Some(desktop) = &self.desktop {
            let result = if let Some((store, id)) = self.subscription_for(&path) {
                desktop.apply_subscription(store, id)
            } else {
                desktop.apply_profile(path)
            };
            self.error = result.err().map(|error| error.to_string());
        }
        #[cfg(not(any(target_os = "linux", windows)))]
        let _ = path;
    }

    fn select_local_profile(&mut self, path: PathBuf) {
        if !path.is_file() {
            self.error = Some(self.locale.message(Message::ChooseExistingYaml).into());
            return;
        }
        let path = std::path::absolute(&path).unwrap_or(path);
        self.remember_selection(false, &path);
        self.profile = Some(path);
        self.error = None;
    }

    fn proxies(&self, window_width: Pixels, cx: &mut Context<Self>) -> Div {
        #[cfg(any(target_os = "linux", windows))]
        if let Some(inventory) = self.snapshot.inventory.as_ref() {
            let value = self.search.read(cx).value();
            let query = value.trim().to_lowercase();
            let mut count = 0;
            let columns = route_columns(window_width);
            use meow_paw::app::{ProxyProbeStatus, ProxyProbeTarget, parse_probe_url};
            let delays = unambiguous_route_delays(inventory);
            let probe_url = parse_probe_url(&self.probe_url.read(cx).value());
            let probe_ready = probe_url.is_some();
            let mut probe_candidates = std::collections::HashMap::new();
            for node in &inventory.nodes {
                probe_candidates
                    .entry(node.name.as_str())
                    .and_modify(|origin| *origin = None)
                    .or_insert(Some(&node.origin));
            }
            let probe_busy = self.snapshot.probe_busy
                || self.snapshot.group_probe.as_ref().is_some_and(|g| g.active);
            let epoch = self.snapshot.probe_epoch;
            let groups = mode_groups(inventory, self.snapshot.mode)
                .into_iter()
                .filter_map(|(name, group)| {
                    let all = unique_route_members(group.all.as_deref().unwrap_or_default());
                    let route_count = all.len();
                    let show_all = route_matches(name, &query);
                    let mut members: Vec<_> = all
                        .into_iter()
                        .filter(|(_, member)| show_all || route_matches(member, &query))
                        .collect();
                    if !show_all && members.is_empty() {
                        return None;
                    }
                    count += 1;
                    let expandable = name != "GLOBAL";
                    let open =
                        !expandable || !query.is_empty() || self.expanded_groups.contains(name);
                    let order = self.proxy_order.get(name).copied().unwrap_or_default();
                    if open {
                        sort_route_members(&mut members, order, &delays);
                    }
                    let toggle = name.clone();
                    let heading = card()
                        .px_3()
                        .py_2()
                        .id(SharedString::from(format!("group:{name}")))
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap_3()
                        .when(expandable && query.is_empty(), |row| {
                            row.cursor_pointer()
                                .hover(|style| style.bg(rgb(0xf6f9ff)))
                                .on_click(cx.listener(move |paw, _, _, cx| {
                                    if !paw.expanded_groups.remove(&toggle) {
                                        paw.expanded_groups.insert(toggle.clone());
                                    }
                                    cx.notify();
                                }))
                        })
                        .child(
                            div()
                                .min_w_0()
                                .flex_1()
                                .child(
                                    div()
                                        .flex()
                                        .flex_wrap()
                                        .items_center()
                                        .gap_2()
                                        .child(
                                            div()
                                                .min_w_0()
                                                .font_weight(FontWeight::BOLD)
                                                .child(name.clone()),
                                        )
                                        .child(compact_chip(group.kind.clone(), BLUE, SOFT_BLUE))
                                        .child(compact_chip(
                                            format!(
                                                "{route_count} {}",
                                                self.locale.message(if route_count == 1 {
                                                    Message::Route
                                                } else {
                                                    Message::Routes
                                                })
                                            ),
                                            FAINT,
                                            BG,
                                        )),
                                )
                                .child(
                                    div().text_xs().text_color(rgb(FAINT)).child(
                                        group
                                            .now
                                            .clone()
                                            .unwrap_or_else(|| "No active route".into()),
                                    ),
                                ),
                        )
                        .when(expandable, |heading| {
                            heading.child(line_icon(
                                if open {
                                    "icons/chevron-down.svg"
                                } else {
                                    "icons/chevron-right.svg"
                                },
                                FAINT,
                                18.,
                            ))
                        });
                    let mutable = group.kind.eq_ignore_ascii_case("selector");
                    Some(
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(heading)
                            .when(open, |panel| {
                                if members.is_empty() {
                                    return panel.child(
                                        div()
                                            .px_3()
                                            .py_2()
                                            .text_sm()
                                            .text_color(rgb(FAINT))
                                            .child(self.locale.message(Message::NoCandidateRoutes)),
                                    );
                                }
                                let group_name = name.clone();
                                let comparison = self.snapshot.group_probe.as_ref().filter(|g| {
                                    g.group == *name && probe_url.as_ref() == Some(g.results.url())
                                });
                                let running = comparison.is_some_and(|g| g.active);
                                let compare_name = name.clone();
                                let labels = match self.locale {
                                    Locale::En => (
                                        "Compare nodes",
                                        "Cancel comparison",
                                        "Probe busy",
                                        "Waiting",
                                        "Testing…",
                                        "Unavailable",
                                        "Interrupted",
                                        "Complete",
                                    ),
                                    Locale::ZhCn => (
                                        "比较节点",
                                        "取消比较",
                                        "测试进行中",
                                        "等待中",
                                        "测试中…",
                                        "不可用",
                                        "已中断",
                                        "已完成",
                                    ),
                                };
                                let compare_button = secondary_button(
                                    SharedString::from(format!("group-compare:{name}")),
                                    if running {
                                        labels.1
                                    } else if probe_busy {
                                        labels.2
                                    } else {
                                        labels.0
                                    },
                                )
                                .when(
                                    running || (probe_ready && !probe_busy),
                                    |button| {
                                        button.on_click(cx.listener(move |paw, _, _, cx| {
                                            if let Some(desktop) = &paw.desktop {
                                                if running {
                                                    desktop.cancel_group_probe();
                                                } else {
                                                    let value = paw.probe_url.read(cx).value();
                                                    paw.error = desktop
                                                        .compare_group(epoch, &compare_name, &value)
                                                        .err()
                                                        .map(|e| e.to_string());
                                                }
                                            }
                                            cx.notify();
                                        }))
                                    },
                                );
                                let comparison_panel = div()
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .child(
                                        div()
                                            .flex()
                                            .flex_wrap()
                                            .items_center()
                                            .gap_2()
                                            .child(compare_button)
                                            .when_some(comparison, |row, g| {
                                                let done = g
                                                    .results
                                                    .rows()
                                                    .iter()
                                                    .filter(|(_, status)| {
                                                        matches!(
                                                            status,
                                                            Some(
                                                                ProxyProbeStatus::Measured(_)
                                                                    | ProxyProbeStatus::Unavailable
                                                            )
                                                        )
                                                    })
                                                    .count();
                                                row.child(
                                                    div().text_xs().text_color(rgb(FAINT)).child(
                                                        format!(
                                                            "{done}/{} · {}",
                                                            g.results.rows().len(),
                                                            if g.interrupted {
                                                                labels.6
                                                            } else if g.active {
                                                                labels.4
                                                            } else {
                                                                labels.7
                                                            }
                                                        ),
                                                    ),
                                                )
                                            }),
                                    )
                                    .when_some(comparison, |panel, g| {
                                        panel.children(g.results.rows().iter().map(
                                            |(target, status)| {
                                                let text = match status {
                                                    Some(ProxyProbeStatus::Measured(ms)) => {
                                                        format!("{ms} ms")
                                                    }
                                                    Some(ProxyProbeStatus::Unavailable) => {
                                                        labels.5.to_owned()
                                                    }
                                                    Some(ProxyProbeStatus::Running) if g.active => {
                                                        labels.4.to_owned()
                                                    }
                                                    _ if g.interrupted => labels.6.to_owned(),
                                                    _ => labels.3.to_owned(),
                                                };
                                                div()
                                                    .flex()
                                                    .flex_wrap()
                                                    .justify_between()
                                                    .gap_1()
                                                    .text_xs()
                                                    .text_color(rgb(FAINT))
                                                    .child(target.name.clone())
                                                    .child(text)
                                            },
                                        ))
                                    });
                                let candidates = div()
                                    .flex()
                                    .flex_col()
                                    .gap_2()
                                    .child(comparison_panel)
                                    .child(
                                        div().flex().justify_end().child(
                                            secondary_button(
                                                SharedString::from(format!("proxy-order:{name}")),
                                                self.locale.message(order.message()),
                                            )
                                            .on_click(
                                                cx.listener(move |paw, _, _, cx| {
                                                    let next = order.next();
                                                    if next == ProxyOrder::Configured {
                                                        paw.proxy_order.remove(&group_name);
                                                    } else {
                                                        paw.proxy_order
                                                            .insert(group_name.clone(), next);
                                                    }
                                                    cx.notify();
                                                }),
                                            ),
                                        ),
                                    )
                                    .children(members.chunks(columns).map(|pair| {
                                        let row = div().flex().gap_2().children(pair.iter().map(
                                            |&(index, member)| {
                                                let (g, n) = (name.clone(), member.to_owned());
                                                let selected = group.now.as_deref() == Some(member);
                                                let target = probe_candidates
                                                    .get(member)
                                                    .copied()
                                                    .flatten()
                                                    .filter(|_| {
                                                        !inventory.groups.contains_key(member)
                                                    })
                                                    .map(|origin| ProxyProbeTarget {
                                                        name: member.into(),
                                                        origin: origin.clone(),
                                                    });
                                                let status_text = target
                                                    .as_ref()
                                                    .zip(probe_url.as_ref())
                                                    .and_then(|(target, url)| {
                                                        self.snapshot.probes.status(url, target)
                                                    })
                                                    .map(|status| match status {
                                                        ProxyProbeStatus::Running => {
                                                            "Testing…".into()
                                                        }
                                                        ProxyProbeStatus::Measured(ms) => {
                                                            format!("Test: {ms} ms")
                                                        }
                                                        ProxyProbeStatus::Unavailable => {
                                                            "Test unavailable".into()
                                                        }
                                                    });
                                                card()
                                                    .id(SharedString::from(format!(
                                                        "proxy:{name}:{index}"
                                                    )))
                                                    .flex_1()
                                                    .min_w_0()
                                                    .min_h(px(48.))
                                                    .px_3()
                                                    .py_2()
                                                    .flex()
                                                    .items_center()
                                                    .justify_between()
                                                    .gap_2()
                                                    .border_l(px(3.))
                                                    .border_color(rgb(if selected {
                                                        BLUE
                                                    } else {
                                                        LINE
                                                    }))
                                                    .bg(rgb(if selected {
                                                        SOFT_BLUE
                                                    } else {
                                                        SURFACE
                                                    }))
                                                    .child(
                                                        div()
                                                            .min_w_0()
                                                            .text_sm()
                                                            .child(member.to_owned()),
                                                    )
                                                    .when_some(
                                                        delays.get(member).copied().flatten(),
                                                        |el, ms| {
                                                            el.child(
                                                                div()
                                                                    .flex_shrink_0()
                                                                    .text_sm()
                                                                    .text_color(rgb(FAINT))
                                                                    .child(format!("{ms} ms")),
                                                            )
                                                        },
                                                    )
                                                    .when(selected, |el| {
                                                        el.child(line_icon(
                                                            "icons/circle-check.svg",
                                                            BLUE,
                                                            17.,
                                                        ))
                                                    })
                                                    .when_some(status_text, |el, status| {
                                                        el.child(
                                                            div()
                                                                .text_xs()
                                                                .text_color(rgb(FAINT))
                                                                .child(status),
                                                        )
                                                    })
                                                    .when_some(
                                                        target.filter(|_| probe_ready),
                                                        |el, target| {
                                                            let test = secondary_button(
                                                                SharedString::from(format!(
                                                                    "probe:{name}:{index}"
                                                                )),
                                                                self.locale.message(Message::Test),
                                                            )
                                                            .on_click(cx.listener(
                                                                move |paw, _, _, cx| {
                                                                    cx.stop_propagation();
                                                                    if !probe_busy {
                                                                        let value = paw
                                                                            .probe_url
                                                                            .read(cx)
                                                                            .value();
                                                                        paw.error = paw
                                                                            .desktop
                                                                            .as_ref()
                                                                            .and_then(|desktop| {
                                                                                desktop
                                                                                    .probe_proxy(
                                                                                        epoch,
                                                                                        target
                                                                                            .clone(
                                                                                            ),
                                                                                        &value,
                                                                                    )
                                                                                    .err()
                                                                                    .map(|e| {
                                                                                        e.to_string(
                                                                                        )
                                                                                    })
                                                                            });
                                                                        cx.notify();
                                                                    }
                                                                },
                                                            ));
                                                            el.child(test)
                                                        },
                                                    )
                                                    .when(mutable && !selected, |el| {
                                                        el.cursor_pointer()
                                                            .hover(|style| style.bg(rgb(SOFT_BLUE)))
                                                            .on_click(cx.listener(
                                                                move |this, _, _, cx| {
                                                                    this.error = this
                                                                        .desktop
                                                                        .as_ref()
                                                                        .and_then(|d| {
                                                                            d.select(
                                                                                g.clone(),
                                                                                n.clone(),
                                                                            )
                                                                            .err()
                                                                            .map(|e| e.to_string())
                                                                        });
                                                                    cx.notify();
                                                                },
                                                            ))
                                                    })
                                            },
                                        ));
                                        row.children(
                                            (pair.len()..columns).map(|_| div().flex_1().min_w_0()),
                                        )
                                    }));
                                if expandable && query.is_empty() {
                                    panel.child(
                                        candidates.with_animation(
                                            SharedString::from(format!("expand:{name}")),
                                            Animation::new(Duration::from_millis(150))
                                                .with_easing(|t| 1. - (1. - t).powi(3)),
                                            |view, delta| view.opacity(0.76 + 0.24 * delta),
                                        ),
                                    )
                                } else {
                                    panel.child(candidates)
                                }
                            }),
                    )
                })
                .collect::<Vec<_>>();
            let toolbar = div()
                .flex()
                .items_center()
                .gap_3()
                .child(
                    div()
                        .text_sm()
                        .text_color(rgb(FAINT))
                        .child(self.locale.message(Message::Filter)),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(Input::new(&self.search).cleanable(true)),
                )
                .child(chip(
                    format!("{count} {}", self.locale.message(Message::Groups)),
                    BLUE,
                    SOFT_BLUE,
                ));
            let mut node_counts = std::collections::HashMap::new();
            for node in &inventory.nodes {
                if let meow_paw::engine::controller::Origin::Provider(origin) = &node.origin {
                    *node_counts.entry(origin.as_str()).or_insert(0_usize) += 1;
                }
            }
            let providers = inventory
                .providers
                .iter()
                .map(|(name, status)| {
                    let label = name.clone();
                    let pending =
                        self.snapshot.refreshing_provider.as_deref() == Some(name.as_str());
                    let busy = self.snapshot.refreshing_provider.is_some();
                    let count = node_counts.get(name.as_str()).copied().unwrap_or(0);
                    let usage = status
                        .subscription_info
                        .as_ref()
                        .map(|usage| {
                            format!(
                                "↑ {}  ↓ {}  / {}",
                                bytes(usage.upload),
                                bytes(usage.download),
                                bytes(usage.total)
                            )
                        })
                        .unwrap_or_else(|| "No usage information".into());
                    let updated = status
                        .updated_at
                        .clone()
                        .unwrap_or_else(|| "Awaiting first update".into());
                    card()
                        .px_3()
                        .py_2()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap_3()
                        .child(
                            div()
                                .min_w_0()
                                .flex_1()
                                .child(
                                    div()
                                        .flex()
                                        .flex_wrap()
                                        .items_center()
                                        .gap_2()
                                        .child(
                                            div()
                                                .min_w_0()
                                                .font_weight(FontWeight::BOLD)
                                                .child(name.clone()),
                                        )
                                        .child(compact_chip(
                                            status.vehicle_type.clone(),
                                            BLUE,
                                            SOFT_BLUE,
                                        ))
                                        .child(compact_chip(
                                            format!(
                                                "{count} {}",
                                                self.locale.message(if count == 1 {
                                                    Message::Node
                                                } else {
                                                    Message::Nodes
                                                })
                                            ),
                                            FAINT,
                                            BG,
                                        )),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(rgb(FAINT))
                                        .child(format!("{updated} · {usage}")),
                                ),
                        )
                        .when(status.vehicle_type.eq_ignore_ascii_case("http"), |row| {
                            row.child(
                                secondary_button(
                                    SharedString::from(format!("provider-refresh:{label}")),
                                    if pending {
                                        "Refreshing…"
                                    } else if busy {
                                        "Refresh busy"
                                    } else {
                                        self.locale.message(Message::Refresh)
                                    },
                                )
                                .when(!busy, |button| {
                                    button.on_click(cx.listener(move |paw, _, _, cx| {
                                        paw.error = paw.desktop.as_ref().and_then(|desktop| {
                                            desktop
                                                .refresh_provider(label.clone())
                                                .err()
                                                .map(|e| e.to_string())
                                        });
                                        cx.notify();
                                    }))
                                }),
                            )
                        })
                })
                .collect::<Vec<_>>();
            let probe_settings = div()
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    div()
                        .text_sm()
                        .text_color(rgb(FAINT))
                        .child(self.locale.message(Message::LatencyTestDescription)),
                )
                .child(Input::new(&self.probe_url).cleanable(true))
                .child(div().text_xs().text_color(rgb(FAINT)).child(match self.locale {
                    Locale::En => "Comparison is opt-in and tests each eligible node once; unknown or ambiguous nodes are omitted. Node health probes may change an automatic route’s effective choice.",
                    Locale::ZhCn => "比较需手动启动，每个符合条件的节点测试一次；未知或有歧义的节点会被跳过。节点健康检测可能改变自动路由的实际选择。",
                }));
            let page = div()
                .flex()
                .flex_col()
                .gap_3()
                .child(toolbar)
                .child(probe_settings)
                .when(!providers.is_empty(), |page| {
                    page.child(
                        div()
                            .mt_2()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(self.locale.message(Message::Providers)),
                    )
                    .children(providers)
                });
            return if groups.is_empty() {
                page.child(empty_state(
                    self.locale.message(Message::NoMatchingGroups),
                    self.locale.message(Message::TryAnotherRoute),
                ))
            } else {
                page.children(groups)
            };
        }
        let _ = (window_width, cx);
        empty_state(
            self.locale.message(Message::NoProxyGroups),
            self.locale.message(Message::StartToViewGroups),
        )
    }

    fn connections(&self, cx: &mut Context<Self>) -> Div {
        #[cfg(any(target_os = "linux", windows))]
        if let Some(list) = self.snapshot.connections.as_ref() {
            let value = self.connection_filter.read(cx).value();
            let query = value.trim().to_lowercase();
            let results = list
                .iter()
                .filter_map(|c| {
                    let target = if c.metadata.host.is_empty() {
                        &c.metadata.destination_ip
                    } else {
                        &c.metadata.host
                    };
                    let destination = destination_label(target, &c.metadata.destination_port);
                    let route = c.chains.join(" / ");
                    if !(route_matches(&destination, &query)
                        || route_matches(&route, &query)
                        || route_matches(&c.rule, &query))
                    {
                        return None;
                    }
                    Some((c, destination, route))
                })
                .collect::<Vec<_>>();
            let toolbar = div()
                .flex()
                .items_center()
                .gap_3()
                .child(
                    div()
                        .text_sm()
                        .text_color(rgb(FAINT))
                        .child(self.locale.message(Message::Filter)),
                )
                .child(
                    div()
                        .flex_1()
                        .child(Input::new(&self.connection_filter).cleanable(true)),
                )
                .child(chip(
                    if query.is_empty() {
                        format!(
                            "{} {}",
                            list.len(),
                            self.locale.message(Message::ActiveConnections)
                        )
                    } else {
                        self.locale
                            .connection_match_count(results.len(), list.len())
                    },
                    BLUE,
                    SOFT_BLUE,
                ));
            let table = if results.is_empty() {
                let (title, detail) = connection_empty_messages(list.len());
                empty_state(self.locale.message(title), self.locale.message(detail))
            } else {
                card()
                    .p_0()
                    .child(
                        active_connection_row(
                            section_label(self.locale.message(Message::DestinationColumn)),
                            section_label(self.locale.message(Message::RouteColumn)),
                            section_label(self.locale.message(Message::RuleColumn)),
                            div(),
                        )
                        .bg(rgb(CHROME)),
                    )
                    .children(results.into_iter().map(|(c, destination, route)| {
                        let key = c.id.clone();
                        let close = div()
                            .id(SharedString::from(format!("connection-close:{key}")))
                            .px_2()
                            .py_1()
                            .rounded_md()
                            .cursor_pointer()
                            .text_xs()
                            .text_color(rgb(BLUE))
                            .hover(|s| s.bg(rgb(SOFT_BLUE)))
                            .child(self.locale.message(Message::CloseConnection))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.error = this
                                    .desktop
                                    .as_ref()
                                    .and_then(|d| d.close_connection(key.clone()).err())
                                    .map(|err| err.to_string());
                                cx.notify();
                            }));
                        active_connection_row(
                            div().font_weight(FontWeight::MEDIUM).child(destination),
                            div().child(if route.is_empty() {
                                "—".into()
                            } else {
                                route
                            }),
                            div().child(if c.rule.is_empty() {
                                "—".into()
                            } else {
                                c.rule.clone()
                            }),
                            close,
                        )
                    }))
            };
            return div().flex().flex_col().gap_3().child(toolbar).child(table);
        }
        let _ = cx;
        empty_state(
            self.locale.message(Message::NoActiveConnections),
            self.locale.message(Message::StartToViewConnections),
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
                    self.show_engine_path = false;
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

    fn settings(&self, wide: bool, cx: &mut Context<Self>) -> Div {
        let saved_language = self.preferences.language();
        let can_save_language = self.preferences.has_persistent_file();
        let language_name = |language| match language {
            LanguagePreference::System => self.locale.message(Message::LanguageSystem),
            LanguagePreference::En => "English",
            LanguagePreference::ZhCn => "简体中文",
        };
        let language = form_card(self.locale.message(Message::Language)).child(
            div()
                .mx_4()
                .py_3()
                .border_t_1()
                .border_color(rgb(LINE))
                .flex()
                .flex_col()
                .gap_2()
                .child(format!(
                    "{}: {}",
                    self.locale.message(Message::SavedLanguage),
                    language_name(saved_language)
                ))
                .child(format!(
                    "{}: {}",
                    self.locale.message(Message::CurrentLanguage),
                    match self.locale {
                        Locale::En => "English",
                        Locale::ZhCn => "简体中文",
                    }
                ))
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(FAINT))
                        .child(self.locale.message(Message::LanguageRestart)),
                )
                .when(!can_save_language, |el| {
                    el.child(
                        div()
                            .text_xs()
                            .text_color(rgb(FAINT))
                            .child(self.locale.message(Message::LanguageUnavailable)),
                    )
                })
                .when(self.language_save_failed, |el| {
                    el.child(
                        div()
                            .text_sm()
                            .text_color(rgb(0xb42318))
                            .child(self.locale.message(Message::LanguageSaveFailed)),
                    )
                })
                .children(
                    [
                        LanguagePreference::System,
                        LanguagePreference::En,
                        LanguagePreference::ZhCn,
                    ]
                    .into_iter()
                    .enumerate()
                    .map(|(index, choice)| {
                        let selected = choice == saved_language;
                        Button::new(("language-choice", index))
                            .label(language_name(choice))
                            .selected(selected)
                            .toggled(selected)
                            .disabled(!can_save_language)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if this.preferences.has_persistent_file()
                                    && this.preferences.language() != choice
                                {
                                    this.language_save_failed =
                                        this.preferences.set_language(choice).is_err();
                                }
                                cx.notify();
                            }))
                    }),
                ),
        );
        let binary = self.binary.as_ref().map_or_else(
            || self.locale.message(Message::NoExecutableSelected).into(),
            |p| p.display().to_string(),
        );
        let system = form_card(self.locale.message(Message::System))
            .child(form_row(
                self.locale.message(Message::SystemProxy),
                self.locale.message(Message::SystemProxyDescription),
                chip(self.locale.message(Message::Unmanaged), FAINT, BG),
            ))
            .child(form_row(
                self.locale.message(Message::TunMode),
                self.locale.message(Message::TunDescription),
                chip(self.locale.message(Message::Unavailable), FAINT, BG),
            ))
            .child(form_row(
                self.locale.message(Message::LocalListener),
                self.locale.message(Message::ListenerDescription),
                chip(self.locale.message(Message::Loopback), BLUE, SOFT_BLUE),
            ));
        let shown_path = binary.clone();
        let path_detail = div()
            .id("engine-path-preview")
            .min_w_0()
            .truncate()
            .child(shown_path.clone())
            .when(self.binary.is_some(), |detail| {
                detail.tooltip(move |window, cx| {
                    gpui_kit::component::tooltip::Tooltip::new(shown_path.clone()).build(window, cx)
                })
            });
        let engine = form_card(self.locale.message(Message::MeowEngine))
            .child(form_row_with_detail(
                self.locale.message(Message::MeowExecutable),
                path_detail,
                if self.can_change_binary() {
                    secondary_button("select-binary", self.locale.message(Message::Browse))
                        .on_click(cx.listener(|this, _, _, cx| this.choose_file(true, cx)))
                        .into_any_element()
                } else {
                    chip("Locked", FAINT, BG).into_any_element()
                },
            ))
            .when(self.binary.is_some(), |engine| {
                engine.child(
                    div()
                        .mx_4()
                        .pb_3()
                        .child(
                            secondary_button(
                                "engine-path-reveal",
                                if self.show_engine_path {
                                    self.locale.message(Message::HideFullPath)
                                } else {
                                    self.locale.message(Message::ShowFullPath)
                                },
                            )
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.show_engine_path = !this.show_engine_path;
                                cx.notify();
                            })),
                        )
                        .when(self.show_engine_path, |row| {
                            row.child(
                                div()
                                    .mt_2()
                                    .min_w_0()
                                    .text_xs()
                                    .text_color(rgb(FAINT))
                                    .child(binary.clone()),
                            )
                            .child(
                                secondary_button(
                                    "engine-path-copy",
                                    self.locale.message(Message::CopyPath),
                                )
                                .on_click(cx.listener(
                                    move |_, _, _, cx| {
                                        cx.write_to_clipboard(ClipboardItem::new_string(
                                            binary.clone(),
                                        ));
                                    },
                                )),
                            )
                        }),
                )
            });
        let app = form_card("Meow Paw")
            .child(form_row(
                self.locale.message(Message::TrayBehavior),
                self.locale.message(Message::TrayDescription),
                chip(self.locale.message(Message::KeepRunning), BLUE, SOFT_BLUE),
            ))
            .child(form_row(
                self.locale.message(Message::ApplicationVersion),
                std::env::consts::OS,
                chip(env!("CARGO_PKG_VERSION"), FAINT, BG),
            ))
            .child(form_row(
                self.locale.message(Message::QuitApplication),
                self.locale.message(Message::QuitDescription),
                secondary_button("quit-paw", self.locale.message(Message::QuitPaw))
                    .on_click(cx.listener(|_, _, _, cx| cx.quit())),
            ));
        div()
            .flex()
            .gap_3()
            .items_start()
            .when(!wide, |el| el.flex_col())
            .child(
                div()
                    .min_w_0()
                    .w_full()
                    .when(wide, |el| el.flex_1())
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(system)
                    .child(engine),
            )
            .child(
                div()
                    .min_w_0()
                    .w_full()
                    .when(wide, |el| el.flex_1())
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(language)
                    .child(app),
            )
    }
}

impl Render for Paw {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let compact = window.viewport_size().width < px(820.);
        let body = match self.page {
            Page::Overview => self.overview(window.viewport_size().width >= px(960.), cx),
            Page::Proxies => self.proxies(window.viewport_size().width, cx),
            Page::Profiles => self.profiles(cx),
            Page::Connections => self.connections(cx),
            Page::Rules => self.rules(cx),
            Page::Logs => self.logs(cx),
            Page::Unlock => empty_state(
                self.locale.message(Message::Unlock),
                self.locale.message(Message::UnlockUnavailableDetail),
            ),
            Page::Settings => self.settings(window.viewport_size().width >= px(960.), cx),
        };
        div()
            .size_full()
            .flex()
            .bg(rgb(BG))
            .text_color(rgb(INK))
            .child(self.sidebar(compact, cx))
            .child(
                div()
                    .flex_1()
                    .h_full()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .h(px(65.))
                            .flex_shrink_0()
                            .bg(rgb(CHROME))
                            .border_b_1()
                            .border_color(rgb(LINE))
                            .px_5()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap_2()
                            .child(
                                div()
                                    .min_w_0()
                                    .truncate()
                                    .text_xl()
                                    .font_weight(FontWeight::BOLD)
                                    .child(self.locale.message(self.page.message())),
                            )
                            .child(if self.page == Page::Proxies {
                                self.proxy_header(cx)
                            } else {
                                self.actions(cx)
                            }),
                    )
                    .child(
                        div()
                            .id(("paw-page", self.page as usize))
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scroll()
                            .p_4()
                            .when_some(self.issue(), |pane, issue| {
                                pane.child(
                                    div()
                                        .p_3()
                                        .mb_3()
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
                                        .mb_3()
                                        .rounded_md()
                                        .bg(rgb(0xfff4d5))
                                        .text_color(rgb(0x76571b))
                                        .child(warning.clone()),
                                )
                            })
                            .child(body),
                    ),
            )
    }
}

fn card() -> Div {
    div()
        .rounded_xl()
        .border_1()
        .border_color(rgb(LINE))
        .bg(rgb(SURFACE))
        .shadow_sm()
        .p_4()
}

fn chip(label: impl Into<SharedString>, foreground: u32, background: u32) -> Div {
    div()
        .flex_shrink_0()
        .rounded_md()
        .px_2()
        .py_1()
        .text_xs()
        .font_weight(FontWeight::MEDIUM)
        .text_color(rgb(foreground))
        .bg(rgb(background))
        .child(label.into())
}

fn compact_chip(label: impl Into<SharedString>, foreground: u32, background: u32) -> Div {
    chip(label, foreground, background).px_2().py(px(1.))
}

fn form_card(title: &'static str) -> Div {
    card().p_0().child(
        div()
            .px_4()
            .pt_4()
            .pb_2()
            .text_base()
            .font_weight(FontWeight::BOLD)
            .child(title),
    )
}

fn form_row(
    title: &'static str,
    detail: impl Into<SharedString>,
    control: impl IntoElement,
) -> Div {
    form_row_with_detail(title, detail.into(), control)
}

fn form_row_with_detail(
    title: &'static str,
    detail: impl IntoElement,
    control: impl IntoElement,
) -> Div {
    div()
        .mx_4()
        .min_h(px(60.))
        .py_3()
        .border_t_1()
        .border_color(rgb(LINE))
        .flex()
        .items_center()
        .justify_between()
        .gap_3()
        .child(
            div()
                .min_w_0()
                .flex_1()
                .child(div().text_sm().font_weight(FontWeight::MEDIUM).child(title))
                .child(div().mt_1().text_xs().text_color(rgb(FAINT)).child(detail)),
        )
        .child(div().flex_shrink_0().child(control))
}

fn sidebar_rate(icon: &'static str, label: &'static str, rate: String, color: u32) -> Div {
    div()
        .flex()
        .items_center()
        .gap_1()
        .py_1()
        .text_color(rgb(FAINT))
        .child(line_icon(icon, color, 13.))
        .child(label)
        .child(
            div()
                .flex_1()
                .text_right()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(INK))
                .child(rate),
        )
}

fn line_icon(path: &'static str, color: u32, side: f32) -> impl IntoElement {
    svg()
        .path(path)
        .size(px(side))
        .text_color(rgb(color))
        .flex_shrink_0()
}

fn secondary_button(id: impl Into<ElementId>, label: &'static str) -> Stateful<Div> {
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
        .child(paw_badge(42.))
        .child(
            div()
                .text_lg()
                .font_weight(FontWeight::SEMIBOLD)
                .child(title),
        )
        .child(div().text_sm().text_color(rgb(FAINT)).child(detail))
}

#[cfg(any(target_os = "linux", windows))]
fn mode_groups(
    inventory: &meow_paw::engine::controller::Inventory,
    mode: Option<meow_paw::engine::controller::RoutingMode>,
) -> Vec<(&String, &meow_paw::engine::controller::Proxy)> {
    use meow_paw::engine::controller::RoutingMode;
    let global = inventory.groups.get_key_value("GLOBAL");
    let mut visible: Vec<_> = inventory
        .groups
        .iter()
        .filter(|(name, group)| name.as_str() != "GLOBAL" && !group.hidden)
        .collect();
    if matches!(mode, Some(RoutingMode::Global | RoutingMode::Direct)) {
        visible.clear();
        if let Some((key, group)) = global {
            visible.push((key, group));
            if let Some(active) = group.now.as_ref()
                && active != key
                && let Some(selected) = inventory.groups.get_key_value(active)
                && !selected.1.hidden
            {
                visible.push(selected);
            }
        }
    } else if visible.is_empty() {
        visible.extend(global);
    }
    visible
}

#[cfg(any(target_os = "linux", windows))]
fn route_columns(width: Pixels) -> usize {
    if width > px(1920.) {
        5
    } else if width > px(1450.) {
        4
    } else if width > px(1024.) {
        3
    } else if width >= px(600.) {
        2
    } else {
        1
    }
}

#[cfg(any(target_os = "linux", windows))]
fn unique_route_members(names: &[String]) -> Vec<(usize, &str)> {
    let mut seen = std::collections::HashSet::new();
    names
        .iter()
        .map(String::as_str)
        .enumerate()
        .filter(|(_, name)| seen.insert(*name))
        .collect()
}

#[cfg(any(target_os = "linux", windows))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum ProxyOrder {
    #[default]
    Configured,
    Delay,
    Name,
}

#[cfg(any(target_os = "linux", windows))]
impl ProxyOrder {
    fn next(self) -> Self {
        match self {
            Self::Configured => Self::Delay,
            Self::Delay => Self::Name,
            Self::Name => Self::Configured,
        }
    }

    fn message(self) -> Message {
        match self {
            Self::Configured => Message::SortDefault,
            Self::Delay => Message::SortLatency,
            Self::Name => Message::SortName,
        }
    }
}

#[cfg(any(target_os = "linux", windows))]
fn unambiguous_route_delays(
    inventory: &meow_paw::engine::controller::Inventory,
) -> std::collections::HashMap<&str, Option<u16>> {
    let mut delays = std::collections::HashMap::new();
    let routes = inventory
        .groups
        .iter()
        .chain(inventory.nodes.iter().map(|node| (&node.name, &node.proxy)));
    for (name, proxy) in routes {
        delays
            .entry(name.as_str())
            .and_modify(|delay| *delay = None)
            .or_insert_with(|| proxy.latest_delay_ms());
    }
    delays
}

#[cfg(any(target_os = "linux", windows))]
fn sort_route_members(
    members: &mut [(usize, &str)],
    order: ProxyOrder,
    delays: &std::collections::HashMap<&str, Option<u16>>,
) {
    match order {
        ProxyOrder::Configured => {}
        ProxyOrder::Delay => members.sort_by_key(|(index, name)| {
            let delay = delays.get(name).copied().flatten();
            (delay.is_none(), delay, *index)
        }),
        ProxyOrder::Name => {
            members.sort_by_cached_key(|(index, name)| (name.to_lowercase(), *index))
        }
    }
}

#[cfg(any(target_os = "linux", windows))]
fn route_matches(name: &str, normalized_query: &str) -> bool {
    normalized_query.is_empty() || name.to_lowercase().contains(normalized_query)
}

#[cfg(any(target_os = "linux", windows))]
fn connection_empty_messages(total: usize) -> (Message, Message) {
    if total == 0 {
        (
            Message::NoActiveConnections,
            Message::ListenerConnectionsDetail,
        )
    } else {
        (
            Message::NoMatchingConnections,
            Message::ConnectionFilterDetail,
        )
    }
}

#[cfg(any(target_os = "linux", windows))]
fn active_connection_row(a: Div, b: Div, c: Div, action: impl IntoElement) -> Div {
    div()
        .flex()
        .gap_3()
        .items_center()
        .px_4()
        .py_3()
        .border_b_1()
        .border_color(rgb(LINE))
        .text_sm()
        .child(a.flex_1().min_w_0())
        .child(b.flex_1().min_w_0().text_color(rgb(FAINT)))
        .child(c.flex_1().min_w_0().text_color(rgb(FAINT)))
        .child(div().w(px(54.)).flex_shrink_0().child(action))
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

fn traffic_metric(label: &'static str, value: String, color: u32) -> Div {
    div()
        .flex_1()
        .min_w_0()
        .child(div().text_xs().text_color(rgb(FAINT)).child(label))
        .child(
            div()
                .text_base()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(color))
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
type TrafficBars = Option<(u64, Vec<Option<(f32, f32)>>)>;

#[cfg(any(target_os = "linux", windows))]
fn traffic_bars(samples: &[Option<meow_paw::app::TrafficRate>], height: f32) -> TrafficBars {
    let peak = samples.iter().flatten().map(|s| s.up.max(s.down)).max()?;
    let scale = |value: u64| {
        if peak == 0 || value == 0 {
            0.
        } else {
            (height * (value.min(peak) as f64 / peak as f64) as f32).max(1.)
        }
    };
    Some((
        peak,
        samples
            .iter()
            .map(|sample| sample.map(|s| (scale(s.up), scale(s.down))))
            .collect(),
    ))
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

fn paw_badge(size: f32) -> Div {
    div()
        .size(px(size))
        .flex_shrink_0()
        .child(img(crate::brand::MARK).size_full())
}

#[cfg(test)]
mod tests {
    use super::Page;

    #[test]
    fn subscription_url_preflight_matches_downloader_policy() {
        use super::valid_subscription_url;
        for url in [
            "",
            "http://example.invalid/?token=secret",
            "https://user:secret@example.invalid/",
            "https://example.invalid/#secret",
            "not a url",
        ] {
            assert!(!valid_subscription_url(url));
        }
        assert!(valid_subscription_url(
            " https://example.invalid/feed?token=secret "
        ));
    }

    #[test]
    #[cfg(any(target_os = "linux", windows))]
    fn traffic_chart_keeps_unknown_and_zero_samples_distinct() {
        use super::traffic_bars;
        use meow_paw::app::TrafficRate;

        assert_eq!(traffic_bars(&[], 28.), None);
        assert_eq!(traffic_bars(&[None, None], 28.), None);
        assert_eq!(
            traffic_bars(&[None, Some(TrafficRate { up: 0, down: 0 })], 28.),
            Some((0, vec![None, Some((0., 0.))]))
        );
        assert_eq!(
            traffic_bars(
                &[
                    Some(TrafficRate { up: 5, down: 10 }),
                    None,
                    Some(TrafficRate { up: 0, down: 1 })
                ],
                60.,
            ),
            Some((10, vec![Some((30., 60.)), None, Some((0., 6.))]))
        );
    }

    #[test]
    #[cfg(any(target_os = "linux", windows))]
    fn connection_empty_state_distinguishes_no_traffic_from_no_matches() {
        use super::{Message, connection_empty_messages};
        assert_eq!(
            connection_empty_messages(0),
            (
                Message::NoActiveConnections,
                Message::ListenerConnectionsDetail
            )
        );
        assert_eq!(
            connection_empty_messages(3),
            (
                Message::NoMatchingConnections,
                Message::ConnectionFilterDetail
            )
        );
    }

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
    #[cfg(any(target_os = "linux", windows))]
    fn duplicate_candidates_keep_first_index_and_exact_identity() {
        use super::unique_route_members;

        let names = ["DIRECT", "南京", "DIRECT", "direct", "南京", "Reading"].map(str::to_owned);
        assert_eq!(
            unique_route_members(&names),
            [(0, "DIRECT"), (1, "南京"), (3, "direct"), (5, "Reading")]
        );
        assert!(unique_route_members(&[]).is_empty());
    }

    #[test]
    #[cfg(any(target_os = "linux", windows))]
    fn proxy_order_keeps_stable_route_ids_and_does_not_guess_duplicate_delays() {
        use super::{
            ProxyOrder, sort_route_members, unambiguous_route_delays, unique_route_members,
        };
        use meow_paw::engine::controller::{Inventory, Node, Origin, Proxy};
        let proxy = |samples: &[u16]| -> Proxy {
            let history: Vec<_> = samples
                .iter()
                .map(|ms| serde_json::json!({"delay":ms}))
                .collect();
            serde_json::from_value(serde_json::json!({"history": history})).unwrap()
        };
        let nodes = [
            ("Zulu", Origin::Local, proxy(&[5, 20])),
            ("alpha", Origin::Provider("feed".into()), proxy(&[10])),
            ("Bravo", Origin::Local, proxy(&[1, 0])),
            ("zulu", Origin::Local, proxy(&[20])),
            ("Ambiguous", Origin::Local, proxy(&[1])),
            ("Ambiguous", Origin::Provider("feed".into()), proxy(&[2])),
        ];
        let inventory = Inventory {
            nodes: nodes
                .into_iter()
                .map(|(name, origin, proxy)| Node {
                    name: name.into(),
                    origin,
                    proxy,
                })
                .collect(),
            ..Default::default()
        };
        let delays = unambiguous_route_delays(&inventory);
        assert_eq!(delays["Zulu"], Some(20));
        assert_eq!(delays["Bravo"], None);
        assert_eq!(delays["Ambiguous"], None);
        let names = ["Zulu", "Bravo", "alpha", "zulu", "Ambiguous", "Unknown"].map(str::to_owned);
        let order = |mode| {
            let mut visible = unique_route_members(&names);
            sort_route_members(&mut visible, mode, &delays);
            visible
        };
        let default = ProxyOrder::default();
        assert_eq!(
            order(default),
            [
                (0, "Zulu"),
                (1, "Bravo"),
                (2, "alpha"),
                (3, "zulu"),
                (4, "Ambiguous"),
                (5, "Unknown")
            ]
        );
        assert_eq!(
            order(default.next()),
            [
                (2, "alpha"),
                (0, "Zulu"),
                (3, "zulu"),
                (1, "Bravo"),
                (4, "Ambiguous"),
                (5, "Unknown")
            ]
        );
        assert_eq!(
            order(default.next().next()),
            [
                (2, "alpha"),
                (4, "Ambiguous"),
                (1, "Bravo"),
                (5, "Unknown"),
                (0, "Zulu"),
                (3, "zulu")
            ]
        );
        assert_eq!(default.next().next().next(), default);
    }

    #[test]
    #[cfg(any(target_os = "linux", windows))]
    fn route_grid_adapts_at_desktop_breakpoints() {
        for (width, columns) in [
            (599., 1),
            (600., 2),
            (1024., 2),
            (1025., 3),
            (1450., 3),
            (1451., 4),
            (1920., 4),
            (1921., 5),
        ] {
            assert_eq!(super::route_columns(super::px(width)), columns);
        }
    }

    #[test]
    #[cfg(any(target_os = "linux", windows))]
    fn proxy_groups_follow_the_current_mode() {
        use meow_paw::engine::controller::{Inventory, Proxy, RoutingMode};
        let mut inventory = Inventory::default();
        inventory.groups.insert("Default".into(), Proxy::default());
        inventory.groups.insert("Roaming".into(), Proxy::default());
        inventory.groups.insert(
            "GLOBAL".into(),
            Proxy {
                now: Some("Roaming".into()),
                ..Default::default()
            },
        );
        let names = |mode| {
            super::mode_groups(&inventory, Some(mode))
                .into_iter()
                .map(|(name, _)| name.as_str())
                .collect::<Vec<_>>()
        };
        assert_eq!(names(RoutingMode::Rule), ["Default", "Roaming"]);
        assert_eq!(names(RoutingMode::Global), ["GLOBAL", "Roaming"]);
    }

    #[test]
    fn every_nav_and_workspace_icon_is_embedded() {
        use gpui_kit::AssetSource;

        let assets = crate::brand::PawAssets;
        let paths = Page::ALL.into_iter().map(Page::icon).chain([
            "icons/arrow-up-right.svg",
            "icons/arrow-down-left.svg",
            "icons/chevron-down.svg",
            "icons/chevron-right.svg",
            "icons/circle-check.svg",
        ]);
        for path in paths {
            assert!(assets.load(path).unwrap().is_some(), "missing SVG: {path}");
        }
    }

    #[test]
    fn startup_pages_match_the_nav() {
        assert_eq!(Page::ALL.len(), 8);
        assert!(Page::from_name("Overview") == Some(Page::Overview));
        for expected in Page::ALL {
            assert!(Page::from_name(expected.cli_name()) == Some(expected));
        }
        assert!(Page::from_name("首页").is_none());
        assert!(Page::from_name("not-a-page").is_none());
    }
}
