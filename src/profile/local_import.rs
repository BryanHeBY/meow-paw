//! Import independently owned YAML copies without changing proxy state.

use super::{MAX_PROFILE_BYTES, Profile};
use std::{
    fs::{self, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

/// An unregistered copy. Dropping it rolls back the complete staged file.
pub struct StagedYaml {
    directory: tempfile::TempDir,
    path: PathBuf,
}

impl StagedYaml {
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Retain the copy once its path has been registered successfully.
    pub fn keep(self) {
        let _ = self.directory.keep();
    }
}

/// Validate a user-selected local YAML file and stage a private copy.
pub fn stage_yaml(path: &Path, directory: &Path) -> io::Result<StagedYaml> {
    let invalid = || io::Error::new(io::ErrorKind::InvalidInput, "Select a regular YAML file");
    let name = path.file_name().ok_or_else(invalid)?;
    if !path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            extension.eq_ignore_ascii_case("yaml") || extension.eq_ignore_ascii_case("yml")
        })
        || !fs::symlink_metadata(path)?.is_file()
    {
        return Err(invalid());
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT;
        options.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    }
    let file = options.open(path)?;
    let metadata = file.metadata()?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(invalid());
        }
    }
    if !metadata.is_file() {
        return Err(invalid());
    }
    let mut source = String::new();
    file.take(MAX_PROFILE_BYTES as u64 + 1)
        .read_to_string(&mut source)?;
    Profile::parse(&source).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;

    fs::create_dir_all(directory)?;
    let destination = tempfile::Builder::new()
        .prefix("profile-")
        .tempdir_in(directory)?;
    let mut staged = tempfile::NamedTempFile::new_in(destination.path())?;
    staged.write_all(source.as_bytes())?;
    staged.as_file().sync_all()?;
    let path = destination.path().join(name);
    staged
        .persist_noclobber(&path)
        .map_err(|error| error.error)?;
    Ok(StagedYaml {
        directory: destination,
        path,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn copy_yaml(path: &Path, destination: &Path) -> io::Result<PathBuf> {
        let staged = stage_yaml(path, destination)?;
        let path = staged.path().to_path_buf();
        staged.keep();
        Ok(path)
    }

    #[test]
    fn dropping_unregistered_copy_rolls_back_only_its_destination() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("original.yaml");
        fs::write(&source, "mode: rule").unwrap();
        let stage = stage_yaml(&source, &root.path().join("imports")).unwrap();
        let path = stage.path().to_path_buf();
        assert!(path.exists());
        drop(stage);
        assert!(!path.parent().unwrap().exists());
        assert_eq!(fs::read_to_string(source).unwrap(), "mode: rule");
    }

    #[test]
    fn same_name_imports_are_independent_and_preserve_the_original() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("Demo.YAML");
        let destination = root.path().join("profiles");
        let original = "# preserve comment\nmode: rule\n";
        fs::write(&source, original).unwrap();
        let first = copy_yaml(&source, &destination).unwrap();
        assert_eq!(fs::read_to_string(&source).unwrap(), original);
        fs::write(&source, "mode: direct\n").unwrap();
        let second = copy_yaml(&source, &destination).unwrap();
        assert_ne!(first, second);
        assert_eq!(first.file_name(), source.file_name());
        assert_eq!(fs::read_to_string(first).unwrap(), original);
        assert_eq!(fs::read_to_string(second).unwrap(), "mode: direct\n");
    }

    #[test]
    fn invalid_inputs_do_not_create_a_staging_directory() {
        let root = tempfile::tempdir().unwrap();
        let destination = root.path().join("profiles");
        for (name, bytes) in [
            ("wrong.txt", &b"mode: rule"[..]),
            ("bad.yaml", &b"["[..]),
            ("list.yml", &b"[1, 2]"[..]),
            ("encoding.yaml", &b"\xff"[..]),
        ] {
            let path = root.path().join(name);
            fs::write(&path, bytes).unwrap();
            assert!(copy_yaml(&path, &destination).is_err());
        }
        let oversized = root.path().join("large.yaml");
        fs::File::create(&oversized)
            .unwrap()
            .set_len(MAX_PROFILE_BYTES as u64 + 1)
            .unwrap();
        assert!(copy_yaml(&oversized, &destination).is_err());
        let folder = root.path().join("directory.yaml");
        fs::create_dir(&folder).unwrap();
        assert!(copy_yaml(&folder, &destination).is_err());
        assert!(!destination.exists());
    }

    #[cfg(unix)]
    #[test]
    fn symbolic_link_is_not_imported() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("original.yaml");
        let source = root.path().join("link.yaml");
        let destination = root.path().join("profiles");
        fs::write(&target, "mode: rule\n").unwrap();
        std::os::unix::fs::symlink(&target, &source).unwrap();
        assert!(copy_yaml(&source, &destination).is_err());
        assert!(!destination.exists());
    }
}
