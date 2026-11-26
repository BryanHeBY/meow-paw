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
    MeowExecutable,
    Browse,
    HideFullPath,
    ShowFullPath,
    CopyPath,
    TrayBehavior,
    TrayDescription,
    ApplicationVersion,
    QuitApplication,
    QuitDescription,
    QuitPaw,
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
        Message::MeowExecutable => "Meow executable",
        Message::Browse => "Browse…",
        Message::HideFullPath => "Hide full path",
        Message::ShowFullPath => "Show full path",
        Message::CopyPath => "Copy path",
        Message::TrayBehavior => "Tray behavior",
        Message::TrayDescription => {
            "Closing the window keeps Paw running when a desktop tray is available."
        }
        Message::ApplicationVersion => "Application version",
        Message::QuitApplication => "Quit application",
        Message::QuitDescription => "Quitting also stops the managed engine.",
        Message::QuitPaw => "Quit Meow Paw",
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
        Message::MeowExecutable => "Meow 可执行文件",
        Message::Browse => "浏览…",
        Message::HideFullPath => "隐藏完整路径",
        Message::ShowFullPath => "显示完整路径",
        Message::CopyPath => "复制路径",
        Message::TrayBehavior => "托盘行为",
        Message::TrayDescription => "桌面托盘可用时，关闭窗口后 Paw 继续运行。",
        Message::ApplicationVersion => "应用版本",
        Message::QuitApplication => "退出应用",
        Message::QuitDescription => "退出时也会停止托管的引擎。",
        Message::QuitPaw => "退出 Meow Paw",
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
        ];
        for key in keys {
            assert!(!Locale::En.message(key).is_empty(), "English: {key:?}");
            assert!(!Locale::ZhCn.message(key).is_empty(), "Chinese: {key:?}");
        }
        assert_eq!(Locale::En.message(Message::Home), "Home");
        assert_eq!(Locale::ZhCn.message(Message::Home), "首页");
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
