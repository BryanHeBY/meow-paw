use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

const LIMIT: u64 = 64 * 1024;

#[derive(Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Selections {
    engine: Option<PathBuf>,
    profile: Option<PathBuf>,
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
                let value: Selections = serde_json::from_slice(&raw)?;
                if value
                    .engine
                    .iter()
                    .chain(&value.profile)
                    .any(|p| !p.is_absolute())
                {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "Saved paths must be absolute",
                    ));
                }
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

    pub fn remember(&mut self, engine: bool, path: &Path) -> io::Result<()> {
        let Some(file) = self.file.as_ref() else {
            return Ok(());
        };
        let path = std::path::absolute(path)?;
        let next = if engine {
            Selections {
                engine: Some(path),
                profile: self.selected.profile.clone(),
            }
        } else {
            Selections {
                engine: self.selected.engine.clone(),
                profile: Some(path),
            }
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
}
