//! One atomic record per subscription. Saving never changes the live session.

use crate::{
    profile::{MAX_PROFILE_BYTES, Profile},
    subscription::{Download, parse_url},
};
use serde::{Deserialize, Serialize};
use std::{
    fmt, fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
};
use tempfile::NamedTempFile;

// JSON escaping can expand one byte to six. Leave room for URL and metadata.
const MAX_RECORD_BYTES: u64 = (6 * MAX_PROFILE_BYTES + 16 * 1024) as u64;
const MAX_URL_BYTES: usize = 8192;
const MAX_NAME_CHARS: usize = 80;
const GUARD_FILE: &str = ".write-lock";

#[derive(Clone, Copy, Eq, PartialEq)]
pub struct SubscriptionId([u8; 16]);

impl fmt::Display for SubscriptionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

impl std::str::FromStr for SubscriptionId {
    type Err = io::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.len() != 32 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(invalid("Malformed subscription ID"));
        }
        let mut id = [0; 16];
        for (index, byte) in id.iter_mut().enumerate() {
            let offset = 2 * index;
            *byte = u8::from_str_radix(&value[offset..offset + 2], 16)
                .map_err(|_| invalid("Malformed subscription ID"))?;
        }
        Ok(Self(id))
    }
}

#[derive(Clone, Copy, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct RefreshPolicy {
    pub automatic: bool,
    pub interval_override_minutes: Option<u32>,
}

/// Non-sensitive metadata for display. The interval is informational only.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DisplayInterval {
    Local(u32),
    Provider(u32),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SubscriptionMetadata {
    pub name: Option<String>,
    pub interval: Option<DisplayInterval>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SavedSubscription {
    // No Debug implementation: the URL and YAML can both contain credentials.
    url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    yaml: String,
    policy: RefreshPolicy,
    provider_interval_minutes: Option<u32>,
}

impl SavedSubscription {
    pub fn yaml(&self) -> &str {
        &self.yaml
    }

    pub fn policy(&self) -> RefreshPolicy {
        self.policy
    }

    pub fn interval_minutes(&self) -> Option<u32> {
        self.policy
            .interval_override_minutes
            .or(self.provider_interval_minutes)
    }

    pub fn metadata(&self) -> SubscriptionMetadata {
        SubscriptionMetadata {
            name: self.name.as_deref().and_then(sanitize_name),
            interval: self
                .policy
                .interval_override_minutes
                .map(DisplayInterval::Local)
                .or_else(|| {
                    self.provider_interval_minutes
                        .map(DisplayInterval::Provider)
                }),
        }
    }

    pub fn fetch_latest(&self) -> Result<Download, crate::subscription::DownloadError> {
        crate::subscription::fetch(&self.url)
    }
}

pub struct SubscriptionStore {
    dir: PathBuf,
}

impl SubscriptionStore {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    pub fn import(
        &mut self,
        url: &str,
        download: Download,
        policy: RefreshPolicy,
    ) -> io::Result<SubscriptionId> {
        let url = parse_url(url).map_err(|_| invalid("Invalid subscription URL"))?;
        if url.as_str().len() > MAX_URL_BYTES || policy.interval_override_minutes == Some(0) {
            return Err(invalid("Invalid subscription settings"));
        }
        validate(&download)?;
        let mut bytes = [0; 16];
        getrandom::fill(&mut bytes).map_err(|_| io::Error::other("Cannot generate an ID"))?;
        let id = SubscriptionId(bytes);
        let record = SavedSubscription {
            url: url.into(),
            name: None,
            yaml: download.yaml,
            policy,
            provider_interval_minutes: download.suggested_interval_minutes,
        };
        let _guard = self.write_guard()?;
        self.write(id, &record, |staged, path| {
            staged
                .persist_noclobber(path)
                .map(|_| ())
                .map_err(|e| e.error)
        })?;
        Ok(id)
    }

    pub fn read(&self, id: SubscriptionId) -> io::Result<SavedSubscription> {
        let mut bytes = Vec::new();
        fs::File::open(self.record_path(id))?
            .take(MAX_RECORD_BYTES + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 > MAX_RECORD_BYTES {
            return Err(invalid("Subscription record exceeds the size limit"));
        }
        let mut record: SavedSubscription =
            serde_json::from_slice(&bytes).map_err(|_| invalid("Invalid subscription record"))?;
        if record.url.len() > MAX_URL_BYTES
            || record.yaml.len() > MAX_PROFILE_BYTES
            || parse_url(&record.url).is_err()
            || record.policy.interval_override_minutes == Some(0)
            || record.provider_interval_minutes == Some(0)
        {
            return Err(invalid("Invalid subscription record"));
        }
        record.name = record.name.as_deref().and_then(sanitize_name);
        Ok(record)
    }

    /// Read a bounded record and return only metadata safe for display.
    pub fn metadata(&self, id: SubscriptionId) -> io::Result<SubscriptionMetadata> {
        self.read(id).map(|record| record.metadata())
    }

    /// A manual refresh retains the local policy and never applies a profile.
    /// Returns `WouldBlock` on contention. This legacy method does not check
    /// whether the record changed while the caller fetched the download; use
    /// `refresh_if_unchanged` when the fetch is based on a saved snapshot.
    pub fn refresh(&mut self, id: SubscriptionId, download: Download) -> io::Result<()> {
        self.refresh_with(id, download, |staged, path| {
            staged.persist(path).map(|_| ()).map_err(|e| e.error)
        })
    }

    /// Publish a download only if its source, remote content and refresh policy
    /// still match the record used to fetch it. Fetch before calling this method;
    /// the guard is never held during network work. A local rename is preserved.
    /// Contention or a stale snapshot returns `WouldBlock` without changing a record.
    pub fn refresh_if_unchanged(
        &mut self,
        id: SubscriptionId,
        expected: &SavedSubscription,
        download: Download,
    ) -> io::Result<()> {
        self.refresh_checked_with(id, Some(expected), download, |staged, path| {
            staged.persist(path).map(|_| ()).map_err(|e| e.error)
        })
    }

    fn refresh_with(
        &mut self,
        id: SubscriptionId,
        download: Download,
        commit: impl FnOnce(NamedTempFile, &Path) -> io::Result<()>,
    ) -> io::Result<()> {
        self.refresh_checked_with(id, None, download, commit)
    }

    fn refresh_checked_with(
        &mut self,
        id: SubscriptionId,
        expected: Option<&SavedSubscription>,
        download: Download,
        commit: impl FnOnce(NamedTempFile, &Path) -> io::Result<()>,
    ) -> io::Result<()> {
        validate(&download)?;
        let _guard = self.write_guard()?;
        let mut record = self.read(id)?;
        if let Some(expected) = expected
            && (record.url != expected.url
                || record.yaml != expected.yaml
                || record.policy != expected.policy
                || record.provider_interval_minutes != expected.provider_interval_minutes)
        {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "Subscription changed during refresh",
            ));
        }
        record.yaml = download.yaml;
        record.provider_interval_minutes = download.suggested_interval_minutes;
        self.write(id, &record, commit)
    }

    /// Update only the local display name, retaining the rest of the record.
    pub fn rename(&mut self, id: SubscriptionId, name: &str) -> io::Result<SubscriptionMetadata> {
        self.rename_with(id, name, |staged, path| {
            staged.persist(path).map(|_| ()).map_err(|e| e.error)
        })
    }

    fn rename_with(
        &mut self,
        id: SubscriptionId,
        name: &str,
        commit: impl FnOnce(NamedTempFile, &Path) -> io::Result<()>,
    ) -> io::Result<SubscriptionMetadata> {
        let _guard = self.write_guard()?;
        let mut record = self.read(id)?;
        record.name = sanitize_name(name);
        self.write(id, &record, commit)?;
        Ok(record.metadata())
    }

    pub fn ids(&self) -> io::Result<Vec<SubscriptionId>> {
        let entries = match fs::read_dir(&self.dir) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error),
        };
        let mut ids = Vec::new();
        for entry in entries {
            let entry = entry?;
            if entry.file_type()?.is_file()
                && let Some(id) = self.id_for_record(&entry.path())
            {
                ids.push(id);
            }
        }
        ids.sort_unstable_by_key(|id| id.0);
        Ok(ids)
    }

    pub fn id_for_record(&self, path: &Path) -> Option<SubscriptionId> {
        if path.parent() != Some(self.dir.as_path()) || path.extension()? != "json" {
            return None;
        }
        let id = path.file_stem()?.to_str()?.parse().ok()?;
        (self.record_path(id) == path).then_some(id)
    }

    pub fn record_path(&self, id: SubscriptionId) -> PathBuf {
        self.dir.join(format!("{id}.json"))
    }

    // This stable sidecar is never replaced or removed by the store. All writers
    // must cooperate; older versions and external editors do not honor the lock.
    // The directory must be private and on a local filesystem with working file
    // locks and atomic same-directory replacement, and must not be moved or
    // modified by uncooperative actors while a write is in progress.
    fn write_guard(&self) -> io::Result<fs::File> {
        let mut dirs = fs::DirBuilder::new();
        dirs.recursive(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            dirs.mode(0o700);
        }
        dirs.create(&self.dir)?;
        let path = self.dir.join(GUARD_FILE);
        let mut options = fs::OpenOptions::new();
        options.read(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        // Do not follow a lock-file symlink; nonblocking open also avoids hanging
        // on a FIFO if an unexpected entry appears between the type check and open.
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
        let file = match options.create_new(true).open(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                if !fs::symlink_metadata(&path)?.file_type().is_file() {
                    return Err(invalid("Invalid subscription write guard"));
                }
                options.create_new(false).open(&path)?
            }
            Err(error) => return Err(error),
        };
        if !file.metadata()?.file_type().is_file()
            || !fs::symlink_metadata(&path)?.file_type().is_file()
        {
            return Err(invalid("Invalid subscription write guard"));
        }
        file.try_lock()?;
        Ok(file)
    }

    fn write(
        &self,
        id: SubscriptionId,
        record: &SavedSubscription,
        commit: impl FnOnce(NamedTempFile, &Path) -> io::Result<()>,
    ) -> io::Result<()> {
        fs::create_dir_all(&self.dir)?;
        let mut staged = NamedTempFile::new_in(&self.dir)?;
        serde_json::to_writer(&mut staged, record)
            .map_err(|_| io::Error::other("Cannot write the subscription record"))?;
        staged.flush()?;
        if staged.as_file().metadata()?.len() > MAX_RECORD_BYTES {
            return Err(invalid("Subscription record exceeds the size limit"));
        }
        staged.as_file().sync_all()?;
        // The YAML and metadata become visible together. Avoid a second fallible
        // write after replacing the last-good record.
        commit(staged, &self.record_path(id))
    }
}

// Keep display text single-line and bounded, including when reading edited records.
fn sanitize_name(input: &str) -> Option<String> {
    let clean: String = input
        .chars()
        .filter(|ch| !ch.is_control() && !matches!(*ch, '\u{061c}' | '\u{200e}'..='\u{200f}' | '\u{2028}'..='\u{202e}' | '\u{2066}'..='\u{2069}'))
        .take(MAX_NAME_CHARS)
        .collect();
    let clean = clean.trim();
    (!clean.is_empty()).then(|| clean.to_owned())
}

fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

fn validate(download: &Download) -> io::Result<()> {
    if download.suggested_interval_minutes == Some(0) {
        return Err(invalid("Invalid provider interval"));
    }
    let profile =
        Profile::parse(&download.yaml).map_err(|_| invalid("Invalid subscription YAML"))?;
    if !profile.has_subscription_content() {
        return Err(invalid("Subscription has no proxies or proxy providers"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    const OLD: &str = "proxies: [{name: old, type: direct}]\n";
    const NEW: &str = "proxies: [{name: new, type: direct}]\n";

    fn download(yaml: &str, minutes: u32) -> Download {
        Download {
            yaml: yaml.to_owned(),
            suggested_interval_minutes: Some(minutes),
        }
    }

    #[test]
    fn checked_refresh_preserves_concurrent_rename() {
        let dir = tempfile::tempdir().unwrap();
        let mut worker = SubscriptionStore::new(dir.path().into());
        let mut ui = SubscriptionStore::new(dir.path().into());
        let id = worker
            .import(
                "https://example.test/sub",
                download(OLD, 60),
                Default::default(),
            )
            .unwrap();
        let snapshot = worker.read(id).unwrap();
        ui.rename(id, "Local name").unwrap();
        worker
            .refresh_if_unchanged(id, &snapshot, download(NEW, 120))
            .unwrap();
        let record = ui.read(id).unwrap();
        assert_eq!(record.yaml(), NEW);
        assert_eq!(record.provider_interval_minutes, Some(120));
        assert_eq!(record.metadata().name.as_deref(), Some("Local name"));
    }

    #[test]
    fn guard_contention_leaves_records_unchanged_for_all_writers() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = SubscriptionStore::new(dir.path().into());
        let id = store
            .import(
                "https://example.test/sub",
                download(OLD, 60),
                Default::default(),
            )
            .unwrap();
        let snapshot = store.read(id).unwrap();
        let before = fs::read(store.record_path(id)).unwrap();
        let guard = store.write_guard().unwrap();
        let mut other = SubscriptionStore::new(dir.path().into());
        assert_eq!(
            other
                .import(
                    "https://example.test/other",
                    download(NEW, 60),
                    Default::default()
                )
                .err()
                .unwrap()
                .kind(),
            io::ErrorKind::WouldBlock
        );
        assert_eq!(
            other.rename(id, "Other").unwrap_err().kind(),
            io::ErrorKind::WouldBlock
        );
        assert_eq!(
            other.refresh(id, download(NEW, 120)).unwrap_err().kind(),
            io::ErrorKind::WouldBlock
        );
        assert_eq!(
            other
                .refresh_if_unchanged(id, &snapshot, download(NEW, 120))
                .unwrap_err()
                .kind(),
            io::ErrorKind::WouldBlock
        );
        assert_eq!(fs::read(store.record_path(id)).unwrap(), before);
        assert_eq!(store.ids().unwrap().len(), 1);
        drop(guard);
        other.rename(id, "After").unwrap();
        assert_eq!(other.metadata(id).unwrap().name.as_deref(), Some("After"));
    }

    #[test]
    fn checked_refresh_rejects_stale_source_content_and_policy() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = SubscriptionStore::new(dir.path().into());
        let id = store
            .import(
                "https://example.test/sub",
                download(OLD, 60),
                Default::default(),
            )
            .unwrap();
        for change in 0..4 {
            let snapshot = store.read(id).unwrap();
            let mut latest = store.read(id).unwrap();
            match change {
                0 => latest.url = "https://example.test/changed".into(),
                1 => latest.yaml = NEW.into(),
                2 => latest.provider_interval_minutes = Some(120),
                _ => latest.policy.automatic = true,
            }
            store
                .write(id, &latest, |staged, path| {
                    staged.persist(path).map(|_| ()).map_err(|e| e.error)
                })
                .unwrap();
            let before = fs::read(store.record_path(id)).unwrap();
            assert_eq!(
                store
                    .refresh_if_unchanged(id, &snapshot, download(OLD, 180))
                    .unwrap_err()
                    .kind(),
                io::ErrorKind::WouldBlock
            );
            assert_eq!(fs::read(store.record_path(id)).unwrap(), before);
        }
    }

    #[test]
    fn guard_rejects_wrong_type_without_changing_it() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(GUARD_FILE);
        fs::create_dir(&path).unwrap();
        let mut store = SubscriptionStore::new(dir.path().into());
        assert!(
            store
                .import(
                    "https://example.test/sub",
                    download(OLD, 60),
                    Default::default()
                )
                .is_err()
        );
        assert!(path.is_dir());
        assert!(store.ids().unwrap().is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn guard_rejects_symlinks_and_creates_private_files() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("subscriptions");
        let mut store = SubscriptionStore::new(dir.clone());
        store
            .import(
                "https://example.test/sub",
                download(OLD, 60),
                Default::default(),
            )
            .unwrap();
        assert_eq!(
            fs::metadata(&dir).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(dir.join(GUARD_FILE))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );

        let other_dir = root.path().join("other");
        fs::create_dir(&other_dir).unwrap();
        let target = root.path().join("target");
        fs::write(&target, b"untouched").unwrap();
        symlink(&target, other_dir.join(GUARD_FILE)).unwrap();
        let mut other = SubscriptionStore::new(other_dir);
        assert!(
            other
                .import(
                    "https://example.test/sub",
                    download(OLD, 60),
                    Default::default()
                )
                .is_err()
        );
        assert_eq!(fs::read(target).unwrap(), b"untouched");
        assert!(other.ids().unwrap().is_empty());
    }

    #[test]
    fn old_records_without_names_load_and_names_are_bounded_and_sanitized() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = SubscriptionStore::new(dir.path().into());
        let id = store
            .import(
                "https://example.test/sub",
                download(OLD, 60),
                Default::default(),
            )
            .unwrap();
        // Imports omit absent names, matching records written before this field existed.
        assert!(
            !fs::read_to_string(store.record_path(id))
                .unwrap()
                .contains("\"name\":")
        );
        assert_eq!(store.metadata(id).unwrap().name, None);
        store.rename(id, "  家\n庭\u{2028}\u{202e}  ").unwrap();
        assert_eq!(store.metadata(id).unwrap().name.as_deref(), Some("家庭"));
        store.rename(id, &"猫".repeat(100)).unwrap();
        assert_eq!(
            store.metadata(id).unwrap().name.unwrap(),
            "猫".repeat(MAX_NAME_CHARS)
        );
        // Display also sanitizes a syntactically valid record edited outside Paw.
        let mut record = store.read(id).unwrap();
        record.name = Some(format!("\u{202e}{}", "a".repeat(100)));
        store
            .write(id, &record, |staged, path| {
                staged.persist(path).map(|_| ()).map_err(|e| e.error)
            })
            .unwrap();
        assert_eq!(
            store.metadata(id).unwrap().name.unwrap(),
            "a".repeat(MAX_NAME_CHARS)
        );
    }

    #[test]
    fn rename_preserves_record_and_survives_refresh_then_clear_restores_fallback() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = SubscriptionStore::new(dir.path().into());
        let policy = RefreshPolicy {
            automatic: true,
            interval_override_minutes: Some(45),
        };
        let id = store
            .import("https://example.test/sub", download(OLD, 60), policy)
            .unwrap();
        store.rename(id, "Personal").unwrap();
        let renamed = store.read(id).unwrap();
        assert_eq!(renamed.url, "https://example.test/sub");
        assert_eq!(renamed.yaml(), OLD);
        assert!(renamed.policy() == policy);
        assert_eq!(renamed.provider_interval_minutes, Some(60));
        store.refresh(id, download(NEW, 120)).unwrap();
        let refreshed = SubscriptionStore::new(dir.path().into()).read(id).unwrap();
        assert_eq!(refreshed.metadata().name.as_deref(), Some("Personal"));
        assert_eq!(refreshed.yaml(), NEW);
        assert_eq!(refreshed.url, "https://example.test/sub");
        assert!(refreshed.policy() == policy);
        assert_eq!(refreshed.provider_interval_minutes, Some(120));
        store.rename(id, " \n ").unwrap();
        assert_eq!(store.metadata(id).unwrap().name, None);
        assert_eq!(store.read(id).unwrap().yaml(), NEW);
    }

    #[test]
    fn failed_rename_commit_keeps_last_good_record() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = SubscriptionStore::new(dir.path().into());
        let id = store
            .import(
                "https://example.test/sub",
                download(OLD, 60),
                Default::default(),
            )
            .unwrap();
        store.rename(id, "Before").unwrap();
        let failed = store.rename_with(id, "After", |temp, path| {
            assert!(temp.as_file().metadata().unwrap().len() > 0);
            assert!(path.exists());
            Err(io::Error::from(io::ErrorKind::PermissionDenied))
        });
        assert_eq!(failed.unwrap_err().kind(), io::ErrorKind::PermissionDenied);
        let record = store.read(id).unwrap();
        assert_eq!(record.metadata().name.as_deref(), Some("Before"));
        assert_eq!(record.yaml(), OLD);
        assert_eq!(record.interval_minutes(), Some(60));
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
    }

    #[test]
    fn manual_refresh_preserves_the_local_policy_and_id_on_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = SubscriptionStore::new(dir.path().into());
        let policy = RefreshPolicy {
            automatic: false,
            interval_override_minutes: Some(45),
        };
        let id = store
            .import(
                "https://example.test/s?token=secret",
                download(OLD, 60),
                policy,
            )
            .unwrap();
        assert!(id.to_string().parse::<SubscriptionId>().unwrap() == id);
        assert!("../token=secret".parse::<SubscriptionId>().is_err());
        store.refresh(id, download(NEW, 120)).unwrap();
        let reopened = SubscriptionStore::new(dir.path().into()).read(id).unwrap();
        assert_eq!(reopened.yaml(), NEW);
        assert!(!reopened.policy().automatic);
        assert_eq!(reopened.interval_minutes(), Some(45));
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
    }

    #[test]
    fn metadata_tracks_missing_and_replaced_hints_and_local_precedence() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = SubscriptionStore::new(dir.path().into());
        let missing = Download {
            yaml: OLD.into(),
            suggested_interval_minutes: None,
        };
        let id = store
            .import(
                "https://example.test/sub",
                missing,
                RefreshPolicy {
                    automatic: true,
                    interval_override_minutes: None,
                },
            )
            .unwrap();
        assert_eq!(store.metadata(id).unwrap().interval, None);
        store.refresh(id, download(NEW, 120)).unwrap();
        assert_eq!(
            store.metadata(id).unwrap().interval,
            Some(DisplayInterval::Provider(120))
        );
        store.refresh(id, download(NEW, 180)).unwrap();
        assert_eq!(
            store.metadata(id).unwrap().interval,
            Some(DisplayInterval::Provider(180))
        );
        store
            .refresh(
                id,
                Download {
                    yaml: NEW.into(),
                    suggested_interval_minutes: None,
                },
            )
            .unwrap();
        assert_eq!(store.metadata(id).unwrap().interval, None);

        let overridden = store
            .import(
                "https://example.test/sub",
                download(OLD, 60),
                RefreshPolicy {
                    automatic: true,
                    interval_override_minutes: Some(45),
                },
            )
            .unwrap();
        store.refresh(overridden, download(NEW, 120)).unwrap();
        assert_eq!(
            store.metadata(overridden).unwrap().interval,
            Some(DisplayInterval::Local(45))
        );
    }

    #[test]
    fn invalid_response_or_failed_final_commit_leaves_last_good_record() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = SubscriptionStore::new(dir.path().into());
        let id = store
            .import(
                "https://example.test/s?token=secret",
                download(OLD, 60),
                Default::default(),
            )
            .unwrap();
        assert!(
            store
                .refresh(id, download("error: unavailable", 120))
                .is_err()
        );
        let failed = store.refresh_with(id, download(NEW, 120), |temp, path| {
            assert!(temp.as_file().metadata().unwrap().len() > 0);
            assert!(path.exists());
            Err(io::Error::from(io::ErrorKind::PermissionDenied))
        });
        assert_eq!(failed.unwrap_err().kind(), io::ErrorKind::PermissionDenied);
        let reopened = SubscriptionStore::new(dir.path().into()).read(id).unwrap();
        assert_eq!(reopened.yaml(), OLD);
        assert_eq!(reopened.interval_minutes(), Some(60));
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
    }
}
