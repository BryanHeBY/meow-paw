use crate::preferences::{LanguagePreference, Preferences};
use gpui_kit::base::{Button as SemanticButton, Disableable, Selectable};
#[cfg(any(target_os = "linux", windows))]
use gpui_kit::component::Sizable;
#[cfg(any(target_os = "linux", windows))]
use gpui_kit::component::WindowExt;
use gpui_kit::component::button::{Button, ButtonCustomVariant, ButtonVariants};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use meow_paw::i18n::{Locale, Message};
use meow_paw::subscription;
use meow_paw::subscription_store::{
    DisplayInterval, RefreshPolicy, SubscriptionId, SubscriptionMetadata, SubscriptionStore,
};
use std::collections::HashMap;
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

#[cfg(any(target_os = "linux", windows))]
fn page_filter_rendered(
    page: Page,
    snapshot: &meow_paw::app::Snapshot,
    rule_sets_open: bool,
    closed_connections_open: bool,
) -> bool {
    use meow_paw::app::EngineStatus;
    let running = matches!(snapshot.engine, EngineStatus::Running { .. });
    match page {
        Page::Profiles => true,
        Page::Proxies => snapshot.inventory.is_some(),
        Page::Rules => running && !rule_sets_open && snapshot.rules.is_some(),
        Page::Connections => running && (closed_connections_open || snapshot.connections.is_some()),
        Page::Logs => snapshot.logs.is_some(),
        _ => false,
    }
}

// Provider metadata is controller-supplied. Avoid displaying URL-like values or
// control characters even if they appear in an otherwise displayable field.
#[cfg(any(target_os = "linux", windows))]
fn safe_rule_set_text(value: &str, hidden: &str) -> String {
    if value.is_empty()
        || value.chars().count() > 160
        || value
            .chars()
            .any(|c| c.is_control() || matches!(c, '/' | '\\' | '?' | '#' | '@'))
    {
        hidden.to_owned()
    } else {
        value.to_owned()
    }
}

fn subscription_download_feedback(error: subscription::DownloadError) -> Message {
    use subscription::DownloadError;
    match error {
        DownloadError::InvalidUrl => Message::InvalidSubscriptionUrl,
        DownloadError::Request => Message::SubscriptionDownloadFailed,
        DownloadError::TooLarge => Message::SubscriptionTooLarge,
        DownloadError::InvalidText => Message::SubscriptionInvalidText,
        DownloadError::NoProxies => Message::SubscriptionNoProxies,
        DownloadError::InvalidProfile(_) => Message::SubscriptionInvalidProfile,
    }
}

fn subscription_write_feedback(error: std::io::Error, fallback: Message) -> Message {
    if error.kind() == std::io::ErrorKind::WouldBlock {
        Message::SubscriptionWriteConflict
    } else {
        fallback
    }
}

#[derive(Debug, PartialEq, Eq)]
enum ImportEditorEffect {
    Clear,
    Failure(Message),
    Unchanged,
}

// A result may only change the editor that launched it.
fn import_editor_effect(
    start_revision: u64,
    current_revision: u64,
    entry_open: bool,
    failure: Option<Message>,
) -> ImportEditorEffect {
    if !entry_open || start_revision != current_revision {
        ImportEditorEffect::Unchanged
    } else if let Some(message) = failure {
        ImportEditorEffect::Failure(message)
    } else {
        ImportEditorEffect::Clear
    }
}

// Mirrors the downloader policy; the downloader validates again before a request.
fn valid_subscription_url(input: &str) -> bool {
    url::Url::parse(input.trim()).is_ok_and(|url| {
        url.scheme() == "https"
            && url.has_host()
            && url.username().is_empty()
            && url.password().is_none()
            && url.fragment().is_none()
    })
}

fn subscription_name_valid(input: &str) -> bool {
    input.chars().count() <= 80
}

fn subscription_form_status(
    busy: bool,
    pending: Message,
    notice: Option<Message>,
) -> Option<Message> {
    if busy { Some(pending) } else { notice }
}

#[cfg(test)]
#[path = "view/profile_form_tests.rs"]
mod profile_form_tests;

fn subscription_paths(dir: PathBuf) -> std::io::Result<Vec<PathBuf>> {
    let store = SubscriptionStore::new(dir);
    Ok(store
        .ids()?
        .into_iter()
        .map(|id| store.record_path(id))
        .collect())
}

const METADATA_BATCH_SIZE: usize = 8;

// A manual result, including a failed metadata read, wins over an older batch.
fn merge_subscription_metadata(
    current: &mut HashMap<PathBuf, Option<SubscriptionMetadata>>,
    batch: Vec<(PathBuf, Option<SubscriptionMetadata>)>,
) {
    for (path, metadata) in batch {
        current.entry(path).or_insert(metadata);
    }
}

fn subscription_display_name(
    locale: Locale,
    id: SubscriptionId,
    metadata: Option<&SubscriptionMetadata>,
) -> String {
    metadata
        .and_then(|metadata| metadata.name.clone())
        .unwrap_or_else(|| {
            format!(
                "{} {}",
                locale.message(Message::Subscription),
                &id.to_string()[..8]
            )
        })
}

// Only the card title is searchable; callers supply no paths or metadata fields.
fn profile_name_matches(name: &str, query: &str) -> bool {
    name.to_lowercase().contains(&query.trim().to_lowercase())
}

#[derive(Debug, PartialEq, Eq)]
enum ProfileListState {
    Empty,
    NoMatches,
    Matches,
}

fn profile_list_state(total: usize, matches: usize) -> ProfileListState {
    if total == 0 {
        ProfileListState::Empty
    } else if matches == 0 {
        ProfileListState::NoMatches
    } else {
        ProfileListState::Matches
    }
}

fn profile_card_visible(matches: bool, editing: bool) -> bool {
    matches || editing
}

fn local_profile_display_name(path: &std::path::Path) -> String {
    path.file_name()
        .unwrap_or(path.as_os_str())
        .to_string_lossy()
        .into_owned()
}

#[cfg(any(target_os = "linux", windows))]
#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum LogLevelFilter {
    #[default]
    All,
    Debug,
    Info,
    Warn,
    Error,
}

#[cfg(any(target_os = "linux", windows))]
impl LogLevelFilter {
    const ALL: [Self; 5] = [Self::All, Self::Debug, Self::Info, Self::Warn, Self::Error];

    fn label(self) -> Message {
        match self {
            Self::All => Message::LogLevelAll,
            Self::Debug => Message::LogLevelDebug,
            Self::Info => Message::LogLevelInfo,
            Self::Warn => Message::LogLevelWarn,
            Self::Error => Message::LogLevelError,
        }
    }

    fn matches(self, level: &str) -> bool {
        match self {
            Self::All => true,
            Self::Debug => level.eq_ignore_ascii_case("debug"),
            Self::Info => level.eq_ignore_ascii_case("info"),
            Self::Warn => {
                level.eq_ignore_ascii_case("warn") || level.eq_ignore_ascii_case("warning")
            }
            Self::Error => level.eq_ignore_ascii_case("error") || level.eq_ignore_ascii_case("err"),
        }
    }
}

#[cfg(any(target_os = "linux", windows))]
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
enum LogOrder {
    #[default]
    NewestFirst,
    OldestFirst,
}

#[cfg(any(target_os = "linux", windows))]
impl LogOrder {
    fn toggle(&mut self) {
        *self = match self {
            Self::NewestFirst => Self::OldestFirst,
            Self::OldestFirst => Self::NewestFirst,
        };
    }

    fn label(self) -> Message {
        match self {
            Self::NewestFirst => Message::LogNewestFirst,
            Self::OldestFirst => Message::LogOldestFirst,
        }
    }
}

#[cfg(any(target_os = "linux", windows))]
#[derive(Default)]
struct LogReview {
    paused: Option<Vec<meow_paw::engine::controller::LogEntry>>,
    order: LogOrder,
}

#[cfg(any(target_os = "linux", windows))]
impl LogReview {
    fn toggle_pause(&mut self, live: impl FnOnce() -> Vec<meow_paw::engine::controller::LogEntry>) {
        if self.paused.is_some() {
            self.paused = None;
        } else {
            self.paused = Some(live());
        }
    }

    fn entries(
        &self,
        live: impl FnOnce() -> Vec<meow_paw::engine::controller::LogEntry>,
    ) -> Vec<meow_paw::engine::controller::LogEntry> {
        self.paused.clone().unwrap_or_else(live)
    }

    fn reset_pause(&mut self) {
        self.paused = None;
    }

    fn visible<'a>(
        &self,
        entries: &'a [meow_paw::engine::controller::LogEntry],
        query: &str,
        level: LogLevelFilter,
    ) -> Vec<&'a meow_paw::engine::controller::LogEntry> {
        let mut visible = filter_logs(entries, query, level);
        if self.order == LogOrder::OldestFirst {
            visible.reverse();
        }
        visible
    }
}

#[cfg(any(target_os = "linux", windows))]
fn log_review_context_changed(
    old: &meow_paw::app::Snapshot,
    new: &meow_paw::app::Snapshot,
) -> bool {
    old.probe_epoch != new.probe_epoch
        || old.profile != new.profile
        || new.applying_profile.is_some()
        || !matches!(new.engine, meow_paw::app::EngineStatus::Running { .. })
        || new.logs.is_none()
}

#[cfg(any(target_os = "linux", windows))]
fn filter_logs<'a>(
    entries: &'a [meow_paw::engine::controller::LogEntry],
    query: &str,
    level: LogLevelFilter,
) -> Vec<&'a meow_paw::engine::controller::LogEntry> {
    let query = query.trim().to_lowercase();
    entries
        .iter()
        .filter(|entry| {
            level.matches(&entry.level)
                && (query.is_empty()
                    || format!("{} {} {}", entry.time, entry.level, entry.message)
                        .to_lowercase()
                        .contains(&query))
        })
        .collect()
}

#[cfg(any(target_os = "linux", windows))]
#[derive(Clone)]
enum ConnectionDetailSelection {
    Active {
        sample: std::sync::Arc<Vec<meow_paw::engine::controller::Connection>>,
        index: usize,
    },
    Closed(meow_paw::app::ClosedConnectionKey),
}

pub struct Paw {
    #[cfg(any(target_os = "linux", windows))]
    _page_filter_shortcut: Subscription,
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
    subscription_metadata: HashMap<PathBuf, Option<SubscriptionMetadata>>,
    subscription_metadata_started: bool,
    subscription_busy: bool,
    subscription_busy_message: Message,
    subscription_rename: Option<PathBuf>,
    subscription_name: Entity<InputState>,
    subscription_notice: Option<Message>,
    subscription_entry_open: bool,
    subscription_url_revision: u64,
    subscription_window: AnyWindowHandle,
    subscription_url: Option<Entity<InputState>>,
    subscription_add_focus: FocusHandle,
    _subscription_url_changes: Subscription,
    preference_warning: Option<String>,
    language_save_failed: bool,
    motion_save_failed: bool,
    language_choice_focus: [FocusHandle; 3],
    engine_browse_focus: FocusHandle,
    engine_reveal_focus: FocusHandle,
    engine_copy_focus: FocusHandle,
    search: Entity<InputState>,
    _search_changes: Subscription,
    profile_filter: Entity<InputState>,
    _profile_filter_changes: Subscription,
    rule_filter: Entity<InputState>,
    _rule_filter_changes: Subscription,
    rule_page: usize,
    rule_sets_open: bool,
    rule_sets_read_failed: bool,
    rules_tab_focus: FocusHandle,
    rule_sets_tab_focus: FocusHandle,
    rule_sets_read_focus: FocusHandle,
    rule_previous_focus: FocusHandle,
    rule_next_focus: FocusHandle,
    closed_connections_open: bool,
    #[cfg(any(target_os = "linux", windows))]
    connection_detail: Option<ConnectionDetailSelection>,
    connection_detail_focus: FocusHandle,
    connection_detail_return: Option<ConnectionDetailSelection>,
    active_connections_tab_focus: FocusHandle,
    closed_connections_tab_focus: FocusHandle,
    connection_filter: Entity<InputState>,
    _connection_filter_changes: Subscription,
    log_filter: Entity<InputState>,
    _log_filter_changes: Subscription,
    #[cfg(any(target_os = "linux", windows))]
    log_level: LogLevelFilter,
    #[cfg(any(target_os = "linux", windows))]
    log_review: LogReview,
    log_pause_focus: FocusHandle,
    log_order_focus: FocusHandle,
    probe_url: Entity<InputState>,
    _probe_url_changes: Subscription,
    #[cfg(any(target_os = "linux", windows))]
    desktop: Option<meow_paw::app::Desktop>,
    #[cfg(any(target_os = "linux", windows))]
    expanded_groups: std::collections::HashSet<String>,
    #[cfg(any(target_os = "linux", windows))]
    proxy_order: std::collections::HashMap<String, ProxyOrder>,
    #[cfg(any(target_os = "linux", windows))]
    proxy_group_scroll: ScrollHandle,
    #[cfg(any(target_os = "linux", windows))]
    proxy_providers_open: bool,
    #[cfg(any(target_os = "linux", windows))]
    proxy_probe_settings_open: bool,
    #[cfg(any(target_os = "linux", windows))]
    locate_route: Option<(String, String)>,
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

fn subscribe_rule_filter(input: &Entity<InputState>, cx: &mut Context<Paw>) -> Subscription {
    cx.subscribe(input, |paw: &mut Paw, _, event, cx| {
        if matches!(event, InputEvent::Change) {
            paw.rule_page = 0;
            cx.notify();
        }
    })
}

fn subscribe_subscription_url(input: &Entity<InputState>, cx: &mut Context<Paw>) -> Subscription {
    cx.subscribe(input, |paw: &mut Paw, _, event, cx| {
        if matches!(event, InputEvent::Change) {
            paw.subscription_url_revision = paw.subscription_url_revision.wrapping_add(1);
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

#[cfg(any(target_os = "linux", windows))]
fn register_page_filter_shortcut(
    paw: WeakEntity<Paw>,
    window: &Window,
    cx: &mut App,
) -> Subscription {
    let window_id = window.window_handle().window_id();
    // A Div action cannot receive a key when no element in the window
    // has focus. Scope this interceptor to the originating window and
    // keep it alive only as long as this view.
    cx.intercept_keystrokes(move |event, window, cx| {
        if window.window_handle().window_id() == window_id
            && event.keystroke.key == "f"
            && event.keystroke.modifiers
                == (Modifiers {
                    control: true,
                    ..Modifiers::default()
                })
            && paw
                .update(cx, |paw, cx| paw.focus_page_filter(window, cx))
                .unwrap_or(false)
        {
            cx.stop_propagation();
        }
    })
}

/// Deterministic visual-test data; no controller, profile, tray or external requests.
#[cfg(feature = "visual-test")]
pub fn seed_proxy_fixture(paw: &Entity<Paw>, cx: &mut App) {
    #[cfg(any(target_os = "linux", windows))]
    {
        use meow_paw::engine::controller::{Inventory, ProviderStatus, Proxy, RoutingMode};
        let group = |kind: &str, selected: &str, members: &[&str]| Proxy {
            kind: kind.into(),
            now: Some(selected.into()),
            all: Some(members.iter().map(|name| (*name).into()).collect()),
            ..Proxy::default()
        };
        let (auto, daily, work, hk, jp, provider) = match paw.read(cx).locale {
            Locale::En => (
                "Auto Select",
                "Daily Proxy",
                "Work",
                "Hong Kong 01",
                "Tokyo 02",
                "Sample Provider",
            ),
            Locale::ZhCn => (
                "自动选择",
                "日常代理",
                "工作连接",
                "香港 01",
                "东京 02",
                "示例供应商",
            ),
        };
        let inventory = Inventory {
            groups: [
                (
                    auto.into(),
                    group("URLTest", hk, &[hk, jp, "Singapore East"]),
                ),
                (
                    daily.into(),
                    group("Selector", jp, &[auto, hk, jp, "Singapore East", "DIRECT"]),
                ),
                (work.into(), group("Selector", "DIRECT", &["DIRECT", jp])),
            ]
            .into(),
            providers: [(
                provider.into(),
                ProviderStatus {
                    vehicle_type: "HTTP".into(),
                    updated_at: None,
                    subscription_info: None,
                },
            )]
            .into(),
            ..Inventory::default()
        };
        paw.update(cx, |paw, cx| {
            paw.snapshot.mode = Some(RoutingMode::Rule);
            paw.snapshot.inventory = Some(std::sync::Arc::new(inventory));
            paw.expanded_groups.insert(daily.into());
            cx.notify();
        });
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    let _ = (paw, cx);
}

pub fn rebind_window(paw: &Entity<Paw>, window: &mut Window, cx: &mut App) {
    let query = paw.read(cx).search.read(cx).value();
    let search = cx.new(|cx| InputState::new(window, cx).default_value(query));
    let query = paw.read(cx).profile_filter.read(cx).value();
    let profile_filter = cx.new(|cx| InputState::new(window, cx).default_value(query));
    let query = paw.read(cx).rule_filter.read(cx).value();
    let rule_filter = cx.new(|cx| InputState::new(window, cx).default_value(query));
    let query = paw.read(cx).connection_filter.read(cx).value();
    let connection_filter = cx.new(|cx| InputState::new(window, cx).default_value(query));
    let query = paw.read(cx).log_filter.read(cx).value();
    let log_filter = cx.new(|cx| InputState::new(window, cx).default_value(query));
    let query = paw.read(cx).probe_url.read(cx).value();
    let probe_url = cx.new(|cx| InputState::new(window, cx).default_value(query));
    let query = paw.read(cx).subscription_name.read(cx).value();
    let subscription_name = cx.new(|cx| InputState::new(window, cx).default_value(query));
    let query = paw
        .read(cx)
        .subscription_url
        .as_ref()
        .map(|input| input.read(cx).value())
        .unwrap_or_default();
    let subscription_url = cx.new(|cx| InputState::new(window, cx).default_value(query));
    paw.update(cx, |state, cx| {
        #[cfg(any(target_os = "linux", windows))]
        {
            state._page_filter_shortcut =
                register_page_filter_shortcut(cx.weak_entity(), window, cx);
        }
        state.subscription_window = window.window_handle();
        state._subscription_url_changes = subscribe_subscription_url(&subscription_url, cx);
        state.subscription_url = Some(subscription_url);
        state.subscription_name = subscription_name;
        state._log_filter_changes = subscribe_search(&log_filter, cx);
        state.log_filter = log_filter;
        state._probe_url_changes = subscribe_probe_url(&probe_url, cx);
        state.probe_url = probe_url;
        state._search_changes = subscribe_search(&search, cx);
        state.search = search;
        state._profile_filter_changes = subscribe_search(&profile_filter, cx);
        state.profile_filter = profile_filter;
        state._rule_filter_changes = subscribe_rule_filter(&rule_filter, cx);
        state.rule_filter = rule_filter;
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
    let profile_filter = cx.new(|cx| InputState::new(window, cx));
    let rule_filter = cx.new(|cx| InputState::new(window, cx));
    let connection_filter = cx.new(|cx| InputState::new(window, cx));
    let log_filter = cx.new(|cx| InputState::new(window, cx));
    let probe_url = cx.new(|cx| InputState::new(window, cx));
    let subscription_url = cx.new(|cx| InputState::new(window, cx));
    let subscription_name = cx.new(|cx| InputState::new(window, cx));
    cx.new(|cx| {
        let subscription_url_changes = subscribe_subscription_url(&subscription_url, cx);
        let log_filter_changes = subscribe_search(&log_filter, cx);
        let probe_url_changes = subscribe_probe_url(&probe_url, cx);
        let search_changes = subscribe_search(&search, cx);
        let profile_filter_changes = subscribe_search(&profile_filter, cx);
        let rule_filter_changes = subscribe_rule_filter(&rule_filter, cx);
        let connection_filter_changes = subscribe_search(&connection_filter, cx);
        let (subscription_paths, subscription_notice) = preferences
            .subscription_dir()
            .map(subscription_paths)
            .map_or((Vec::new(), None), |result| match result {
                Ok(paths) => (paths, None),
                Err(_) => (Vec::new(), Some(Message::ListSubscriptionsFailed)),
            });
        let mut paw = Paw {
            #[cfg(any(target_os = "linux", windows))]
            _page_filter_shortcut: register_page_filter_shortcut(cx.weak_entity(), window, cx),
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
            #[cfg(any(target_os = "linux", windows))]
            proxy_group_scroll: ScrollHandle::new(),
            #[cfg(any(target_os = "linux", windows))]
            proxy_providers_open: false,
            #[cfg(any(target_os = "linux", windows))]
            proxy_probe_settings_open: false,
            #[cfg(any(target_os = "linux", windows))]
            locate_route: None,
            error: None,
            copied_listener: None,
            show_engine_path: false,
            show_usage: false,
            preferences,
            subscription_paths,
            subscription_metadata: HashMap::new(),
            subscription_metadata_started: false,
            subscription_busy: false,
            subscription_busy_message: Message::DownloadingSubscription,
            subscription_rename: None,
            subscription_name,
            subscription_notice,
            subscription_entry_open: false,
            subscription_url_revision: 0,
            subscription_window: window.window_handle(),
            subscription_url: Some(subscription_url),
            subscription_add_focus: cx.focus_handle(),
            _subscription_url_changes: subscription_url_changes,
            preference_warning: warning.map(str::to_owned),
            language_save_failed: false,
            motion_save_failed: false,
            language_choice_focus: std::array::from_fn(|_| cx.focus_handle()),
            engine_browse_focus: cx.focus_handle(),
            engine_reveal_focus: cx.focus_handle(),
            engine_copy_focus: cx.focus_handle(),
            search,
            _search_changes: search_changes,
            profile_filter,
            _profile_filter_changes: profile_filter_changes,
            rule_filter,
            _rule_filter_changes: rule_filter_changes,
            rule_page: 0,
            rule_sets_open: false,
            rule_sets_read_failed: false,
            rules_tab_focus: cx.focus_handle(),
            rule_sets_tab_focus: cx.focus_handle(),
            rule_sets_read_focus: cx.focus_handle(),
            rule_previous_focus: cx.focus_handle(),
            rule_next_focus: cx.focus_handle(),
            closed_connections_open: false,
            #[cfg(any(target_os = "linux", windows))]
            connection_detail: None,
            connection_detail_focus: cx.focus_handle(),
            connection_detail_return: None,
            active_connections_tab_focus: cx.focus_handle(),
            closed_connections_tab_focus: cx.focus_handle(),
            connection_filter,
            _connection_filter_changes: connection_filter_changes,
            log_filter,
            _log_filter_changes: log_filter_changes,
            #[cfg(any(target_os = "linux", windows))]
            log_level: LogLevelFilter::All,
            #[cfg(any(target_os = "linux", windows))]
            log_review: LogReview::default(),
            log_pause_focus: cx.focus_handle(),
            log_order_focus: cx.focus_handle(),
            probe_url,
            _probe_url_changes: probe_url_changes,
            #[cfg(any(target_os = "linux", windows))]
            snapshot: Default::default(),
        };
        paw.load_subscription_metadata(cx);
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
                if !matches!(&snap.engine, meow_paw::app::EngineStatus::Running { .. }) {
                    self.rule_sets_read_failed = false;
                }
                if self.snapshot.probe_epoch != snap.probe_epoch
                    || self.snapshot.profile != snap.profile
                    || snap.applying_profile.is_some()
                    || !matches!(snap.engine, meow_paw::app::EngineStatus::Running { .. })
                {
                    self.connection_detail = None;
                    self.connection_detail_return = None;
                }
                if log_review_context_changed(&self.snapshot, &snap) {
                    self.log_review.reset_pause();
                }
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
    ) -> Vec<SemanticButton> {
        Page::ALL
            .into_iter()
            .enumerate()
            .map(|(index, page)| {
                let on = active == page;
                let label = locale.message(page.message());
                SemanticButton::new(("paw-nav", index))
                    .accessibility_label(label)
                    .flex()
                    .items_center()
                    .justify_start()
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
                    .focus_visible(|s| s.border_2().border_color(rgb(BLUE)))
                    .on_click(cx.listener(move |this, event, window, cx| {
                        if matches!(event, ClickEvent::Keyboard(_)) {
                            cx.stop_propagation();
                        }
                        this.page = page;
                        if page != Page::Profiles
                            && !this.subscription_busy
                            && this.subscription_rename.is_some()
                        {
                            this.cancel_rename(window, cx);
                        }
                        if page == Page::Profiles {
                            this.load_subscription_metadata(cx);
                        }
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
        let label = self.locale.message(if running {
            Message::StopSession
        } else if phase == "Stopping" {
            Message::Stopping
        } else if supported && !ready {
            Message::ChooseEngine
        } else if supported && self.profile.is_none() {
            Message::ChooseProfile
        } else {
            Message::StartSession
        });
        div().child(
            SemanticButton::new("session-control")
                .disabled(!enabled)
                .accessibility_label(label)
                .rounded_md()
                .px_4()
                .py_2()
                .bg(rgb(if enabled { BLUE } else { LINE }))
                .text_color(rgb(if enabled { 0xffffff } else { FAINT }))
                .font_weight(FontWeight::SEMIBOLD)
                .child(label)
                .when(enabled, |button| {
                    button
                        .cursor_pointer()
                        .hover(|style| style.bg(rgb(0x1149ae)))
                        .focus_visible(|style| style.border_2().border_color(rgb(0x9eb9ee)))
                        .on_click(cx.listener(move |this, event, _, cx| {
                            if matches!(event, ClickEvent::Keyboard(_)) {
                                cx.stop_propagation();
                            }
                            if running {
                                this.stop();
                            } else if !ready {
                                this.page = Page::Settings;
                            } else if this.profile.is_none() {
                                this.page = Page::Profiles;
                                this.load_subscription_metadata(cx);
                            } else {
                                this.start();
                            }
                            cx.notify();
                        }))
                }),
        )
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
                                let label = self.locale.message(match mode {
                                    RoutingMode::Rule => Message::ModeRule,
                                    RoutingMode::Global => Message::ModeGlobal,
                                    RoutingMode::Direct => Message::ModeDirect,
                                });
                                let accessible_label = if selected {
                                    format!(
                                        "{} · {}",
                                        label,
                                        self.locale.message(Message::ActiveTab)
                                    )
                                } else {
                                    label.to_owned()
                                };
                                SemanticButton::new(("routing-mode", index))
                                    .accessibility_label(accessible_label)
                                    .aria_toggled(if selected {
                                        gpui_kit::accesskit::Toggled::True
                                    } else {
                                        gpui_kit::accesskit::Toggled::False
                                    })
                                    .px_4()
                                    .py_2()
                                    .text_sm()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(rgb(if selected { SURFACE } else { BLUE }))
                                    .bg(rgb(if selected { BLUE } else { SURFACE }))
                                    .when(index != 0, |el| {
                                        el.border_l_1().border_color(rgb(0x9eb9ee))
                                    })
                                    .focus_visible(|s| {
                                        s.border_2().border_color(rgb(if selected {
                                            SURFACE
                                        } else {
                                            BLUE
                                        }))
                                    })
                                    .child(label)
                                    .when(!selected, |el| {
                                        el.cursor_pointer().hover(|s| s.bg(rgb(SOFT_BLUE)))
                                    })
                                    .on_click(cx.listener(move |this, event, _, cx| {
                                        if matches!(event, ClickEvent::Keyboard(_)) {
                                            cx.stop_propagation();
                                        }
                                        if this.snapshot.mode != Some(mode) {
                                            this.error = this
                                                .desktop
                                                .as_ref()
                                                .and_then(|d| d.set_mode(mode).err())
                                                .map(|e| e.to_string());
                                            cx.notify();
                                        }
                                    }))
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
            let rules_tab = subscription_button(
                "rules-tab",
                self.locale.message(Message::Rules),
                !self.rule_sets_open,
                cx,
            )
            .toggled(!self.rule_sets_open)
            .track_focus(&self.rules_tab_focus)
            .on_click(cx.listener(|this, _, window, cx| {
                this.rules_tab_focus.focus(window, cx);
                this.rule_sets_open = false;
                cx.notify();
            }));
            let rule_sets_tab = subscription_button(
                "rule-sets-tab",
                self.locale.message(Message::RuleSets),
                self.rule_sets_open,
                cx,
            )
            .toggled(self.rule_sets_open)
            .track_focus(&self.rule_sets_tab_focus)
            .on_click(cx.listener(|this, _, window, cx| {
                this.rule_sets_tab_focus.focus(window, cx);
                this.rule_sets_open = true;
                this.rule_sets_read_failed = false;
                cx.notify();
            }));
            let page = div().flex().flex_col().gap_3().child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .child(div().flex().gap_2().child(rules_tab).child(rule_sets_tab))
                    .when(!self.rule_sets_open, |row| row.child(button)),
            );
            if self.rule_sets_open {
                return page.child(self.rule_sets(cx));
            }
            return match &self.snapshot.rules {
                Some(rules) => {
                    const PAGE_SIZE: usize = 100;
                    let value = self.rule_filter.read(cx).value();
                    let results = meow_paw::rule_inspection::matching_rules(rules, &value);
                    let (current, start, end) = meow_paw::rule_inspection::page_bounds(
                        results.len(),
                        self.rule_page,
                        PAGE_SIZE,
                    );
                    let toolbar = div()
                        .flex()
                        .items_center()
                        .gap_3()
                        .child(
                            div()
                                .text_sm()
                                .text_color(rgb(FAINT))
                                .child(self.locale.message(Message::RulePayloadFilter)),
                        )
                        .child(
                            div()
                                .id("rules-payload-search")
                                .flex_1()
                                .min_w_0()
                                .child(Input::new(&self.rule_filter).cleanable(true)),
                        )
                        .child(chip(
                            self.locale.rule_match_count(results.len(), rules.len()),
                            BLUE,
                            SOFT_BLUE,
                        ));
                    let table = if results.is_empty() {
                        empty_state(
                            self.locale.message(Message::NoMatchingRules),
                            self.locale.message(Message::RuleFilterDetail),
                        )
                    } else {
                        let next_is_last = results.len() - end <= PAGE_SIZE;
                        let navigation = div()
                            .flex()
                            .items_center()
                            .gap_3()
                            .child(section_label(self.locale.rule_page_range(
                                start + 1,
                                end,
                                results.len(),
                            )))
                            .when(current > 0, |nav| {
                                nav.child(
                                    subscription_button(
                                        "rules-previous-page",
                                        self.locale.message(Message::PreviousRulesPage),
                                        false,
                                        cx,
                                    )
                                    .track_focus(&self.rule_previous_focus)
                                    .on_click(cx.listener(
                                        move |this, _, window, cx| {
                                            if current == 1
                                                && this.rule_previous_focus.is_focused(window)
                                            {
                                                this.rule_next_focus.focus(window, cx);
                                            }
                                            this.rule_page = current - 1;
                                            cx.notify();
                                        },
                                    )),
                                )
                            })
                            .when(end < results.len(), |nav| {
                                nav.child(
                                    subscription_button(
                                        "rules-next-page",
                                        self.locale.message(Message::NextRulesPage),
                                        false,
                                        cx,
                                    )
                                    .track_focus(&self.rule_next_focus)
                                    .on_click(cx.listener(
                                        move |this, _, window, cx| {
                                            if next_is_last
                                                && this.rule_next_focus.is_focused(window)
                                            {
                                                this.rule_previous_focus.focus(window, cx);
                                            }
                                            this.rule_page = current + 1;
                                            cx.notify();
                                        },
                                    )),
                                )
                            });
                        card()
                            .child(navigation)
                            .child(
                                rule_row(
                                    section_label(self.locale.message(Message::RuleLineColumn)),
                                    section_label(self.locale.message(Message::RuleTypeColumn)),
                                    section_label(self.locale.message(Message::RuleMatchColumn)),
                                    section_label(self.locale.message(Message::RulePolicyColumn)),
                                )
                                .mt_2()
                                .bg(rgb(SOFT_BLUE))
                                .rounded_md(),
                            )
                            .children(results[start..end].iter().map(|(line, rule)| {
                                rule_row(
                                    div().text_color(rgb(FAINT)).child(line.to_string()),
                                    div()
                                        .font_weight(FontWeight::MEDIUM)
                                        .child(rule.kind.clone()),
                                    div().text_color(rgb(FAINT)).child(
                                        if rule.payload.is_empty() {
                                            "—".into()
                                        } else {
                                            rule.payload.clone()
                                        },
                                    ),
                                    div().child(rule.proxy.clone()),
                                )
                                .border_b_1()
                                .border_color(rgb(LINE))
                            }))
                    };
                    page.child(toolbar).child(table)
                }
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

    #[cfg(any(target_os = "linux", windows))]
    fn rule_sets(&self, cx: &mut Context<Self>) -> Div {
        let pending = self.snapshot.rule_provider_pending;
        let read = subscription_button(
            "read-rule-sets",
            self.locale.message(if pending {
                Message::ReadingRuleSets
            } else {
                Message::ReadRuleSets
            }),
            false,
            cx,
        )
        .track_focus(&self.rule_sets_read_focus)
        .when(pending, |read| read.opacity(0.65))
        .on_click(cx.listener(|this, _, _, cx| {
            if this.snapshot.rule_provider_pending {
                return;
            }
            this.rule_sets_read_failed = this
                .desktop
                .as_ref()
                .is_none_or(|desktop| desktop.read_rule_providers().is_err());
            cx.notify();
        }));
        let mut panel = card()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(self.locale.message(Message::RuleSets)),
                    )
                    .child(read),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(rgb(FAINT))
                    .child(self.locale.message(Message::RuleSetsReadOnly)),
            );
        if self.rule_sets_read_failed || self.snapshot.rule_provider_error.is_some() {
            panel = panel.child(
                div()
                    .text_sm()
                    .text_color(rgb(0xa83a33))
                    .child(self.locale.message(Message::RuleSetsUnavailable)),
            );
        }
        match &self.snapshot.rule_providers {
            None if !pending
                && !self.rule_sets_read_failed
                && self.snapshot.rule_provider_error.is_none() =>
            {
                panel.child(
                    div()
                        .text_sm()
                        .text_color(rgb(FAINT))
                        .child(self.locale.message(Message::RuleSetsNotLoaded)),
                )
            }
            None => panel,
            Some(providers) if providers.is_empty() => panel.child(
                div()
                    .text_sm()
                    .text_color(rgb(FAINT))
                    .child(self.locale.message(Message::RuleSetsEmpty)),
            ),
            Some(providers) => panel.children(providers.iter().map(|(name, provider)| {
                let hidden = self.locale.message(Message::RuleSetRedacted);
                let safe = |value: &str| safe_rule_set_text(value, hidden);
                let mut details = format!(
                    "{} / {} · {}: {}",
                    safe(&provider.behavior),
                    safe(&provider.format),
                    self.locale.message(Message::RuleSetSource),
                    safe(&provider.vehicle_type)
                );
                if let Some(count) = provider.rule_count {
                    details.push_str(&format!(
                        " · {}: {}",
                        self.locale.message(Message::RuleSetCount),
                        count
                    ));
                }
                if let Some(updated) = &provider.updated_at {
                    details.push_str(&format!(
                        " · {}: {}",
                        self.locale.message(Message::RuleSetUpdated),
                        safe(updated)
                    ));
                }
                div()
                    .border_t_1()
                    .border_color(rgb(LINE))
                    .pt_2()
                    .min_w_0()
                    .child(div().font_weight(FontWeight::MEDIUM).child(safe(name)))
                    .child(div().text_sm().text_color(rgb(FAINT)).child(details))
            })),
        }
    }

    fn logs(&self, cx: &mut Context<Self>) -> Div {
        #[cfg(any(target_os = "linux", windows))]
        if let Some(history) = self.snapshot.logs.as_ref() {
            let entries = self.log_review.entries(|| history.recent());
            let query = self.log_filter.read(cx).value();
            let visible = self.log_review.visible(&entries, &query, self.log_level);
            let paused = self.log_review.paused.is_some();
            let pause_context = self.snapshot.clone();
            let pause = subscription_button(
                "log-pause",
                self.locale.message(if paused {
                    Message::ResumeLogs
                } else {
                    Message::PauseLogs
                }),
                paused,
                cx,
            )
            .track_focus(&self.log_pause_focus)
            .on_click(cx.listener(move |this, _, window, cx| {
                this.log_pause_focus.focus(window, cx);
                this.refresh();
                if !log_review_context_changed(&pause_context, &this.snapshot)
                    && let Some(history) = this.snapshot.logs.as_ref()
                {
                    this.log_review.toggle_pause(|| history.recent());
                }
                cx.notify();
            }));
            let order = subscription_button(
                "log-order",
                self.locale.message(self.log_review.order.label()),
                false,
                cx,
            )
            .track_focus(&self.log_order_focus)
            .on_click(cx.listener(|this, _, window, cx| {
                this.log_order_focus.focus(window, cx);
                this.log_review.order.toggle();
                cx.notify();
            }));
            let clear_context = self.snapshot.clone();
            let clear = subscription_button(
                "clear-logs",
                self.locale.message(if paused {
                    Message::ClearLiveLogs
                } else {
                    Message::ClearLogs
                }),
                false,
                cx,
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.refresh();
                if !log_review_context_changed(&clear_context, &this.snapshot)
                    && let Some(history) = this.snapshot.logs.as_ref()
                {
                    history.clear();
                }
                cx.notify();
            }));
            let panel = div()
                .flex()
                .flex_col()
                .gap_3()
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap_2()
                        .child(chip(
                            self.locale.visible_log_count(visible.len(), entries.len()),
                            BLUE,
                            SOFT_BLUE,
                        ))
                        .child(pause)
                        .child(order)
                        .child(clear),
                )
                .when(paused, |panel| {
                    panel.child(
                        div()
                            .text_sm()
                            .text_color(rgb(FAINT))
                            .child(self.locale.message(Message::LogsPausedCaption)),
                    )
                });
            let panel = panel
                .child(
                    div()
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
                                .child(Input::new(&self.log_filter).cleanable(true)),
                        ),
                )
                .child(
                    div().flex().flex_wrap().gap_2().children(
                        LogLevelFilter::ALL
                            .into_iter()
                            .enumerate()
                            .map(|(index, level)| {
                                let selected = self.log_level == level;
                                subscription_button(
                                    ("log-level-filter", index),
                                    self.locale.message(level.label()),
                                    selected,
                                    cx,
                                )
                                .toggled(selected)
                                .px_3()
                                .py_1()
                                .on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        this.log_level = level;
                                        cx.notify();
                                    },
                                ))
                            }),
                    ),
                );
            if entries.is_empty() {
                return panel.child(empty_state(
                    self.locale.message(if paused {
                        Message::PausedLogsEmpty
                    } else {
                        Message::WaitingForLogs
                    }),
                    self.locale.message(if paused {
                        Message::PausedLogsEmptyDetail
                    } else {
                        Message::WaitingForLogsDetail
                    }),
                ));
            }
            if visible.is_empty() {
                return panel.child(empty_state(
                    self.locale.message(Message::NoMatchingLogs),
                    self.locale.message(Message::LogFilterDetail),
                ));
            }
            return panel.child(card().children(visible.into_iter().map(|entry| {
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
                            .child(entry.time.clone()),
                    )
                    .child(div().w(px(56.)).flex_shrink_0().child(chip(
                        entry.level.clone(),
                        color,
                        BG,
                    )))
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .text_sm()
                            .child(entry.message.clone()),
                    )
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
        let phase = self.locale.message(match phase {
            "Running" => Message::Running,
            "Starting" => Message::OverviewStarting,
            "Stopping" => Message::Stopping,
            "Needs attention" => Message::OverviewNeedsAttention,
            _ => Message::OverviewStopped,
        });
        let session_state = chip(phase, color, if port.is_some() { 0xe2f6ec } else { BG });
        let address = port.map_or_else(
            || self.locale.message(Message::OverviewNoListener).into(),
            |p| format!("127.0.0.1:{p}"),
        );
        let copy = if let Some(port) = port {
            secondary_button(
                "copy-listener",
                if self.copied_listener == Some(port) {
                    self.locale.message(Message::OverviewCopied)
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
            chip(self.locale.message(Message::Loopback), BLUE, SOFT_BLUE).into_any_element()
        };
        #[cfg(any(target_os = "linux", windows))]
        let (running, applying, active) = (
            self.state().2.is_some(),
            self.snapshot.applying_profile.is_some(),
            self.snapshot.profile.as_deref(),
        );
        #[cfg(not(any(target_os = "linux", windows)))]
        let (running, applying, active) = (false, false, None);
        let current = overview_current_profile(running, applying, active);
        let current_label = current.map_or_else(
            || {
                self.locale
                    .message(if running {
                        Message::OverviewUnconfirmedProfile
                    } else {
                        Message::OverviewNoRunningProfile
                    })
                    .to_owned()
            },
            |path| self.display_profile_name(path),
        );
        let next_label = self.profile.as_deref().map_or_else(
            || self.locale.message(Message::NoProfileSelected).to_owned(),
            |path| self.display_profile_name(path),
        );
        let refresh = overview_refresh_path(current, &self.subscription_paths).and_then(|path| {
            self.subscription_for(path)
                .map(|(_, id)| (id, path.to_owned()))
        });
        let profile = form_card(self.locale.message(Message::OverviewCurrentProfile))
            .child(form_row(
                self.locale.message(Message::Running),
                current_label,
                subscription_button(
                    "home-profiles",
                    self.locale.message(Message::Profiles),
                    false,
                    cx,
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.page = Page::Profiles;
                    this.load_subscription_metadata(cx);
                    cx.notify();
                })),
            ))
            .child(form_row(
                self.locale.message(Message::OverviewNextProfile),
                next_label,
                div(),
            ))
            .when_some(refresh, |card, (id, path)| {
                card.child(
                    div()
                        .px_4()
                        .pb_4()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(
                            div()
                                .text_xs()
                                .text_color(rgb(FAINT))
                                .child(self.locale.message(Message::SubscriptionDetail)),
                        )
                        .when(
                            !self.subscription_busy && self.subscription_rename.is_none(),
                            |el| {
                                el.child(
                                    secondary_button(
                                        "home-refresh-subscription",
                                        self.locale.message(Message::OverviewRefreshSubscription),
                                    )
                                    .on_click(cx.listener(
                                        move |paw, _, _, cx| {
                                            #[cfg(any(target_os = "linux", windows))]
                                            if overview_current_profile(
                                                paw.state().2.is_some(),
                                                paw.snapshot.applying_profile.is_some(),
                                                paw.snapshot.profile.as_deref(),
                                            ) == Some(path.as_path())
                                                && paw.subscription_paths.contains(&path)
                                            {
                                                paw.refresh_subscription(id, cx);
                                            }
                                            #[cfg(not(any(target_os = "linux", windows)))]
                                            let _ = (paw, id, &path, cx);
                                        },
                                    )),
                                )
                            },
                        ),
                )
            })
            .when(
                self.subscription_busy || self.subscription_notice.is_some(),
                |card| {
                    card.child(div().px_4().pb_4().text_sm().text_color(rgb(FAINT)).child(
                        if self.subscription_busy {
                            self.locale.message(self.subscription_busy_message)
                        } else {
                            self.subscription_notice
                                .map(|notice| self.locale.message(notice))
                                .unwrap_or_default()
                        },
                    ))
                },
            );
        let route = self.overview_route(cx);
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
                self.locale.message(Message::SystemProxyDescription),
                chip(self.locale.message(Message::Unmanaged), FAINT, BG),
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
            .child(metric_row().child(width(profile)).child(width(route)))
            .child(network)
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
                            .flex_wrap()
                            .justify_between()
                            .gap_2()
                            .mt_1()
                            .text_xs()
                            .text_color(rgb(FAINT))
                            .child(
                                self.locale
                                    .traffic_chart_history(TRAFFIC_WINDOW_SECS.saturating_sub(1)),
                            )
                            .child(
                                self.locale
                                    .traffic_chart_peak(&format!("{}/s", bytes(peak))),
                            ),
                    );
            }
        }
        let _ = height;
        div()
            .mt_2()
            .text_xs()
            .text_color(rgb(FAINT))
            .child(self.locale.message(Message::TrafficNoSamples))
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
            return subscription_display_name(
                self.locale,
                id,
                self.subscription_metadata
                    .get(path)
                    .and_then(Option::as_ref),
            );
        }
        local_profile_display_name(path)
    }

    fn overview_route(&self, cx: &mut Context<Self>) -> Div {
        #[cfg(any(target_os = "linux", windows))]
        let (detail, selections, excess) = if self.state().2.is_some()
            && self.snapshot.applying_profile.is_none()
            && !self.snapshot.inventory_stale
        {
            overview_route_summary(self.snapshot.mode, self.snapshot.inventory.as_deref())
        } else {
            (Message::OverviewRouteUnavailable, Vec::new(), 0)
        };
        #[cfg(not(any(target_os = "linux", windows)))]
        let (detail, selections, excess): (Message, Vec<String>, usize) =
            (Message::OverviewRouteUnavailable, Vec::new(), 0);
        let mut preview = div().min_w_0();
        if selections.is_empty() {
            preview = preview.child("—");
        } else {
            for selection in selections {
                preview = preview.child(
                    div()
                        .min_w_0()
                        .h(px(16.))
                        .overflow_hidden()
                        .truncate()
                        .child(selection),
                );
            }
            if excess > 0 {
                preview = preview.child(self.locale.overview_route_excess(excess));
            }
        }
        form_card(self.locale.message(Message::OverviewCurrentRoute)).child(form_row_with_detail(
            self.locale.message(detail),
            preview,
            subscription_button(
                "home-proxies",
                self.locale.message(Message::Proxies),
                false,
                cx,
            )
            .on_click(cx.listener(|this, _, _, cx| {
                this.page = Page::Proxies;
                cx.notify();
            })),
        ))
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
        if self.preferences.subscription_dir().is_some() {
            if self.subscription_entry_open {
                page =
                    page.child(
                        card()
                            .id("subscription-entry")
                            .on_key_down(cx.listener(|paw, event: &KeyDownEvent, window, cx| {
                                // Leave input keystrokes to the input's IME and editing handlers.
                                if event.keystroke.key == "escape"
                                    && !paw.subscription_busy
                                    && !event.keystroke.modifiers.modified()
                                    && !paw
                                        .subscription_url
                                        .as_ref()
                                        .unwrap()
                                        .focus_handle(cx)
                                        .is_focused(window)
                                {
                                    paw.cancel_subscription(window, cx);
                                    cx.stop_propagation();
                                }
                            }))
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(section_label(self.locale.message(Message::SubscriptionUrl)))
                            .child(
                                div()
                                    .flex()
                                    .flex_wrap()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div().flex_basis(px(260.)).flex_grow(1.).min_w_0().child(
                                            Input::new(self.subscription_url.as_ref().unwrap())
                                                .cleanable(true),
                                        ),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .flex_shrink_0()
                                            .gap_2()
                                            .child(
                                                subscription_button(
                                                    "confirm-subscription",
                                                    self.locale
                                                        .message(Message::ImportSubscription),
                                                    true,
                                                    cx,
                                                )
                                                .disabled(self.subscription_busy)
                                                .on_click(cx.listener(|paw, _, _, cx| {
                                                    paw.import_subscription(cx)
                                                })),
                                            )
                                            .child(
                                                subscription_button(
                                                    "cancel-subscription",
                                                    self.locale
                                                        .message(Message::CancelSubscription),
                                                    false,
                                                    cx,
                                                )
                                                .disabled(self.subscription_busy)
                                                .on_click(cx.listener(|paw, _, window, cx| {
                                                    paw.cancel_subscription(window, cx)
                                                })),
                                            ),
                                    ),
                            )
                            .when_some(
                                subscription_form_status(
                                    self.subscription_busy,
                                    self.subscription_busy_message,
                                    self.subscription_notice,
                                ),
                                |form, message| {
                                    form.child(
                                        div()
                                            .text_sm()
                                            .text_color(rgb(
                                                if self.subscription_busy
                                                    || message == Message::SubscriptionSaved
                                                {
                                                    FAINT
                                                } else {
                                                    0xb42318
                                                },
                                            ))
                                            .child(self.locale.message(message)),
                                    )
                                },
                            ),
                    );
            } else if !self.subscription_busy && self.subscription_rename.is_none() {
                page = page.child(
                    subscription_button(
                        "import-subscription",
                        self.locale.message(Message::AddSubscription),
                        false,
                        cx,
                    )
                    .track_focus(&self.subscription_add_focus)
                    .on_click(cx.listener(|paw, _, window, cx| {
                        paw.subscription_entry_open = true;
                        paw.subscription_notice = None;
                        paw.subscription_url
                            .as_ref()
                            .unwrap()
                            .focus_handle(cx)
                            .focus(window, cx);
                        cx.notify();
                    })),
                );
            }
        }
        if !self.subscription_entry_open
            && self.subscription_rename.is_none()
            && (self.subscription_busy || self.subscription_notice.is_some())
        {
            page = page.child(div().text_sm().text_color(rgb(FAINT)).child(
                if self.subscription_busy {
                    self.locale.message(self.subscription_busy_message)
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
        page = page.child(
            div()
                .flex()
                .items_center()
                .gap_3()
                .child(
                    div()
                        .text_sm()
                        .text_color(rgb(FAINT))
                        .child(self.locale.message(Message::ProfileNameFilter)),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(Input::new(&self.profile_filter).cleanable(true)),
                ),
        );
        if profile_list_state(profiles.len(), 0) == ProfileListState::Empty {
            page = page.child(empty_state(
                self.locale.message(Message::NoLocalProfiles),
                self.locale.message(Message::ImportYamlDetail),
            ));
        } else {
            let query = self.profile_filter.read(cx).value();
            let total = profiles.len();
            let mut matching = 0;
            let profiles = profiles
                .into_iter()
                .filter_map(|profile| {
                    let name = self.display_profile_name(profile);
                    let matches = profile_name_matches(&name, &query);
                    matching += usize::from(matches);
                    profile_card_visible(
                        matches,
                        self.subscription_rename.as_ref() == Some(profile),
                    )
                    .then_some((profile, name))
                })
                .collect::<Vec<_>>();
            if profile_list_state(total, matching) == ProfileListState::NoMatches {
                page = page.child(empty_state(
                    self.locale.message(Message::NoMatchingProfiles),
                    self.locale.message(Message::ProfileFilterDetail),
                ));
            }
            let cards = profiles.into_iter().map(|(profile, name)| {
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
                #[cfg(any(target_os = "linux", windows))]
                let confirmed_active = overview_current_profile(
                    live,
                    applying.is_some(),
                    self.snapshot.profile.as_deref(),
                ) == Some(profile.as_path());
                #[cfg(not(any(target_os = "linux", windows)))]
                let confirmed_active = false;
                let apply_path = profile.clone();
                let use_path = profile.clone();
                let forget_path = profile.clone();
                let subscription = self.subscription_for(profile).map(|(_, id)| id);
                let unreadable = subscription.is_some()
                    && self
                        .subscription_metadata
                        .get(profile)
                        .is_some_and(Option::is_none);
                let action_id = |action: &str| SharedString::from(format!("{action}:{profile:?}"));
                let details = if subscription.is_some() {
                    self.locale.message(Message::SubscriptionDetail).to_owned()
                } else {
                    profile.display().to_string()
                };
                let interval = self
                    .subscription_metadata
                    .get(profile)
                    .and_then(Option::as_ref)
                    .and_then(|metadata| {
                        metadata.interval.map(|interval| {
                            let (label, minutes) = match interval {
                                DisplayInterval::Local(minutes) => {
                                    (Message::SubscriptionLocalInterval, minutes)
                                }
                                DisplayInterval::Provider(minutes) => {
                                    (Message::SubscriptionProviderInterval, minutes)
                                }
                            };
                            self.locale.subscription_interval(label, minutes)
                        })
                    });
                card()
                    .flex_basis(px(300.))
                    .flex_grow(1.)
                    .min_w_0()
                    .p_3()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .child(
                                div()
                                    .font_weight(FontWeight::BOLD)
                                    .truncate()
                                    .child(name.clone()),
                            )
                            .child(
                                div()
                                    .mt_1()
                                    .text_xs()
                                    .text_color(rgb(FAINT))
                                    .truncate()
                                    .child(details),
                            ),
                    )
                    .when(subscription.is_some(), |card| {
                        card.child(div().text_xs().text_color(rgb(FAINT)).child(
                            self.locale.message(if unreadable {
                                Message::SubscriptionReadFailed
                            } else {
                                Message::SubscriptionManualOnly
                            }),
                        ))
                        .when_some(interval, |card, interval| {
                            card.child(div().text_xs().text_color(rgb(FAINT)).child(interval))
                        })
                    })
                    .when(self.subscription_rename.as_ref() == Some(profile), |card| {
                        card.child(
                            div()
                                .id(action_id("rename-editor"))
                                .on_key_down(cx.listener(
                                    |paw, event: &KeyDownEvent, window, cx| {
                                        // Keep input keystrokes with its IME and editing handlers.
                                        if event.keystroke.key == "escape"
                                            && !paw.subscription_busy
                                            && !event.keystroke.modifiers.modified()
                                            && !paw
                                                .subscription_name
                                                .focus_handle(cx)
                                                .is_focused(window)
                                        {
                                            paw.cancel_rename(window, cx);
                                            cx.stop_propagation();
                                        }
                                    },
                                ))
                                .flex()
                                .flex_col()
                                .gap_2()
                                .child(section_label(
                                    self.locale.message(Message::SubscriptionName),
                                ))
                                .child(
                                    Input::new(&self.subscription_name)
                                        .cleanable(true)
                                        .disabled(self.subscription_busy),
                                )
                                .when_some(
                                    subscription_form_status(
                                        self.subscription_busy,
                                        self.subscription_busy_message,
                                        self.subscription_notice,
                                    ),
                                    |form, message| {
                                        form.child(
                                            div()
                                                .text_sm()
                                                .text_color(rgb(if self.subscription_busy {
                                                    FAINT
                                                } else {
                                                    0xb42318
                                                }))
                                                .child(self.locale.message(message)),
                                        )
                                    },
                                )
                                .child(
                                    div()
                                        .flex()
                                        .gap_2()
                                        .child(
                                            subscription_button(
                                                action_id("rename-save"),
                                                self.locale.message(Message::SaveSubscriptionName),
                                                true,
                                                cx,
                                            )
                                            .disabled(self.subscription_busy)
                                            .on_click(
                                                cx.listener(|paw, _, _, cx| paw.save_rename(cx)),
                                            ),
                                        )
                                        .child(
                                            subscription_button(
                                                action_id("rename-cancel"),
                                                self.locale.message(Message::CancelSubscription),
                                                false,
                                                cx,
                                            )
                                            .disabled(self.subscription_busy)
                                            .on_click(
                                                cx.listener(|paw, _, window, cx| {
                                                    paw.cancel_rename(window, cx)
                                                }),
                                            ),
                                        ),
                                ),
                        )
                    })
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .gap_2()
                            .when(confirmed_active, |el| {
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
                            .when(live && applying.is_none() && !unreadable, |el| {
                                el.child(
                                    subscription_button(
                                        action_id("profile-apply"),
                                        self.locale.message(if active {
                                            Message::Reload
                                        } else {
                                            Message::ApplyLive
                                        }),
                                        false,
                                        cx,
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
                                subscription.filter(|_| {
                                    !self.subscription_busy
                                        && self.subscription_rename.is_none()
                                        && !self.subscription_entry_open
                                }),
                                |el, id| {
                                    let label = self.locale.message(if unreadable {
                                        Message::CheckSavedSubscription
                                    } else {
                                        Message::Refresh
                                    });
                                    el.child(
                                        profile_action_button(
                                            action_id("profile-refresh"),
                                            label,
                                            &name,
                                        )
                                        .on_click(
                                            cx.listener(move |paw, event, _, cx| {
                                                if matches!(event, ClickEvent::Keyboard(_)) {
                                                    cx.stop_propagation();
                                                }
                                                if unreadable {
                                                    paw.check_saved_subscription(id, cx);
                                                } else {
                                                    paw.refresh_subscription(id, cx);
                                                }
                                            }),
                                        ),
                                    )
                                },
                            )
                            .when(
                                subscription.is_some()
                                    && !self.subscription_busy
                                    && self.subscription_rename.is_none()
                                    && !self.subscription_entry_open
                                    && self
                                        .subscription_metadata
                                        .get(profile)
                                        .is_some_and(Option::is_some),
                                |el| {
                                    let rename_path = profile.clone();
                                    let saved_name = self
                                        .subscription_metadata
                                        .get(profile)
                                        .and_then(Option::as_ref)
                                        .and_then(|metadata| metadata.name.clone())
                                        .unwrap_or_default();
                                    el.child(
                                        profile_action_button(
                                            action_id("profile-rename"),
                                            self.locale.message(Message::RenameSubscription),
                                            &name,
                                        )
                                        .on_click(
                                            cx.listener(move |paw, event, window, cx| {
                                                if matches!(event, ClickEvent::Keyboard(_)) {
                                                    cx.stop_propagation();
                                                }
                                                if paw.subscription_busy
                                                    || paw.subscription_rename.is_some()
                                                {
                                                    return;
                                                }
                                                paw.subscription_name.update(cx, |input, cx| {
                                                    input.set_value(saved_name.clone(), window, cx)
                                                });
                                                paw.subscription_rename = Some(rename_path.clone());
                                                paw.subscription_notice = None;
                                                paw.error = None;
                                                paw.subscription_name
                                                    .focus_handle(cx)
                                                    .focus(window, cx);
                                                cx.notify();
                                            }),
                                        ),
                                    )
                                },
                            )
                            .when(!next && !unreadable, |el| {
                                el.child(
                                    subscription_button(
                                        action_id("profile-use"),
                                        self.locale.message(Message::Use),
                                        false,
                                        cx,
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
                                        profile_action_button(
                                            action_id("profile-forget"),
                                            self.locale.message(Message::Forget),
                                            &name,
                                        )
                                        .on_click(
                                            cx.listener(move |paw, event, _, cx| {
                                                if matches!(event, ClickEvent::Keyboard(_)) {
                                                    cx.stop_propagation();
                                                }
                                                match paw.preferences.forget_profile(&forget_path) {
                                                    Ok(()) => {
                                                        if paw.profile.as_ref()
                                                            == Some(&forget_path)
                                                        {
                                                            paw.profile = None;
                                                            #[cfg(any(
                                                                target_os = "linux",
                                                                windows
                                                            ))]
                                                            paw.log_review.reset_pause();
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
            });
            page = page.child(div().flex().flex_wrap().gap_3().children(cards));
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

    fn load_subscription_metadata(&mut self, cx: &mut Context<Self>) {
        if self.subscription_metadata_started {
            return;
        }
        self.subscription_metadata_started = true;
        let Some(dir) = self.preferences.subscription_dir() else {
            return;
        };
        let paths = self.subscription_paths.clone();
        cx.spawn(async move |weak, cx| {
            for chunk in paths.chunks(METADATA_BATCH_SIZE) {
                let chunk = chunk.to_vec();
                let dir = dir.clone();
                let batch = cx
                    .background_executor()
                    .spawn(async move {
                        let store = SubscriptionStore::new(dir);
                        chunk
                            .into_iter()
                            .map(|path| {
                                let metadata = store
                                    .id_for_record(&path)
                                    .and_then(|id| store.metadata(id).ok());
                                (path, metadata)
                            })
                            .collect()
                    })
                    .await;
                if weak
                    .update(cx, |paw, cx| {
                        merge_subscription_metadata(&mut paw.subscription_metadata, batch);
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
    }

    fn cancel_subscription(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.subscription_busy {
            return;
        }
        self.subscription_url_revision = self.subscription_url_revision.wrapping_add(1);
        self.subscription_url
            .as_ref()
            .unwrap()
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.subscription_entry_open = false;
        self.subscription_notice = None;
        if self.page == Page::Profiles && self.preferences.subscription_dir().is_some() {
            self.subscription_add_focus.focus(window, cx);
        }
        cx.notify();
    }

    fn cancel_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.subscription_busy {
            return;
        }
        self.subscription_name
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.subscription_rename = None;
        self.subscription_notice = None;
        if self.page == Page::Profiles && !self.subscription_busy {
            self.subscription_add_focus.focus(window, cx);
        }
        cx.notify();
    }

    fn save_rename(&mut self, cx: &mut Context<Self>) {
        if self.subscription_busy {
            return;
        }
        let Some(path) = self.subscription_rename.clone() else {
            return;
        };
        let Some((mut store, id)) = self.subscription_for(&path) else {
            return;
        };
        let name = self.subscription_name.read(cx).value().to_string();
        if !subscription_name_valid(&name) {
            self.subscription_notice = Some(Message::SubscriptionNameTooLong);
            cx.notify();
            return;
        }
        self.subscription_busy = true;
        self.subscription_busy_message = Message::SavingSubscriptionName;
        self.subscription_notice = None;
        cx.notify();
        cx.spawn(async move |weak, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { store.rename(id, &name) })
                .await;
            let _ = weak.update(cx, |paw, cx| {
                paw.subscription_busy = false;
                match result {
                    Ok(metadata) => {
                        paw.subscription_metadata.insert(path, Some(metadata));
                        paw.subscription_rename = None;
                    }
                    Err(error) => {
                        if paw.page != Page::Profiles {
                            paw.subscription_rename = None;
                        }
                        paw.subscription_notice = Some(subscription_write_feedback(
                            error,
                            Message::SubscriptionNameSaveFailed,
                        ));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn import_subscription(&mut self, cx: &mut Context<Self>) {
        if self.subscription_busy {
            return;
        }
        let Some(dir) = self.preferences.subscription_dir() else {
            return;
        };
        let url = self
            .subscription_url
            .as_ref()
            .unwrap()
            .read(cx)
            .value()
            .to_string();
        if !valid_subscription_url(&url) {
            self.subscription_notice = Some(Message::InvalidSubscriptionUrl);
            cx.notify();
            return;
        }
        let revision = self.subscription_url_revision;
        self.subscription_busy = true;
        self.subscription_busy_message = Message::DownloadingSubscription;
        self.subscription_notice = None;
        cx.notify();
        cx.spawn(async move |weak, cx| {
            let mut result = Some(
                cx.background_executor()
                    .spawn(async move {
                        let candidate =
                            subscription::fetch(&url).map_err(subscription_download_feedback)?;
                        let mut store = SubscriptionStore::new(dir);
                        let id = store
                            .import(&url, candidate, RefreshPolicy::default())
                            .map_err(|error| {
                                subscription_write_feedback(error, Message::SubscriptionSaveFailed)
                            })?;
                        Ok((store.record_path(id), store.metadata(id).ok()))
                    })
                    .await,
            );
            let Ok(handle) = weak.update(cx, |paw, _| paw.subscription_window) else {
                return;
            };
            let _ = handle.update(cx, |_, window, cx| {
                let _ = weak.update(cx, |paw, cx| {
                    paw.finish_subscription_import(
                        result.take().unwrap(),
                        revision,
                        Some(window),
                        cx,
                    );
                });
            });
            if let Some(result) = result {
                let _ = weak.update(cx, |paw, cx| {
                    paw.finish_subscription_import(result, revision, None, cx);
                });
            }
        })
        .detach();
    }

    fn finish_subscription_import(
        &mut self,
        result: Result<(PathBuf, Option<SubscriptionMetadata>), Message>,
        revision: u64,
        window: Option<&mut Window>,
        cx: &mut Context<Self>,
    ) {
        self.subscription_busy = false;
        let effect = import_editor_effect(
            revision,
            self.subscription_url_revision,
            self.subscription_entry_open,
            result.as_ref().err().copied(),
        );
        match result {
            Ok((path, metadata)) => {
                if !self.subscription_paths.contains(&path) {
                    self.subscription_paths.push(path.clone());
                }
                self.subscription_metadata.insert(path, metadata);
                self.subscription_notice = Some(Message::SubscriptionSaved);
                if effect == ImportEditorEffect::Clear {
                    if let Some(window) = window {
                        self.subscription_url
                            .as_ref()
                            .unwrap()
                            .update(cx, |input, cx| input.set_value("", window, cx));
                    } else {
                        // The window was closed; discard the input and recreate it on rebind.
                        self.subscription_url = None;
                    }
                    self.subscription_entry_open = false;
                }
            }
            Err(_) => {
                if let ImportEditorEffect::Failure(message) = effect {
                    self.subscription_notice = Some(message);
                }
            }
        }
        cx.notify();
    }

    // Recheck a known unreadable managed record without sending any provider request.
    fn check_saved_subscription(&mut self, id: SubscriptionId, cx: &mut Context<Self>) {
        self.run_subscription_task(
            Message::CheckingSavedSubscription,
            Message::SavedSubscriptionReadable,
            cx,
            move |store| {
                let metadata = store
                    .metadata(id)
                    .map_err(|_| Message::SubscriptionReadFailed)?;
                Ok((store.record_path(id), Some(metadata)))
            },
        );
    }

    fn refresh_subscription(&mut self, id: SubscriptionId, cx: &mut Context<Self>) {
        if self.subscription_rename.is_some() {
            return;
        }
        self.run_subscription_task(
            Message::DownloadingSubscription,
            Message::SubscriptionRefreshed,
            cx,
            move |store| {
                let prior = store
                    .read(id)
                    .map_err(|_| Message::SubscriptionReadFailed)?;
                let candidate = prior
                    .fetch_latest()
                    .map_err(subscription_download_feedback)?;
                store
                    .refresh_if_unchanged(id, &prior, candidate)
                    .map_err(|error| {
                        subscription_write_feedback(error, Message::SubscriptionRefreshSaveFailed)
                    })?;
                Ok((store.record_path(id), store.metadata(id).ok()))
            },
        );
    }

    fn run_subscription_task(
        &mut self,
        pending: Message,
        success: Message,
        cx: &mut Context<Self>,
        job: impl FnOnce(
            &mut SubscriptionStore,
        ) -> Result<(PathBuf, Option<SubscriptionMetadata>), Message>
        + Send
        + 'static,
    ) {
        let Some(dir) = self.preferences.subscription_dir() else {
            return;
        };
        if self.subscription_busy {
            return;
        }
        self.subscription_busy = true;
        self.subscription_busy_message = pending;
        self.subscription_notice = None;
        cx.notify();
        cx.spawn(async move |weak, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { job(&mut SubscriptionStore::new(dir)) })
                .await;
            let _ = weak.update(cx, |paw, cx| {
                paw.subscription_busy = false;
                match result {
                    Ok((path, metadata)) => {
                        if !paw.subscription_paths.contains(&path) {
                            paw.subscription_paths.push(path.clone());
                        }
                        paw.subscription_metadata.insert(path, metadata);
                        paw.subscription_notice = Some(success);
                    }
                    Err(notice) => paw.subscription_notice = Some(notice),
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
        #[cfg(any(target_os = "linux", windows))]
        if self.profile.as_ref() != Some(&path) {
            self.log_review.reset_pause();
        }
        self.profile = Some(path);
        self.error = None;
    }

    fn proxies(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Div {
        #[cfg(any(target_os = "linux", windows))]
        let locate_route = self.locate_route.take();
        #[cfg(any(target_os = "linux", windows))]
        if let Some(inventory) = self.snapshot.inventory.as_ref() {
            let value = self.search.read(cx).value();
            let query = value.trim().to_lowercase();
            let mut count = 0;
            let mut group_anchors = Vec::new();
            let columns = route_columns(window.viewport_size().width);
            use meow_paw::app::{ProxyProbeStatus, ProxyProbeTarget, parse_probe_url};
            let probe_url = parse_probe_url(&self.probe_url.read(cx).value());
            let probe_ready = probe_url.is_some();
            let ownership_available =
                !self.snapshot.inventory_stale && inventory.provider_error.is_none();
            let history_delays = if ownership_available {
                unambiguous_route_delays(inventory)
            } else {
                Default::default()
            };
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
                    let anchor = ScrollAnchor::for_handle(self.proxy_group_scroll.clone());
                    group_anchors.push((name.clone(), anchor.clone()));
                    let expandable = name != "GLOBAL";
                    let open =
                        !expandable || !query.is_empty() || self.expanded_groups.contains(name);
                    let order = self.proxy_order.get(name).copied().unwrap_or_default();
                    let comparison = self.snapshot.group_probe.as_ref().filter(|g| {
                        open && ownership_available
                            && g.is_current_completed(&self.snapshot, name, probe_url.as_ref())
                    });
                    let delays = route_delays(
                        order,
                        &history_delays,
                        inventory,
                        comparison.map(|g| &g.results),
                    );
                    if open {
                        sort_route_members(&mut members, order, &delays);
                    }
                    let visible_selection = group
                        .now
                        .as_deref()
                        .filter(|selected| members.iter().any(|(_, member)| member == selected));
                    let route_anchor = visible_selection.and_then(|selected| {
                        let (wanted_group, wanted_member) = locate_route.as_ref()?;
                        (open && name == wanted_group && selected == wanted_member).then(|| {
                            let anchor = ScrollAnchor::for_handle(self.proxy_group_scroll.clone());
                            anchor.scroll_to(window, cx);
                            anchor
                        })
                    });
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
                    let compare_button = semantic_secondary_button(
                        SharedString::from(format!("group-compare:{name}")),
                        if running {
                            labels.1
                        } else if probe_busy {
                            labels.2
                        } else {
                            labels.0
                        },
                    )
                    .disabled(!(running || (ownership_available && probe_ready && !probe_busy)))
                    .when(
                        running || (ownership_available && probe_ready && !probe_busy),
                        |button| {
                            button.on_click(cx.listener(move |paw, _, _, cx| {
                                cx.stop_propagation();
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
                    let sort_button = semantic_secondary_button(
                        SharedString::from(format!("proxy-order:{name}")),
                        self.locale.message(order.message()),
                    )
                    .on_click(cx.listener(move |paw, _, _, cx| {
                        cx.stop_propagation();
                        let next = order.next();
                        if next == ProxyOrder::Configured {
                            paw.proxy_order.remove(&group_name);
                        } else {
                            paw.proxy_order.insert(group_name.clone(), next);
                        }
                        cx.notify();
                    }));
                    let toggle = name.clone();
                    let heading = card()
                        .px_3()
                        .py_2()
                        .id(SharedString::from(format!("group:{name}")))
                        .anchor_scroll(Some(anchor))
                        .flex()
                        .flex_wrap()
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
                        .when(open && !members.is_empty(), |heading| {
                            heading.child(
                                div()
                                    .flex()
                                    .flex_wrap()
                                    .items_center()
                                    .gap_1()
                                    .child(compare_button)
                                    .child(sort_button),
                            )
                        })
                        .when_some(visible_selection, |heading, selected| {
                            let group = name.clone();
                            let member = selected.to_owned();
                            let label = match self.locale {
                                Locale::En => "Locate selected node",
                                Locale::ZhCn => "定位当前节点",
                            };
                            heading.child(
                                SemanticButton::new(SharedString::from(format!("locate:{name}")))
                                    .accessibility_label(format!("{label}: {name}, {selected}"))
                                    .cursor_pointer()
                                    .rounded_md()
                                    .px_2()
                                    .py_1()
                                    .border_1()
                                    .border_color(rgb(LINE))
                                    .text_sm()
                                    .text_color(rgb(BLUE))
                                    .bg(rgb(SOFT_BLUE))
                                    .hover(|style| style.bg(rgb(0xd8e7ff)))
                                    .focus_visible(|style| style.border_color(rgb(BLUE)))
                                    .child(label)
                                    .on_click(cx.listener(move |paw, _, _, cx| {
                                        cx.stop_propagation();
                                        paw.expanded_groups.insert(group.clone());
                                        paw.locate_route = Some((group.clone(), member.clone()));
                                        cx.notify();
                                    })),
                            )
                        })
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
                    let mutable = !self.snapshot.inventory_stale
                        && group.kind.eq_ignore_ascii_case("selector");
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
                                let comparison_panel = div()
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .when_some(comparison, |panel, g| {
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
                                        panel.child(div().text_xs().text_color(rgb(FAINT)).child(
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
                                        ))
                                    })
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
                                    .when(comparison.is_some(), |rows| rows.child(comparison_panel))
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
                                                        ownership_available
                                                            && !inventory
                                                                .groups
                                                                .contains_key(member)
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
                                                    .when(selected, |card| {
                                                        card.anchor_scroll(route_anchor.clone())
                                                    })
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
                                                        route_delay(&delays, member),
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
                                if expandable
                                    && query.is_empty()
                                    && !self.preferences.reduce_motion()
                                {
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
            if self.proxy_providers_open && ownership_available {
                for node in &inventory.nodes {
                    if let meow_paw::engine::controller::Origin::Provider(origin) = &node.origin {
                        *node_counts.entry(origin.as_str()).or_insert(0_usize) += 1;
                    }
                }
            }
            let providers = inventory
                .providers
                .iter()
                .filter(|_| ownership_available && self.proxy_providers_open)
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
                        .unwrap_or_else(|| self.locale.message(Message::ProviderNoUsage).into());
                    let updated = status.updated_at.clone().unwrap_or_else(|| {
                        self.locale.message(Message::ProviderAwaitingUpdate).into()
                    });
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
                                provider_refresh_button(self.locale, &label, pending, busy).when(
                                    !busy,
                                    |button| {
                                        button.on_click(cx.listener(move |paw, event, _, cx| {
                                            if matches!(event, ClickEvent::Keyboard(_)) {
                                                cx.stop_propagation();
                                            }
                                            paw.error = paw.desktop.as_ref().and_then(|desktop| {
                                                desktop
                                                    .refresh_provider(label.clone())
                                                    .err()
                                                    .map(|e| e.to_string())
                                            });
                                            cx.notify();
                                        }))
                                    },
                                ),
                            )
                        })
                })
                .collect::<Vec<_>>();
            let probe_settings = || {
                div()
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
                }))
            };
            let probe_caption = match (self.proxy_probe_settings_open, probe_ready, self.locale) {
                (true, _, Locale::En) => "Hide latency test settings",
                (false, true, Locale::En) => "Latency test settings",
                (false, false, Locale::En) => "Set latency test URL",
                (true, _, Locale::ZhCn) => "收起延迟测试设置",
                (false, true, Locale::ZhCn) => "延迟测试设置",
                (false, false, Locale::ZhCn) => "设置延迟测试地址",
            };
            let probe_control =
                semantic_secondary_button("proxy-probe-settings-toggle", probe_caption).on_click(
                    cx.listener(|paw, event, _, cx| {
                        if matches!(event, ClickEvent::Keyboard(_)) {
                            cx.stop_propagation();
                        }
                        paw.proxy_probe_settings_open = !paw.proxy_probe_settings_open;
                        cx.notify();
                    }),
                );
            let page = div()
                .flex()
                .flex_col()
                .gap_3()
                .child(toolbar)
                .when(self.snapshot.inventory_stale || inventory.provider_error.is_some(), |page| {
                    page.child(div().text_sm().text_color(rgb(FAINT)).child(
                        match (self.snapshot.inventory_stale, self.locale) {
                            (true, Locale::En) => "Route and provider data may be stale; route selection, node tests and provider refresh are disabled.",
                            (true, Locale::ZhCn) => "路线和提供者数据可能已过期；路线选择、节点测试和提供者刷新已停用。",
                            (false, Locale::En) => "Provider data unavailable; node tests and provider refresh are disabled. Routes remain available.",
                            (false, Locale::ZhCn) => "提供者数据不可用；节点测试和提供者刷新已停用。路线仍可使用。",
                        },
                    ))
                })
                .child({
                    let controls = div()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap_2()
                        .child(probe_control);
                    controls.when(ownership_available && !inventory.providers.is_empty(), |row| {
                        let label = match (self.proxy_providers_open, self.locale) {
                            (false, Locale::En) => "Show providers",
                            (true, Locale::En) => "Hide providers",
                            (false, Locale::ZhCn) => "展开提供者",
                            (true, Locale::ZhCn) => "收起提供者",
                        };
                        let caption = format!("{label} ({})", inventory.providers.len());
                        row.child(
                            semantic_secondary_button("proxy-providers-toggle", caption).on_click(
                                cx.listener(|paw, event, _, cx| {
                                    if matches!(event, ClickEvent::Keyboard(_)) {
                                        cx.stop_propagation();
                                    }
                                    paw.proxy_providers_open = !paw.proxy_providers_open;
                                    cx.notify();
                                }),
                            ),
                        )
                        .when_some(self.snapshot.refreshing_provider.as_ref(), |row, name| {
                            row.child(div().text_sm().text_color(rgb(FAINT)).child(format!(
                                "{} {name}",
                                self.locale.message(Message::ProviderRefreshing),
                            )))
                        })
                    })
                })
                .when(self.proxy_probe_settings_open, |page| page.child(probe_settings()))
                .children(providers);
            return if groups.is_empty() {
                page.child(empty_state(
                    self.locale.message(Message::NoMatchingGroups),
                    self.locale.message(Message::TryAnotherRoute),
                ))
            } else {
                let navigator = div()
                    .id("proxy-group-navigator")
                    .w_full()
                    .min_w_0()
                    .overflow_x_scroll()
                    .flex()
                    .gap_2()
                    .children(group_anchors.into_iter().map(|(name, anchor)| {
                        Button::new(SharedString::from(format!("jump-group:{name}")))
                            .label(name.clone())
                            .tooltip(name)
                            .custom(
                                ButtonCustomVariant::new(cx)
                                    .color(rgb(BLUE).into())
                                    .foreground(rgb(BLUE).into())
                                    .hover(rgb(0xd8e8ff).into())
                                    .active(rgb(0xc8deff).into()),
                            )
                            .max_w(px(180.))
                            .on_click(move |_, window, cx| anchor.scroll_to(window, cx))
                    }));
                page.child(
                    div()
                        .text_sm()
                        .text_color(rgb(FAINT))
                        .child(self.locale.message(Message::JumpToGroup)),
                )
                .child(navigator)
                .children(groups)
            };
        }
        let _ = (window, cx);
        #[cfg(any(target_os = "linux", windows))]
        if self.snapshot.inventory_stale {
            return empty_state(
                self.locale.message(Message::OverviewRouteUnavailable),
                match self.locale {
                    Locale::En => "Waiting for the controller to provide routes.",
                    Locale::ZhCn => "正在等待控制器提供路线。",
                },
            );
        }
        empty_state(
            self.locale.message(Message::NoProxyGroups),
            self.locale.message(Message::StartToViewGroups),
        )
    }

    #[cfg(any(target_os = "linux", windows))]
    fn reconcile_connection_detail(&mut self, cx: &App) {
        if self.page != Page::Connections {
            self.connection_detail = None;
            self.connection_detail_return = None;
            return;
        }
        let Some(selection) = &self.connection_detail else {
            return;
        };
        let visible = self.page == Page::Connections
            && matches!(
                self.snapshot.engine,
                meow_paw::app::EngineStatus::Running { .. }
            )
            && self.snapshot.applying_profile.is_none()
            && detail_row_index(selection, &self.snapshot, self.closed_connections_open)
                .is_some_and(|index| {
                    let row = if self.closed_connections_open {
                        self.snapshot.connection_history.recent().nth(index)
                    } else {
                        self.snapshot
                            .connections
                            .as_ref()
                            .and_then(|rows| rows.get(index))
                    };
                    let query = self
                        .connection_filter
                        .read(cx)
                        .value()
                        .trim()
                        .to_lowercase();
                    row.is_some_and(|row| {
                        !matching_connections(std::iter::once(row), &query).is_empty()
                    })
                });
        if !visible {
            self.connection_detail = None;
            self.connection_detail_return = None;
        }
    }

    fn connections(&self, cx: &mut Context<Self>) -> Div {
        #[cfg(any(target_os = "linux", windows))]
        if matches!(
            self.snapshot.engine,
            meow_paw::app::EngineStatus::Running { .. }
        ) {
            let active_tab = subscription_button(
                "connections-active-tab",
                self.locale.message(Message::ActiveTab),
                !self.closed_connections_open,
                cx,
            )
            .toggled(!self.closed_connections_open)
            .track_focus(&self.active_connections_tab_focus)
            .on_click(cx.listener(|this, _, window, cx| {
                this.active_connections_tab_focus.focus(window, cx);
                if this.closed_connections_open {
                    this.connection_detail = None;
                    this.connection_detail_return = None;
                }
                this.closed_connections_open = false;
                cx.notify();
            }));
            let closed_tab = subscription_button(
                "connections-closed-tab",
                self.locale.message(Message::ClosedTab),
                self.closed_connections_open,
                cx,
            )
            .toggled(self.closed_connections_open)
            .track_focus(&self.closed_connections_tab_focus)
            .on_click(cx.listener(|this, _, window, cx| {
                this.closed_connections_tab_focus.focus(window, cx);
                if !this.closed_connections_open {
                    this.connection_detail = None;
                    this.connection_detail_return = None;
                }
                this.closed_connections_open = true;
                cx.notify();
            }));
            let mut page = div()
                .flex()
                .flex_col()
                .gap_3()
                .child(div().flex().gap_2().child(active_tab).child(closed_tab));
            if self.closed_connections_open {
                page =
                    page.child(div().text_sm().text_color(rgb(FAINT)).child(
                        self.locale.closed_connection_history_detail(
                            meow_paw::app::CLOSED_CONNECTION_LIMIT,
                        ),
                    ));
            }
            let rows: Vec<_> = if self.closed_connections_open {
                self.snapshot.connection_history.recent().collect()
            } else if let Some(list) = &self.snapshot.connections {
                list.iter().collect()
            } else {
                return page.child(empty_state(
                    self.locale.message(Message::LiveConnectionsUnavailable),
                    self.locale
                        .message(Message::LiveConnectionsUnavailableDetail),
                ));
            };
            let value = self.connection_filter.read(cx).value();
            let query = value.trim().to_lowercase();
            let results = matching_connections(rows.iter().copied(), &query);
            let count = if self.closed_connections_open {
                self.locale
                    .closed_connection_count(results.len(), rows.len(), !query.is_empty())
            } else if query.is_empty() {
                format!(
                    "{} {}",
                    rows.len(),
                    self.locale.message(Message::ActiveConnections)
                )
            } else {
                self.locale
                    .connection_match_count(results.len(), rows.len())
            };
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
                        .child(Input::new(&self.connection_filter).cleanable(true)),
                )
                .child(chip(count, BLUE, SOFT_BLUE));
            let selected_index = self.connection_detail.as_ref().and_then(|selection| {
                detail_row_index(selection, &self.snapshot, self.closed_connections_open)
            });
            let return_index = self
                .connection_detail_return
                .as_ref()
                .and_then(|selection| {
                    detail_row_index(selection, &self.snapshot, self.closed_connections_open)
                });
            let table = if results.is_empty() {
                let (title, detail) =
                    connection_empty_messages(self.closed_connections_open, rows.len());
                empty_state(self.locale.message(title), self.locale.message(detail))
            } else {
                card()
                    .p_0()
                    .child(
                        connection_row(
                            section_label(self.locale.message(Message::DestinationColumn)),
                            section_label(self.locale.message(Message::RouteColumn)),
                            section_label(self.locale.message(Message::RuleColumn)),
                            None,
                            self.closed_connections_open,
                        )
                        .bg(rgb(CHROME)),
                    )
                    .children(results.into_iter().map(|(c, destination, route)| {
                        let index = rows.iter().position(|row| std::ptr::eq(*row, c)).unwrap();
                        let selected = selected_index == Some(index);
                        let return_focus =
                            selected || (selected_index.is_none() && return_index == Some(index));
                        let selection = if self.closed_connections_open {
                            ConnectionDetailSelection::Closed(
                                self.snapshot
                                    .connection_history
                                    .recent_keyed()
                                    .nth(index)
                                    .unwrap()
                                    .0,
                            )
                        } else {
                            ConnectionDetailSelection::Active {
                                sample: self.snapshot.connections.as_ref().unwrap().clone(),
                                index,
                            }
                        };
                        let close_selection = selection.clone();
                        let details = small_connection_button(
                            ("connection-details", index),
                            self.locale.message(Message::ConnectionDetails),
                            cx,
                        )
                        .aria_expanded(selected)
                        .when(return_focus, |button| {
                            button.track_focus(&self.connection_detail_focus)
                        })
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                this.connection_detail_focus.focus(window, cx);
                                this.connection_detail_return = None;
                                this.connection_detail = Some(selection.clone());
                                cx.notify();
                            },
                        ));
                        let mut actions = div().flex().items_center().gap_1().child(details);
                        if !self.closed_connections_open {
                            let key = c.id.clone();
                            actions = actions.child(
                                div()
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
                                    })),
                            );
                        }
                        let row = connection_row(
                            div()
                                .font_weight(FontWeight::MEDIUM)
                                .child(destination.clone()),
                            div().child(if route.is_empty() {
                                "—".into()
                            } else {
                                route.clone()
                            }),
                            div().child(if c.rule.is_empty() {
                                "—".into()
                            } else {
                                c.rule.clone()
                            }),
                            Some(actions.into_any_element()),
                            self.closed_connections_open,
                        );
                        let mut item = div().child(row);
                        if selected {
                            let close = small_connection_button(
                                "connection-details-close",
                                self.locale.message(Message::CloseDetails),
                                cx,
                            )
                            .on_click(cx.listener(
                                move |this, _, window, cx| {
                                    this.connection_detail_return = Some(close_selection.clone());
                                    this.connection_detail = None;
                                    this.connection_detail_focus.focus(window, cx);
                                    cx.notify();
                                },
                            ));
                            item = item.child(connection_detail_area(
                                c,
                                &destination,
                                &route,
                                self.closed_connections_open,
                                self.locale,
                                close,
                            ));
                        }
                        item
                    }))
            };
            return page.child(toolbar).child(table);
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
                    self.log_review.reset_pause();
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

    fn unlock(&self, cx: &mut Context<Self>) -> Div {
        #[cfg(any(target_os = "linux", windows))]
        {
            use meow_paw::{
                app::EngineStatus,
                unlock::{Phase, ROUTE_LIMIT, Status},
            };

            let state = &self.snapshot.unlock;
            let running = matches!(self.snapshot.engine, EngineStatus::Running { .. });
            let active = running && state.phase == Phase::Checking;
            let show_cancel = active || running && state.phase == Phase::Cancelling;
            let inventory = self
                .snapshot
                .inventory
                .as_ref()
                .filter(|_| running && !self.snapshot.inventory_stale);
            let groups: Vec<_> = inventory
                .into_iter()
                .flat_map(|inventory| inventory.groups.iter())
                .filter_map(|(name, group)| (!group.hidden).then_some(name))
                .collect();
            let can_run = self.desktop.is_some()
                && self.snapshot.applying_profile.is_none()
                && !state.is_busy();
            let summary = match (&self.snapshot.engine, &state.phase) {
                (EngineStatus::Starting, _) => Some(Message::OverviewStarting),
                (EngineStatus::Stopping, _) => Some(Message::Stopping),
                (_, _) if !running => Some(Message::UnlockStartSession),
                (_, _) if self.snapshot.applying_profile.is_some() => Some(Message::Applying),
                (_, Phase::Idle) if state.is_busy() => Some(Message::UnlockCancelling),
                (_, Phase::Idle) if inventory.is_none() => Some(Message::OverviewRouteUnavailable),
                (_, Phase::Idle) if groups.is_empty() => Some(Message::NoProxyGroups),
                (_, Phase::Idle) => None,
                (_, Phase::Checking) => Some(Message::UnlockChecking),
                (_, Phase::Cancelling) => Some(Message::UnlockCancelling),
                (_, Phase::Unsupported) => Some(Message::UnlockUnsupportedCore),
                (_, Phase::NoRoutes) => Some(Message::UnlockNoRoutes),
                (_, Phase::Complete) => Some(Message::UnlockComplete),
                (_, Phase::Cancelled) => Some(Message::UnlockCancelled),
                (_, Phase::Unavailable) => Some(Message::UnlockUnavailableDetail),
            };
            let show_groups = !groups.is_empty() && !matches!(state.phase, Phase::Unsupported);
            let group_buttons = groups.into_iter().enumerate().map(|(index, name)| {
                let group = name.clone();
                let mut label = safe_rule_set_text(&group, "");
                if label.trim().is_empty() {
                    label = format!(
                        "{} {}",
                        self.locale.message(Message::UnlockGroup),
                        index + 1
                    );
                }
                SemanticButton::new(("unlock-group", index))
                    .accessibility_label(label.clone())
                    .disabled(!can_run)
                    .max_w(px(248.))
                    .px_3()
                    .py_2()
                    .rounded_md()
                    .bg(rgb(if can_run { SOFT_BLUE } else { BG }))
                    .text_color(rgb(if can_run { BLUE } else { FAINT }))
                    .child(div().truncate().child(label))
                    .when(can_run, |button| {
                        button
                            .cursor_pointer()
                            .hover(|style| style.bg(rgb(0xd8e7ff)))
                            .focus_visible(|style| style.border_2().border_color(rgb(BLUE)))
                            .on_click(cx.listener(move |this, event, _, cx| {
                                if matches!(event, ClickEvent::Keyboard(_)) {
                                    cx.stop_propagation();
                                }
                                if let Some(desktop) = &this.desktop {
                                    this.error = desktop
                                        .run_unlock(this.snapshot.probe_epoch, &group)
                                        .err()
                                        .map(|_| {
                                            this.locale
                                                .message(Message::UnlockUnavailableDetail)
                                                .into()
                                        });
                                }
                                this.refresh();
                                cx.notify();
                            }))
                    })
            });
            let setup = card()
                .flex()
                .flex_col()
                .items_start()
                .gap_3()
                .child(
                    div()
                        .text_sm()
                        .text_color(rgb(FAINT))
                        .child(self.locale.unlock_manual_detail(ROUTE_LIMIT)),
                )
                .when_some(summary, |panel, message| {
                    panel.child(
                        div()
                            .text_sm()
                            .text_color(rgb(FAINT))
                            .child(self.locale.message(message)),
                    )
                })
                .when(show_cancel, |panel| {
                    panel.child(
                        subscription_button(
                            "unlock-cancel",
                            self.locale.message(Message::UnlockCancel),
                            false,
                            cx,
                        )
                        .disabled(!active)
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(desktop) = &this.desktop {
                                desktop.cancel_unlock();
                            }
                            this.refresh();
                            cx.notify();
                        })),
                    )
                })
                .when(show_groups, |panel| {
                    panel
                        .child(section_label(
                            self.locale.message(Message::UnlockChooseGroup),
                        ))
                        .child(div().flex().flex_wrap().gap_2().children(group_buttons))
                });
            let ranked = state.phase == Phase::Complete && state.rows.len() > 1;
            let message = |key| self.locale.message(key);
            let result_text = |status| match status {
                Status::Passed { elapsed_ms } => {
                    format!("{} · {elapsed_ms} ms", message(Message::UnlockPassed))
                }
                Status::Http(code) => format!("HTTP {code}"),
                Status::Pending => message(Message::UnlockPending).to_owned(),
                Status::NotRun => message(Message::UnlockNotRun).to_owned(),
                Status::TimedOut => message(Message::UnlockTimedOut).to_owned(),
                Status::Failed => message(Message::UnlockCheckFailed).to_owned(),
                Status::Cancelled => message(Message::UnlockCancelled).to_owned(),
            };
            let rows = state.rows.iter().enumerate().map(|(index, row)| {
                let label = safe_rule_set_text(&row.route, message(Message::RuleSetRedacted));
                let heading = if ranked {
                    format!("{}. {label}", index + 1)
                } else {
                    label
                };
                div()
                    .py_3()
                    .border_t_1()
                    .border_color(rgb(LINE))
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(div().font_weight(FontWeight::SEMIBOLD).child(heading))
                    .child(div().flex().flex_wrap().gap_3().children(
                        ["Google", "Cloudflare"].into_iter().zip(row.checks).map(
                            |(label, status)| {
                                let color = if matches!(status, Status::Passed { .. }) {
                                    GREEN
                                } else {
                                    FAINT
                                };
                                div()
                                    .min_w(px(124.))
                                    .flex()
                                    .flex_col()
                                    .child(section_label(label))
                                    .child(
                                        div()
                                            .text_sm()
                                            .text_color(rgb(color))
                                            .child(result_text(status)),
                                    )
                            },
                        ),
                    ))
            });
            div().flex().flex_col().gap_3().child(setup).when(
                running && !state.rows.is_empty(),
                |page| {
                    page.child(
                        card()
                            .when(ranked, |results| {
                                results.child(
                                    div()
                                        .pb_3()
                                        .text_sm()
                                        .text_color(rgb(FAINT))
                                        .child(message(Message::UnlockRankDetail)),
                                )
                            })
                            .children(rows),
                    )
                },
            )
        }
        #[cfg(not(any(target_os = "linux", windows)))]
        {
            let _ = cx;
            empty_state(
                self.locale.message(Message::Unlock),
                self.locale.message(Message::UnlockPlatformUnavailable),
            )
        }
    }

    fn settings(&self, wide: bool, cx: &mut Context<Self>) -> Div {
        let saved_language = self.preferences.language();
        let can_save_language = self.preferences.has_persistent_file();
        let language_name = |language| match language {
            LanguagePreference::System => self.locale.message(Message::LanguageSystem),
            LanguagePreference::En => "English",
            LanguagePreference::ZhCn => "简体中文",
        };
        let language = settings_card(self.locale.message(Message::Language), "icons/globe.svg")
            .child(
                div()
                    .mx_4()
                    .py_3()
                    .border_t_1()
                    .border_color(rgb(LINE))
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(format!(
                                "{}: {}",
                                self.locale.message(Message::SavedLanguage),
                                language_name(saved_language),
                            )),
                    )
                    .child(
                        div()
                            .id("language-options")
                            .role(Role::Group)
                            .aria_label(self.locale.message(Message::SavedLanguage))
                            .flex()
                            .flex_wrap()
                            .gap_1()
                            .p_1()
                            .rounded_lg()
                            .bg(rgb(SOFT_BLUE))
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
                                    subscription_button(
                                        ("language-choice", index),
                                        language_name(choice),
                                        selected,
                                        cx,
                                    )
                                    .px_3()
                                    .py_1()
                                    .track_focus(&self.language_choice_focus[index])
                                    .selected(selected)
                                    .toggled(selected)
                                    .disabled(!can_save_language)
                                    .on_click(cx.listener(
                                        move |this, _, window, cx| {
                                            this.language_choice_focus[index].focus(window, cx);
                                            if this.preferences.has_persistent_file()
                                                && this.preferences.language() != choice
                                            {
                                                this.language_save_failed =
                                                    this.preferences.set_language(choice).is_err();
                                            }
                                            cx.notify();
                                        },
                                    ))
                                }),
                            ),
                    )
                    .child(div().text_sm().text_color(rgb(FAINT)).child(format!(
                        "{}: {}",
                        self.locale.message(Message::CurrentLanguage),
                        match self.locale {
                            Locale::En => "English",
                            Locale::ZhCn => "简体中文",
                        }
                    )))
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
                    }),
            );
        let binary = self.binary.as_ref().map_or_else(
            || self.locale.message(Message::NoExecutableSelected).into(),
            |p| p.display().to_string(),
        );
        let system = settings_card(self.locale.message(Message::System), "icons/settings.svg")
            .child(
                div()
                    .px_4()
                    .pb_2()
                    .text_xs()
                    .text_color(rgb(FAINT))
                    .child(self.locale.message(Message::SettingsReadOnly)),
            )
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
        let engine = settings_card(self.locale.message(Message::MeowEngine), "icons/server.svg")
            .child(form_row_with_detail(
                self.locale.message(Message::MeowExecutable),
                path_detail,
                if self.can_change_binary() {
                    subscription_button(
                        "select-binary",
                        self.locale.message(Message::Browse),
                        false,
                        cx,
                    )
                    .track_focus(&self.engine_browse_focus)
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.engine_browse_focus.focus(window, cx);
                        this.choose_file(true, cx);
                    }))
                    .into_any_element()
                } else {
                    chip(self.locale.message(Message::EngineLocked), FAINT, BG).into_any_element()
                },
            ))
            .when(self.binary.is_some(), |engine| {
                engine.child(
                    div()
                        .mx_4()
                        .pb_3()
                        .child(
                            subscription_button(
                                "engine-path-reveal",
                                if self.show_engine_path {
                                    self.locale.message(Message::HideFullPath)
                                } else {
                                    self.locale.message(Message::ShowFullPath)
                                },
                                false,
                                cx,
                            )
                            .track_focus(&self.engine_reveal_focus)
                            .aria_expanded(self.show_engine_path)
                            .on_click(cx.listener(
                                |this, _, window, cx| {
                                    this.engine_reveal_focus.focus(window, cx);
                                    this.show_engine_path = !this.show_engine_path;
                                    cx.notify();
                                },
                            )),
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
                                subscription_button(
                                    "engine-path-copy",
                                    self.locale.message(Message::CopyPath),
                                    false,
                                    cx,
                                )
                                .track_focus(&self.engine_copy_focus)
                                .on_click(cx.listener(
                                    move |this, _, window, cx| {
                                        this.engine_copy_focus.focus(window, cx);
                                        cx.write_to_clipboard(ClipboardItem::new_string(
                                            binary.clone(),
                                        ));
                                    },
                                )),
                            )
                        }),
                )
            });
        let reduce_motion = self.preferences.reduce_motion();
        let app = settings_card("Meow Paw", "icons/house.svg")
            .child(form_row(
                self.locale.message(Message::ReduceMotion),
                self.locale.message(Message::ReduceMotionDescription),
                subscription_button(
                    "reduce-motion",
                    self.locale.message(if reduce_motion {
                        Message::ReduceMotionEnabled
                    } else {
                        Message::ReduceMotionDisabled
                    }),
                    reduce_motion,
                    cx,
                )
                .accessibility_label(self.locale.message(Message::ReduceMotion))
                .toggled(reduce_motion)
                .on_click(cx.listener(|this, _, _, cx| {
                    let next = !this.preferences.reduce_motion();
                    this.motion_save_failed = this.preferences.set_reduce_motion(next).is_err();
                    cx.notify();
                })),
            ))
            .when(self.motion_save_failed, |el| {
                el.child(
                    div()
                        .px_4()
                        .pb_3()
                        .text_sm()
                        .text_color(rgb(0xb42318))
                        .child(self.locale.message(Message::ReduceMotionSaveFailed)),
                )
            })
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
                subscription_button("quit-paw", self.locale.message(Message::QuitPaw), false, cx)
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
        #[cfg(any(target_os = "linux", windows))]
        self.reconcile_connection_detail(cx);
        let compact = window.viewport_size().width < px(820.);
        #[cfg(any(target_os = "linux", windows))]
        if self.page != Page::Proxies {
            self.locate_route = None;
        }
        let body = match self.page {
            Page::Overview => self.overview(window.viewport_size().width >= px(960.), cx),
            Page::Proxies => self.proxies(window, cx),
            Page::Profiles => self.profiles(cx),
            Page::Connections => self.connections(cx),
            Page::Rules => self.rules(cx),
            Page::Logs => self.logs(cx),
            Page::Unlock => self.unlock(cx),
            Page::Settings => self.settings(window.viewport_size().width >= px(960.), cx),
        };
        let pane = div().id(("paw-page", self.page as usize));
        #[cfg(any(target_os = "linux", windows))]
        let pane = pane.when(self.page == Page::Proxies, |pane| {
            pane.track_scroll(&self.proxy_group_scroll)
        });
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
                        pane.flex_1()
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

#[cfg(any(target_os = "linux", windows))]
impl Paw {
    fn focus_page_filter(&self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if !page_filter_rendered(
            self.page,
            &self.snapshot,
            self.rule_sets_open,
            self.closed_connections_open,
        ) {
            return false;
        }
        // Leave keystrokes in active text inputs and their IME alone. The
        // registry can lag a focus change, so also check Paw's own handles.
        if window.has_active_dialog(cx)
            || window.has_active_sheet(cx)
            || window.has_focused_input(cx)
            || self
                .subscription_url
                .as_ref()
                .is_some_and(|input| input.focus_handle(cx).is_focused(window))
            || [
                &self.search,
                &self.profile_filter,
                &self.rule_filter,
                &self.connection_filter,
                &self.log_filter,
                &self.probe_url,
                &self.subscription_name,
            ]
            .iter()
            .any(|input| input.focus_handle(cx).is_focused(window))
        {
            return false;
        }
        let input = match self.page {
            Page::Proxies => &self.search,
            Page::Profiles => &self.profile_filter,
            Page::Rules => &self.rule_filter,
            Page::Connections => &self.connection_filter,
            Page::Logs => &self.log_filter,
            _ => return false,
        };
        input.focus_handle(cx).focus(window, cx);
        true
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

// Settings has its own card treatment; the shared form cards also serve other pages.
fn settings_card(title: &'static str, icon: &'static str) -> Div {
    card().p_0().child(
        div()
            .px_4()
            .pt_4()
            .pb_3()
            .flex()
            .items_center()
            .gap_3()
            .child(
                div()
                    .size(px(36.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_lg()
                    .border_1()
                    .border_color(rgb(0xe9bd56))
                    .bg(rgb(0xffe5a3))
                    .shadow_sm()
                    .child(line_icon(icon, BLUE, 18.)),
            )
            .child(
                div()
                    .text_base()
                    .font_weight(FontWeight::BOLD)
                    .text_color(rgb(INK))
                    .child(title),
            ),
    )
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

fn action_button_base(id: impl Into<ElementId>, label: &'static str) -> Stateful<Div> {
    div()
        .id(id)
        .rounded_md()
        .border_1()
        .border_color(rgb(0xb5cafa))
        .font_weight(FontWeight::SEMIBOLD)
        .cursor_pointer()
        .px_4()
        .py_2()
        .child(label)
}

fn subscription_button(
    id: impl Into<ElementId>,
    label: &'static str,
    primary: bool,
    cx: &App,
) -> Button {
    let (background, foreground, hover) = if primary {
        (BLUE, SURFACE, 0x124db5)
    } else {
        (SOFT_BLUE, BLUE, 0xd8e7ff)
    };
    Button::new(id)
        .label(label)
        .custom(
            ButtonCustomVariant::new(cx)
                .color(rgb(background).into())
                .foreground(rgb(foreground).into())
                .hover(rgb(hover).into())
                .active(rgb(hover).into()),
        )
        .bg(rgb(background))
        .border_1()
        .border_color(rgb(0xb5cafa))
        .rounded_md()
        .font_weight(FontWeight::SEMIBOLD)
        .px_4()
        .py_2()
}

fn profile_action_button(
    id: impl Into<ElementId>,
    label: &'static str,
    profile: &str,
) -> SemanticButton {
    semantic_secondary_button(id, label)
        .accessibility_label(format!("{label}: {profile}"))
        .px_4()
        .py_2()
}

fn semantic_secondary_button(
    id: impl Into<ElementId>,
    caption: impl Into<SharedString>,
) -> SemanticButton {
    let caption = caption.into();
    SemanticButton::new(id)
        .accessibility_label(caption.clone())
        .rounded_md()
        .px_3()
        .py_1()
        .border_1()
        .border_color(rgb(0xb5cafa))
        .bg(rgb(SOFT_BLUE))
        .text_color(rgb(BLUE))
        .cursor_pointer()
        .hover(|style| style.bg(rgb(0xd8e7ff)))
        .focus_visible(|style| style.border_color(rgb(BLUE)))
        .child(caption)
}

#[cfg(any(target_os = "linux", windows))]
fn provider_refresh_button(
    locale: Locale,
    name: &str,
    pending: bool,
    busy: bool,
) -> SemanticButton {
    let label = locale.message(if pending {
        Message::ProviderRefreshing
    } else if busy {
        Message::ProviderRefreshBusy
    } else {
        Message::Refresh
    });
    SemanticButton::new(SharedString::from(format!("provider-refresh:{name}")))
        .accessibility_label(format!("{label}: {name}"))
        .disabled(busy)
        .rounded_md()
        .px_4()
        .py_2()
        .font_weight(FontWeight::SEMIBOLD)
        .border_1()
        .border_color(rgb(0xb5cafa))
        .bg(rgb(if busy { BG } else { SOFT_BLUE }))
        .text_color(rgb(if busy { FAINT } else { BLUE }))
        .child(label)
        .when(!busy, |button| {
            button
                .cursor_pointer()
                .hover(|style| style.bg(rgb(0xd8e7ff)))
                .focus_visible(|style| style.border_color(rgb(BLUE)))
        })
}

fn secondary_button(id: impl Into<ElementId>, label: &'static str) -> Stateful<Div> {
    action_button_base(id, label)
        .bg(rgb(SOFT_BLUE))
        .text_color(rgb(BLUE))
        .hover(|style| style.bg(rgb(0xd8e7ff)))
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

fn overview_current_profile(
    running: bool,
    applying: bool,
    active: Option<&std::path::Path>,
) -> Option<&std::path::Path> {
    if running && !applying { active } else { None }
}

fn overview_refresh_path<'a>(
    current: Option<&'a std::path::Path>,
    known_subscriptions: &[PathBuf],
) -> Option<&'a std::path::Path> {
    current.filter(|path| {
        known_subscriptions
            .iter()
            .any(|known| known.as_path() == *path)
    })
}

#[cfg(any(target_os = "linux", windows))]
fn overview_route_summary(
    mode: Option<meow_paw::engine::controller::RoutingMode>,
    inventory: Option<&meow_paw::engine::controller::Inventory>,
) -> (Message, Vec<String>, usize) {
    use meow_paw::engine::controller::RoutingMode;
    const PREVIEW_LIMIT: usize = 3;
    match mode {
        Some(RoutingMode::Direct) => (Message::OverviewDirectRoute, Vec::new(), 0),
        Some(mode @ (RoutingMode::Rule | RoutingMode::Global)) => {
            let mut selections = Vec::new();
            let mut excess = 0;
            if let Some(inventory) = inventory {
                for (name, group) in mode_groups(inventory, Some(mode)) {
                    if group.hidden
                        || name.trim().is_empty()
                        || (mode == RoutingMode::Global && name.as_str() != "GLOBAL")
                    {
                        continue;
                    }
                    if let Some(value) = group
                        .now
                        .as_deref()
                        .filter(|value| !value.trim().is_empty())
                    {
                        if selections.len() < PREVIEW_LIMIT {
                            selections.push(format!("{name} → {value}"));
                        } else {
                            excess += 1;
                        }
                    }
                }
            }
            if selections.is_empty() {
                (Message::OverviewRouteUnavailable, selections, 0)
            } else {
                (
                    if mode == RoutingMode::Global {
                        Message::OverviewGlobalSelection
                    } else {
                        Message::OverviewRouteSelections
                    },
                    selections,
                    excess,
                )
            }
        }
        None => (Message::OverviewRouteUnavailable, Vec::new(), 0),
    }
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
type RouteDelayMap<'a> = std::collections::HashMap<&'a str, Option<u32>>;

#[cfg(any(target_os = "linux", windows))]
fn unambiguous_route_delays(
    inventory: &meow_paw::engine::controller::Inventory,
) -> RouteDelayMap<'_> {
    let mut delays = std::collections::HashMap::new();
    let routes = inventory
        .groups
        .iter()
        .chain(inventory.nodes.iter().map(|node| (&node.name, &node.proxy)));
    for (name, proxy) in routes {
        delays
            .entry(name.as_str())
            .and_modify(|delay| *delay = None)
            .or_insert_with(|| proxy.latest_delay_ms().map(u32::from));
    }
    delays
}

#[cfg(any(target_os = "linux", windows))]
fn route_delays<'a>(
    order: ProxyOrder,
    history_delays: &'a RouteDelayMap<'a>,
    inventory: &meow_paw::engine::controller::Inventory,
    comparison: Option<&'a meow_paw::app::GroupProbeResults>,
) -> std::borrow::Cow<'a, RouteDelayMap<'a>> {
    if order == ProxyOrder::Delay
        && let Some(comparison) = comparison
    {
        // A comparison shares one URL and run; don't mix unrelated history.
        let mut delays = std::collections::HashMap::new();
        for (target, status) in comparison.rows() {
            if meow_paw::app::unique_proxy(inventory, &target.name).as_ref() == Some(&target.origin)
                && let Some(meow_paw::app::ProxyProbeStatus::Measured(ms)) = status
            {
                delays.insert(target.name.as_str(), Some(*ms));
            }
        }
        std::borrow::Cow::Owned(delays)
    } else {
        std::borrow::Cow::Borrowed(history_delays)
    }
}

#[cfg(any(target_os = "linux", windows))]
fn route_delay(delays: &RouteDelayMap<'_>, name: &str) -> Option<u32> {
    delays.get(name).copied().flatten()
}

#[cfg(any(target_os = "linux", windows))]
fn sort_route_members(
    members: &mut [(usize, &str)],
    order: ProxyOrder,
    delays: &RouteDelayMap<'_>,
) {
    match order {
        ProxyOrder::Configured => {}
        ProxyOrder::Delay => {
            members.sort_by_key(|(index, name)| {
                let delay = route_delay(delays, name);
                (delay.is_none(), delay, *index)
            });
        }
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
fn matching_connections<'a>(
    connections: impl Iterator<Item = &'a meow_paw::engine::controller::Connection>,
    query: &str,
) -> Vec<(&'a meow_paw::engine::controller::Connection, String, String)> {
    connections
        .filter_map(|c| {
            let target = if c.metadata.host.is_empty() {
                &c.metadata.destination_ip
            } else {
                &c.metadata.host
            };
            let destination = destination_label(target, &c.metadata.destination_port);
            let route = c.chains.join(" / ");
            (route_matches(&destination, query)
                || route_matches(&route, query)
                || route_matches(&c.rule, query))
            .then_some((c, destination, route))
        })
        .collect()
}

#[cfg(any(target_os = "linux", windows))]
fn detail_row_index(
    selection: &ConnectionDetailSelection,
    snapshot: &meow_paw::app::Snapshot,
    closed: bool,
) -> Option<usize> {
    match selection {
        ConnectionDetailSelection::Closed(key) if closed => snapshot
            .connection_history
            .recent_keyed()
            .position(|(candidate, _)| candidate == *key),
        ConnectionDetailSelection::Active { sample, index } if !closed => {
            let captured = sample.get(*index)?;
            let current = snapshot.connections.as_ref()?;
            if std::sync::Arc::ptr_eq(sample, current) {
                return current.get(*index).map(|_| *index);
            }
            if captured.id.is_empty()
                || sample.iter().filter(|row| row.id == captured.id).count() != 1
            {
                return None;
            }
            let mut matches = current
                .iter()
                .enumerate()
                .filter(|(_, row)| row.id == captured.id);
            let (index, row) = matches.next()?;
            if matches.next().is_some() || !same_connection_identity(captured, row) {
                return None;
            }
            Some(index)
        }
        _ => None,
    }
}

#[cfg(any(target_os = "linux", windows))]
fn same_connection_identity(
    a: &meow_paw::engine::controller::Connection,
    b: &meow_paw::engine::controller::Connection,
) -> bool {
    a.id == b.id
        && a.metadata.host == b.metadata.host
        && a.metadata.destination_ip == b.metadata.destination_ip
        && a.metadata.destination_port == b.metadata.destination_port
        && a.metadata.network == b.metadata.network
        && a.chains == b.chains
        && a.rule == b.rule
}

#[cfg(any(target_os = "linux", windows))]
fn connection_transport<'a>(network: Option<&'a str>, unknown: &'a str) -> &'a str {
    network
        .filter(|value| {
            !value.is_empty()
                && value.len() <= 64
                && value
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
        })
        .unwrap_or(unknown)
}

#[cfg(any(target_os = "linux", windows))]
fn connection_total(value: Option<u64>, locale: Locale) -> String {
    value
        .map(bytes)
        .unwrap_or_else(|| locale.message(Message::UnknownValue).to_owned())
}

#[cfg(any(target_os = "linux", windows))]
fn small_connection_button(id: impl Into<ElementId>, label: &'static str, cx: &App) -> Button {
    subscription_button(id, label, false, cx)
        .small()
        .px_2()
        .py_1()
}

#[cfg(any(target_os = "linux", windows))]
fn connection_detail_area(
    connection: &meow_paw::engine::controller::Connection,
    destination: &str,
    route: &str,
    closed: bool,
    locale: Locale,
    close: Button,
) -> Div {
    let unknown = locale.message(Message::UnknownValue);
    let field = |label: &'static str, value: String| {
        div()
            .flex()
            .gap_3()
            .items_start()
            .text_sm()
            .child(
                div()
                    .w(px(105.))
                    .flex_shrink_0()
                    .text_color(rgb(FAINT))
                    .child(label),
            )
            .child(div().flex_1().min_w_0().child(value))
    };
    let value = |text: &str| {
        if text.is_empty() || text == "—" {
            unknown.to_owned()
        } else {
            text.to_owned()
        }
    };
    div()
        .px_4()
        .py_3()
        .border_b_1()
        .border_color(rgb(LINE))
        .bg(rgb(CHROME))
        .flex()
        .flex_col()
        .gap_2()
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap_2()
                .child(
                    div()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(locale.message(Message::ConnectionDetails)),
                )
                .child(close),
        )
        .child(field(
            locale.message(Message::DestinationColumn),
            value(destination),
        ))
        .child(field(locale.message(Message::RouteColumn), value(route)))
        .child(field(
            locale.message(Message::RuleColumn),
            value(&connection.rule),
        ))
        .child(field(
            locale.message(Message::Transport),
            connection_transport(connection.metadata.network.as_deref(), unknown).to_owned(),
        ))
        .child(
            div()
                .text_xs()
                .text_color(rgb(FAINT))
                .child(locale.message(if closed {
                    Message::ClosedSampleTotals
                } else {
                    Message::CurrentSampleTotals
                })),
        )
        .child(field(
            locale.message(Message::Upload),
            connection_total(connection.upload, locale),
        ))
        .child(field(
            locale.message(Message::Download),
            connection_total(connection.download, locale),
        ))
}

#[cfg(any(target_os = "linux", windows))]
fn connection_empty_messages(closed: bool, total: usize) -> (Message, Message) {
    if total == 0 {
        if closed {
            (
                Message::NoClosedConnections,
                Message::NoClosedConnectionsDetail,
            )
        } else {
            (
                Message::NoActiveConnections,
                Message::ListenerConnectionsDetail,
            )
        }
    } else {
        (
            Message::NoMatchingConnections,
            Message::ConnectionFilterDetail,
        )
    }
}

#[cfg(any(target_os = "linux", windows))]
fn connection_row(a: Div, b: Div, c: Div, action: Option<AnyElement>, closed: bool) -> Div {
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
        .child(
            div()
                .w(px(if closed { 80. } else { 132. }))
                .flex_shrink_0()
                .children(action),
        )
}

#[cfg(any(target_os = "linux", windows))]
fn rule_row(line: Div, kind: Div, payload: Div, policy: Div) -> Div {
    div()
        .flex()
        .gap_3()
        .px_3()
        .py_2()
        .items_center()
        .text_sm()
        .child(line.w(px(60.)).flex_shrink_0())
        .child(kind.w(px(140.)).min_w_0().flex_shrink(1.))
        .child(payload.flex_1().min_w_0())
        .child(policy.w(px(112.)).min_w_0().flex_shrink(1.))
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
    #[cfg(any(target_os = "linux", windows))]
    #[test]
    fn rule_set_text_hides_url_like_and_control_values() {
        use super::safe_rule_set_text;
        assert_eq!(safe_rule_set_text("work rules", "hidden"), "work rules");
        assert_eq!(
            safe_rule_set_text("2026-11-26T12:30:00Z", "hidden"),
            "2026-11-26T12:30:00Z"
        );
        for value in [
            "https://example.test/token",
            "name?token=secret",
            "user@host",
            "line\nbreak",
        ] {
            assert_eq!(safe_rule_set_text(value, "hidden"), "hidden");
        }
    }

    use super::Page;

    #[cfg(any(target_os = "linux", windows))]
    #[test]
    fn shortcut_filter_visibility_tracks_rendered_page_states() {
        use super::page_filter_rendered;
        use meow_paw::app::{EngineStatus, LogHistory, Snapshot};
        use meow_paw::engine::controller::Inventory;
        use std::sync::Arc;

        let mut snapshot = Snapshot::default();
        let visible = |page, snapshot: &Snapshot, sets, closed| {
            page_filter_rendered(page, snapshot, sets, closed)
        };
        for page in Page::ALL {
            assert_eq!(
                visible(page, &snapshot, false, false),
                page == Page::Profiles
            );
        }
        snapshot.inventory = Some(Arc::new(Inventory::default()));
        snapshot.logs = Some(LogHistory::default());
        snapshot.rules = Some(Arc::new(vec![]));
        snapshot.connections = Some(Arc::new(vec![]));
        assert!(visible(Page::Proxies, &snapshot, false, false));
        assert!(visible(Page::Logs, &snapshot, false, false));
        assert!(!visible(Page::Rules, &snapshot, false, false));
        assert!(!visible(Page::Connections, &snapshot, false, true));
        snapshot.engine = EngineStatus::Running { mixed_port: 7890 };
        assert!(visible(Page::Rules, &snapshot, false, false));
        assert!(!visible(Page::Rules, &snapshot, true, false));
        snapshot.rules = None;
        assert!(!visible(Page::Rules, &snapshot, false, false));
        assert!(visible(Page::Connections, &snapshot, false, false));
        snapshot.connections = None;
        assert!(!visible(Page::Connections, &snapshot, false, false));
        assert!(visible(Page::Connections, &snapshot, false, true));
        assert!(visible(Page::Profiles, &snapshot, false, false));
        for page in [Page::Overview, Page::Unlock, Page::Settings] {
            assert!(!visible(page, &snapshot, false, false));
        }
    }

    #[test]
    fn profile_filter_uses_only_displayed_names_and_preserves_editors() {
        use super::{
            Locale, ProfileListState, SubscriptionMetadata, local_profile_display_name,
            profile_card_visible, profile_list_state, profile_name_matches,
            subscription_display_name,
        };
        use std::path::Path;

        let id = "0123456789abcdef0123456789abcdef".parse().unwrap();
        let metadata = SubscriptionMetadata {
            name: Some("办公 Work".into()),
            interval: None,
        };
        let names = [
            local_profile_display_name(Path::new("/private/secret/Home.yaml")),
            local_profile_display_name(Path::new("/other/Home.yaml")),
            subscription_display_name(Locale::En, id, Some(&metadata)),
            subscription_display_name(Locale::En, id, None),
        ];
        let count = |query| {
            names
                .iter()
                .filter(|name| profile_name_matches(name, query))
                .count()
        };
        assert_eq!(count(""), 4);
        assert_eq!(count("  "), 4);
        assert_eq!(count(" HOME "), 2);
        assert_eq!(count("办公"), 1);
        assert_eq!(count("work"), 1);
        assert_eq!(count("01234567"), 1);
        for hidden in [
            "private",
            "secret",
            "/other/",
            "https://example.test/token",
            "contents",
        ] {
            assert_eq!(count(hidden), 0);
        }
        assert_eq!(profile_list_state(0, 0), ProfileListState::Empty);
        assert_eq!(
            profile_list_state(names.len(), 0),
            ProfileListState::NoMatches
        );
        assert_eq!(
            profile_list_state(names.len(), count("home")),
            ProfileListState::Matches
        );
        assert!(profile_card_visible(false, true));
        assert!(!profile_card_visible(false, false));
    }

    #[test]
    fn subscription_path_enumeration_does_not_read_records() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("0123456789abcdef0123456789abcdef.json");
        // This is deliberately not a valid subscription record.
        std::fs::write(&path, b"invalid").unwrap();
        assert_eq!(
            super::subscription_paths(dir.path().into()).unwrap(),
            vec![path]
        );
    }

    #[test]
    fn older_metadata_batches_cannot_replace_manual_results() {
        use super::{DisplayInterval, SubscriptionMetadata, merge_subscription_metadata};
        use std::{collections::HashMap, path::PathBuf};

        let refreshed = PathBuf::from("refreshed.json");
        let failed = PathBuf::from("failed.json");
        let untouched = PathBuf::from("untouched.json");
        let old = Some(SubscriptionMetadata {
            name: None,
            interval: Some(DisplayInterval::Provider(60)),
        });
        let new = Some(SubscriptionMetadata {
            name: Some("New name".into()),
            interval: Some(DisplayInterval::Provider(120)),
        });
        let mut current = HashMap::from([(refreshed.clone(), new.clone()), (failed.clone(), None)]);
        merge_subscription_metadata(
            &mut current,
            vec![
                (refreshed.clone(), old.clone()),
                (failed.clone(), old.clone()),
                (untouched.clone(), old.clone()),
            ],
        );
        assert_eq!(current[&refreshed], new);
        assert_eq!(current[&failed], None);
        assert_eq!(current[&untouched], old);
    }

    #[test]
    fn subscription_name_falls_back_to_opaque_id_when_cleared() {
        let id = "0123456789abcdef0123456789abcdef".parse().unwrap();
        let mut metadata = super::SubscriptionMetadata {
            name: Some("Home".into()),
            interval: None,
        };
        assert_eq!(
            super::subscription_display_name(super::Locale::En, id, Some(&metadata)),
            "Home"
        );
        metadata.name = None;
        assert_eq!(
            super::subscription_display_name(super::Locale::En, id, Some(&metadata)),
            "Subscription 01234567"
        );
        assert_eq!(
            super::subscription_display_name(super::Locale::ZhCn, id, None),
            "订阅 01234567"
        );
    }

    #[test]
    fn overview_current_profile_requires_a_confirmed_running_identity() {
        use super::overview_current_profile;
        use std::path::Path;
        let active = Some(Path::new("active"));
        assert_eq!(overview_current_profile(true, false, active), active);
        assert_eq!(overview_current_profile(false, false, active), None);
        assert_eq!(overview_current_profile(true, true, active), None);
        assert_eq!(overview_current_profile(true, false, None), None);
    }

    #[test]
    fn profile_running_badge_tracks_confirmed_identity_during_live_apply() {
        use super::overview_current_profile;
        use std::path::Path;

        let previous = Path::new("previous");
        let candidate = Path::new("candidate");

        // Selecting a candidate does not change the confirmed running profile.
        assert_eq!(
            overview_current_profile(true, false, Some(previous)),
            Some(previous)
        );
        assert_ne!(
            overview_current_profile(true, false, Some(previous)),
            Some(candidate)
        );
        // An apply may already have changed Meow before its result is known.
        assert_eq!(overview_current_profile(true, true, Some(previous)), None);
        assert_eq!(
            overview_current_profile(true, false, Some(candidate)),
            Some(candidate)
        );
        // A rejected apply retains the previous identity; an unconfirmed one does not.
        assert_eq!(
            overview_current_profile(true, false, Some(previous)),
            Some(previous)
        );
        assert_eq!(overview_current_profile(true, false, None), None);
    }

    #[test]
    fn overview_refresh_requires_the_confirmed_profile_to_be_a_known_subscription() {
        use super::{overview_current_profile, overview_refresh_path};
        use std::path::{Path, PathBuf};
        let active = Path::new("active");
        let next = PathBuf::from("next");
        let known = vec![next];
        assert_eq!(overview_refresh_path(Some(active), &known), None);
        assert_eq!(overview_refresh_path(None, &known), None);
        assert_eq!(
            overview_refresh_path(Some(Path::new("next")), &known),
            Some(Path::new("next"))
        );
        assert_eq!(
            overview_refresh_path(
                overview_current_profile(true, true, Some(Path::new("next"))),
                &known
            ),
            None
        );
    }

    #[test]
    #[cfg(any(target_os = "linux", windows))]
    fn overview_routes_report_mode_and_group_selections_without_guessing() {
        use super::overview_route_summary;
        use meow_paw::engine::controller::{Inventory, Proxy, RoutingMode};
        use meow_paw::i18n::Message;
        let mut inventory = Inventory::default();
        inventory.groups.insert(
            "GLOBAL".into(),
            Proxy {
                now: Some("chosen".into()),
                ..Proxy::default()
            },
        );
        inventory.groups.insert(
            "work".into(),
            Proxy {
                now: Some("node".into()),
                ..Proxy::default()
            },
        );
        inventory.groups.insert(
            "hidden".into(),
            Proxy {
                now: Some("secret".into()),
                hidden: true,
                ..Proxy::default()
            },
        );
        assert_eq!(
            overview_route_summary(None, Some(&inventory)),
            (Message::OverviewRouteUnavailable, vec![], 0)
        );
        assert_eq!(
            overview_route_summary(Some(RoutingMode::Direct), Some(&inventory)),
            (Message::OverviewDirectRoute, vec![], 0)
        );
        assert_eq!(
            overview_route_summary(Some(RoutingMode::Global), Some(&inventory)),
            (
                Message::OverviewGlobalSelection,
                vec!["GLOBAL → chosen".into()],
                0
            )
        );
        assert_eq!(
            overview_route_summary(Some(RoutingMode::Rule), Some(&inventory)),
            (
                Message::OverviewRouteSelections,
                vec!["work → node".into()],
                0
            )
        );
        assert_eq!(
            overview_route_summary(Some(RoutingMode::Rule), None),
            (Message::OverviewRouteUnavailable, vec![], 0)
        );
    }

    #[test]
    #[cfg(any(target_os = "linux", windows))]
    fn overview_rule_preview_is_bounded_sorted_and_counts_only_available_groups() {
        use super::overview_route_summary;
        use meow_paw::engine::controller::{Inventory, Proxy, RoutingMode};
        use meow_paw::i18n::Message;

        let mut inventory = Inventory::default();
        for index in (0..30).rev() {
            inventory.groups.insert(
                format!("group-{index:02}"),
                Proxy {
                    now: Some(format!("node-{index:02}")),
                    ..Proxy::default()
                },
            );
        }
        inventory.groups.insert(
            "00-hidden".into(),
            Proxy {
                now: Some("secret".into()),
                hidden: true,
                ..Proxy::default()
            },
        );
        inventory
            .groups
            .insert("01-missing".into(), Proxy::default());
        inventory.groups.insert(
            "02-empty".into(),
            Proxy {
                now: Some("  ".into()),
                ..Proxy::default()
            },
        );
        assert_eq!(
            overview_route_summary(Some(RoutingMode::Rule), Some(&inventory)),
            (
                Message::OverviewRouteSelections,
                vec![
                    "group-00 → node-00".into(),
                    "group-01 → node-01".into(),
                    "group-02 → node-02".into()
                ],
                27,
            )
        );
        inventory
            .groups
            .retain(|name, _| name == "group-00" || name == "group-01");
        assert_eq!(
            overview_route_summary(Some(RoutingMode::Rule), Some(&inventory)),
            (
                Message::OverviewRouteSelections,
                vec!["group-00 → node-00".into(), "group-01 → node-01".into()],
                0,
            )
        );
        inventory.groups.clear();
        assert_eq!(
            overview_route_summary(Some(RoutingMode::Rule), Some(&inventory)),
            (Message::OverviewRouteUnavailable, vec![], 0)
        );
    }

    #[test]
    #[cfg(any(target_os = "linux", windows))]
    fn overview_global_reports_only_visible_global_selection() {
        use super::overview_route_summary;
        use meow_paw::engine::controller::{Inventory, Proxy, RoutingMode};
        use meow_paw::i18n::Message;

        let mut inventory = Inventory::default();
        inventory.groups.insert(
            "GLOBAL".into(),
            Proxy {
                now: Some("work".into()),
                ..Proxy::default()
            },
        );
        inventory.groups.insert(
            "work".into(),
            Proxy {
                now: Some("node".into()),
                ..Proxy::default()
            },
        );
        assert_eq!(
            overview_route_summary(Some(RoutingMode::Global), Some(&inventory)),
            (
                Message::OverviewGlobalSelection,
                vec!["GLOBAL → work".into()],
                0
            )
        );
        inventory.groups.get_mut("GLOBAL").unwrap().hidden = true;
        assert_eq!(
            overview_route_summary(Some(RoutingMode::Global), Some(&inventory)),
            (Message::OverviewRouteUnavailable, vec![], 0)
        );
        inventory.groups.remove("work");
        assert_eq!(
            overview_route_summary(Some(RoutingMode::Rule), Some(&inventory)),
            (Message::OverviewRouteUnavailable, vec![], 0)
        );
    }

    #[test]
    fn import_results_respect_edits_and_cancel() {
        use super::{ImportEditorEffect, Message, import_editor_effect};

        // Exercise the same result decision used by the editor, with synthetic input.
        let submitted = "https://example.invalid/feed?token=synthetic";
        let newer = "https://example.invalid/other?token=new";
        let failure = Message::SubscriptionDownloadFailed;
        let mut input = submitted.to_owned();
        let mut notice = None;
        let start = 10;
        if let ImportEditorEffect::Failure(message) =
            import_editor_effect(start, start, true, Some(failure))
        {
            notice = Some(message);
        }
        assert_eq!(input, submitted);
        assert_eq!(notice, Some(failure));

        if import_editor_effect(start, start, true, None) == ImportEditorEffect::Clear {
            input.clear();
        }
        assert!(input.is_empty());

        input = newer.to_owned();
        // Editing, including changing back to the submitted value, invalidates the result.
        for result in [None, Some(failure)] {
            assert_eq!(
                import_editor_effect(start, 12, true, result),
                ImportEditorEffect::Unchanged,
            );
            assert_eq!(
                import_editor_effect(start, 11, false, result),
                ImportEditorEffect::Unchanged,
            );
            // Cancel followed by reopening is also a different entry.
            assert_eq!(
                import_editor_effect(start, 11, true, result),
                ImportEditorEffect::Unchanged,
            );
        }
        assert_eq!(input, newer);
    }

    #[test]
    fn subscription_download_feedback_is_localized_and_does_not_expose_input() {
        use super::{Locale, Message, subscription_download_feedback};
        use meow_paw::{profile::ProfileError, subscription::DownloadError};

        let secret = "https://example.invalid/profile?token=synthetic-secret";
        let message = subscription_download_feedback(DownloadError::InvalidProfile(
            ProfileError::UnsupportedField(secret.to_owned()),
        ));
        assert_eq!(message, Message::SubscriptionInvalidProfile);
        for locale in [Locale::En, Locale::ZhCn] {
            assert!(!locale.message(message).contains(secret));
        }
        for (error, expected) in [
            (DownloadError::Request, Message::SubscriptionDownloadFailed),
            (DownloadError::TooLarge, Message::SubscriptionTooLarge),
            (DownloadError::InvalidText, Message::SubscriptionInvalidText),
            (DownloadError::NoProxies, Message::SubscriptionNoProxies),
            (DownloadError::InvalidUrl, Message::InvalidSubscriptionUrl),
        ] {
            assert_eq!(subscription_download_feedback(error), expected);
        }
    }

    #[test]
    fn subscription_write_feedback_is_retryable_and_provider_safe() {
        use super::{Locale, Message, subscription_write_feedback};
        use std::io::{Error, ErrorKind};

        let secret = "https://example.invalid/feed?token=synthetic-secret";
        for fallback in [
            Message::SubscriptionSaveFailed,
            Message::SubscriptionNameSaveFailed,
            Message::SubscriptionRefreshSaveFailed,
        ] {
            let message =
                subscription_write_feedback(Error::new(ErrorKind::WouldBlock, secret), fallback);
            assert_eq!(message, Message::SubscriptionWriteConflict);
            for locale in [Locale::En, Locale::ZhCn] {
                assert!(!locale.message(message).contains(secret));
            }
            assert_eq!(
                subscription_write_feedback(Error::other(secret), fallback),
                fallback
            );
        }
    }

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
            connection_empty_messages(false, 0),
            (
                Message::NoActiveConnections,
                Message::ListenerConnectionsDetail
            )
        );
        assert_eq!(
            connection_empty_messages(true, 0),
            (
                Message::NoClosedConnections,
                Message::NoClosedConnectionsDetail
            )
        );
        assert_eq!(
            connection_empty_messages(true, 3),
            (
                Message::NoMatchingConnections,
                Message::ConnectionFilterDetail
            )
        );
        assert_eq!(
            connection_empty_messages(false, 3),
            (
                Message::NoMatchingConnections,
                Message::ConnectionFilterDetail
            )
        );
    }

    #[test]
    #[cfg(any(target_os = "linux", windows))]
    fn connection_filter_matches_destination_route_and_rule_for_any_pane() {
        use super::matching_connections;
        use meow_paw::engine::controller::{Connection, Metadata};
        use serde_json::json;

        let rows = [
            Connection {
                metadata: Metadata {
                    host: "Example.Test".into(),
                    destination_ip: "192.0.2.1".into(),
                    destination_port: json!(443),
                    ..Default::default()
                },
                chains: vec!["Proxy".into(), "出口".into()],
                rule: "Domain-Suffix".into(),
                ..Default::default()
            },
            Connection {
                metadata: Metadata {
                    destination_ip: "2001:db8::1".into(),
                    destination_port: json!(80),
                    ..Default::default()
                },
                ..Default::default()
            },
        ];
        for query in ["example.test:443", "proxy / 出口", "domain-suffix"] {
            assert_eq!(matching_connections(rows.iter(), query).len(), 1);
        }
        assert_eq!(
            matching_connections(rows.iter(), "[2001:db8::1]:80").len(),
            1
        );
        assert!(matching_connections(rows.iter(), "192.0.2.1").is_empty());
        assert!(matching_connections(rows.iter(), "missing").is_empty());
        assert_eq!(matching_connections(rows.iter(), "").len(), 2);
    }

    #[test]
    #[cfg(any(target_os = "linux", windows))]
    fn connection_details_keep_unknown_and_zero_distinct() {
        use super::{connection_total, connection_transport};
        use meow_paw::i18n::Locale;
        assert_eq!(connection_total(Some(0), Locale::En), "0 B");
        assert_eq!(connection_total(None, Locale::En), "Unknown");
        assert_eq!(connection_total(None, Locale::ZhCn), "未知");
        assert_eq!(connection_transport(Some("tcp"), "Unknown"), "tcp");
        assert_eq!(
            connection_transport(Some("tcp\nsecret"), "Unknown"),
            "Unknown"
        );
        assert_eq!(connection_transport(None, "Unknown"), "Unknown");
    }

    #[test]
    #[cfg(any(target_os = "linux", windows))]
    fn active_detail_tracks_only_an_unambiguous_unchanged_identity() {
        use super::{ConnectionDetailSelection, detail_row_index};
        use meow_paw::{app::Snapshot, engine::controller::Connection};
        use std::sync::Arc;
        let first = Connection {
            id: "one".into(),
            upload: Some(0),
            ..Default::default()
        };
        let sample = Arc::new(vec![first.clone()]);
        let selected = ConnectionDetailSelection::Active {
            sample: sample.clone(),
            index: 0,
        };
        let mut snapshot = Snapshot::default();
        snapshot.connections = Some(sample);
        assert_eq!(detail_row_index(&selected, &snapshot, false), Some(0));
        let mut updated = first.clone();
        updated.upload = Some(10);
        snapshot.connections = Some(Arc::new(vec![updated.clone()]));
        assert_eq!(detail_row_index(&selected, &snapshot, false), Some(0));
        snapshot.connections = Some(Arc::new(vec![updated.clone(), updated.clone()]));
        assert_eq!(detail_row_index(&selected, &snapshot, false), None);
        updated.metadata.host = "different".into();
        snapshot.connections = Some(Arc::new(vec![updated]));
        assert_eq!(detail_row_index(&selected, &snapshot, false), None);
        snapshot.connections = Some(Arc::new(vec![]));
        assert_eq!(detail_row_index(&selected, &snapshot, false), None);
        assert_eq!(detail_row_index(&selected, &snapshot, true), None);
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
    fn proxy_order_uses_only_current_comparable_direct_nodes() {
        use super::{
            ProxyOrder, route_delays, sort_route_members, unambiguous_route_delays,
            unique_route_members,
        };
        use meow_paw::app::{GroupProbeResults, ProxyProbeStatus, ProxyProbeTarget};
        use meow_paw::engine::controller::{Inventory, Node, Origin, Proxy};
        let proxy: Proxy =
            serde_json::from_value(serde_json::json!({"history": [{"delay": 1}]})).unwrap();
        let nodes = [
            ("A", Origin::Local),
            ("B", Origin::Provider("feed".into())),
            ("Tie", Origin::Local),
            ("Changed", Origin::Local),
            ("Ambiguous", Origin::Local),
            ("Ambiguous", Origin::Provider("feed".into())),
        ];
        let inventory = Inventory {
            nodes: nodes
                .into_iter()
                .map(|(name, origin)| Node {
                    name: name.into(),
                    origin,
                    proxy: proxy.clone(),
                })
                .collect(),
            groups: [("Nested".into(), proxy)].into(),
            ..Default::default()
        };
        let targets: Vec<_> = [
            ("A", Origin::Local, 100),
            ("B", Origin::Provider("feed".into()), 10),
            ("Tie", Origin::Local, 10),
            ("Changed", Origin::Provider("feed".into()), 1),
            ("Ambiguous", Origin::Local, 1),
            ("Nested", Origin::Local, 1),
        ]
        .into_iter()
        .map(|(name, origin, ms)| {
            (
                ProxyProbeTarget {
                    name: name.into(),
                    origin,
                },
                ms,
            )
        })
        .collect();
        let mut results = GroupProbeResults::new(
            7,
            1,
            url::Url::parse("https://example.com/").unwrap(),
            targets.iter().map(|(target, _)| target.clone()).collect(),
        )
        .unwrap();
        for (index, (target, ms)) in targets.iter().enumerate() {
            assert!(results.begin(index));
            assert!(results.finish(7, 1, index, target, ProxyProbeStatus::Measured(*ms)));
        }
        let names =
            ["A", "Unknown", "B", "Tie", "Changed", "Ambiguous", "Nested"].map(str::to_owned);
        let history_delays = unambiguous_route_delays(&inventory);
        let sorted = |order, comparison| {
            let mut members = unique_route_members(&names);
            let delays = route_delays(order, &history_delays, &inventory, comparison);
            sort_route_members(&mut members, order, &delays);
            members
                .into_iter()
                .map(|(_, name)| name)
                .collect::<Vec<_>>()
        };
        assert_eq!(
            sorted(ProxyOrder::Delay, Some(&results)),
            ["B", "Tie", "A", "Unknown", "Changed", "Ambiguous", "Nested"]
        );
        assert_eq!(
            sorted(ProxyOrder::Delay, None),
            ["A", "B", "Tie", "Changed", "Nested", "Unknown", "Ambiguous"]
        );
        assert_eq!(
            sorted(ProxyOrder::Configured, Some(&results)),
            ["A", "Unknown", "B", "Tie", "Changed", "Ambiguous", "Nested"]
        );
        assert_eq!(
            sorted(ProxyOrder::Name, Some(&results)),
            ["A", "Ambiguous", "B", "Changed", "Nested", "Tie", "Unknown"]
        );
    }

    #[test]
    #[cfg(any(target_os = "linux", windows))]
    fn latency_cards_and_order_share_the_selected_delay_source() {
        use super::{
            ProxyOrder, route_delay, route_delays, sort_route_members, unambiguous_route_delays,
            unique_route_members,
        };
        use meow_paw::app::{GroupProbeResults, ProxyProbeStatus, ProxyProbeTarget};
        use meow_paw::engine::controller::{Inventory, Node, Origin};

        let nodes = [("A", 5), ("B", 90), ("C", 30)];
        let inventory = Inventory {
            nodes: nodes
                .into_iter()
                .map(|(name, delay)| Node {
                    name: name.into(),
                    origin: Origin::Local,
                    proxy: serde_json::from_value(serde_json::json!({
                        "history": [{"delay": delay}]
                    }))
                    .unwrap(),
                })
                .collect(),
            ..Default::default()
        };
        let targets: Vec<_> = ["A", "B", "C"]
            .into_iter()
            .map(|name| ProxyProbeTarget {
                name: name.into(),
                origin: Origin::Local,
            })
            .collect();
        let mut comparison = GroupProbeResults::new(
            7,
            1,
            url::Url::parse("https://example.com/").unwrap(),
            targets.clone(),
        )
        .unwrap();
        for (index, status) in [
            ProxyProbeStatus::Measured(70),
            ProxyProbeStatus::Measured(10),
            ProxyProbeStatus::Unavailable,
        ]
        .into_iter()
        .enumerate()
        {
            assert!(comparison.begin(index));
            assert!(comparison.finish(7, 1, index, &targets[index], status));
        }

        let history = unambiguous_route_delays(&inventory);
        let names = ["A", "B", "C"].map(str::to_owned);
        let check = |order, comparison, expected| {
            let delays = route_delays(order, &history, &inventory, comparison);
            let mut members = unique_route_members(&names);
            sort_route_members(&mut members, order, &delays);
            let cards: Vec<_> = members
                .iter()
                .map(|(_, name)| (*name, route_delay(&delays, name)))
                .collect();
            assert_eq!(cards, expected);
        };
        check(
            ProxyOrder::Delay,
            Some(&comparison),
            vec![("B", Some(10)), ("A", Some(70)), ("C", None)],
        );
        check(
            ProxyOrder::Delay,
            None,
            vec![("A", Some(5)), ("C", Some(30)), ("B", Some(90))],
        );
        check(
            ProxyOrder::Configured,
            Some(&comparison),
            vec![("A", Some(5)), ("B", Some(90)), ("C", Some(30))],
        );
        check(
            ProxyOrder::Name,
            Some(&comparison),
            vec![("A", Some(5)), ("B", Some(90)), ("C", Some(30))],
        );
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
    #[test]
    #[cfg(any(target_os = "linux", windows))]
    fn log_filters_are_local_combined_and_preserve_order() {
        use super::{LogLevelFilter as Level, filter_logs};
        use meow_paw::engine::controller::LogEntry;
        let entries = vec![
            LogEntry {
                time: "12:03".into(),
                level: "INFO".into(),
                message: "Connected Alpha".into(),
            },
            LogEntry {
                time: "12:02".into(),
                level: "warning".into(),
                message: "Alpha retry".into(),
            },
            LogEntry {
                time: "12:01".into(),
                level: "error".into(),
                message: "Beta failed".into(),
            },
        ];
        let original = entries.clone();
        assert_eq!(
            filter_logs(&entries, "", Level::All),
            entries.iter().collect::<Vec<_>>()
        );
        assert_eq!(filter_logs(&entries, "  ALPHA  ", Level::All).len(), 2);
        assert_eq!(
            filter_logs(&entries, "12:02", Level::All)[0].message,
            "Alpha retry"
        );
        assert_eq!(
            filter_logs(&entries, "info", Level::All)[0].message,
            "Connected Alpha"
        );
        assert_eq!(filter_logs(&entries, "alpha", Level::Warn)[0].time, "12:02");
        assert_eq!(
            filter_logs(&entries, "", Level::Error)[0].message,
            "Beta failed"
        );
        assert!(filter_logs(&entries, "", Level::Debug).is_empty());
        assert!(filter_logs(&entries, "beta", Level::Info).is_empty());
        assert!(Level::Error.matches("ERR"));
        assert!(Level::Warn.matches("warn"));
        assert!(!Level::Info.matches("information"));
        assert!(filter_logs(&[], "", Level::All).is_empty());
        assert_eq!(entries, original);
    }
    #[test]
    #[cfg(any(target_os = "linux", windows))]
    fn log_review_freezes_filters_orders_and_catches_up() {
        use super::{LogLevelFilter as Level, LogOrder, LogReview};
        use meow_paw::engine::controller::LogEntry;

        let entry = |message: &str, level: &str| LogEntry {
            time: "same time".into(),
            level: level.into(),
            message: message.into(),
        };
        let mut live = vec![entry("new alpha", "info"), entry("old alpha", "warn")];
        let mut review = LogReview::default();
        assert_eq!(review.order, LogOrder::NewestFirst);
        review.toggle_pause(|| live.clone());
        live.clear();
        assert_eq!(review.entries(|| live.clone()).len(), 2);
        live = vec![entry("newest alpha", "warn"), entry("new alpha", "info")];
        let frozen = review.entries(|| panic!("paused view must not read live history"));
        assert_eq!(frozen[0].message, "new alpha");
        assert_eq!(
            review.visible(&frozen, "alpha", Level::Warn)[0].message,
            "old alpha"
        );
        review.order.toggle();
        let ordered = review.visible(&frozen, "alpha", Level::All);
        assert_eq!(
            ordered
                .iter()
                .map(|e| e.message.as_str())
                .collect::<Vec<_>>(),
            ["old alpha", "new alpha"]
        );
        assert_eq!(frozen[0].message, "new alpha");
        review.toggle_pause(|| panic!("resume must not capture"));
        assert_eq!(review.entries(|| live.clone()), live);
        assert_eq!(
            review.visible(&live, "alpha", Level::All)[0].message,
            "new alpha"
        );
        review.order.toggle();
        assert_eq!(review.order, LogOrder::NewestFirst);

        review.toggle_pause(Vec::new);
        assert!(review.paused.is_some());
        assert!(review.entries(|| live.clone()).is_empty());
        review.reset_pause();
        assert_eq!(review.entries(|| live.clone()), live);
    }

    #[test]
    #[cfg(any(target_os = "linux", windows))]
    fn log_review_resets_on_session_and_profile_transitions() {
        use super::log_review_context_changed;
        use meow_paw::app::{EngineStatus, LogHistory, Snapshot};
        use std::path::PathBuf;

        let mut current = Snapshot::default();
        current.engine = EngineStatus::Running { mixed_port: 7890 };
        current.profile = Some(PathBuf::from("profile-a"));
        current.probe_epoch = 7;
        current.logs = Some(LogHistory::default());
        let mut next = current.clone();
        next.revision += 1;
        assert!(!log_review_context_changed(&current, &next));
        next.probe_epoch += 1;
        assert!(log_review_context_changed(&current, &next));
        next = current.clone();
        next.profile = Some(PathBuf::from("profile-b"));
        assert!(log_review_context_changed(&current, &next));
        next = current.clone();
        next.applying_profile = Some(PathBuf::from("profile-a"));
        assert!(log_review_context_changed(&current, &next));
        next = current.clone();
        next.engine = EngineStatus::Starting;
        assert!(log_review_context_changed(&current, &next));
        next = current.clone();
        next.logs = None;
        assert!(log_review_context_changed(&current, &next));
    }
}

#[cfg(all(test, feature = "visual-test", target_os = "linux"))]
mod routing_tests;
