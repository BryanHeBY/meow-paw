//! Shared lifecycle for an isolated Meow instance owned by Paw.
use crate::{
    engine::controller::{Controller, ControllerError},
    profile::Profile,
};
use std::{
    ffi::OsStr,
    fmt::Write as _,
    fs,
    io::{self, Error, ErrorKind, Write as _},
    net::{Ipv4Addr, SocketAddrV4, TcpListener, TcpStream},
    path::Path,
    process::ExitStatus,
    thread,
    time::{Duration, Instant},
};
use tempfile::TempDir;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(windows)]
mod windows;
#[cfg(target_os = "linux")]
use linux::OwnedChild;
#[cfg(windows)]
use windows::OwnedChild;

pub struct Session {
    pub controller: Controller,
    pub mixed_port: u16,
    process: OwnedChild,
    directory: TempDir,
    controller_addr: SocketAddrV4,
    secret: String,
}

impl Session {
    pub fn launch(
        binary: &Path,
        profile: &Profile,
        cancelled: impl Fn() -> bool,
    ) -> io::Result<Self> {
        let binary = binary.canonicalize()?;
        let directory = tempfile::Builder::new().prefix("meow-paw-").tempdir()?;
        let config = directory.path().join("config.yaml");
        let control_slot = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
        let mixed_slot = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
        let mixed_port = mixed_slot.local_addr()?.port();
        let controller_addr =
            SocketAddrV4::new(Ipv4Addr::LOCALHOST, control_slot.local_addr()?.port());
        let mut raw_secret = [0_u8; 32];
        getrandom::fill(&mut raw_secret)
            .map_err(|_| Error::other("Could not generate a controller secret"))?;
        let secret = raw_secret
            .iter()
            .fold(String::with_capacity(64), |mut hex, byte| {
                let _ = write!(hex, "{byte:02x}");
                hex
            });
        let candidate = profile
            .managed_with_https(controller_addr, mixed_port, &secret, true)
            .map_err(Error::other)?;
        fs::write(&config, candidate)?;
        let args = [
            OsStr::new("--no-external-plugins"),
            // Old engines must fail closed at preflight rather than silently
            // downloading provider credentials over an unguarded transport.
            OsStr::new("--providers-require-https"),
            OsStr::new("-d"),
            directory.path().as_os_str(),
            OsStr::new("-f"),
            config.as_os_str(),
        ];
        if cancelled() {
            return Err(Error::from(ErrorKind::Interrupted));
        }
        let mut preflight_args = args.to_vec();
        preflight_args.push(OsStr::new("-t"));
        let mut preflight = OwnedChild::spawn(&binary, directory.path(), &preflight_args)?;
        let deadline = Instant::now() + Duration::from_secs(6);
        loop {
            if cancelled() {
                return Err(Error::from(ErrorKind::Interrupted));
            }
            if let Some(status) = preflight.try_wait()? {
                if !status.success() {
                    return Err(Error::other(format!(
                        "Meow rejected the candidate configuration ({status})"
                    )));
                }
                break;
            }
            if Instant::now() >= deadline {
                return Err(Error::new(
                    ErrorKind::TimedOut,
                    "Configuration preflight timed out",
                ));
            }
            thread::sleep(Duration::from_millis(30));
        }
        drop(preflight);
        drop(control_slot);
        drop(mixed_slot);
        let mut process = OwnedChild::spawn(&binary, directory.path(), &args)?;
        let controller =
            Controller::new(controller_addr.into(), secret.clone()).map_err(Error::other)?;
        let deadline = Instant::now() + Duration::from_secs(6);
        loop {
            if cancelled() {
                return Err(Error::from(ErrorKind::Interrupted));
            }
            if let Some(status) = process.try_wait()? {
                return Err(Error::other(format!(
                    "Meow exited before its listeners became ready ({status})"
                )));
            }
            if Instant::now() >= deadline {
                return Err(Error::new(
                    ErrorKind::TimedOut,
                    "Meow listeners did not become ready",
                ));
            }
            if controller.verify().is_ok()
                && TcpStream::connect_timeout(
                    &(Ipv4Addr::LOCALHOST, mixed_port).into(),
                    Duration::from_millis(100),
                )
                .is_ok()
            {
                break;
            }
            thread::sleep(Duration::from_millis(50));
        }
        Ok(Self {
            controller,
            mixed_port,
            process,
            directory,
            controller_addr,
            secret,
        })
    }

    pub(crate) fn apply_profile(&self, profile: &Profile) -> Result<(), ApplyError> {
        let prepared = profile
            .managed_with_https(self.controller_addr, self.mixed_port, &self.secret, true)
            .map_err(Error::other)
            .map_err(ApplyError::Prepare)?;
        let mut staging = tempfile::Builder::new()
            .prefix("apply-")
            .suffix(".yaml")
            .tempfile_in(self.directory.path())
            .map_err(ApplyError::Prepare)?;
        staging
            .write_all(prepared.as_bytes())
            .map_err(ApplyError::Prepare)?;
        self.controller
            .reload_config(staging.path())
            .map_err(ApplyError::Reload)
    }

    pub fn status(&mut self) -> io::Result<Option<ExitStatus>> {
        self.process.try_wait()
    }

    pub fn stop(mut self) -> io::Result<()> {
        self.process.stop()
    }
}

pub(crate) enum ApplyError {
    Prepare(io::Error),
    Reload(ControllerError),
}

impl ApplyError {
    pub(crate) fn rejected(&self) -> bool {
        matches!(
            self,
            Self::Prepare(_) | Self::Reload(ControllerError::Status(400..=499))
        )
    }

    pub(crate) fn message(&self) -> String {
        match self {
            Self::Prepare(error) => error.to_string(),
            Self::Reload(error) if self.rejected() => format!("Meow rejected the profile: {error}"),
            Self::Reload(_) => "Meow did not confirm the change. Apply a profile again to confirm which configuration is running.".into(),
        }
    }
}
