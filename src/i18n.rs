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
}
