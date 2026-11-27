//! PDEATHSIG tracks the spawning thread, which must own the session.
use std::{
    ffi::OsStr,
    fs,
    io::{self, BufRead as _, BufReader, Error, ErrorKind},
    net::{Ipv4Addr, SocketAddrV4},
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

    pub(super) fn id(&self) -> u32 {
        self.0.id()
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

/// Read the app network namespace, then require a unique exact IPv4 listener
/// whose socket inode is held by the unreaped direct child. Wildcard and v6
/// listeners on the same port are ambiguous (a dual-stack socket can take IPv4).
pub(super) fn verify_loopback_listener_owner(pid: u32, endpoint: SocketAddrV4) -> io::Result<()> {
    if *endpoint.ip() != Ipv4Addr::LOCALHOST || endpoint.port() == 0 {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "expected a nonzero IPv4 loopback listener",
        ));
    }
    let mut matching = None;
    for (table, ipv4) in [("/proc/self/net/tcp", true), ("/proc/self/net/tcp6", false)] {
        for line in BufReader::new(fs::File::open(table)?).lines().skip(1) {
            let line = line?;
            let mut fields = line.split_ascii_whitespace();
            let (Some(local), Some(state)) = (fields.nth(1), fields.nth(1)) else {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    "malformed Linux TCP socket table",
                ));
            };
            let (address, port) = local.split_once(':').ok_or_else(|| {
                Error::new(ErrorKind::InvalidData, "malformed Linux TCP socket address")
            })?;
            let port =
                u16::from_str_radix(port, 16).map_err(|e| Error::new(ErrorKind::InvalidData, e))?;
            if state != "0A" || port != endpoint.port() {
                continue;
            }
            // /proc renders native-endian IPv4 words, including on big-endian Linux.
            let loopback = u32::from_ne_bytes(Ipv4Addr::LOCALHOST.octets());
            let exact = ipv4 && u32::from_str_radix(address, 16).ok() == Some(loopback);
            if !exact || matching.is_some() {
                return Err(Error::other(format!(
                    "mixed listener at {endpoint} has ambiguous ownership"
                )));
            }
            let inode = fields
                .nth(5)
                .and_then(|value| value.parse::<u64>().ok())
                .filter(|&value| value != 0)
                .ok_or_else(|| {
                    Error::new(
                        ErrorKind::InvalidData,
                        "missing Linux listener socket inode",
                    )
                })?;
            matching = Some(inode);
        }
    }
    let inode = matching.ok_or_else(|| {
        Error::new(
            ErrorKind::NotFound,
            format!("no mixed listener at {endpoint}"),
        )
    })?;
    let expected = format!("socket:[{inode}]");
    for fd in fs::read_dir(format!("/proc/{pid}/fd"))? {
        match fs::read_link(fd?.path()) {
            Ok(link) if link.as_os_str() == OsStr::new(&expected) => return Ok(()),
            // Other process descriptors can close while we inspect them.
            Err(error) if error.kind() != ErrorKind::NotFound => return Err(error),
            _ => {}
        }
    }
    Err(Error::new(
        ErrorKind::PermissionDenied,
        format!("mixed listener at {endpoint} is not owned by the managed Meow process"),
    ))
}

#[cfg(test)]
mod listener_ownership_tests {
    use super::*;
    use std::net::TcpListener;

    #[test]
    fn verifies_only_exact_live_owned_listeners() -> io::Result<()> {
        let local = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
        let endpoint = SocketAddrV4::new(Ipv4Addr::LOCALHOST, local.local_addr()?.port());
        verify_loopback_listener_owner(std::process::id(), endpoint)?;

        let mut other = Command::new("sleep").arg("30").spawn()?;
        let result = verify_loopback_listener_owner(other.id(), endpoint);
        other.kill()?;
        other.wait()?;
        assert_eq!(result.unwrap_err().kind(), ErrorKind::PermissionDenied);

        let other_address = TcpListener::bind((Ipv4Addr::new(127, 0, 0, 2), 0))?;
        let other_port = other_address.local_addr()?.port();
        assert!(
            verify_loopback_listener_owner(
                std::process::id(),
                SocketAddrV4::new(Ipv4Addr::LOCALHOST, other_port),
            )
            .is_err()
        );
        assert_eq!(
            verify_loopback_listener_owner(
                std::process::id(),
                SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, other_port),
            )
            .unwrap_err()
            .kind(),
            ErrorKind::InvalidInput
        );
        Ok(())
    }
}
