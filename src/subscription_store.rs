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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SubscriptionMetadata {
    pub interval: Option<DisplayInterval>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SavedSubscription {
    // No Debug implementation: the URL and YAML can both contain credentials.
    url: String,
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
            yaml: download.yaml,
            policy,
            provider_interval_minutes: download.suggested_interval_minutes,
        };
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
        let record: SavedSubscription =
            serde_json::from_slice(&bytes).map_err(|_| invalid("Invalid subscription record"))?;
        if record.url.len() > MAX_URL_BYTES
            || record.yaml.len() > MAX_PROFILE_BYTES
            || parse_url(&record.url).is_err()
            || record.policy.interval_override_minutes == Some(0)
            || record.provider_interval_minutes == Some(0)
        {
            return Err(invalid("Invalid subscription record"));
        }
        Ok(record)
    }

    /// Read a bounded record and return only metadata safe for display.
    pub fn metadata(&self, id: SubscriptionId) -> io::Result<SubscriptionMetadata> {
        self.read(id).map(|record| record.metadata())
    }

    /// A manual refresh retains the local policy and never applies a profile.
    pub fn refresh(&mut self, id: SubscriptionId, download: Download) -> io::Result<()> {
        self.refresh_with(id, download, |staged, path| {
            staged.persist(path).map(|_| ()).map_err(|e| e.error)
        })
    }

    fn refresh_with(
        &mut self,
        id: SubscriptionId,
        download: Download,
        commit: impl FnOnce(NamedTempFile, &Path) -> io::Result<()>,
    ) -> io::Result<()> {
        validate(&download)?;
        let mut record = self.read(id)?;
        record.yaml = download.yaml;
        record.provider_interval_minutes = download.suggested_interval_minutes;
        self.write(id, &record, commit)
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
        staged.as_file().sync_all()?;
        // The YAML and metadata become visible together. Avoid a second fallible
        // write after replacing the last-good record.
        commit(staged, &self.record_path(id))
    }
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
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
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
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }
}
