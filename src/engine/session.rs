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
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

#[cfg(target_os = "linux")]
mod linux;
#[cfg(windows)]
mod windows;
#[cfg(target_os = "linux")]
use linux::OwnedChild;
#[cfg(windows)]
use windows::OwnedChild;

pub const DEFAULT_MIXED_PORT: u16 = 7890;

fn reserve_mixed_port(port: u16) -> io::Result<TcpListener> {
    TcpListener::bind((Ipv4Addr::LOCALHOST, port)).map_err(|error| {
        if error.kind() == ErrorKind::AddrInUse {
            Error::new(ErrorKind::AddrInUse, format!(
                "Paw mixed proxy port 127.0.0.1:{port} is already in use; choose another port with --mixed-port"
            ))
        } else {
            error
        }
    })
}

fn reserve_listener_ports(mixed_port: u16) -> io::Result<(TcpListener, TcpListener)> {
    let mixed = reserve_mixed_port(mixed_port)?;
    let controller = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
    Ok((mixed, controller))
}

pub struct Session {
    pub controller: Controller,
    pub mixed_port: u16,
    process: OwnedChild,
    directory: TempDir,
    controller_addr: SocketAddrV4,
    secret: String,
    https_policy_supported: bool,
}

impl Session {
    pub fn launch(
        binary: &Path,
        profile: &Profile,
        cancelled: impl Fn() -> bool,
    ) -> io::Result<Self> {
        Self::launch_with_mixed_port(binary, profile, DEFAULT_MIXED_PORT, cancelled)
    }

    /// A port of zero requests an ephemeral listener for isolated fixtures.
    pub fn launch_with_mixed_port(
        binary: &Path,
        profile: &Profile,
        requested_port: u16,
        cancelled: impl Fn() -> bool,
    ) -> io::Result<Self> {
        let binary = binary.canonicalize()?;
        let directory = session_directory()?;
        let config = directory.path().join("config.yaml");
        // Reserve the requested port first so the dynamic controller cannot take it.
        let (mixed_slot, control_slot) = reserve_listener_ports(requested_port)?;
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
        let https_policy_supported = Self::detect_https_policy(
            &binary,
            directory.path(),
            controller_addr,
            mixed_port,
            &secret,
            &cancelled,
        )?;
        let candidate = profile
            .managed_with_https(controller_addr, mixed_port, &secret, https_policy_supported)
            .map_err(Error::other)?;
        write_config(&config, candidate.as_bytes())?;
        let mut args = vec![
            OsStr::new("--no-external-plugins"),
            OsStr::new("-d"),
            directory.path().as_os_str(),
            OsStr::new("-f"),
            config.as_os_str(),
        ];
        if https_policy_supported {
            args.push(OsStr::new("--providers-require-https"));
        }
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
            https_policy_supported,
        })
    }

    fn detect_https_policy(
        binary: &Path,
        directory: &Path,
        controller: SocketAddrV4,
        mixed_port: u16,
        secret: &str,
        cancelled: &impl Fn() -> bool,
    ) -> io::Result<bool> {
        // A network-free known-good probe cannot leak profile contents, and
        // recognizes older Meow builds by their rejecting the new CLI flag.
        let source = "mode: direct\nrules: ['MATCH,DIRECT']\n";
        let probe = Profile::parse(source)
            .map_err(Error::other)?
            .managed(controller, mixed_port, secret)
            .map_err(Error::other)?;
        let mut file = tempfile::Builder::new()
            .prefix("capability-")
            .suffix(".yaml")
            .tempfile_in(directory)?;
        file.write_all(probe.as_bytes())?;
        let args = [
            OsStr::new("--no-external-plugins"),
            OsStr::new("--providers-require-https"),
            OsStr::new("-d"),
            directory.as_os_str(),
            OsStr::new("-f"),
            file.path().as_os_str(),
            OsStr::new("-t"),
        ];
        let mut process = OwnedChild::spawn(binary, directory, &args)?;
        let deadline = Instant::now() + Duration::from_secs(6);
        loop {
            if cancelled() {
                return Err(Error::from(ErrorKind::Interrupted));
            }
            if let Some(status) = process.try_wait()? {
                return Ok(status.success());
            }
            if Instant::now() >= deadline {
                return Ok(false);
            }
            thread::sleep(Duration::from_millis(30));
        }
    }

    pub(crate) fn apply_profile(&self, profile: &Profile) -> Result<(), ApplyError> {
        let prepared = profile
            .managed_with_https(
                self.controller_addr,
                self.mixed_port,
                &self.secret,
                self.https_policy_supported,
            )
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

    /// Fail closed before exporting the mixed port to an operating-system proxy.
    /// Ordinary Paw sessions can still run on platforms without socket-owner evidence.
    pub fn verified_mixed_listener_addr(&mut self) -> io::Result<SocketAddrV4> {
        if self.process.try_wait()?.is_some() {
            return Err(Error::other("the managed Meow process has exited"));
        }
        self.controller.verify().map_err(Error::other)?;
        let endpoint = SocketAddrV4::new(Ipv4Addr::LOCALHOST, self.mixed_port);
        #[cfg(target_os = "linux")]
        linux::verify_loopback_listener_owner(self.process.id(), endpoint)?;
        #[cfg(windows)]
        windows::verify_loopback_listener_owner(self.process.id()?, endpoint)?;
        if self.process.try_wait()?.is_some() {
            return Err(Error::other("the managed Meow process has exited"));
        }
        Ok(endpoint)
    }

    pub fn status(&mut self) -> io::Result<Option<ExitStatus>> {
        self.process.try_wait()
    }

    pub fn stop(mut self) -> io::Result<()> {
        self.process.stop()
    }
}

fn session_directory() -> io::Result<TempDir> {
    let mut builder = tempfile::Builder::new();
    builder.prefix("meow-paw-");
    // Set the mode on mkdir itself, before any credentials enter the directory.
    #[cfg(target_os = "linux")]
    builder.permissions(fs::Permissions::from_mode(0o700));
    builder.tempdir()
}

fn write_config(path: &Path, contents: &[u8]) -> io::Result<()> {
    #[cfg(target_os = "linux")]
    {
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)?
            .write_all(contents)
    }
    #[cfg(not(target_os = "linux"))]
    {
        fs::write(path, contents)
    }
}

pub(crate) enum ApplyError {
    Prepare(io::Error),
    Reload(ControllerError),
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::{session_directory, write_config};
    use std::{fs, os::unix::fs::PermissionsExt, process::Command};

    #[test]
    fn managed_config_permissions_with_permissive_umask() {
        const CHILD: &str = "MEOW_PAW_PERMISSION_TEST_CHILD";
        if std::env::var_os(CHILD).is_none() {
            let output = Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "engine::session::tests::managed_config_permissions_with_permissive_umask",
                    "--nocapture",
                ])
                .env(CHILD, "1")
                .output()
                .unwrap();
            assert!(
                output.status.success()
                    && String::from_utf8_lossy(&output.stdout).contains("permissions verified"),
                "isolated permission test failed: {}\n{}\n{}",
                output.status,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr),
            );
            return;
        }

        // This test runs alone in a child process, so it cannot change the
        // umask of other concurrently running tests.
        unsafe { libc::umask(0) };
        let directory = session_directory().unwrap();
        assert_eq!(
            directory.path().metadata().unwrap().permissions().mode() & 0o777,
            0o700
        );

        let config = directory.path().join("config.yaml");
        write_config(&config, b"synthetic config").unwrap();
        assert_eq!(
            config.metadata().unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            write_config(&config, b"replacement").unwrap_err().kind(),
            std::io::ErrorKind::AlreadyExists
        );
        assert_eq!(fs::read(config).unwrap(), b"synthetic config");
        println!("permissions verified");
    }
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

#[cfg(test)]
mod mixed_port_tests {
    use super::*;

    #[test]
    fn ephemeral_fixture_reserves_distinct_loopback_proxy_and_controller() -> io::Result<()> {
        let (mixed, controller) = reserve_listener_ports(0)?;
        let (mixed, controller) = (mixed.local_addr()?, controller.local_addr()?);
        assert!(mixed.ip().is_loopback() && controller.ip().is_loopback());
        assert_ne!(mixed.port(), 0);
        assert_ne!(controller.port(), 0);
        assert_ne!(mixed.port(), controller.port());
        Ok(())
    }

    #[test]
    fn occupied_mixed_port_is_not_stolen_or_replaced() -> io::Result<()> {
        assert_eq!(DEFAULT_MIXED_PORT, 7890);
        let existing = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
        let port = existing.local_addr()?.port();
        let binary = std::env::current_exe()?;
        let profile = Profile::parse(
            "mode: direct
rules: ['MATCH,DIRECT']
",
        )
        .map_err(Error::other)?;
        let error = match Session::launch_with_mixed_port(&binary, &profile, port, || false) {
            Ok(_) => return Err(Error::other("occupied port was accepted")),
            Err(error) => error,
        };
        assert_eq!(error.kind(), ErrorKind::AddrInUse);
        assert!(
            error
                .to_string()
                .contains(&format!("127.0.0.1:{port} is already in use"))
        );
        assert_eq!(existing.local_addr()?.port(), port);
        Ok(())
    }
}
