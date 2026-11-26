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
    Providers,
    NoCandidateRoutes,
    SortDefault,
    SortLatency,
    SortName,
    Test,
    Refresh,
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
    UnlockUnavailableDetail,
    MeowExecutable,
    Browse,
    HideFullPath,
    ShowFullPath,
    CopyPath,
    Language,
    SavedLanguage,
    CurrentLanguage,
    LanguageSystem,
    LanguageRestart,
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
    DownloadingSubscription,
    LiveApplyWarning,
    ProfileSelectionDetail,
    NoLocalProfiles,
    ImportYamlDetail,
    SubscriptionDetail,
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
    SubscriptionNoProxies,
    SubscriptionSaveFailed,
    SubscriptionReadFailed,
    SubscriptionRefreshSaveFailed,
    ActiveConnections,
    NoActiveConnections,
    ListenerConnectionsDetail,
    NoMatchingConnections,
    ConnectionFilterDetail,
    StartToViewConnections,
    DestinationColumn,
    RouteColumn,
    RuleColumn,
    CloseConnection,
    RefreshRules,
    RuleTypeColumn,
    RuleMatchColumn,
    RulePolicyColumn,
    NoRuleSnapshot,
    RefreshRulesDetail,
    NoRunningSession,
    StartToViewRules,
    ClearLogs,
    WaitingForLogs,
    WaitingForLogsDetail,
    StartToViewLogs,
}

impl Locale {
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

    pub fn connection_match_count(self, matched: usize, total: usize) -> String {
        match self {
            Self::En => format!("{matched} of {total} active"),
            Self::ZhCn => format!("{total} 个活动连接中匹配 {matched} 个"),
        }
    }

    pub fn rule_count(self, count: usize) -> String {
        match self {
            Self::En => format!("{count} RULES"),
            Self::ZhCn => format!("{count} 条规则"),
        }
    }

    pub fn retained_log_count(self, count: usize) -> String {
        match self {
            Self::En => format!("{count} retained entries"),
            Self::ZhCn => format!("保留 {count} 条日志"),
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
        Message::Providers => "Providers",
        Message::NoCandidateRoutes => "This group has no candidate routes.",
        Message::SortDefault => "Sort: Default",
        Message::SortLatency => "Sort: Latency",
        Message::SortName => "Sort: Name",
        Message::Test => "Test",
        Message::Refresh => "Refresh",
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
        Message::SystemProxyDescription => "Paw does not change the operating system’s proxy.",
        Message::TunMode => "TUN mode",
        Message::TunDescription => "A virtual network interface is not available in this build.",
        Message::ListenerDescription => "Only apps configured to use Paw are routed through it.",
        Message::Unmanaged => "Unmanaged",
        Message::Unavailable => "Unavailable",
        Message::Loopback => "Loopback",
        Message::KeepRunning => "Keep running",
        Message::NoExecutableSelected => "No executable selected",
        Message::UnlockUnavailableDetail => {
            "Service availability tests are not available in this build."
        }
        Message::MeowExecutable => "Meow executable",
        Message::Browse => "Browse…",
        Message::HideFullPath => "Hide full path",
        Message::ShowFullPath => "Show full path",
        Message::CopyPath => "Copy path",
        Message::Language => "Language",
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
        Message::DownloadingSubscription => "Downloading subscription…",
        Message::ProfileSelectionDetail => {
            "Use selects the next session; Apply live changes the running profile."
        }
        Message::LiveApplyWarning => {
            "Apply live and Reload keep Meow running, but can disconnect active connections."
        }
        Message::NoLocalProfiles => "No local profiles",
        Message::ImportYamlDetail => {
            "Import a YAML file. Paw prepares a separate session copy and leaves your file unchanged."
        }
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
        Message::SubscriptionNoProxies => "Subscription has no proxies or proxy providers",
        Message::SubscriptionSaveFailed => "Could not save the subscription",
        Message::SubscriptionReadFailed => "Could not read the saved subscription",
        Message::SubscriptionRefreshSaveFailed => "Could not save the refreshed subscription",
        Message::ActiveConnections => "active",
        Message::NoActiveConnections => "No active connections",
        Message::ListenerConnectionsDetail => "Traffic handled by the Paw listener appears here.",
        Message::NoMatchingConnections => "No matching connections",
        Message::ConnectionFilterDetail => {
            "Change or clear the filter. Search destination, policy chain, or matched rule."
        }
        Message::StartToViewConnections => "Start a profile to monitor its live connections.",
        Message::DestinationColumn => "DESTINATION",
        Message::RouteColumn => "ROUTE",
        Message::RuleColumn => "RULE",
        Message::CloseConnection => "Close",
        Message::RefreshRules => "Refresh rules",
        Message::RuleTypeColumn => "TYPE",
        Message::RuleMatchColumn => "MATCH",
        Message::RulePolicyColumn => "POLICY",
        Message::NoRuleSnapshot => "No rule snapshot",
        Message::RefreshRulesDetail => "Refresh to read the live rules from Meow.",
        Message::NoRunningSession => "No running session",
        Message::StartToViewRules => {
            "Start a profile to inspect its rule order and routing policies."
        }
        Message::ClearLogs => "Clear",
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
        Message::Providers => "提供者",
        Message::NoCandidateRoutes => "此组没有候选路线。",
        Message::SortDefault => "排序：默认",
        Message::SortLatency => "排序：延迟",
        Message::SortName => "排序：名称",
        Message::Test => "测试",
        Message::Refresh => "刷新",
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
        Message::SystemProxyDescription => "Paw 不会更改操作系统的代理设置。",
        Message::TunMode => "TUN 模式",
        Message::TunDescription => "此版本不支持虚拟网络接口。",
        Message::ListenerDescription => "只有配置为使用 Paw 的应用才会通过它连接。",
        Message::Unmanaged => "未管理",
        Message::Unavailable => "不可用",
        Message::Loopback => "回环",
        Message::KeepRunning => "继续运行",
        Message::NoExecutableSelected => "未选择可执行文件",
        Message::UnlockUnavailableDetail => "此版本不提供服务可用性测试。",
        Message::MeowExecutable => "Meow 可执行文件",
        Message::Browse => "浏览…",
        Message::HideFullPath => "隐藏完整路径",
        Message::ShowFullPath => "显示完整路径",
        Message::CopyPath => "复制路径",
        Message::Language => "语言",
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
        Message::DownloadingSubscription => "正在下载订阅…",
        Message::ProfileSelectionDetail => {
            "使用会选择下次会话的配置；实时应用会更改正在运行的配置。"
        }
        Message::LiveApplyWarning => "实时应用和重新加载会保持 Meow 运行，但可能断开活动连接。",
        Message::NoLocalProfiles => "暂无本地配置",
        Message::ImportYamlDetail => "导入 YAML 文件。Paw 会为会话准备单独的副本，不会更改原文件。",
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
        Message::SubscriptionNoProxies => "订阅中没有代理或代理提供者",
        Message::SubscriptionSaveFailed => "无法保存订阅",
        Message::SubscriptionReadFailed => "无法读取已保存的订阅",
        Message::SubscriptionRefreshSaveFailed => "无法保存刷新后的订阅",
        Message::ActiveConnections => "个活动连接",
        Message::NoActiveConnections => "暂无活动连接",
        Message::ListenerConnectionsDetail => "由 Paw 监听器处理的流量会显示在这里。",
        Message::NoMatchingConnections => "没有匹配的连接",
        Message::ConnectionFilterDetail => "更改或清除筛选条件。可搜索目标地址、策略链或匹配规则。",
        Message::StartToViewConnections => "启动配置后即可监控实时连接。",
        Message::DestinationColumn => "目标地址",
        Message::RouteColumn => "路线",
        Message::RuleColumn => "规则",
        Message::CloseConnection => "关闭",
        Message::RefreshRules => "刷新规则",
        Message::RuleTypeColumn => "类型",
        Message::RuleMatchColumn => "匹配内容",
        Message::RulePolicyColumn => "策略",
        Message::NoRuleSnapshot => "暂无规则快照",
        Message::RefreshRulesDetail => "刷新以读取 Meow 的实时规则。",
        Message::NoRunningSession => "没有运行中的会话",
        Message::StartToViewRules => "启动配置后即可查看规则顺序和路由策略。",
        Message::ClearLogs => "清空",
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
                "Service availability tests are not available in this build.",
                "此版本不提供服务可用性测试。",
            ),
        ];
        for (key, en, zh) in cases {
            assert_eq!(Locale::En.message(key), en, "English: {key:?}");
            assert_eq!(Locale::ZhCn.message(key), zh, "Chinese: {key:?}");
            assert_eq!(Locale::select(Some("unsupported"), None).message(key), en);
        }
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
    fn rules_and_logs_catalog() {
        let cases = [
            (Message::RefreshRules, "Refresh rules", "刷新规则"),
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
            (Message::ActiveConnections, "active", "个活动连接"),
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
                "Change or clear the filter. Search destination, policy chain, or matched rule.",
                "更改或清除筛选条件。可搜索目标地址、策略链或匹配规则。",
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
                "Paw does not change the operating system’s proxy.",
                "Paw 不会更改操作系统的代理设置。",
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
