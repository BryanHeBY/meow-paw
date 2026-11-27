//! Explicitly managed desktop proxy for one ready Paw listener.
//! Never start or recover this automatically during UI construction.
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::num::NonZeroU16;
use std::path::{Path, PathBuf};

#[cfg(target_os = "linux")]
mod gnome;
#[cfg(windows)]
mod wininet;

#[derive(Debug)]
pub enum ProxyError {
    Unsupported,
    ConsentRequired,
    Busy,
    Conflict,
    InvalidJournal,
    Io(io::Error),
}
impl From<io::Error> for ProxyError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}
impl std::fmt::Display for ProxyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unsupported => f.write_str("This desktop configuration cannot be managed safely."),
            Self::ConsentRequired => f.write_str("Enabling system proxy requires explicit consent."),
            Self::Busy => f.write_str("Another Paw instance is using the system proxy journal."),
            Self::Conflict => f.write_str("System proxy changed outside Paw or a proxy operation was interrupted. Paw did not overwrite it."),
            Self::InvalidJournal => f.write_str("System proxy recovery journal could not be validated."),
            Self::Io(e) => write!(f, "System proxy operation failed: {e}"),
        }
    }
}
impl std::error::Error for ProxyError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Consent {
    NotGranted,
    GrantedByUser,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Status {
    Off,
    Owned(NonZeroU16),
    InterruptedOrChanged,
    RecoveryPending,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Snapshot(Vec<Option<String>>);

trait Backend {
    fn id(&self) -> &'static str;
    fn read(&self) -> Result<Snapshot, ProxyError>;
    fn desired(&self, previous: &Snapshot, port: NonZeroU16) -> Result<Snapshot, ProxyError>;
    // Each backend rechecks expectations immediately before writes, and
    // must not overwrite fields outside this settings group.
    fn apply(&self, from: &Snapshot, to: &Snapshot) -> Result<(), ProxyError>;
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Journal {
    version: u8,
    backend: String,
    port: NonZeroU16,
    original: Snapshot,
    expected: Snapshot,
}

pub struct SystemProxy {
    dir: PathBuf,
    backend: Box<dyn Backend>,
    owner: RefCell<Option<File>>,
}

impl SystemProxy {
    /// Use a private, per-user, persistent application directory. All Paw
    /// instances in this desktop session must use the same directory.
    pub fn discover(state_dir: &Path) -> Result<Self, ProxyError> {
        #[cfg(target_os = "linux")]
        let backend: Box<dyn Backend> = Box::new(gnome::Gnome::discover()?);
        #[cfg(windows)]
        let backend: Box<dyn Backend> = Box::new(wininet::WinInet::discover()?);
        #[cfg(not(any(target_os = "linux", windows)))]
        {
            let _ = state_dir;
            return Err(ProxyError::Unsupported);
        }
        #[cfg(any(target_os = "linux", windows))]
        {
            if !state_dir.is_absolute() {
                return Err(ProxyError::Unsupported);
            }
            Ok(Self {
                dir: state_dir.join("system-proxy"),
                backend,
                owner: RefCell::new(None),
            })
        }
    }

    /// Call only for a verified Paw-owned IPv4 loopback mixed listener.
    /// This API never takes a caller-supplied host or a PAC URL.
    pub fn activate(&self, port: NonZeroU16, consent: Consent) -> Result<(), ProxyError> {
        if consent != Consent::GrantedByUser {
            return Err(ProxyError::ConsentRequired);
        }
        if self.owns_operation() {
            return Err(ProxyError::Conflict);
        }
        fs::create_dir_all(self.dir.parent().ok_or(ProxyError::InvalidJournal)?)?;
        let lock = self.lock()?;
        if self.journal()?.is_some() {
            return Err(ProxyError::Conflict);
        }
        let original = self.backend.read()?;
        let expected = self.backend.desired(&original, port)?;
        // Do not assume ownership of an already-identical foreign proxy.
        if expected == original {
            return Err(ProxyError::Conflict);
        }
        let journal = Journal {
            version: 1,
            backend: self.backend.id().into(),
            port,
            original,
            expected,
        };
        self.save(&journal)?;
        self.owner.replace(Some(lock));
        self.backend.apply(&journal.original, &journal.expected)?;
        if self.backend.read()? != journal.expected {
            return Err(ProxyError::Conflict);
        }
        Ok(())
    }

    /// Also usable for explicit post-crash recovery. A journal whose current
    /// settings exactly equal neither snapshot is retained without writing.
    pub fn restore(&self) -> Result<(), ProxyError> {
        if !self.dir.exists() && !self.owns_operation() {
            return Ok(());
        }
        let _lock = self.borrow_or_lock()?;
        let Some(journal) = self.journal()? else {
            self.owner.replace(None);
            return Ok(());
        };
        let observed = self.backend.read()?;
        if observed != journal.original {
            if observed != journal.expected {
                return Err(ProxyError::Conflict);
            }
            self.backend.apply(&journal.expected, &journal.original)?;
            if self.backend.read()? != journal.original {
                return Err(ProxyError::Conflict);
            }
        }
        fs::remove_file(self.dir.join("recovery.json"))?;
        self.sync_dir()?;
        self.owner.replace(None);
        Ok(())
    }

    pub fn status(&self) -> Result<Status, ProxyError> {
        if !self.dir.exists() && !self.owns_operation() {
            return Ok(Status::Off);
        }
        let _lock = self.borrow_or_lock()?;
        let Some(journal) = self.journal()? else {
            return Ok(Status::Off);
        };
        let observed = self.backend.read()?;
        Ok(if observed == journal.expected {
            Status::Owned(journal.port)
        } else if observed == journal.original {
            Status::RecoveryPending
        } else {
            Status::InterruptedOrChanged
        })
    }

    /// Whether this instance durably recorded its own operation and still
    /// holds the OS lease, even if the subsequent write returned an error.
    pub fn owns_operation(&self) -> bool {
        self.owner.borrow().is_some()
    }

    fn borrow_or_lock(&self) -> Result<Option<File>, ProxyError> {
        if self.owns_operation() {
            Ok(None)
        } else {
            self.lock().map(Some)
        }
    }

    fn lock(&self) -> Result<File, ProxyError> {
        use std::fs::TryLockError;
        let builder = fs::DirBuilder::new();
        #[cfg(target_os = "linux")]
        let builder = {
            use std::os::unix::fs::DirBuilderExt;
            let mut builder = builder;
            builder.mode(0o700);
            builder
        };
        match builder.create(&self.dir) {
            Ok(()) => {
                #[cfg(target_os = "linux")]
                File::open(self.dir.parent().ok_or(ProxyError::InvalidJournal)?)?.sync_all()?;
            }
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e.into()),
        }
        let metadata = fs::symlink_metadata(&self.dir)?;
        if !metadata.is_dir() || unsafe_path(&metadata) {
            return Err(ProxyError::InvalidJournal);
        }
        #[cfg(target_os = "linux")]
        {
            use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
            if metadata.mode() & 0o077 != 0 || metadata.uid() != unsafe { libc::geteuid() } {
                return Err(ProxyError::InvalidJournal);
            }
            // A permanent separate lock inode protects recovery.json across
            // rename and removal. Never remove the lock file.
            let file = OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .mode(0o600)
                .custom_flags(libc::O_NOFOLLOW)
                .open(self.dir.join("lease"))?;
            if unsafe_path(&file.metadata()?) {
                return Err(ProxyError::InvalidJournal);
            }
            match file.try_lock() {
                Ok(()) => Ok(file),
                Err(TryLockError::WouldBlock) => Err(ProxyError::Busy),
                Err(TryLockError::Error(e)) => Err(e.into()),
            }
        }
        #[cfg(not(target_os = "linux"))]
        {
            #[cfg(windows)]
            use std::os::windows::fs::OpenOptionsExt;
            let mut opts = OpenOptions::new();
            #[cfg(windows)]
            opts.custom_flags(
                windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT,
            );
            let file = opts
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .open(self.dir.join("lease"))?;
            if unsafe_path(&file.metadata()?) {
                return Err(ProxyError::InvalidJournal);
            }
            match file.try_lock() {
                Ok(()) => Ok(file),
                Err(TryLockError::WouldBlock) => Err(ProxyError::Busy),
                Err(TryLockError::Error(e)) => Err(e.into()),
            }
        }
    }

    fn journal(&self) -> Result<Option<Journal>, ProxyError> {
        const MAX: u64 = 32_768;
        let path = self.dir.join("recovery.json");
        match fs::symlink_metadata(&path) {
            Ok(meta) if !meta.is_file() || unsafe_path(&meta) => {
                return Err(ProxyError::InvalidJournal);
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.into()),
            Ok(_) => {}
        }
        let mut bytes = Vec::new();
        File::open(path)?.take(MAX + 1).read_to_end(&mut bytes)?;
        if bytes.len() as u64 > MAX {
            return Err(ProxyError::InvalidJournal);
        }
        let journal: Journal =
            serde_json::from_slice(&bytes).map_err(|_| ProxyError::InvalidJournal)?;
        if journal.version != 1
            || journal.backend != self.backend.id()
            || journal.original == journal.expected
            || self
                .backend
                .desired(&journal.original, journal.port)
                .ok()
                .as_ref()
                != Some(&journal.expected)
        {
            return Err(ProxyError::InvalidJournal);
        }
        Ok(Some(journal))
    }

    fn save(&self, j: &Journal) -> Result<(), ProxyError> {
        let mut temp = tempfile::NamedTempFile::new_in(&self.dir)?;
        serde_json::to_writer(&mut temp, j).map_err(io::Error::other)?;
        temp.flush()?;
        temp.as_file().sync_all()?;
        #[cfg(windows)]
        wininet::persist(temp, &self.dir.join("recovery.json"))?;
        #[cfg(not(windows))]
        temp.persist_noclobber(self.dir.join("recovery.json"))
            .map_err(|e| e.error)?;
        self.sync_dir()
    }

    fn sync_dir(&self) -> Result<(), ProxyError> {
        #[cfg(target_os = "linux")]
        File::open(&self.dir)?.sync_all()?;
        Ok(())
    }
}

fn unsafe_path(meta: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;
        if meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return true;
        }
    }
    meta.file_type().is_symlink()
}

#[cfg(test)]
mod tests;
