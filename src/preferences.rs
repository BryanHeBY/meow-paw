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
}

#[derive(Default)]
pub struct Preferences {
    file: Option<PathBuf>,
    selected: Selections,
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

    pub fn subscription_dir(&self) -> Option<PathBuf> {
        Some(self.file.as_ref()?.parent()?.join("subscriptions"))
    }

    pub fn profiles(&self) -> &[PathBuf] {
        &self.selected.profiles
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
        let mut next = self.selected.clone();
        next.language = language;
        self.persist(next)
    }

    pub fn remember(&mut self, engine: bool, path: &Path) -> io::Result<()> {
        let path = std::path::absolute(path)?;
        let mut next = self.selected.clone();
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
        self.persist(next)
    }

    pub fn forget_profile(&mut self, path: &Path) -> io::Result<()> {
        let mut next = self.selected.clone();
        next.profiles.retain(|candidate| candidate != path);
        if next.profile.as_deref() == Some(path) {
            next.profile = None;
        }
        self.persist(next)
    }

    fn persist(&mut self, next: Selections) -> io::Result<()> {
        let Some(file) = self.file.as_ref() else {
            self.selected = next;
            return Ok(());
        };
        let raw = serde_json::to_vec_pretty(&next)?;
        if raw.len() as u64 > LIMIT {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Preferences exceed size limit",
            ));
        }
        let dir = file
            .parent()
            .ok_or_else(|| io::Error::other("Preferences parent is missing"))?;
        fs::create_dir_all(dir)?;
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
        prefs.set_language(LanguagePreference::ZhCn).unwrap();
        prefs.remember(false, &path).unwrap();
        assert_eq!(prefs.language(), LanguagePreference::ZhCn);
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
}
