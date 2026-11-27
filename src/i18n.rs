//! Launch-time locale selection and the shell's typed message catalog.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Locale {
    En,
    ZhCn,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Message {
    AppTitle,
    Home,
    Proxies,
    Profiles,
    Connections,
    Rules,
    Logs,
    Unlock,
    Settings,
    Traffic,
    TrafficActivity,
    TrafficNoSamples,
    SessionTotal,
    Upload,
    Download,
    StopSession,
    Stopping,
    ChooseEngine,
    ChooseProfile,
    StartSession,
    ModeRule,
    ModeGlobal,
    ModeDirect,
    ActiveProfile,
    OverviewCurrentProfile,
    OverviewAppliedProfile,
    OverviewAwaitingProfile,
    OverviewProxyChecking,
    OverviewProxyOff,
    OverviewProxyOwned,
    OverviewProxyRecovery,
    OverviewProxyUnknown,
    OverviewProxyError,
    OverviewProxyReviewRecovery,
    OverviewProxyReviewError,
    OverviewStarting,
    OverviewNeedsAttention,
    OverviewStopped,
    OverviewNoListener,
    OverviewCopied,
    OverviewNoRunningProfile,
    OverviewUnconfirmedProfile,
    OverviewNextProfile,
    OverviewRefreshSubscription,
    OverviewCurrentRoute,
    OverviewRouteUnavailable,
    OverviewRouteSelections,
    OverviewGlobalSelection,
    OverviewDirectRoute,
    Configuration,
    ProxyGroups,
    SelectedRoutes,
    Network,
    MeowEngine,
    ManagedSession,
    LocalListener,
    SystemProxy,
    ExistingSystemProxy,
    Copy,
    ShowDetails,
    HideDetails,
    Filter,
    JumpToGroup,
    Providers,
    ManageProviders,
    ProviderLastUpdate,
    ProviderQuota,
    ProviderQuotaUnknown,
    ProviderTypeUnknown,
    ProviderEmpty,
    ProviderDataStale,
    ProviderDataUnavailable,
    ProviderCountUnavailable,
    NoCandidateRoutes,
    SortDefault,
    SortLatency,
    SortName,
    Test,
    Refresh,
    ProviderNoUsage,
    ProviderAwaitingUpdate,
    ProviderRefreshing,
    ProviderRefreshBusy,
    ProviderRefreshUnavailable,
    LatencyTestDescription,
    NoMatchingGroups,
    TryAnotherRoute,
    NoProxyGroups,
    StartToViewGroups,
    Route,
    Routes,
    Groups,
    Node,
    Nodes,
    System,
    SystemProxyDescription,
    TunMode,
    TunDescription,
    ListenerDescription,
    Unmanaged,
    Unavailable,
    Loopback,
    KeepRunning,
    NoExecutableSelected,
    UnlockStartSession,
    UnlockChooseGroup,
    UnlockGroup,
    UnlockCancel,
    UnlockChecking,
    UnlockCancelling,
    UnlockUnsupportedCore,
    UnlockNoRoutes,
    UnlockComplete,
    UnlockRankDetail,
    UnlockCancelled,
    UnlockUnavailableDetail,
    UnlockPlatformUnavailable,
    UnlockPending,
    UnlockNotRun,
    UnlockPassed,
    UnlockTimedOut,
    UnlockCheckFailed,
    MeowExecutable,
    Browse,
    HideFullPath,
    ShowFullPath,
    CopyPath,
    Language,
    SettingsReadOnly,
    EngineLocked,
    SavedLanguage,
    CurrentLanguage,
    LanguageSystem,
    LanguageRestart,
    MixedProxyPort,
    SystemProxyHelp,
    SystemProxyChecking,
    SystemProxyUnavailable,
    SystemProxyOff,
    SystemProxyOn,
    SystemProxyActive,
    SystemProxyRecovery,
    SystemProxyBlocked,
    SystemProxyEnable,
    SystemProxyDisable,
    SystemProxyRestore,
    SystemProxyRefresh,

    MixedProxyPortHelp,
    MixedProxyPortCli,
    MixedProxyPortSave,
    MixedProxyPortSaved,
    MixedProxyPortInvalid,
    MixedProxyPortSaveFailed,
    ReduceMotion,
    ReduceMotionDescription,
    ReduceMotionSaveFailed,
    ReduceMotionEnabled,
    ReduceMotionDisabled,
    LanguageUnavailable,
    LanguageSaveFailed,
    TrayBehavior,
    TrayDescription,
    ApplicationVersion,
    QuitApplication,
    QuitDescription,
    QuitPaw,
    NoProfileSelected,
    Subscription,
    ImportLocalYaml,
    AddSubscription,
    SubscriptionUrl,
    ImportSubscription,
    CancelSubscription,
    RenameSubscription,
    RemoveSubscription,
    DetachSubscription,
    SubscriptionDetached,
    ConfirmRemoveSubscription,
    RemoveSubscriptionDetail,
    SubscriptionRemoveBlocked,
    SubscriptionRemoveFailed,
    SubscriptionRemoved,
    SubscriptionName,
    SubscriptionNameTooLong,
    SaveSubscriptionName,
    SavingSubscriptionName,
    SubscriptionNameSaveFailed,
    DownloadingSubscription,
    LiveApplyWarning,
    ProfileSelectionDetail,
    NoLocalProfiles,
    ProfileNameFilter,
    NoMatchingProfiles,
    ProfileFilterDetail,
    ImportYamlDetail,
    SubscriptionDetail,
    SubscriptionManualOnly,
    SubscriptionLocalInterval,
    SubscriptionProviderInterval,
    Running,
    NextSession,
    Applying,
    Reload,
    ApplyLive,
    Use,
    Forget,
    LastStartDiagnostics,
    ChooseYaml,
    ListSubscriptionsFailed,
    SubscriptionSaved,
    SubscriptionRefreshed,
    ChooseExistingYaml,
    InvalidSubscriptionUrl,
    SubscriptionDownloadFailed,
    SubscriptionTooLarge,
    SubscriptionInvalidText,
    SubscriptionInvalidProfile,
    SubscriptionNoProxies,
    SubscriptionSaveFailed,
    SubscriptionReadFailed,
    CheckSavedSubscription,
    CheckingSavedSubscription,
    SavedSubscriptionReadable,
    SubscriptionRefreshSaveFailed,
    SubscriptionWriteConflict,
    ActiveConnections,
    ActiveTab,
    ClosedTab,
    NoClosedConnections,
    NoClosedConnectionsDetail,
    LiveConnectionsUnavailable,
    LiveConnectionsUnavailableDetail,
    NoActiveConnections,
    ListenerConnectionsDetail,
    NoMatchingConnections,
    ConnectionFilterDetail,
    ConnectionSortOriginal,
    ConnectionSortDestination,
    ConnectionSortTotalData,
    StartToViewConnections,
    DestinationColumn,
    RouteColumn,
    RuleColumn,
    CloseConnection,
    CloseConnectionAction,
    ConnectionDetails,
    CloseDetails,
    Transport,
    UnknownValue,
    CurrentSampleTotals,
    ClosedSampleTotals,
    RuleSets,
    ReadRuleSets,
    ReadingRuleSets,
    RuleSetsReadOnly,
    RuleSetsNotLoaded,
    RuleSetsEmpty,
    RuleSetsUnavailable,
    RuleSetSource,
    RuleSetCount,
    RuleSetUpdated,
    RuleSetRedacted,
    RefreshRules,
    RuleLineColumn,
    RulePayloadFilter,
    NoMatchingRules,
    RuleFilterDetail,
    PreviousRulesPage,
    NextRulesPage,
    RuleTypeColumn,
    RuleMatchColumn,
    RulePolicyColumn,
    NoRuleSnapshot,
    RefreshRulesDetail,
    NoRunningSession,
    StartToViewRules,
    LogLevelAll,
    LogLevelDebug,
    LogLevelInfo,
    LogLevelWarn,
    LogLevelError,
    NoMatchingLogs,
    LogFilterDetail,
    ClearLogs,
    ClearLiveLogs,
    PauseLogs,
    ResumeLogs,
    LogNewestFirst,
    LogOldestFirst,
    LogsPausedCaption,
    PausedLogsEmpty,
    PausedLogsEmptyDetail,
    WaitingForLogs,
    WaitingForLogsDetail,
    StartToViewLogs,
}

impl Locale {
    pub fn unlock_manual_detail(self, route_limit: usize) -> String {
        match self {
            Self::En => format!(
                "Checks HTTPS connectivity to Google and Cloudflare through the first {route_limit} unambiguous proxy routes in a group."
            ),
            Self::ZhCn => format!(
                "使用组内前 {route_limit} 条可明确识别的代理路线，检测到 Google 和 Cloudflare 的 HTTPS 连通性。"
            ),
        }
    }

    pub fn subscription_interval(self, label: Message, minutes: u32) -> String {
        match self {
            Self::En => format!("{}: {} min", self.message(label), minutes),
            Self::ZhCn => format!("{}：{} 分钟", self.message(label), minutes),
        }
    }

    /// An explicit selection takes precedence even when it is unsupported.
    pub fn select(explicit: Option<&str>, system: Option<&str>) -> Self {
        explicit.or(system).map(Self::parse).unwrap_or(Self::En)
    }

    fn parse(name: &str) -> Self {
        // POSIX locale names can contain an encoding and a modifier. Neither
        // changes the language, and Windows uses hyphens instead of underscores.
        let base = name.split(['.', '@']).next().unwrap_or("");
        let normalized = base.replace('_', "-");
        if normalized.eq_ignore_ascii_case("zh-CN")
            || normalized.eq_ignore_ascii_case("zh-Hans")
            || normalized.eq_ignore_ascii_case("zh-Hans-CN")
        {
            Self::ZhCn
        } else {
            Self::En
        }
    }

    pub fn overview_route_excess(self, count: usize) -> String {
        match self {
            Self::En if count == 1 => format!("{count} more group"),
            Self::En => format!("{count} more groups"),
            Self::ZhCn => format!("另有 {count} 个代理组"),
        }
    }

    pub fn connection_match_count(self, matched: usize, total: usize) -> String {
        match self {
            Self::En => format!("{matched} of {total} active"),
            Self::ZhCn => format!("{total} 个活动连接中匹配 {matched} 个"),
        }
    }

    pub fn closed_connection_count(self, matched: usize, total: usize, filtered: bool) -> String {
        match (self, filtered) {
            (Self::En, false) => format!("{total} closed"),
            (Self::En, true) => format!("{matched} of {total} closed"),
            (Self::ZhCn, false) => format!("{total} 个已关闭连接"),
            (Self::ZhCn, true) => format!("{total} 个已关闭连接中匹配 {matched} 个"),
        }
    }

    pub fn closed_connection_history_detail(self, limit: usize) -> String {
        match self {
            Self::En => {
                format!("Read-only · Up to {limit} closures observed in this session.")
            }
            Self::ZhCn => format!("只读 · 仅显示本次会话中观察到关闭的连接，最多 {limit} 条。"),
        }
    }

    pub fn rule_count(self, count: usize) -> String {
        match self {
            Self::En => format!("{count} RULES"),
            Self::ZhCn => format!("{count} 条规则"),
        }
    }

    pub fn rule_match_count(self, matched: usize, total: usize) -> String {
        match self {
            Self::En => format!("{matched} of {total} rules"),
            Self::ZhCn => format!("{total} 条规则中匹配 {matched} 条"),
        }
    }

    pub fn rule_page_range(self, start: usize, end: usize, total: usize) -> String {
        match self {
            Self::En => format!("Showing {start}–{end} of {total} matches"),
            Self::ZhCn => format!("显示 {total} 条匹配结果中的第 {start}–{end} 条"),
        }
    }

    pub fn visible_log_count(self, visible: usize, retained: usize) -> String {
        match self {
            Self::En => format!("{visible} of {retained} retained entries visible"),
            Self::ZhCn => format!("保留 {retained} 条日志，显示 {visible} 条"),
        }
    }

    pub fn retained_log_count(self, count: usize) -> String {
        match self {
            Self::En => format!("{count} retained entries"),
            Self::ZhCn => format!("保留 {count} 条日志"),
        }
    }

    pub fn traffic_chart_history(self, seconds: usize) -> String {
        match self {
            Self::En => format!("{seconds}s ago · gaps unavailable · grey zero"),
            Self::ZhCn => format!("{seconds} 秒前 · 空隙表示数据不可用 · 灰色表示零流量"),
        }
    }

    pub fn traffic_chart_peak(self, rate: &str) -> String {
        match self {
            Self::En => format!("{rate} shared peak · now"),
            Self::ZhCn => format!("共用峰值 {rate} · 现在"),
        }
    }

    pub fn message(self, key: Message) -> &'static str {
        match self {
            Self::En => english(key),
            Self::ZhCn => chinese(key),
        }
    }
}

fn english(key: Message) -> &'static str {
    match key {
        Message::AppTitle => "Meow Paw",
        Message::Home => "Home",
        Message::Proxies => "Proxies",
        Message::Profiles => "Profiles",
        Message::Connections => "Connections",
        Message::Rules => "Rules",
        Message::Logs => "Logs",
        Message::Unlock => "Unlock",
        Message::Settings => "Settings",
        Message::Traffic => "Traffic",
        Message::TrafficActivity => "Traffic activity",
        Message::TrafficNoSamples => "No traffic samples available yet.",
        Message::SessionTotal => "Session total",
        Message::Upload => "Upload",
        Message::Download => "Download",
        Message::StopSession => "Stop session",
        Message::Stopping => "Stopping…",
        Message::ChooseEngine => "Choose engine",
        Message::ChooseProfile => "Choose profile",
        Message::StartSession => "Start session",
        Message::ModeRule => "Rule",
        Message::ModeGlobal => "Global",
        Message::ModeDirect => "Direct",
        Message::ActiveProfile => "Active profile",
        Message::OverviewCurrentProfile => "Current profile",
        Message::OverviewAppliedProfile => "Applied profile",
        Message::OverviewAwaitingProfile => "Awaiting profile confirmation",
        Message::OverviewProxyChecking => "Checking",
        Message::OverviewProxyOff => "Paw proxy off",
        Message::OverviewProxyOwned => "On for Paw",
        Message::OverviewProxyRecovery => "Recovery record present",
        Message::OverviewProxyUnknown => "Ownership unknown",
        Message::OverviewProxyError => "Error",
        Message::OverviewProxyReviewRecovery => "Review recovery",
        Message::OverviewProxyReviewError => "Review error",
        Message::OverviewStarting => "Starting…",
        Message::OverviewNeedsAttention => "Needs attention",
        Message::OverviewStopped => "Stopped",
        Message::OverviewNoListener => "No listener",
        Message::OverviewCopied => "Copied",
        Message::OverviewNoRunningProfile => "No running profile",
        Message::OverviewUnconfirmedProfile => "Running profile unconfirmed",
        Message::OverviewNextProfile => "Next session",
        Message::OverviewRefreshSubscription => "Refresh subscription",
        Message::OverviewCurrentRoute => "Current route",
        Message::OverviewRouteUnavailable => "Route information unavailable",
        Message::OverviewRouteSelections => "Group selections · Rules may use different routes",
        Message::OverviewGlobalSelection => "Global group selection",
        Message::OverviewDirectRoute => "Direct mode",
        Message::Configuration => "Configuration",
        Message::ProxyGroups => "Proxy groups",
        Message::SelectedRoutes => "Selected routes",
        Message::Network => "Network",
        Message::MeowEngine => "Meow engine",
        Message::ManagedSession => "Managed session",
        Message::LocalListener => "Local listener",
        Message::SystemProxy => "System proxy",
        Message::ExistingSystemProxy => "Existing system proxy",
        Message::Copy => "Copy",
        Message::ShowDetails => "Show details",
        Message::HideDetails => "Hide details",
        Message::Filter => "Filter",
        Message::JumpToGroup => "Jump to group",
        Message::Providers => "Providers",
        Message::ManageProviders => "Manage providers",
        Message::ProviderLastUpdate => "Last update",
        Message::ProviderQuota => "Quota",
        Message::ProviderQuotaUnknown => "Unknown",
        Message::ProviderTypeUnknown => "Unknown type",
        Message::ProviderEmpty => "No proxy providers in this inventory.",
        Message::ProviderDataStale => "Provider data may be stale. Refresh is disabled.",
        Message::ProviderDataUnavailable => "Provider data is unavailable. Refresh is disabled.",
        Message::ProviderCountUnavailable => "Node count unavailable",
        Message::NoCandidateRoutes => "This group has no candidate routes.",
        Message::SortDefault => "Sort: Default",
        Message::SortLatency => "Sort: Latency",
        Message::SortName => "Sort: Name",
        Message::Test => "Test",
        Message::Refresh => "Refresh",
        Message::MixedProxyPort => "Mixed proxy port",
        Message::SystemProxyHelp => {
            "Changes this account’s desktop HTTP/HTTPS proxy. Only apps that use desktop proxy settings are affected."
        }
        Message::SystemProxyChecking => "Checking desktop support…",
        Message::SystemProxyUnavailable => "Not supported in this desktop session.",
        Message::SystemProxyOff => "Off. Start Meow before enabling.",
        Message::SystemProxyOn => "On for this Paw session.",
        Message::SystemProxyActive => "On",
        Message::SystemProxyRecovery => {
            "A recovery record is present. Paw will restore it only if the desktop settings still match."
        }
        Message::SystemProxyBlocked => "Could not confirm desktop proxy ownership.",
        Message::SystemProxyEnable => "Enable system proxy",
        Message::SystemProxyDisable => "Disable and restore",
        Message::SystemProxyRestore => "Restore previous settings",
        Message::SystemProxyRefresh => "Check again",

        Message::MixedProxyPortHelp => {
            "Listens on 127.0.0.1. Applies the next time you start the engine."
        }
        Message::MixedProxyPortCli => {
            "The launch flag overrides this saved port until you relaunch Paw."
        }
        Message::MixedProxyPortSave => "Save port",
        Message::MixedProxyPortSaved => "Port saved. The active session is unchanged.",
        Message::MixedProxyPortInvalid => "Enter a port from 1 to 65535.",
        Message::MixedProxyPortSaveFailed => "Could not save the mixed proxy port.",
        Message::ReduceMotion => "Reduce motion",
        Message::ReduceMotionDescription => {
            "Skip the proxy list expansion animation. Applies immediately."
        }
        Message::ReduceMotionSaveFailed => "Could not save the motion preference.",
        Message::ReduceMotionEnabled => "On",
        Message::ReduceMotionDisabled => "Off",
        Message::ProviderNoUsage => "No usage information",
        Message::ProviderAwaitingUpdate => "Awaiting first update",
        Message::ProviderRefreshing => "Refreshing…",
        Message::ProviderRefreshBusy => "Another provider is refreshing",
        Message::ProviderRefreshUnavailable => "Refresh unavailable",
        Message::LatencyTestDescription => {
            "Latency test · Meow opens this HTTPS URL through the chosen proxy."
        }
        Message::NoMatchingGroups => "No matching groups",
        Message::TryAnotherRoute => "Try another route or group name.",
        Message::NoProxyGroups => "No proxy groups yet",
        Message::StartToViewGroups => {
            "Select a YAML profile and start the engine to view its groups."
        }
        Message::Route => "route",
        Message::Routes => "routes",
        Message::Groups => "groups",
        Message::Node => "node",
        Message::Nodes => "nodes",
        Message::System => "System",
        Message::SystemProxyDescription => {
            "Paw only changes the desktop proxy if you explicitly enable it in Settings."
        }
        Message::TunMode => "TUN mode",
        Message::TunDescription => "A virtual network interface is not available in this build.",
        Message::ListenerDescription => "Only apps configured to use Paw are routed through it.",
        Message::Unmanaged => "Unmanaged",
        Message::Unavailable => "Unavailable",
        Message::Loopback => "Loopback",
        Message::KeepRunning => "Keep running",
        Message::NoExecutableSelected => "No executable selected",
        Message::UnlockStartSession => "Start a managed Meow session to check routes.",
        Message::UnlockChooseGroup => "Run checks for a proxy group",
        Message::UnlockGroup => "Proxy group",
        Message::UnlockCancel => "Cancel checks",
        Message::UnlockChecking => "Checking routes…",
        Message::UnlockCancelling => "Waiting for the current check to finish…",
        Message::UnlockUnsupportedCore => "Route-pinned checks require a compatible Meow core.",
        Message::UnlockNoRoutes => "Choose a proxy group with eligible routes.",
        Message::UnlockComplete => "Checks complete",
        Message::UnlockRankDetail => {
            "Ranked by checks passed, then lowest total latency for those checks."
        }
        Message::UnlockCancelled => "Cancelled",
        Message::UnlockUnavailableDetail => "Could not run checks through the managed core.",
        Message::UnlockPlatformUnavailable => "Managed route checks require Linux or Windows.",
        Message::UnlockPending => "Pending",
        Message::UnlockNotRun => "Not run",
        Message::UnlockPassed => "Passed",
        Message::UnlockTimedOut => "Timed out",
        Message::UnlockCheckFailed => "Check failed",
        Message::MeowExecutable => "Meow executable",
        Message::Browse => "Browse…",
        Message::HideFullPath => "Hide full path",
        Message::ShowFullPath => "Show full path",
        Message::CopyPath => "Copy path",
        Message::Language => "Language",
        Message::SettingsReadOnly => "Read-only status",
        Message::EngineLocked => "Locked",
        Message::SavedLanguage => "Saved choice",
        Message::CurrentLanguage => "Current launch language",
        Message::LanguageSystem => "System",
        Message::LanguageRestart => {
            "Restart Paw to apply a saved change. A launch override takes precedence."
        }
        Message::LanguageUnavailable => {
            "Language selection is unavailable because preferences are not persistent."
        }
        Message::LanguageSaveFailed => {
            "Could not save the language choice. The saved choice has not changed."
        }
        Message::TrayBehavior => "Tray behavior",
        Message::TrayDescription => {
            "Closing the window keeps Paw running when a desktop tray is available."
        }
        Message::ApplicationVersion => "Application version",
        Message::QuitApplication => "Quit application",
        Message::QuitDescription => "Quitting also stops the managed engine.",
        Message::QuitPaw => "Quit Meow Paw",
        Message::NoProfileSelected => "No profile selected",
        Message::Subscription => "Subscription",
        Message::ImportLocalYaml => "Import local YAML",
        Message::AddSubscription => "Add HTTPS subscription",
        Message::SubscriptionUrl => "HTTPS subscription URL",
        Message::ImportSubscription => "Import subscription",
        Message::CancelSubscription => "Cancel",
        Message::RenameSubscription => "Rename",
        Message::RemoveSubscription => "Remove subscription",
        Message::DetachSubscription => "Forget profile reference",
        Message::SubscriptionDetached => {
            "The profile reference was forgotten. You can now remove the subscription."
        }
        Message::ConfirmRemoveSubscription => "Remove local subscription",
        Message::RemoveSubscriptionDetail => {
            "Remove this saved subscription and its downloaded copy from Paw? Nothing will be sent to the provider. This cannot be undone."
        }
        Message::SubscriptionRemoveBlocked => {
            "Stop the engine, select another profile, and forget this profile reference before removing the subscription."
        }
        Message::SubscriptionRemoveFailed => {
            "Could not remove the saved subscription. It has been retained where possible."
        }
        Message::SubscriptionRemoved => "The saved subscription was removed from Paw.",
        Message::SubscriptionName => "Local name, up to 80 characters (blank uses the ID)",
        Message::SubscriptionNameTooLong => "Enter a name of 80 characters or fewer.",
        Message::SaveSubscriptionName => "Save",
        Message::SavingSubscriptionName => "Saving name…",
        Message::SubscriptionNameSaveFailed => "Could not save the subscription name",
        Message::DownloadingSubscription => "Downloading subscription…",
        Message::ProfileSelectionDetail => {
            "Use selects the next session; Apply live changes the running profile."
        }
        Message::LiveApplyWarning => {
            "Apply live and Reload keep Meow running, but can disconnect active connections."
        }
        Message::NoLocalProfiles => "No local profiles",
        Message::ProfileNameFilter => "Filter profile names",
        Message::NoMatchingProfiles => "No matching profiles",
        Message::ProfileFilterDetail => "Try another profile name.",
        Message::ImportYamlDetail => {
            "Import a YAML file. Paw prepares a separate session copy and leaves your file unchanged."
        }
        Message::SubscriptionManualOnly => "Manual refresh only",
        Message::SubscriptionLocalInterval => "Local override",
        Message::SubscriptionProviderInterval => "Provider suggests",
        Message::SubscriptionDetail => {
            "HTTPS subscription · Refresh does not change the running profile"
        }
        Message::Running => "Running",
        Message::NextSession => "Next session",
        Message::Applying => "Applying…",
        Message::Reload => "Reload",
        Message::ApplyLive => "Apply live",
        Message::Use => "Use",
        Message::Forget => "Forget",
        Message::LastStartDiagnostics => "Diagnostics from the last start attempt",
        Message::ChooseYaml => "Choose Meow YAML",
        Message::ListSubscriptionsFailed => "Could not list saved subscriptions.",
        Message::SubscriptionSaved => "Subscription saved. Select Use or Apply live when ready.",
        Message::SubscriptionRefreshed => {
            "Subscription refreshed. The running profile is unchanged."
        }
        Message::ChooseExistingYaml => "Choose an existing local YAML file.",
        Message::InvalidSubscriptionUrl => {
            "Enter an HTTPS subscription URL without user info or a fragment."
        }
        Message::SubscriptionDownloadFailed => "Could not download the subscription",
        Message::SubscriptionTooLarge => "Subscription profile exceeds the size limit",
        Message::SubscriptionInvalidText => "Subscription did not return UTF-8 text",
        Message::SubscriptionInvalidProfile => "Subscription profile is invalid",
        Message::SubscriptionNoProxies => "Subscription has no proxies or proxy providers",
        Message::SubscriptionSaveFailed => "Could not save the subscription",
        Message::SubscriptionReadFailed => "Could not read the saved subscription",
        Message::CheckSavedSubscription => "Check saved copy",
        Message::CheckingSavedSubscription => "Checking saved subscription",
        Message::SavedSubscriptionReadable => "Saved subscription is readable",
        Message::SubscriptionRefreshSaveFailed => "Could not save the refreshed subscription",
        Message::SubscriptionWriteConflict => {
            "Subscription is busy or has changed. Please try again."
        }
        Message::ActiveConnections => "active",
        Message::ActiveTab => "Active",
        Message::ClosedTab => "Closed",
        Message::NoClosedConnections => "No closed connections recorded",
        Message::NoClosedConnectionsDetail => {
            "Connections observed closing in this session appear here."
        }
        Message::LiveConnectionsUnavailable => "Live connections unavailable",
        Message::LiveConnectionsUnavailableDetail => "Waiting for a live connection sample.",
        Message::NoActiveConnections => "No active connections",
        Message::ListenerConnectionsDetail => "Traffic handled by the Paw listener appears here.",
        Message::NoMatchingConnections => "No matching connections",
        Message::ConnectionFilterDetail => {
            "Change or clear the filter. Search destination, policy chain, matched rule, or transport."
        }
        Message::ConnectionSortOriginal => "Sort: Original order",
        Message::ConnectionSortDestination => "Sort: Destination",
        Message::ConnectionSortTotalData => "Sort: Most data",
        Message::StartToViewConnections => "Start a profile to monitor its live connections.",
        Message::DestinationColumn => "DESTINATION",
        Message::RouteColumn => "ROUTE",
        Message::RuleColumn => "RULE",
        Message::CloseConnection => "Close",
        Message::CloseConnectionAction => "Close connection",
        Message::ConnectionDetails => "Details",
        Message::CloseDetails => "Close details",
        Message::Transport => "Transport",
        Message::UnknownValue => "Unknown",
        Message::CurrentSampleTotals => "Last reported totals · Active sample",
        Message::ClosedSampleTotals => "Last reported totals · Last sample before closure",

        Message::RuleSets => "Rule sets",
        Message::ReadRuleSets => "Read rule sets",
        Message::ReadingRuleSets => "Reading rule sets…",
        Message::RuleSetsReadOnly => "Read-only · This list does not update providers.",
        Message::RuleSetsNotLoaded => "Rule sets have not been loaded.",
        Message::RuleSetsEmpty => "No rule sets found.",
        Message::RuleSetsUnavailable => "Could not read rule sets from the controller.",
        Message::RuleSetSource => "Source",
        Message::RuleSetCount => "Rules",
        Message::RuleSetUpdated => "Updated",
        Message::RuleSetRedacted => "[hidden]",
        Message::RefreshRules => "Refresh rules",
        Message::RuleLineColumn => "LINE",
        Message::RulePayloadFilter => "Search payload",
        Message::NoMatchingRules => "No matching rules",
        Message::RuleFilterDetail => "Change or clear the payload search.",
        Message::PreviousRulesPage => "Previous",
        Message::NextRulesPage => "Next",
        Message::RuleTypeColumn => "TYPE",
        Message::RuleMatchColumn => "MATCH",
        Message::RulePolicyColumn => "POLICY",
        Message::NoRuleSnapshot => "No rule snapshot",
        Message::RefreshRulesDetail => "Refresh to read the live rules from Meow.",
        Message::NoRunningSession => "No running session",
        Message::StartToViewRules => {
            "Start a profile to inspect its rule order and routing policies."
        }
        Message::LogLevelAll => "All levels",
        Message::LogLevelDebug => "Debug",
        Message::LogLevelInfo => "Info",
        Message::LogLevelWarn => "Warning",
        Message::LogLevelError => "Error",
        Message::NoMatchingLogs => "No matching log entries",
        Message::LogFilterDetail => "Try a different search or level.",
        Message::ClearLogs => "Clear",
        Message::ClearLiveLogs => "Clear live logs",
        Message::PauseLogs => "Pause view",
        Message::ResumeLogs => "Resume",
        Message::LogNewestFirst => "Order: Newest first",
        Message::LogOldestFirst => "Order: Oldest first",
        Message::LogsPausedCaption => "View paused · Live logs continue; resume to catch up.",
        Message::PausedLogsEmpty => "No entries when paused",
        Message::PausedLogsEmptyDetail => "Resume to see current log entries.",
        Message::WaitingForLogs => "Waiting for log entries",
        Message::WaitingForLogsDetail => "New messages from this Meow session appear here.",
        Message::StartToViewLogs => {
            "Start a profile to view live Meow logs. Entries are kept only for the current session."
        }
    }
}

fn chinese(key: Message) -> &'static str {
    match key {
        Message::AppTitle => "Meow Paw",
        Message::Home => "首页",
        Message::Proxies => "代理",
        Message::Profiles => "配置",
        Message::Connections => "连接",
        Message::Rules => "规则",
        Message::Logs => "日志",
        Message::Unlock => "解锁",
        Message::Settings => "设置",
        Message::Traffic => "流量",
        Message::TrafficActivity => "流量趋势",
        Message::TrafficNoSamples => "暂无流量采样数据。",
        Message::SessionTotal => "会话总流量",
        Message::Upload => "上传",
        Message::Download => "下载",
        Message::StopSession => "停止会话",
        Message::Stopping => "正在停止…",
        Message::ChooseEngine => "选择引擎",
        Message::ChooseProfile => "选择配置",
        Message::StartSession => "启动会话",
        Message::ModeRule => "规则",
        Message::ModeGlobal => "全局",
        Message::ModeDirect => "直连",
        Message::ActiveProfile => "当前配置",
        Message::OverviewCurrentProfile => "当前配置",
        Message::OverviewAppliedProfile => "已应用配置",
        Message::OverviewAwaitingProfile => "等待确认配置",
        Message::OverviewProxyChecking => "正在检查",
        Message::OverviewProxyOff => "Paw 代理已关闭",
        Message::OverviewProxyOwned => "Paw 已启用",
        Message::OverviewProxyRecovery => "发现恢复记录",
        Message::OverviewProxyUnknown => "归属未知",
        Message::OverviewProxyError => "出错",
        Message::OverviewProxyReviewRecovery => "查看恢复选项",
        Message::OverviewProxyReviewError => "查看错误",
        Message::OverviewStarting => "正在启动…",
        Message::OverviewNeedsAttention => "需要处理",
        Message::OverviewStopped => "已停止",
        Message::OverviewNoListener => "未监听",
        Message::OverviewCopied => "已复制",
        Message::OverviewNoRunningProfile => "没有运行中的配置",
        Message::OverviewUnconfirmedProfile => "运行中的配置尚未确认",
        Message::OverviewNextProfile => "下次会话",
        Message::OverviewRefreshSubscription => "刷新订阅",
        Message::OverviewCurrentRoute => "当前路线",
        Message::OverviewRouteUnavailable => "路线信息不可用",
        Message::OverviewRouteSelections => "代理组选择 · 不同规则可能使用不同路线",
        Message::OverviewGlobalSelection => "全局代理组选择",
        Message::OverviewDirectRoute => "直连模式",
        Message::Configuration => "配置文件",
        Message::ProxyGroups => "代理组",
        Message::SelectedRoutes => "已选路线",
        Message::Network => "网络",
        Message::MeowEngine => "Meow 引擎",
        Message::ManagedSession => "托管会话",
        Message::LocalListener => "本地监听",
        Message::SystemProxy => "系统代理",
        Message::ExistingSystemProxy => "现有系统代理",
        Message::Copy => "复制",
        Message::ShowDetails => "显示详情",
        Message::HideDetails => "隐藏详情",
        Message::Filter => "筛选",
        Message::JumpToGroup => "跳转到代理组",
        Message::Providers => "提供者",
        Message::ManageProviders => "管理提供者",
        Message::ProviderLastUpdate => "上次更新",
        Message::ProviderQuota => "流量额度",
        Message::ProviderQuotaUnknown => "未知",
        Message::ProviderTypeUnknown => "类型未知",
        Message::ProviderEmpty => "当前清单中没有代理提供者。",
        Message::ProviderDataStale => "提供者数据可能已过期，刷新已停用。",
        Message::ProviderDataUnavailable => "提供者数据不可用，刷新已停用。",
        Message::ProviderCountUnavailable => "节点数量不可用",
        Message::NoCandidateRoutes => "此组没有候选路线。",
        Message::SortDefault => "排序：默认",
        Message::SortLatency => "排序：延迟",
        Message::SortName => "排序：名称",
        Message::Test => "测试",
        Message::Refresh => "刷新",
        Message::MixedProxyPort => "混合代理端口",
        Message::SystemProxyHelp => {
            "修改当前账号的桌面 HTTP/HTTPS 代理，仅影响遵循桌面代理设置的应用。"
        }
        Message::SystemProxyChecking => "正在检查桌面支持…",
        Message::SystemProxyUnavailable => "当前桌面会话不支持。",
        Message::SystemProxyOff => "已关闭。启用前请先启动 Meow。",
        Message::SystemProxyOn => "已为当前 Paw 会话启用。",
        Message::SystemProxyActive => "已启用",
        Message::SystemProxyRecovery => "发现恢复记录。仅在系统设置仍匹配时才会恢复。",
        Message::SystemProxyBlocked => "无法确认桌面代理归属。",
        Message::SystemProxyEnable => "启用系统代理",
        Message::SystemProxyDisable => "关闭并恢复",
        Message::SystemProxyRestore => "恢复之前的设置",
        Message::SystemProxyRefresh => "重新检查",

        Message::MixedProxyPortHelp => "仅监听 127.0.0.1，下次启动内核时生效。",
        Message::MixedProxyPortCli => "本次启动参数优先；重启 Paw 后使用保存的端口。",
        Message::MixedProxyPortSave => "保存端口",
        Message::MixedProxyPortSaved => "端口已保存，当前会话不受影响。",
        Message::MixedProxyPortInvalid => "请输入 1 到 65535 之间的端口。",
        Message::MixedProxyPortSaveFailed => "未能保存混合代理端口。",
        Message::ReduceMotion => "减少动态效果",
        Message::ReduceMotionDescription => "跳过代理列表展开动画，立即生效。",
        Message::ReduceMotionSaveFailed => "未能保存动态效果偏好。",
        Message::ReduceMotionEnabled => "开启",
        Message::ReduceMotionDisabled => "关闭",
        Message::ProviderNoUsage => "暂无流量用量信息",
        Message::ProviderAwaitingUpdate => "等待首次更新",
        Message::ProviderRefreshing => "正在刷新…",
        Message::ProviderRefreshBusy => "另一个代理集合正在刷新",
        Message::ProviderRefreshUnavailable => "刷新不可用",
        Message::LatencyTestDescription => "延迟测试 · Meow 通过所选代理访问此 HTTPS 地址。",
        Message::NoMatchingGroups => "没有匹配的代理组",
        Message::TryAnotherRoute => "请尝试其他路线或代理组名称。",
        Message::NoProxyGroups => "暂无代理组",
        Message::StartToViewGroups => "选择 YAML 配置并启动引擎以查看代理组。",
        Message::Route => "条路线",
        Message::Routes => "条路线",
        Message::Groups => "个代理组",
        Message::Node => "个节点",
        Message::Nodes => "个节点",
        Message::System => "系统",
        Message::SystemProxyDescription => "仅在你于设置中主动启用时，Paw 才会修改桌面代理。",
        Message::TunMode => "TUN 模式",
        Message::TunDescription => "此版本不支持虚拟网络接口。",
        Message::ListenerDescription => "只有配置为使用 Paw 的应用才会通过它连接。",
        Message::Unmanaged => "未管理",
        Message::Unavailable => "不可用",
        Message::Loopback => "回环",
        Message::KeepRunning => "继续运行",
        Message::NoExecutableSelected => "未选择可执行文件",
        Message::UnlockStartSession => "启动托管的 Meow 会话后，即可检测路线。",
        Message::UnlockChooseGroup => "为代理组运行检测",
        Message::UnlockGroup => "代理组",
        Message::UnlockCancel => "取消检测",
        Message::UnlockChecking => "正在检测路线…",
        Message::UnlockCancelling => "正在等待当前检测结束…",
        Message::UnlockUnsupportedCore => "指定路线检测需要兼容的 Meow 核心。",
        Message::UnlockNoRoutes => "请选择包含可检测路线的代理组。",
        Message::UnlockComplete => "检测完成",
        Message::UnlockRankDetail => "按通过的检测数排序，数量相同时，总延迟较低的路线排在前面。",
        Message::UnlockCancelled => "已取消",
        Message::UnlockUnavailableDetail => "无法通过托管核心运行检测。",
        Message::UnlockPlatformUnavailable => "托管路线检测需要 Linux 或 Windows。",
        Message::UnlockPending => "等待中",
        Message::UnlockNotRun => "未执行",
        Message::UnlockPassed => "通过",
        Message::UnlockTimedOut => "超时",
        Message::UnlockCheckFailed => "检测失败",
        Message::MeowExecutable => "Meow 可执行文件",
        Message::Browse => "浏览…",
        Message::HideFullPath => "隐藏完整路径",
        Message::ShowFullPath => "显示完整路径",
        Message::CopyPath => "复制路径",
        Message::Language => "语言",
        Message::SettingsReadOnly => "只读状态",
        Message::EngineLocked => "已锁定",
        Message::SavedLanguage => "已保存的选择",
        Message::CurrentLanguage => "本次启动使用的语言",
        Message::LanguageSystem => "跟随系统",
        Message::LanguageRestart => "重启 Paw 后应用已保存的更改。启动时指定的语言优先。",
        Message::LanguageUnavailable => "首选项无法持久保存，因此无法选择语言。",
        Message::LanguageSaveFailed => "无法保存语言选择。已保存的选择未更改。",
        Message::TrayBehavior => "托盘行为",
        Message::TrayDescription => "桌面托盘可用时，关闭窗口后 Paw 继续运行。",
        Message::ApplicationVersion => "应用版本",
        Message::QuitApplication => "退出应用",
        Message::QuitDescription => "退出时也会停止托管的引擎。",
        Message::QuitPaw => "退出 Meow Paw",
        Message::NoProfileSelected => "未选择配置",
        Message::Subscription => "订阅",
        Message::ImportLocalYaml => "导入本地 YAML",
        Message::AddSubscription => "添加 HTTPS 订阅",
        Message::SubscriptionUrl => "HTTPS 订阅地址",
        Message::ImportSubscription => "导入订阅",
        Message::CancelSubscription => "取消",
        Message::RenameSubscription => "重命名",
        Message::RemoveSubscription => "移除订阅",
        Message::DetachSubscription => "忘记配置引用",
        Message::SubscriptionDetached => "已忘记配置引用。现在可以移除订阅。",
        Message::ConfirmRemoveSubscription => "移除本地订阅",
        Message::RemoveSubscriptionDetail => {
            "要从 Paw 移除此已保存的订阅及其下载副本吗？不会向提供者发送任何信息。此操作无法撤销。"
        }
        Message::SubscriptionRemoveBlocked => {
            "移除订阅前，请停止引擎、选择其他配置并忘记此配置引用。"
        }
        Message::SubscriptionRemoveFailed => "无法移除已保存的订阅，Paw 会尽可能保留它。",
        Message::SubscriptionRemoved => "已从 Paw 移除保存的订阅。",
        Message::SubscriptionName => "本地名称，最多 80 个字符（留空则使用 ID）",
        Message::SubscriptionNameTooLong => "名称不能超过 80 个字符。",
        Message::SaveSubscriptionName => "保存",
        Message::SavingSubscriptionName => "正在保存名称…",
        Message::SubscriptionNameSaveFailed => "无法保存订阅名称",
        Message::DownloadingSubscription => "正在下载订阅…",
        Message::ProfileSelectionDetail => {
            "使用会选择下次会话的配置；实时应用会更改正在运行的配置。"
        }
        Message::LiveApplyWarning => "实时应用和重新加载会保持 Meow 运行，但可能断开活动连接。",
        Message::NoLocalProfiles => "暂无本地配置",
        Message::ProfileNameFilter => "筛选配置名称",
        Message::NoMatchingProfiles => "没有匹配的配置",
        Message::ProfileFilterDetail => "请尝试其他配置名称。",
        Message::ImportYamlDetail => "导入 YAML 文件。Paw 会为会话准备单独的副本，不会更改原文件。",
        Message::SubscriptionManualOnly => "仅手动刷新",
        Message::SubscriptionLocalInterval => "本地覆盖",
        Message::SubscriptionProviderInterval => "提供者建议",
        Message::SubscriptionDetail => "HTTPS 订阅 · 刷新不会更改正在运行的配置",
        Message::Running => "运行中",
        Message::NextSession => "下次会话",
        Message::Applying => "正在应用…",
        Message::Reload => "重新加载",
        Message::ApplyLive => "实时应用",
        Message::Use => "使用",
        Message::Forget => "移除",
        Message::LastStartDiagnostics => "上次启动尝试的诊断信息",
        Message::ChooseYaml => "选择 Meow YAML",
        Message::ListSubscriptionsFailed => "无法列出已保存的订阅。",
        Message::SubscriptionSaved => "订阅已保存。准备就绪后请选择使用或实时应用。",
        Message::SubscriptionRefreshed => "订阅已刷新。正在运行的配置未更改。",
        Message::ChooseExistingYaml => "请选择存在的本地 YAML 文件。",
        Message::InvalidSubscriptionUrl => "请输入不含用户信息或片段的 HTTPS 订阅地址。",
        Message::SubscriptionDownloadFailed => "无法下载订阅",
        Message::SubscriptionTooLarge => "订阅配置超过大小限制",
        Message::SubscriptionInvalidText => "订阅返回的内容不是 UTF-8 文本",
        Message::SubscriptionInvalidProfile => "订阅配置无效",
        Message::SubscriptionNoProxies => "订阅中没有代理或代理提供者",
        Message::SubscriptionSaveFailed => "无法保存订阅",
        Message::SubscriptionReadFailed => "无法读取已保存的订阅",
        Message::CheckSavedSubscription => "检查本地副本",
        Message::CheckingSavedSubscription => "正在检查已保存的订阅",
        Message::SavedSubscriptionReadable => "已确认订阅副本可读",
        Message::SubscriptionRefreshSaveFailed => "无法保存刷新后的订阅",
        Message::SubscriptionWriteConflict => "订阅正忙或已更改，请重试。",
        Message::ActiveConnections => "个活动连接",
        Message::ActiveTab => "活动",
        Message::ClosedTab => "已关闭",
        Message::NoClosedConnections => "尚无已记录的关闭连接",
        Message::NoClosedConnectionsDetail => "当前会话中观察到关闭的连接会显示在这里。",
        Message::LiveConnectionsUnavailable => "无法获取实时连接",
        Message::LiveConnectionsUnavailableDetail => "正在等待实时连接数据。",
        Message::NoActiveConnections => "暂无活动连接",
        Message::ListenerConnectionsDetail => "由 Paw 监听器处理的流量会显示在这里。",
        Message::NoMatchingConnections => "没有匹配的连接",
        Message::ConnectionFilterDetail => {
            "更改或清除筛选条件。可搜索目标地址、策略链、匹配规则或传输协议。"
        }
        Message::ConnectionSortOriginal => "排序：原始顺序",
        Message::ConnectionSortDestination => "排序：目标地址",
        Message::ConnectionSortTotalData => "排序：流量从高到低",
        Message::StartToViewConnections => "启动配置后即可监控实时连接。",
        Message::DestinationColumn => "目标地址",
        Message::RouteColumn => "路线",
        Message::RuleColumn => "规则",
        Message::CloseConnection => "关闭",
        Message::CloseConnectionAction => "关闭连接",
        Message::ConnectionDetails => "详情",
        Message::CloseDetails => "关闭详情",
        Message::Transport => "传输协议",
        Message::UnknownValue => "未知",
        Message::CurrentSampleTotals => "最近报告的总量 · 活动采样",
        Message::ClosedSampleTotals => "最近报告的总量 · 关闭前最后一次采样",

        Message::RuleSets => "规则集",
        Message::ReadRuleSets => "读取规则集",
        Message::ReadingRuleSets => "正在读取规则集…",
        Message::RuleSetsReadOnly => "只读 · 此列表不会更新提供者。",
        Message::RuleSetsNotLoaded => "尚未加载规则集。",
        Message::RuleSetsEmpty => "未找到规则集。",
        Message::RuleSetsUnavailable => "无法从控制器读取规则集。",
        Message::RuleSetSource => "来源",
        Message::RuleSetCount => "规则数",
        Message::RuleSetUpdated => "更新时间",
        Message::RuleSetRedacted => "[已隐藏]",
        Message::RefreshRules => "刷新规则",
        Message::RuleLineColumn => "行号",
        Message::RulePayloadFilter => "搜索匹配内容",
        Message::NoMatchingRules => "没有匹配的规则",
        Message::RuleFilterDetail => "更改或清除匹配内容搜索。",
        Message::PreviousRulesPage => "上一页",
        Message::NextRulesPage => "下一页",
        Message::RuleTypeColumn => "类型",
        Message::RuleMatchColumn => "匹配内容",
        Message::RulePolicyColumn => "策略",
        Message::NoRuleSnapshot => "暂无规则快照",
        Message::RefreshRulesDetail => "刷新以读取 Meow 的实时规则。",
        Message::NoRunningSession => "没有运行中的会话",
        Message::StartToViewRules => "启动配置后即可查看规则顺序和路由策略。",
        Message::LogLevelAll => "全部级别",
        Message::LogLevelDebug => "调试",
        Message::LogLevelInfo => "信息",
        Message::LogLevelWarn => "警告",
        Message::LogLevelError => "错误",
        Message::NoMatchingLogs => "没有匹配的日志",
        Message::LogFilterDetail => "请尝试其他搜索词或级别。",
        Message::ClearLogs => "清空",
        Message::ClearLiveLogs => "清空实时日志",
        Message::PauseLogs => "暂停视图",
        Message::ResumeLogs => "恢复",
        Message::LogNewestFirst => "排序：最新在前",
        Message::LogOldestFirst => "排序：最早在前",
        Message::LogsPausedCaption => "视图已暂停 · 实时日志仍在记录；恢复后显示最新日志。",
        Message::PausedLogsEmpty => "暂停时没有日志",
        Message::PausedLogsEmptyDetail => "恢复后可查看当前日志。",
        Message::WaitingForLogs => "等待日志",
        Message::WaitingForLogsDetail => "此 Meow 会话的新消息会显示在这里。",
        Message::StartToViewLogs => "启动配置后即可查看 Meow 的实时日志。日志仅在当前会话中保留。",
    }
}

/// Read the user's platform preference without changing process or system settings.
#[cfg(not(windows))]
pub fn system_locale() -> Option<String> {
    posix_locale(["LC_ALL", "LC_MESSAGES", "LANG"].map(std::env::var_os))
}

#[cfg(not(windows))]
fn posix_locale(values: [Option<std::ffi::OsString>; 3]) -> Option<String> {
    values
        .into_iter()
        .flatten()
        .find_map(|value| (!value.is_empty()).then(|| value.to_string_lossy().into_owned()))
}

#[cfg(windows)]
pub fn system_locale() -> Option<String> {
    use windows_sys::Win32::Globalization::GetUserDefaultLocaleName;
    // Windows documents 85 UTF-16 code units as the maximum locale name size.
    let mut name = [0u16; 85];
    // The buffer is writable, correctly sized, and lives through the call.
    let len = unsafe { GetUserDefaultLocaleName(name.as_mut_ptr(), name.len() as i32) };
    if len <= 1 {
        return None;
    }
    String::from_utf16(&name[..len as usize - 1]).ok()
}

#[cfg(test)]
mod tests {
    use super::{Locale, Message};

    #[test]
    fn traffic_chart_text_preserves_time_rate_and_sample_meaning() {
        assert_eq!(
            Locale::En.traffic_chart_history(39),
            "39s ago · gaps unavailable · grey zero"
        );
        assert_eq!(
            Locale::ZhCn.traffic_chart_history(39),
            "39 秒前 · 空隙表示数据不可用 · 灰色表示零流量"
        );
        assert_eq!(
            Locale::En.traffic_chart_peak("1.5 KiB/s"),
            "1.5 KiB/s shared peak · now"
        );
        assert_eq!(
            Locale::ZhCn.traffic_chart_peak("1.5 KiB/s"),
            "共用峰值 1.5 KiB/s · 现在"
        );
        assert_eq!(
            Locale::En.message(Message::TrafficNoSamples),
            "No traffic samples available yet."
        );
        assert_eq!(
            Locale::ZhCn.message(Message::TrafficNoSamples),
            "暂无流量采样数据。"
        );
    }

    #[test]
    fn selection_uses_override_and_falls_back_to_english() {
        for name in ["zh-CN", "zh_CN.UTF-8", "ZH-hans-cn", "zh-Hans"] {
            assert_eq!(Locale::select(Some(name), None), Locale::ZhCn);
        }
        assert_eq!(Locale::select(None, Some("zh_CN.UTF-8")), Locale::ZhCn);
        for name in ["", "zh-TW", "zh", "fr-FR", "C", "not a locale"] {
            assert_eq!(Locale::select(Some(name), Some("zh-CN")), Locale::En);
        }
        assert_eq!(Locale::select(None, None), Locale::En);
        assert_eq!(Locale::select(Some("en-US"), Some("zh-CN")), Locale::En);
    }

    #[test]
    #[cfg(not(windows))]
    fn posix_precedence_skips_empty_values_but_not_unsupported_ones() {
        use super::posix_locale;
        let vars = |values: [Option<&str>; 3]| values.map(|value| value.map(Into::into));
        assert_eq!(
            posix_locale(vars([Some("zh_CN.UTF-8"), Some("en"), Some("fr")])),
            Some("zh_CN.UTF-8".into())
        );
        assert_eq!(
            posix_locale(vars([Some(""), Some("zh-CN"), Some("en")])),
            Some("zh-CN".into())
        );
        assert_eq!(
            posix_locale(vars([None, None, Some("en_US.UTF-8")])),
            Some("en_US.UTF-8".into())
        );
        assert_eq!(posix_locale(vars([None, Some(""), None])), None);
        let selected = posix_locale(vars([Some("fr"), Some("zh-CN"), None]));
        assert_eq!(Locale::select(None, selected.as_deref()), Locale::En);
    }

    #[test]
    fn catalogs_cover_each_shell_key() {
        let keys = [
            Message::AppTitle,
            Message::Home,
            Message::Proxies,
            Message::Profiles,
            Message::Connections,
            Message::Rules,
            Message::Logs,
            Message::Unlock,
            Message::Settings,
            Message::Traffic,
            Message::TrafficActivity,
            Message::SessionTotal,
            Message::Upload,
            Message::Download,
            Message::StopSession,
            Message::Stopping,
            Message::ChooseEngine,
            Message::ChooseProfile,
            Message::StartSession,
            Message::ModeRule,
            Message::ModeGlobal,
            Message::ModeDirect,
            Message::Language,
            Message::SettingsReadOnly,
            Message::EngineLocked,
            Message::SavedLanguage,
            Message::CurrentLanguage,
            Message::LanguageSystem,
            Message::LanguageRestart,
            Message::LanguageUnavailable,
            Message::LanguageSaveFailed,
        ];
        for key in keys {
            assert!(!Locale::En.message(key).is_empty(), "English: {key:?}");
            assert!(!Locale::ZhCn.message(key).is_empty(), "Chinese: {key:?}");
        }
        assert_eq!(Locale::En.message(Message::Home), "Home");
        assert_eq!(Locale::ZhCn.message(Message::Home), "首页");
    }
    #[test]
    fn overview_state_labels_are_localized() {
        let cases = [
            (
                Message::OverviewAppliedProfile,
                "Applied profile",
                "已应用配置",
            ),
            (
                Message::OverviewAwaitingProfile,
                "Awaiting profile confirmation",
                "等待确认配置",
            ),
            (Message::OverviewProxyChecking, "Checking", "正在检查"),
            (Message::OverviewProxyOff, "Paw proxy off", "Paw 代理已关闭"),
            (Message::OverviewProxyOwned, "On for Paw", "Paw 已启用"),
            (
                Message::OverviewProxyRecovery,
                "Recovery record present",
                "发现恢复记录",
            ),
            (
                Message::OverviewProxyUnknown,
                "Ownership unknown",
                "归属未知",
            ),
            (Message::OverviewProxyError, "Error", "出错"),
            (
                Message::OverviewProxyReviewRecovery,
                "Review recovery",
                "查看恢复选项",
            ),
            (
                Message::OverviewProxyReviewError,
                "Review error",
                "查看错误",
            ),
            (
                Message::OverviewCurrentProfile,
                "Current profile",
                "当前配置",
            ),
            (
                Message::OverviewNoRunningProfile,
                "No running profile",
                "没有运行中的配置",
            ),
            (
                Message::OverviewUnconfirmedProfile,
                "Running profile unconfirmed",
                "运行中的配置尚未确认",
            ),
            (Message::OverviewNextProfile, "Next session", "下次会话"),
            (
                Message::OverviewRefreshSubscription,
                "Refresh subscription",
                "刷新订阅",
            ),
            (Message::OverviewCurrentRoute, "Current route", "当前路线"),
            (
                Message::OverviewRouteUnavailable,
                "Route information unavailable",
                "路线信息不可用",
            ),
            (
                Message::OverviewRouteSelections,
                "Group selections · Rules may use different routes",
                "代理组选择 · 不同规则可能使用不同路线",
            ),
            (
                Message::OverviewGlobalSelection,
                "Global group selection",
                "全局代理组选择",
            ),
            (Message::OverviewDirectRoute, "Direct mode", "直连模式"),
        ];
        for (key, english, chinese) in cases {
            assert_eq!(Locale::En.message(key), english);
            assert_eq!(Locale::ZhCn.message(key), chinese);
        }
    }

    #[test]
    fn settings_badges_and_unlock_help_are_localized() {
        let cases = [
            (Message::Unmanaged, "Unmanaged", "未管理"),
            (Message::Unavailable, "Unavailable", "不可用"),
            (Message::Loopback, "Loopback", "回环"),
            (Message::KeepRunning, "Keep running", "继续运行"),
            (
                Message::NoExecutableSelected,
                "No executable selected",
                "未选择可执行文件",
            ),
            (
                Message::UnlockUnavailableDetail,
                "Could not run checks through the managed core.",
                "无法通过托管核心运行检测。",
            ),
        ];
        for (key, en, zh) in cases {
            assert_eq!(Locale::En.message(key), en, "English: {key:?}");
            assert_eq!(Locale::ZhCn.message(key), zh, "Chinese: {key:?}");
            assert_eq!(Locale::select(Some("unsupported"), None).message(key), en);
        }
    }
    #[test]
    fn overview_route_excess_is_localized() {
        assert_eq!(Locale::En.overview_route_excess(1), "1 more group");
        assert_eq!(Locale::En.overview_route_excess(27), "27 more groups");
        assert_eq!(Locale::ZhCn.overview_route_excess(27), "另有 27 个代理组");
    }

    #[test]
    fn connection_match_counts_are_localized() {
        assert_eq!(Locale::En.connection_match_count(2, 7), "2 of 7 active");
        assert_eq!(
            Locale::ZhCn.connection_match_count(0, 7),
            "7 个活动连接中匹配 0 个"
        );
    }

    #[test]
    fn log_review_controls_are_localized() {
        for (message, english, chinese) in [
            (Message::PauseLogs, "Pause view", "暂停视图"),
            (Message::ResumeLogs, "Resume", "恢复"),
            (
                Message::LogNewestFirst,
                "Order: Newest first",
                "排序：最新在前",
            ),
            (
                Message::LogOldestFirst,
                "Order: Oldest first",
                "排序：最早在前",
            ),
            (Message::ClearLiveLogs, "Clear live logs", "清空实时日志"),
            (
                Message::LogsPausedCaption,
                "View paused · Live logs continue; resume to catch up.",
                "视图已暂停 · 实时日志仍在记录；恢复后显示最新日志。",
            ),
            (
                Message::PausedLogsEmpty,
                "No entries when paused",
                "暂停时没有日志",
            ),
            (
                Message::PausedLogsEmptyDetail,
                "Resume to see current log entries.",
                "恢复后可查看当前日志。",
            ),
        ] {
            assert_eq!(Locale::En.message(message), english);
            assert_eq!(Locale::ZhCn.message(message), chinese);
        }
    }

    #[test]
    fn visible_log_counts_cover_both_locales() {
        assert_eq!(
            Locale::En.visible_log_count(0, 3),
            "0 of 3 retained entries visible"
        );
        assert_eq!(
            Locale::ZhCn.visible_log_count(2, 3),
            "保留 3 条日志，显示 2 条"
        );
    }

    #[test]
    fn closed_connection_counts_and_session_limit_are_localized() {
        assert_eq!(Locale::En.closed_connection_count(2, 7, false), "7 closed");
        assert_eq!(
            Locale::En.closed_connection_count(2, 7, true),
            "2 of 7 closed"
        );
        assert_eq!(
            Locale::ZhCn.closed_connection_count(2, 7, false),
            "7 个已关闭连接"
        );
        assert_eq!(
            Locale::ZhCn.closed_connection_count(2, 7, true),
            "7 个已关闭连接中匹配 2 个"
        );
        assert_eq!(
            Locale::En.closed_connection_history_detail(100),
            "Read-only · Up to 100 closures observed in this session."
        );
        assert_eq!(
            Locale::ZhCn.closed_connection_history_detail(100),
            "只读 · 仅显示本次会话中观察到关闭的连接，最多 100 条。"
        );
    }

    #[test]
    fn rules_and_logs_catalog() {
        let cases = [
            (Message::RuleSets, "Rule sets", "规则集"),
            (Message::ReadRuleSets, "Read rule sets", "读取规则集"),
            (
                Message::ReadingRuleSets,
                "Reading rule sets…",
                "正在读取规则集…",
            ),
            (
                Message::RuleSetsReadOnly,
                "Read-only · This list does not update providers.",
                "只读 · 此列表不会更新提供者。",
            ),
            (
                Message::RuleSetsNotLoaded,
                "Rule sets have not been loaded.",
                "尚未加载规则集。",
            ),
            (
                Message::RuleSetsEmpty,
                "No rule sets found.",
                "未找到规则集。",
            ),
            (
                Message::RuleSetsUnavailable,
                "Could not read rule sets from the controller.",
                "无法从控制器读取规则集。",
            ),
            (Message::RuleSetSource, "Source", "来源"),
            (Message::RuleSetCount, "Rules", "规则数"),
            (Message::RuleSetUpdated, "Updated", "更新时间"),
            (Message::RuleSetRedacted, "[hidden]", "[已隐藏]"),
            (Message::RefreshRules, "Refresh rules", "刷新规则"),
            (Message::RuleLineColumn, "LINE", "行号"),
            (Message::RulePayloadFilter, "Search payload", "搜索匹配内容"),
            (
                Message::NoMatchingRules,
                "No matching rules",
                "没有匹配的规则",
            ),
            (
                Message::RuleFilterDetail,
                "Change or clear the payload search.",
                "更改或清除匹配内容搜索。",
            ),
            (Message::PreviousRulesPage, "Previous", "上一页"),
            (Message::NextRulesPage, "Next", "下一页"),
            (Message::RuleTypeColumn, "TYPE", "类型"),
            (Message::RuleMatchColumn, "MATCH", "匹配内容"),
            (Message::RulePolicyColumn, "POLICY", "策略"),
            (Message::NoRuleSnapshot, "No rule snapshot", "暂无规则快照"),
            (
                Message::RefreshRulesDetail,
                "Refresh to read the live rules from Meow.",
                "刷新以读取 Meow 的实时规则。",
            ),
            (
                Message::NoRunningSession,
                "No running session",
                "没有运行中的会话",
            ),
            (
                Message::StartToViewRules,
                "Start a profile to inspect its rule order and routing policies.",
                "启动配置后即可查看规则顺序和路由策略。",
            ),
            (Message::ClearLogs, "Clear", "清空"),
            (
                Message::WaitingForLogs,
                "Waiting for log entries",
                "等待日志",
            ),
            (
                Message::WaitingForLogsDetail,
                "New messages from this Meow session appear here.",
                "此 Meow 会话的新消息会显示在这里。",
            ),
            (
                Message::StartToViewLogs,
                "Start a profile to view live Meow logs. Entries are kept only for the current session.",
                "启动配置后即可查看 Meow 的实时日志。日志仅在当前会话中保留。",
            ),
        ];
        for (key, en, zh) in cases {
            assert_eq!(Locale::En.message(key), en, "English: {key:?}");
            assert_eq!(Locale::ZhCn.message(key), zh, "Chinese: {key:?}");
            assert_eq!(Locale::select(Some("unsupported"), None).message(key), en);
        }
        assert_eq!(Locale::En.rule_match_count(2, 7), "2 of 7 rules");
        assert_eq!(Locale::ZhCn.rule_match_count(2, 7), "7 条规则中匹配 2 条");
        assert_eq!(
            Locale::En.rule_page_range(101, 150, 150),
            "Showing 101–150 of 150 matches"
        );
        assert_eq!(
            Locale::ZhCn.rule_page_range(101, 150, 150),
            "显示 150 条匹配结果中的第 101–150 条"
        );
        for count in [0, 1, 42] {
            assert_eq!(Locale::En.rule_count(count), format!("{count} RULES"));
            assert_eq!(Locale::ZhCn.rule_count(count), format!("{count} 条规则"));
            assert_eq!(
                Locale::En.retained_log_count(count),
                format!("{count} retained entries")
            );
            assert_eq!(
                Locale::ZhCn.retained_log_count(count),
                format!("保留 {count} 条日志")
            );
        }
    }

    #[test]
    fn subscription_name_controls_are_localized() {
        for (message, en, zh) in [
            (Message::RenameSubscription, "Rename", "重命名"),
            (
                Message::SubscriptionName,
                "Local name, up to 80 characters (blank uses the ID)",
                "本地名称，最多 80 个字符（留空则使用 ID）",
            ),
            (Message::SaveSubscriptionName, "Save", "保存"),
            (
                Message::SavingSubscriptionName,
                "Saving name…",
                "正在保存名称…",
            ),
            (
                Message::SubscriptionNameSaveFailed,
                "Could not save the subscription name",
                "无法保存订阅名称",
            ),
        ] {
            assert_eq!(Locale::En.message(message), en);
            assert_eq!(Locale::ZhCn.message(message), zh);
        }
    }

    #[test]
    fn subscription_removal_controls_are_localized() {
        for message in [
            Message::RemoveSubscription,
            Message::DetachSubscription,
            Message::SubscriptionDetached,
            Message::ConfirmRemoveSubscription,
            Message::RemoveSubscriptionDetail,
            Message::SubscriptionRemoveBlocked,
            Message::SubscriptionRemoveFailed,
            Message::SubscriptionRemoved,
        ] {
            assert!(!Locale::En.message(message).is_empty());
            assert!(!Locale::ZhCn.message(message).is_empty());
            assert_ne!(Locale::En.message(message), Locale::ZhCn.message(message));
        }
    }

    #[test]
    fn subscription_metadata_labels_are_localized() {
        assert_eq!(
            Locale::En.message(Message::SubscriptionManualOnly),
            "Manual refresh only"
        );
        assert_eq!(
            Locale::ZhCn.message(Message::SubscriptionManualOnly),
            "仅手动刷新"
        );
        assert_eq!(
            Locale::En.subscription_interval(Message::SubscriptionLocalInterval, 45),
            "Local override: 45 min"
        );
        assert_eq!(
            Locale::ZhCn.subscription_interval(Message::SubscriptionLocalInterval, 45),
            "本地覆盖：45 分钟"
        );
        assert_eq!(
            Locale::En.subscription_interval(Message::SubscriptionProviderInterval, 120),
            "Provider suggests: 120 min"
        );
        assert_eq!(
            Locale::ZhCn.subscription_interval(Message::SubscriptionProviderInterval, 120),
            "提供者建议：120 分钟"
        );
    }

    #[test]
    fn profiles_and_connections_catalog() {
        let cases = [
            (
                Message::NoProfileSelected,
                "No profile selected",
                "未选择配置",
            ),
            (Message::Subscription, "Subscription", "订阅"),
            (
                Message::ImportLocalYaml,
                "Import local YAML",
                "导入本地 YAML",
            ),
            (
                Message::AddSubscription,
                "Add HTTPS subscription",
                "添加 HTTPS 订阅",
            ),
            (
                Message::SubscriptionUrl,
                "HTTPS subscription URL",
                "HTTPS 订阅地址",
            ),
            (
                Message::ImportSubscription,
                "Import subscription",
                "导入订阅",
            ),
            (Message::CancelSubscription, "Cancel", "取消"),
            (
                Message::DownloadingSubscription,
                "Downloading subscription…",
                "正在下载订阅…",
            ),
            (
                Message::ProfileSelectionDetail,
                "Use selects the next session; Apply live changes the running profile.",
                "使用会选择下次会话的配置；实时应用会更改正在运行的配置。",
            ),
            (
                Message::LiveApplyWarning,
                "Apply live and Reload keep Meow running, but can disconnect active connections.",
                "实时应用和重新加载会保持 Meow 运行，但可能断开活动连接。",
            ),
            (
                Message::NoLocalProfiles,
                "No local profiles",
                "暂无本地配置",
            ),
            (
                Message::ProfileNameFilter,
                "Filter profile names",
                "筛选配置名称",
            ),
            (
                Message::NoMatchingProfiles,
                "No matching profiles",
                "没有匹配的配置",
            ),
            (
                Message::ProfileFilterDetail,
                "Try another profile name.",
                "请尝试其他配置名称。",
            ),
            (
                Message::ImportYamlDetail,
                "Import a YAML file. Paw prepares a separate session copy and leaves your file unchanged.",
                "导入 YAML 文件。Paw 会为会话准备单独的副本，不会更改原文件。",
            ),
            (
                Message::SubscriptionDetail,
                "HTTPS subscription · Refresh does not change the running profile",
                "HTTPS 订阅 · 刷新不会更改正在运行的配置",
            ),
            (Message::Running, "Running", "运行中"),
            (Message::NextSession, "Next session", "下次会话"),
            (Message::Applying, "Applying…", "正在应用…"),
            (Message::Reload, "Reload", "重新加载"),
            (Message::ApplyLive, "Apply live", "实时应用"),
            (Message::Use, "Use", "使用"),
            (Message::Forget, "Forget", "移除"),
            (
                Message::LastStartDiagnostics,
                "Diagnostics from the last start attempt",
                "上次启动尝试的诊断信息",
            ),
            (Message::ChooseYaml, "Choose Meow YAML", "选择 Meow YAML"),
            (
                Message::ListSubscriptionsFailed,
                "Could not list saved subscriptions.",
                "无法列出已保存的订阅。",
            ),
            (
                Message::SubscriptionSaved,
                "Subscription saved. Select Use or Apply live when ready.",
                "订阅已保存。准备就绪后请选择使用或实时应用。",
            ),
            (
                Message::SubscriptionRefreshed,
                "Subscription refreshed. The running profile is unchanged.",
                "订阅已刷新。正在运行的配置未更改。",
            ),
            (
                Message::ChooseExistingYaml,
                "Choose an existing local YAML file.",
                "请选择存在的本地 YAML 文件。",
            ),
            (
                Message::InvalidSubscriptionUrl,
                "Enter an HTTPS subscription URL without user info or a fragment.",
                "请输入不含用户信息或片段的 HTTPS 订阅地址。",
            ),
            (
                Message::SubscriptionDownloadFailed,
                "Could not download the subscription",
                "无法下载订阅",
            ),
            (
                Message::SubscriptionTooLarge,
                "Subscription profile exceeds the size limit",
                "订阅配置超过大小限制",
            ),
            (
                Message::SubscriptionInvalidText,
                "Subscription did not return UTF-8 text",
                "订阅返回的内容不是 UTF-8 文本",
            ),
            (
                Message::SubscriptionInvalidProfile,
                "Subscription profile is invalid",
                "订阅配置无效",
            ),
            (
                Message::SubscriptionNoProxies,
                "Subscription has no proxies or proxy providers",
                "订阅中没有代理或代理提供者",
            ),
            (
                Message::SubscriptionSaveFailed,
                "Could not save the subscription",
                "无法保存订阅",
            ),
            (
                Message::SubscriptionReadFailed,
                "Could not read the saved subscription",
                "无法读取已保存的订阅",
            ),
            (
                Message::SubscriptionRefreshSaveFailed,
                "Could not save the refreshed subscription",
                "无法保存刷新后的订阅",
            ),
            (
                Message::SubscriptionWriteConflict,
                "Subscription is busy or has changed. Please try again.",
                "订阅正忙或已更改，请重试。",
            ),
            (Message::ActiveConnections, "active", "个活动连接"),
            (Message::ActiveTab, "Active", "活动"),
            (Message::ClosedTab, "Closed", "已关闭"),
            (
                Message::NoClosedConnections,
                "No closed connections recorded",
                "尚无已记录的关闭连接",
            ),
            (
                Message::NoClosedConnectionsDetail,
                "Connections observed closing in this session appear here.",
                "当前会话中观察到关闭的连接会显示在这里。",
            ),
            (
                Message::LiveConnectionsUnavailable,
                "Live connections unavailable",
                "无法获取实时连接",
            ),
            (
                Message::LiveConnectionsUnavailableDetail,
                "Waiting for a live connection sample.",
                "正在等待实时连接数据。",
            ),
            (
                Message::NoActiveConnections,
                "No active connections",
                "暂无活动连接",
            ),
            (
                Message::ListenerConnectionsDetail,
                "Traffic handled by the Paw listener appears here.",
                "由 Paw 监听器处理的流量会显示在这里。",
            ),
            (
                Message::NoMatchingConnections,
                "No matching connections",
                "没有匹配的连接",
            ),
            (
                Message::ConnectionFilterDetail,
                "Change or clear the filter. Search destination, policy chain, matched rule, or transport.",
                "更改或清除筛选条件。可搜索目标地址、策略链、匹配规则或传输协议。",
            ),
            (
                Message::StartToViewConnections,
                "Start a profile to monitor its live connections.",
                "启动配置后即可监控实时连接。",
            ),
            (Message::DestinationColumn, "DESTINATION", "目标地址"),
            (Message::RouteColumn, "ROUTE", "路线"),
            (Message::RuleColumn, "RULE", "规则"),
            (Message::CloseConnection, "Close", "关闭"),
            (
                Message::CloseConnectionAction,
                "Close connection",
                "关闭连接",
            ),
            (Message::ConnectionDetails, "Details", "详情"),
            (Message::CloseDetails, "Close details", "关闭详情"),
            (Message::Transport, "Transport", "传输协议"),
            (Message::UnknownValue, "Unknown", "未知"),
            (
                Message::CurrentSampleTotals,
                "Last reported totals · Active sample",
                "最近报告的总量 · 活动采样",
            ),
            (
                Message::ClosedSampleTotals,
                "Last reported totals · Last sample before closure",
                "最近报告的总量 · 关闭前最后一次采样",
            ),
        ];
        for (key, english, chinese) in cases {
            assert_eq!(Locale::En.message(key), english, "English: {key:?}");
            assert_eq!(Locale::ZhCn.message(key), chinese, "Chinese: {key:?}");
        }
    }

    #[test]
    fn page_catalog_preserves_english_and_translates_chinese() {
        let cases = [
            (Message::ActiveProfile, "Active profile", "当前配置"),
            (Message::Configuration, "Configuration", "配置文件"),
            (Message::ProxyGroups, "Proxy groups", "代理组"),
            (Message::SelectedRoutes, "Selected routes", "已选路线"),
            (Message::Network, "Network", "网络"),
            (Message::MeowEngine, "Meow engine", "Meow 引擎"),
            (Message::ManagedSession, "Managed session", "托管会话"),
            (Message::LocalListener, "Local listener", "本地监听"),
            (Message::SystemProxy, "System proxy", "系统代理"),
            (
                Message::ExistingSystemProxy,
                "Existing system proxy",
                "现有系统代理",
            ),
            (Message::Copy, "Copy", "复制"),
            (Message::ShowDetails, "Show details", "显示详情"),
            (Message::HideDetails, "Hide details", "隐藏详情"),
            (Message::Filter, "Filter", "筛选"),
            (Message::JumpToGroup, "Jump to group", "跳转到代理组"),
            (Message::Providers, "Providers", "提供者"),
            (
                Message::NoCandidateRoutes,
                "This group has no candidate routes.",
                "此组没有候选路线。",
            ),
            (Message::SortDefault, "Sort: Default", "排序：默认"),
            (Message::SortLatency, "Sort: Latency", "排序：延迟"),
            (Message::SortName, "Sort: Name", "排序：名称"),
            (Message::Test, "Test", "测试"),
            (Message::Refresh, "Refresh", "刷新"),
            (
                Message::ProviderNoUsage,
                "No usage information",
                "暂无流量用量信息",
            ),
            (
                Message::ProviderAwaitingUpdate,
                "Awaiting first update",
                "等待首次更新",
            ),
            (Message::ProviderRefreshing, "Refreshing…", "正在刷新…"),
            (
                Message::ProviderRefreshBusy,
                "Another provider is refreshing",
                "另一个代理集合正在刷新",
            ),
            (
                Message::LatencyTestDescription,
                "Latency test · Meow opens this HTTPS URL through the chosen proxy.",
                "延迟测试 · Meow 通过所选代理访问此 HTTPS 地址。",
            ),
            (
                Message::NoMatchingGroups,
                "No matching groups",
                "没有匹配的代理组",
            ),
            (
                Message::TryAnotherRoute,
                "Try another route or group name.",
                "请尝试其他路线或代理组名称。",
            ),
            (Message::NoProxyGroups, "No proxy groups yet", "暂无代理组"),
            (
                Message::StartToViewGroups,
                "Select a YAML profile and start the engine to view its groups.",
                "选择 YAML 配置并启动引擎以查看代理组。",
            ),
            (Message::Route, "route", "条路线"),
            (Message::Routes, "routes", "条路线"),
            (Message::Groups, "groups", "个代理组"),
            (Message::Node, "node", "个节点"),
            (Message::Nodes, "nodes", "个节点"),
            (Message::System, "System", "系统"),
            (
                Message::SystemProxyDescription,
                "Paw only changes the desktop proxy if you explicitly enable it in Settings.",
                "仅在你于设置中主动启用时，Paw 才会修改桌面代理。",
            ),
            (Message::TunMode, "TUN mode", "TUN 模式"),
            (
                Message::TunDescription,
                "A virtual network interface is not available in this build.",
                "此版本不支持虚拟网络接口。",
            ),
            (
                Message::ListenerDescription,
                "Only apps configured to use Paw are routed through it.",
                "只有配置为使用 Paw 的应用才会通过它连接。",
            ),
            (
                Message::MeowExecutable,
                "Meow executable",
                "Meow 可执行文件",
            ),
            (Message::SettingsReadOnly, "Read-only status", "只读状态"),
            (Message::EngineLocked, "Locked", "已锁定"),
            (Message::Browse, "Browse…", "浏览…"),
            (Message::HideFullPath, "Hide full path", "隐藏完整路径"),
            (Message::ShowFullPath, "Show full path", "显示完整路径"),
            (Message::CopyPath, "Copy path", "复制路径"),
            (Message::TrayBehavior, "Tray behavior", "托盘行为"),
            (
                Message::TrayDescription,
                "Closing the window keeps Paw running when a desktop tray is available.",
                "桌面托盘可用时，关闭窗口后 Paw 继续运行。",
            ),
            (
                Message::ApplicationVersion,
                "Application version",
                "应用版本",
            ),
            (Message::QuitApplication, "Quit application", "退出应用"),
            (
                Message::QuitDescription,
                "Quitting also stops the managed engine.",
                "退出时也会停止托管的引擎。",
            ),
            (Message::QuitPaw, "Quit Meow Paw", "退出 Meow Paw"),
        ];
        for (key, english, chinese) in cases {
            assert_eq!(Locale::En.message(key), english, "English: {key:?}");
            assert_eq!(Locale::ZhCn.message(key), chinese, "Chinese: {key:?}");
            assert_eq!(
                Locale::select(Some("unsupported"), None).message(key),
                english
            );
        }
    }
}
