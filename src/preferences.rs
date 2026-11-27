use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

const LIMIT: u64 = 64 * 1024;
const PROFILE_LIMIT: usize = 32;

/// The language to use at the next launch. System is stored as a missing field.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum LanguagePreference {
    #[default]
    #[serde(rename = "system")]
    System,
    #[serde(rename = "en")]
    En,
    #[serde(rename = "zh-CN")]
    ZhCn,
}

impl LanguagePreference {
    fn is_system(&self) -> bool {
        *self == Self::System
    }

    pub fn locale_name(self) -> Option<&'static str> {
        match self {
            Self::System => None,
            Self::En => Some("en"),
            Self::ZhCn => Some("zh-CN"),
        }
    }
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Selections {
    engine: Option<PathBuf>,
    profile: Option<PathBuf>,
    profiles: Vec<PathBuf>,
    #[serde(skip_serializing_if = "LanguagePreference::is_system")]
    language: LanguagePreference,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    reduce_motion: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    mixed_port: Option<std::num::NonZeroU16>,
}

#[derive(Default)]
pub struct Preferences {
    file: Option<PathBuf>,
    selected: Selections,
    mixed_port_override: Option<std::num::NonZeroU16>,
}

impl Preferences {
    pub fn discover(enabled: bool) -> (Self, Option<&'static str>) {
        if !enabled {
            return (Self::default(), None);
        }
        let Some(file) = default_file() else {
            return (
                Self::default(),
                Some("No user configuration directory is available. Selections are temporary."),
            );
        };
        match Self::open(file) {
            Ok(prefs) => (prefs, None),
            Err(_) => (
                Self::default(),
                Some(
                    "Saved preferences could not be read. Paw will leave the file untouched; new selections are temporary.",
                ),
            ),
        }
    }

    fn open(file: PathBuf) -> io::Result<Self> {
        let selected = match File::open(&file) {
            Ok(file) => {
                let mut raw = Vec::new();
                file.take(LIMIT + 1).read_to_end(&mut raw)?;
                if raw.len() as u64 > LIMIT {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "Preferences exceed size limit",
                    ));
                }
                let mut value: Selections = serde_json::from_slice(&raw)?;
                if value
                    .engine
                    .iter()
                    .chain(&value.profile)
                    .chain(&value.profiles)
                    .any(|p| !p.is_absolute())
                {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "Saved paths must be absolute",
                    ));
                }
                if let Some(path) = value.profile.as_ref()
                    && !value.profiles.contains(path)
                {
                    value.profiles.push(path.clone());
                }
                let mut seen = std::collections::HashSet::new();
                value.profiles.retain(|path| seen.insert(path.clone()));
                value.profiles.truncate(PROFILE_LIMIT);
                value
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => Selections::default(),
            Err(e) => return Err(e),
        };
        Ok(Self {
            file: Some(file),
            selected,
            mixed_port_override: None,
        })
    }

    pub fn set_mixed_port_override(&mut self, port: Option<std::num::NonZeroU16>) {
        self.mixed_port_override = port;
    }

    #[cfg(any(target_os = "linux", windows))]
    pub fn mixed_port(&self) -> u16 {
        self.mixed_port_override
            .or(self.selected.mixed_port)
            .map_or(
                meow_paw::engine::session::DEFAULT_MIXED_PORT,
                std::num::NonZeroU16::get,
            )
    }

    #[cfg(any(target_os = "linux", windows))]
    pub fn saved_mixed_port(&self) -> u16 {
        self.selected.mixed_port.map_or(
            meow_paw::engine::session::DEFAULT_MIXED_PORT,
            std::num::NonZeroU16::get,
        )
    }

    #[cfg(any(target_os = "linux", windows))]
    pub fn has_mixed_port_override(&self) -> bool {
        self.mixed_port_override.is_some()
    }

    #[cfg(any(target_os = "linux", windows))]
    pub fn save_mixed_port(&mut self, port: std::num::NonZeroU16) -> io::Result<()> {
        self.persist(|next| {
            next.mixed_port = Some(port);
            Ok(())
        })
    }

    pub fn resolve(
        &self,
        cli_engine: Option<PathBuf>,
        cli_profile: Option<PathBuf>,
    ) -> (Option<PathBuf>, Option<PathBuf>) {
        (
            cli_engine.or_else(|| self.selected.engine.clone()),
            cli_profile.or_else(|| self.selected.profile.clone()),
        )
    }

    #[cfg(any(target_os = "linux", windows))]
    pub fn proxy_state_dir(&self) -> Option<PathBuf> {
        Some(self.file.as_ref()?.parent()?.to_path_buf())
    }

    pub fn subscription_dir(&self) -> Option<PathBuf> {
        Some(self.file.as_ref()?.parent()?.join("subscriptions"))
    }

    pub fn profiles(&self) -> &[PathBuf] {
        &self.selected.profiles
    }

    /// Hold the preferences guard while an unreferenced subscription is removed.
    /// Re-read the saved selections so another cooperating Paw's changes win.
    pub fn with_unreferenced_profile<T>(
        &self,
        path: &Path,
        action: impl FnOnce() -> io::Result<T>,
    ) -> io::Result<T> {
        let Some(file) = self.file.as_ref() else {
            return Err(io::Error::other("Preferences unavailable"));
        };
        let _guard = write_guard(file)?;
        let current = Self::open(file.clone())?;
        if current.selected.profile.as_deref() == Some(path)
            || current
                .selected
                .profiles
                .iter()
                .any(|candidate| candidate == path)
        {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "Profile is referenced",
            ));
        }
        action()
    }

    /// Whether selections can be written to a persistent preferences file.
    pub fn has_persistent_file(&self) -> bool {
        self.file.is_some()
    }

    /// Return the saved choice, independent of any temporary launch override.
    pub fn language(&self) -> LanguagePreference {
        self.selected.language
    }

    /// Save the choice for the next launch; the running UI keeps its locale.
    pub fn set_language(&mut self, language: LanguagePreference) -> io::Result<()> {
        self.persist(|next| {
            next.language = language;
            Ok(())
        })
    }

    /// Whether optional in-app transitions are disabled.
    pub fn reduce_motion(&self) -> bool {
        self.selected.reduce_motion
    }

    /// Apply the motion preference without restarting Paw.
    pub fn set_reduce_motion(&mut self, enabled: bool) -> io::Result<()> {
        self.persist(|next| {
            next.reduce_motion = enabled;
            Ok(())
        })
    }

    pub fn remember(&mut self, engine: bool, path: &Path) -> io::Result<()> {
        self.remember_checked(engine, path, || Ok(()))
    }

    /// Validate a managed record while holding the preferences guard, before
    /// publishing its path. Removal takes the same guards in the same order.
    pub fn remember_checked(
        &mut self,
        engine: bool,
        path: &Path,
        validate: impl FnOnce() -> io::Result<()>,
    ) -> io::Result<()> {
        let path = std::path::absolute(path)?;
        self.persist(|next| {
            validate()?;
            if engine {
                next.engine = Some(path);
            } else {
                if !next.profiles.contains(&path) {
                    if next.profiles.len() >= PROFILE_LIMIT {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "Profile list is full",
                        ));
                    }
                    next.profiles.push(path.clone());
                }
                next.profile = Some(path);
            }
            Ok(())
        })
    }

    pub fn forget_profile(&mut self, path: &Path) -> io::Result<()> {
        self.persist(|next| {
            next.profiles.retain(|candidate| candidate != path);
            if next.profile.as_deref() == Some(path) {
                next.profile = None;
            }
            Ok(())
        })
    }

    /// Detach only a remembered profile that is not the saved selection.
    pub fn detach_inactive_profile(&mut self, path: &Path) -> io::Result<()> {
        self.persist(|next| {
            if next.profile.as_deref() == Some(path) || !next.profiles.iter().any(|p| p == path) {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    "Profile is selected or no longer remembered",
                ));
            }
            next.profiles.retain(|candidate| candidate != path);
            Ok(())
        })
    }

    fn persist(
        &mut self,
        mutate: impl FnOnce(&mut Selections) -> io::Result<()>,
    ) -> io::Result<()> {
        let Some(file) = self.file.as_ref() else {
            let mut next = self.selected.clone();
            mutate(&mut next)?;
            self.selected = next;
            return Ok(());
        };
        let dir = file
            .parent()
            .ok_or_else(|| io::Error::other("Preferences parent is missing"))?;
        fs::create_dir_all(dir)?;
        let _guard = write_guard(file)?;
        let mut next = Self::open(file.clone())?.selected;
        mutate(&mut next)?;
        let raw = serde_json::to_vec_pretty(&next)?;
        if raw.len() as u64 > LIMIT {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Preferences exceed size limit",
            ));
        }
        let mut temp = tempfile::NamedTempFile::new_in(dir)?;
        temp.write_all(&raw)?;
        temp.write_all(b"\n")?;
        temp.as_file().sync_all()?;
        // Replace in the same directory, without truncating the existing file
        // if serialization, writing, or publication fails.
        temp.persist(file).map_err(|err| err.error)?;
        self.selected = next;
        Ok(())
    }
}

struct WriteGuard(File);

impl Drop for WriteGuard {
    fn drop(&mut self) {
        // On Unix, a concurrent fork can briefly keep a copy of the file
        // descriptor alive even though it is marked close-on-exec. Release
        // the lock when the transaction ends, not when the last copy closes.
        let _ = self.0.unlock();
    }
}

// The sidecar is stable across replacements and is never removed. Cooperating
// writers require a local filesystem with working locks and atomic replacement;
// older versions and external editors do not participate in this protocol.
fn write_guard(file: &Path) -> io::Result<WriteGuard> {
    let mut name = file
        .file_name()
        .ok_or_else(|| io::Error::other("Preferences filename is missing"))?
        .to_os_string();
    name.push(".lock");
    let path = file.with_file_name(name);
    let mut options = fs::OpenOptions::new();
    options.read(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    // Match the subscription guard's protections against symlinks and FIFOs.
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    #[cfg(target_os = "macos")]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(0x100 | 0x4); // O_NOFOLLOW | O_NONBLOCK
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(0x0020_0000); // FILE_FLAG_OPEN_REPARSE_POINT
    }
    let guard = match options.create_new(true).open(&path) {
        Ok(guard) => guard,
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            if !fs::symlink_metadata(&path)?.file_type().is_file() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Invalid preferences write guard",
                ));
            }
            options.create_new(false).open(&path)?
        }
        Err(error) => return Err(error),
    };
    if !guard.metadata()?.file_type().is_file()
        || !fs::symlink_metadata(&path)?.file_type().is_file()
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Invalid preferences write guard",
        ));
    }
    guard.try_lock()?;
    Ok(WriteGuard(guard))
}

fn default_file() -> Option<PathBuf> {
    let env = |name| {
        std::env::var_os(name)
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
    };
    let root = if cfg!(windows) {
        env("LOCALAPPDATA")
    } else if cfg!(target_os = "macos") {
        env("HOME").map(|p| p.join("Library/Application Support"))
    } else {
        env("XDG_CONFIG_HOME").or_else(|| env("HOME").map(|p| p.join(".config")))
    };
    root.map(|p| p.join("meow-paw").join("preferences.json"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(any(target_os = "linux", windows))]
    #[test]
    fn mixed_port_defaults_and_override_is_temporary() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("preferences.json");
        fs::write(&file, "{}").unwrap();
        assert_eq!(Preferences::open(file.clone()).unwrap().mixed_port(), 7890);

        fs::write(&file, r#"{"mixed_port":18790}"#).unwrap();
        let mut prefs = Preferences::open(file.clone()).unwrap();
        assert_eq!(prefs.mixed_port(), 18790);
        prefs.set_mixed_port_override(std::num::NonZeroU16::new(28790));
        assert_eq!(prefs.mixed_port(), 28790);
        prefs
            .save_mixed_port(std::num::NonZeroU16::new(38790).unwrap())
            .unwrap();
        assert_eq!(prefs.saved_mixed_port(), 38790);
        assert_eq!(prefs.mixed_port(), 28790);
        prefs.set_reduce_motion(true).unwrap();
        assert_eq!(Preferences::open(file.clone()).unwrap().mixed_port(), 38790);
        fs::write(&file, r#"{"mixed_port":0}"#).unwrap();
        assert!(Preferences::open(file).is_err());
    }

    #[test]
    fn reduced_motion_defaults_off_and_round_trips_without_changing_language() {
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join("preferences.json");
        let mut prefs = Preferences::open(file.clone()).unwrap();
        assert!(!prefs.reduce_motion());
        prefs.set_language(LanguagePreference::ZhCn).unwrap();
        prefs.set_reduce_motion(true).unwrap();
        assert!(prefs.reduce_motion());
        let mut restored = Preferences::open(file.clone()).unwrap();
        assert!(restored.reduce_motion());
        assert_eq!(restored.language(), LanguagePreference::ZhCn);
        restored.set_reduce_motion(false).unwrap();
        assert!(!Preferences::open(file.clone()).unwrap().reduce_motion());
        assert!(!fs::read_to_string(file).unwrap().contains("reduce_motion"));
    }

    #[test]
    fn restart_restores_both_paths_but_overrides_are_transient() {
        let root = tempfile::tempdir().unwrap();
        let choices = root.path().join("config/preferences.json");
        let engine = root.path().join("引擎 bin");
        let first = root.path().join("first.yaml");
        let second = root.path().join("改选.yaml");
        let cli = root.path().join("override.yaml");
        let mut prefs = Preferences::open(choices.clone()).unwrap();
        prefs.remember(true, &engine).unwrap();
        prefs.remember(false, &first).unwrap();
        prefs.remember(false, &second).unwrap();
        drop(prefs);
        let prefs = Preferences::open(choices.clone()).unwrap();
        assert_eq!(
            prefs.resolve(None, None),
            (Some(engine.clone()), Some(second.clone()))
        );
        assert_eq!(
            prefs.resolve(None, Some(cli.clone())),
            (Some(engine.clone()), Some(cli))
        );
        assert_eq!(
            Preferences::open(choices).unwrap().resolve(None, None),
            (Some(engine), Some(second))
        );
    }

    #[test]
    fn migrates_legacy_profile_and_forgets_only_the_reference() {
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join("preferences.json");
        let first = root.path().join("first.yaml");
        let second = root.path().join("second.yaml");
        fs::write(&first, "mode: rule").unwrap();
        fs::write(
            &file,
            serde_json::to_vec(&serde_json::json!({"profile": first})).unwrap(),
        )
        .unwrap();
        let mut prefs = Preferences::open(file.clone()).unwrap();
        assert_eq!(prefs.profiles(), std::slice::from_ref(&first));
        prefs.remember(false, &second).unwrap();
        prefs.remember(false, &first).unwrap();
        assert_eq!(prefs.profiles(), &[first.clone(), second.clone()]);
        prefs.forget_profile(&first).unwrap();
        assert_eq!(fs::read_to_string(first).unwrap(), "mode: rule");
        let reopened = Preferences::open(file).unwrap();
        assert_eq!(reopened.profiles(), &[second]);
        assert_eq!(reopened.resolve(None, None), (None, None));
    }

    #[test]
    fn malformed_relative_and_oversize_files_are_not_accepted() {
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join("preferences.json");
        for bytes in [
            b"garbage".to_vec(),
            br#"{"engine":"relative"}"#.to_vec(),
            vec![b' '; LIMIT as usize + 1],
        ] {
            fs::write(&file, &bytes).unwrap();
            assert!(Preferences::open(file.clone()).is_err());
            assert_eq!(fs::read(&file).unwrap(), bytes);
        }
    }

    #[test]
    fn saved_language_changes_next_launch_not_the_current_locale() {
        let dir = tempfile::tempdir().unwrap();
        let mut prefs = Preferences::open(dir.path().join("preferences.json")).unwrap();
        assert!(prefs.has_persistent_file());
        let launch_locale = crate::select_locale(&prefs, Some("en"), Some("zh-CN"));
        prefs.set_language(LanguagePreference::ZhCn).unwrap();
        assert_eq!(prefs.language(), LanguagePreference::ZhCn);
        assert_eq!(launch_locale, meow_paw::i18n::Locale::En);
        assert_eq!(
            crate::select_locale(&prefs, None, Some("en")),
            meow_paw::i18n::Locale::ZhCn
        );
        assert_eq!(
            crate::select_locale(&prefs, Some("en"), Some("zh-CN")),
            meow_paw::i18n::Locale::En
        );
        assert!(!Preferences::default().has_persistent_file());
    }

    #[test]
    fn language_defaults_and_round_trips_without_losing_other_choices() {
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join("preferences.json");
        let engine = root.path().join("engine");
        fs::write(
            &file,
            serde_json::to_vec(&serde_json::json!({"engine": engine})).unwrap(),
        )
        .unwrap();
        let mut prefs = Preferences::open(file.clone()).unwrap();
        assert_eq!(prefs.language(), LanguagePreference::System);
        for (language, stored) in [
            (LanguagePreference::En, Some("en")),
            (LanguagePreference::ZhCn, Some("zh-CN")),
            (LanguagePreference::System, None),
        ] {
            prefs.set_language(language).unwrap();
            assert_eq!(prefs.language(), language);
            let raw: serde_json::Value = serde_json::from_slice(&fs::read(&file).unwrap()).unwrap();
            assert_eq!(raw.get("language").and_then(|value| value.as_str()), stored);
            let reopened = Preferences::open(file.clone()).unwrap();
            assert_eq!(reopened.language(), language);
            assert_eq!(reopened.resolve(None, None).0, Some(engine.clone()));
        }
        fs::write(&file, br#"{"language":"system"}"#).unwrap();
        assert_eq!(
            Preferences::open(file).unwrap().language(),
            LanguagePreference::System
        );
    }

    #[test]
    fn invalid_language_and_unknown_fields_are_rejected_without_writing() {
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join("preferences.json");
        for raw in [
            br#"{"language":"fr"}"#.as_slice(),
            br#"{"language":null}"#,
            br#"{"language":42}"#,
            br#"{"language":"zh-cn"}"#,
            br#"{"language":"en","unknown":true}"#,
        ] {
            fs::write(&file, raw).unwrap();
            assert!(Preferences::open(file.clone()).is_err());
            assert_eq!(fs::read(&file).unwrap(), raw);
        }
    }

    #[test]
    fn disabled_preferences_do_not_read_or_write_and_changes_are_temporary() {
        let (mut prefs, warning) = Preferences::discover(false);
        assert!(warning.is_none());
        assert!(prefs.file.is_none());
        assert_eq!(prefs.language(), LanguagePreference::System);
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("profile.yaml");
        let engine = root.path().join("engine");
        prefs.set_language(LanguagePreference::ZhCn).unwrap();
        prefs.remember(true, &engine).unwrap();
        prefs.remember(false, &path).unwrap();
        assert_eq!(prefs.language(), LanguagePreference::ZhCn);
        assert_eq!(
            prefs.resolve(None, None),
            (Some(engine.clone()), Some(path.clone()))
        );
        prefs.forget_profile(&path).unwrap();
        assert_eq!(prefs.resolve(None, None), (Some(engine), None));
        assert!(prefs.profiles().is_empty());
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 0);
        assert_eq!(
            Preferences::discover(false).0.language(),
            LanguagePreference::System
        );
    }

    #[test]
    fn failed_write_keeps_the_previous_choice_and_file() {
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join("preferences.json");
        fs::write(&file, b"{}\n").unwrap();
        let mut prefs = Preferences::open(file.clone()).unwrap();
        // A directory cannot replace the existing preferences file.
        prefs.file = Some(root.path().to_path_buf());
        assert!(prefs.set_language(LanguagePreference::En).is_err());
        assert_eq!(prefs.language(), LanguagePreference::System);
        assert_eq!(fs::read(file).unwrap(), b"{}\n");
    }

    #[test]
    fn stale_handles_merge_fields_and_profile_list_mutations() {
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join("preferences.json");
        let engine = root.path().join("engine");
        let first = root.path().join("first.yaml");
        let second = root.path().join("second.yaml");
        let third = root.path().join("third.yaml");
        let mut a = Preferences::open(file.clone()).unwrap();
        let mut b = Preferences::open(file.clone()).unwrap();

        a.set_language(LanguagePreference::ZhCn).unwrap();
        b.remember(true, &engine).unwrap();
        a.remember(false, &first).unwrap();
        b.remember(false, &second).unwrap();
        a.remember(false, &third).unwrap();
        // b's last observed selection is second, but removing it must not
        // clear the newer third selection.
        b.forget_profile(&second).unwrap();
        assert_eq!(b.language(), LanguagePreference::ZhCn);
        assert_eq!(
            b.resolve(None, None),
            (Some(engine.clone()), Some(third.clone()))
        );
        assert_eq!(b.profiles(), &[first.clone(), third.clone()]);

        // Removing the actual last selection clears it, while keeping others.
        a.forget_profile(&third).unwrap();
        let saved = Preferences::open(file).unwrap();
        assert_eq!(saved.language(), LanguagePreference::ZhCn);
        assert_eq!(saved.resolve(None, None), (Some(engine), None));
        assert_eq!(saved.profiles(), &[first]);
        assert_eq!(a.profiles(), saved.profiles());
        assert_eq!(a.resolve(None, None), saved.resolve(None, None));
    }

    #[test]
    fn lock_contention_fails_without_changing_bytes_or_memory() {
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join("preferences.json");
        let mut prefs = Preferences::open(file.clone()).unwrap();
        prefs.set_language(LanguagePreference::En).unwrap();
        let before = fs::read(&file).unwrap();
        let guard = write_guard(&file).unwrap();
        assert_eq!(
            prefs
                .set_language(LanguagePreference::ZhCn)
                .unwrap_err()
                .kind(),
            io::ErrorKind::WouldBlock
        );
        assert_eq!(fs::read(&file).unwrap(), before);
        assert_eq!(prefs.language(), LanguagePreference::En);
        drop(guard);
        prefs.set_language(LanguagePreference::ZhCn).unwrap();
        assert_eq!(
            Preferences::open(file).unwrap().language(),
            LanguagePreference::ZhCn
        );
    }

    #[cfg(unix)]
    #[test]
    fn write_guard_releases_lock_even_with_an_inherited_descriptor() {
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join("preferences.json");
        let guard = write_guard(&file).unwrap();
        // An open-file-description duplicate models a fork before exec,
        // without starting a subprocess or depending on its scheduling.
        let inherited = guard.0.try_clone().unwrap();
        assert_eq!(
            write_guard(&file).err().unwrap().kind(),
            io::ErrorKind::WouldBlock
        );
        drop(guard);
        let next = write_guard(&file).unwrap();
        drop(inherited);
        drop(next);
    }

    #[test]
    fn invalid_latest_file_is_never_overwritten_or_loaded_into_memory() {
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join("preferences.json");
        let mut prefs = Preferences::open(file.clone()).unwrap();
        prefs.set_language(LanguagePreference::En).unwrap();
        for raw in [
            b"garbage".to_vec(),
            br#"{"engine":"relative"}"#.to_vec(),
            br#"{"language":"fr"}"#.to_vec(),
            br#"{"unknown":true}"#.to_vec(),
            vec![b' '; LIMIT as usize + 1],
        ] {
            fs::write(&file, &raw).unwrap();
            assert!(prefs.set_language(LanguagePreference::ZhCn).is_err());
            assert_eq!(fs::read(&file).unwrap(), raw);
            assert_eq!(prefs.language(), LanguagePreference::En);
        }
    }

    #[test]
    fn profile_limit_is_checked_against_latest_list() {
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join("preferences.json");
        let mut stale = Preferences::open(file.clone()).unwrap();
        let mut writer = Preferences::open(file.clone()).unwrap();
        for index in 0..PROFILE_LIMIT {
            writer
                .remember(false, &root.path().join(format!("{index}.yaml")))
                .unwrap();
        }
        let before = fs::read(&file).unwrap();
        assert!(
            stale
                .remember(false, &root.path().join("extra.yaml"))
                .is_err()
        );
        assert_eq!(fs::read(&file).unwrap(), before);
        assert!(stale.profiles().is_empty());
        assert_eq!(stale.resolve(None, None), (None, None));
    }

    #[cfg(unix)]
    #[test]
    fn guard_symlink_is_rejected_without_touching_target() {
        use std::os::unix::fs::symlink;

        let root = tempfile::tempdir().unwrap();
        let file = root.path().join("preferences.json");
        let target = root.path().join("target");
        fs::write(&file, b"{}\n").unwrap();
        fs::write(&target, b"untouched").unwrap();
        symlink(&target, root.path().join("preferences.json.lock")).unwrap();
        let mut prefs = Preferences::open(file.clone()).unwrap();
        assert!(prefs.set_language(LanguagePreference::En).is_err());
        assert_eq!(prefs.language(), LanguagePreference::System);
        assert_eq!(fs::read(file).unwrap(), b"{}\n");
        assert_eq!(fs::read(target).unwrap(), b"untouched");
    }

    #[test]
    fn removal_rechecks_saved_references_and_holds_preference_guard() {
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join("preferences.json");
        let profile = root.path().join("subscriptions/record.json");
        let stale = Preferences::open(file.clone()).unwrap();
        let mut other = Preferences::open(file.clone()).unwrap();
        let mut invoked = false;
        stale
            .with_unreferenced_profile(&profile, || {
                invoked = true;
                assert_eq!(
                    other.remember(false, &profile).unwrap_err().kind(),
                    io::ErrorKind::WouldBlock
                );
                Ok(())
            })
            .unwrap();
        assert!(invoked);
        other.remember(false, &profile).unwrap();
        assert_eq!(
            stale
                .with_unreferenced_profile(&profile, || -> io::Result<()> {
                    panic!("referenced profile must be retained")
                })
                .unwrap_err()
                .kind(),
            io::ErrorKind::AlreadyExists
        );
    }

    #[test]
    fn managed_detach_after_switch_and_selection_removal_are_serialized() {
        use meow_paw::{subscription::Download, subscription_store::SubscriptionStore};
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join("preferences.json");
        let mut prefs = Preferences::open(file.clone()).unwrap();
        let mut other = Preferences::open(file.clone()).unwrap();
        let mut store = SubscriptionStore::new(root.path().join("subscriptions"));
        let id = store
            .import(
                "https://example.test/sub",
                Download {
                    yaml: "proxies: [{name: old, type: direct}]\n".into(),
                    suggested_interval_minutes: None,
                },
                Default::default(),
            )
            .unwrap();
        let path = store.record_path(id);
        let snapshot = store.prepare_removal(id).unwrap();
        prefs
            .remember_checked(false, &path, || store.check_unchanged(id, &snapshot))
            .unwrap();
        assert_eq!(
            other.detach_inactive_profile(&path).unwrap_err().kind(),
            io::ErrorKind::AlreadyExists
        );
        let next = root.path().join("next.yaml");
        prefs.remember(false, &next).unwrap();
        other.detach_inactive_profile(&path).unwrap();
        let current = Preferences::open(file.clone()).unwrap();
        assert_eq!(
            current.resolve(None, None).1.as_deref(),
            Some(next.as_path())
        );
        assert!(!current.profiles().contains(&path));
        let snapshot = store.prepare_removal(id).unwrap();
        current
            .with_unreferenced_profile(&path, || {
                assert_eq!(
                    prefs
                        .remember_checked(false, &path, || Ok(()))
                        .unwrap_err()
                        .kind(),
                    io::ErrorKind::WouldBlock
                );
                store.remove_if_unchanged(id, &snapshot)
            })
            .unwrap();
        assert_eq!(
            prefs
                .remember_checked(false, &path, || store.check_unchanged(id, &snapshot))
                .unwrap_err()
                .kind(),
            io::ErrorKind::NotFound
        );
        assert_eq!(
            Preferences::open(file).unwrap().resolve(None, None).1,
            Some(next)
        );
    }

    #[test]
    fn managed_selection_rechecks_identity_under_preferences_guard() {
        use meow_paw::{subscription::Download, subscription_store::SubscriptionStore};
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join("preferences.json");
        let mut prefs = Preferences::open(file.clone()).unwrap();
        let mut store = SubscriptionStore::new(root.path().join("subscriptions"));
        let id = store
            .import(
                "https://example.test/sub",
                Download {
                    yaml: "proxies: [{name: old, type: direct}]\n".into(),
                    suggested_interval_minutes: None,
                },
                Default::default(),
            )
            .unwrap();
        let path = store.record_path(id);
        let snapshot = store.prepare_removal(id).unwrap();
        store.rename(id, "changed").unwrap();
        assert_eq!(
            prefs
                .remember_checked(false, &path, || store.check_unchanged(id, &snapshot))
                .unwrap_err()
                .kind(),
            io::ErrorKind::WouldBlock
        );
        let current = Preferences::open(file.clone()).unwrap();
        assert!(current.profiles().is_empty());
        let fresh = store.prepare_removal(id).unwrap();
        prefs
            .remember_checked(false, &path, || {
                assert_eq!(
                    current
                        .with_unreferenced_profile(&path, || Ok(()))
                        .unwrap_err()
                        .kind(),
                    io::ErrorKind::WouldBlock
                );
                store.check_unchanged(id, &fresh)
            })
            .unwrap();
        assert_eq!(
            Preferences::open(file).unwrap().resolve(None, None).1,
            Some(path)
        );
    }
}
