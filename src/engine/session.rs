//! Process ownership for a disposable Linux desktop session.
//! Keep the spawning thread alive for the session: PDEATHSIG tracks that thread.
use crate::{engine::controller::Controller, profile::Profile};
use std::{
    fmt::Write as _,
    fs,
    io::{self, Error, ErrorKind},
    net::{Ipv4Addr, SocketAddrV4, TcpListener, TcpStream},
    os::unix::process::CommandExt,
    path::Path,
    process::{Child, Command, ExitStatus, Stdio},
    thread,
    time::{Duration, Instant},
};
use tempfile::TempDir;

pub struct Session {
    pub controller: Controller,
    pub mixed_port: u16,
    process: OwnedChild,
    _directory: TempDir,
}

struct OwnedChild(Child);

impl OwnedChild {
    fn stop(&mut self) -> io::Result<()> {
        if self.0.try_wait()?.is_some() {
            return Ok(());
        }
        // The direct child has not yet been reaped, so its PID cannot be reused.
        unsafe { libc::kill(self.0.id() as libc::pid_t, libc::SIGTERM) };
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            if self.0.try_wait()?.is_some() {
                return Ok(());
            }
            thread::sleep(Duration::from_millis(25));
        }
        self.0.kill()?;
        self.0.wait().map(|_| ())
    }
}

impl Drop for OwnedChild {
    fn drop(&mut self) {
        let _ = self.stop();
    }
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
            .managed(controller_addr, mixed_port, &secret)
            .map_err(Error::other)?;
        fs::write(&config, candidate)?;
        let build_command = || {
            let mut cmd = Command::new(&binary);
            cmd.current_dir(directory.path())
                .arg("--no-external-plugins")
                .arg("-d")
                .arg(directory.path())
                .arg("-f")
                .arg(&config)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            let parent = std::process::id() as libc::pid_t;
            // Do not allocate or lock after fork. getppid covers death before prctl.
            unsafe {
                cmd.pre_exec(move || {
                    if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM, 0, 0, 0) == -1 {
                        return Err(Error::last_os_error());
                    }
                    if libc::getppid() != parent {
                        return Err(Error::from_raw_os_error(libc::ECHILD));
                    }
                    Ok(())
                });
            }
            cmd
        };
        if cancelled() {
            return Err(Error::from(ErrorKind::Interrupted));
        }
        let mut preflight = OwnedChild(build_command().arg("-t").spawn()?);
        let deadline = Instant::now() + Duration::from_secs(6);
        loop {
            if cancelled() {
                return Err(Error::from(ErrorKind::Interrupted));
            }
            if let Some(status) = preflight.0.try_wait()? {
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
        let mut process = OwnedChild(build_command().spawn()?);
        let controller = Controller::new(controller_addr.into(), secret).map_err(Error::other)?;
        let deadline = Instant::now() + Duration::from_secs(6);
        loop {
            if cancelled() {
                return Err(Error::from(ErrorKind::Interrupted));
            }
            if let Some(status) = process.0.try_wait()? {
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
            _directory: directory,
        })
    }

    pub fn status(&mut self) -> io::Result<Option<ExitStatus>> {
        self.process.0.try_wait()
    }

    pub fn stop(mut self) -> io::Result<()> {
        self.process.stop()
    }
}
