//! PDEATHSIG tracks the spawning thread, which must own the session.
use std::{
    ffi::OsStr,
    io::{self, Error},
    os::unix::process::CommandExt,
    path::Path,
    process::{Child, Command, ExitStatus, Stdio},
    thread,
    time::{Duration, Instant},
};

pub(super) struct OwnedChild(Child);

impl OwnedChild {
    pub(super) fn spawn(binary: &Path, directory: &Path, args: &[&OsStr]) -> io::Result<Self> {
        let mut cmd = Command::new(binary);
        cmd.current_dir(directory)
            .args(args)
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
        cmd.spawn().map(Self)
    }

    pub(super) fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        self.0.try_wait()
    }

    pub(super) fn stop(&mut self) -> io::Result<()> {
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
