#![cfg(target_os = "linux")]

//! Opt in with `MEOW_BIN=/trusted/local/meow cargo test --test linux_owner_exit
//! -- --ignored --exact owner_exit_stops_managed_core` on Linux with pidfds and
//! readable /proc. The binary must be a trusted, directly executable Meow.
//!
//! Cleanup pins observed, identity-checked children with pidfds before ever
//! signalling them. A hard failure of the test runner itself, or a child that
//! starts and is reparented between /proc scans during a failed launch, cannot
//! be cleaned up reliably without an external containment facility. Run this
//! opt-in regression in a disposable process/container environment if that
//! limitation is unacceptable. A process stuck in uninterruptible kernel sleep
//! or a denied pidfd signal can also outlast bounded cleanup.

use meow_paw::{engine::session::Session, profile::Profile};
use std::{
    collections::HashSet,
    fs,
    io::{self, Read},
    net::{Ipv4Addr, SocketAddr, TcpStream},
    os::fd::{AsRawFd, FromRawFd, OwnedFd},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

const TEST_NAME: &str = "owner_exit_stops_managed_core";
const HELPER: &str = "PAW_OWNER_EXIT_SUPERVISOR";

// A pidfd pins the identity even after the PID is reused. Readability means
// the process has exited, including a zombie awaiting somebody else's reap.
struct PinnedProcess {
    pid: u32,
    start: u64,
    fd: OwnedFd,
}

impl PinnedProcess {
    fn open(pid: u32) -> io::Result<Self> {
        let (_, _, start) = process_stat(pid)?;
        let fd = unsafe { libc::syscall(libc::SYS_pidfd_open, pid, 0) };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        let fd = unsafe { OwnedFd::from_raw_fd(fd as i32) };
        if process_stat(pid)?.2 != start {
            return Err(io::Error::other("PID changed while opening pidfd"));
        }
        Ok(Self { pid, start, fd })
    }

    fn exited(&self) -> io::Result<bool> {
        let mut pollfd = libc::pollfd {
            fd: self.fd.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        let result = unsafe { libc::poll(&mut pollfd, 1, 0) };
        if result < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(result > 0 && pollfd.revents & libc::POLLIN != 0)
    }

    fn kill(&self) -> io::Result<()> {
        let result = unsafe {
            libc::syscall(
                libc::SYS_pidfd_send_signal,
                self.fd.as_raw_fd(),
                libc::SIGKILL,
                std::ptr::null::<libc::siginfo_t>(),
                0,
            )
        };
        if result < 0 {
            let error = io::Error::last_os_error();
            if error.raw_os_error() != Some(libc::ESRCH) {
                return Err(error);
            }
        }
        Ok(())
    }
}

fn process_stat(pid: u32) -> io::Result<(char, u32, u64)> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat"))?;
    let after_name = stat
        .rsplit_once(')')
        .ok_or_else(|| io::Error::other("invalid /proc stat"))?
        .1;
    let fields: Vec<_> = after_name.split_whitespace().collect();
    let bad = || io::Error::other("invalid /proc stat fields");
    Ok((
        fields
            .first()
            .and_then(|s| s.chars().next())
            .ok_or_else(bad)?,
        fields.get(1).and_then(|s| s.parse().ok()).ok_or_else(bad)?,
        fields
            .get(19)
            .and_then(|s| s.parse().ok())
            .ok_or_else(bad)?,
    ))
}

fn children(supervisor: u32) -> io::Result<HashSet<u32>> {
    let mut result = HashSet::new();
    for task in fs::read_dir(format!("/proc/{supervisor}/task"))? {
        let task = task?;
        // A thread can disappear while enumerating tasks.
        if let Ok(list) = fs::read_to_string(task.path().join("children")) {
            result.extend(
                list.split_whitespace()
                    .filter_map(|s| s.parse::<u32>().ok()),
            );
        }
    }
    Ok(result)
}

fn is_owned(pid: u32, supervisor: u32, binary: &Path, root: &Path) -> bool {
    let Ok((state, parent, _)) = process_stat(pid) else {
        return false;
    };
    parent == supervisor
        && state != 'Z'
        && fs::read_link(format!("/proc/{pid}/exe")).is_ok_and(|exe| exe == binary)
        && fs::read_link(format!("/proc/{pid}/cwd")).is_ok_and(|cwd| cwd.starts_with(root))
}

fn owns_listener(pid: u32, port: u16) -> io::Result<bool> {
    let tcp = fs::read_to_string("/proc/net/tcp")?;
    let address = format!("0100007F:{port:04X}");
    let inodes: HashSet<_> = tcp
        .lines()
        .skip(1)
        .filter_map(|line| {
            let fields: Vec<_> = line.split_whitespace().collect();
            (fields.get(1) == Some(&address.as_str()) && fields.get(3) == Some(&"0A"))
                .then(|| fields.get(9).copied())
                .flatten()
        })
        .collect();
    for fd in fs::read_dir(format!("/proc/{pid}/fd"))? {
        let target = fs::read_link(fd?.path());
        if let Ok(target) = target {
            let target = target.to_string_lossy();
            if let Some(inode) = target
                .strip_prefix("socket:[")
                .and_then(|s| s.strip_suffix(']'))
                && inodes.contains(inode)
            {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

struct Supervisor {
    child: Child,
    binary: PathBuf,
    root: PathBuf,
    observed: Vec<PinnedProcess>,
}

impl Supervisor {
    fn observe(&mut self) {
        let supervisor = self.child.id();
        if let Ok(pids) = children(supervisor) {
            for pid in pids {
                if !is_owned(pid, supervisor, &self.binary, &self.root) {
                    continue;
                }
                if let Ok(pinned) = PinnedProcess::open(pid)
                    && is_owned(pid, supervisor, &self.binary, &self.root)
                    && process_stat(pid).is_ok_and(|stat| stat.2 == pinned.start)
                    && !self
                        .observed
                        .iter()
                        .any(|p| p.pid == pid && p.start == pinned.start)
                {
                    self.observed.push(pinned);
                }
            }
        }
    }
}

impl Drop for Supervisor {
    fn drop(&mut self) {
        // First pin any still-identifiable direct children. The normal path
        // needs no child signal; SIGKILL is only a failure cleanup fallback.
        match self.child.try_wait() {
            Ok(None) => {
                self.observe();
                if let Err(error) = self.child.kill() {
                    eprintln!("could not kill test supervisor: {error}");
                }
            }
            Ok(Some(_)) => {} // A reaped PID may already have been reused.
            Err(error) => eprintln!("could not check test supervisor: {error}"),
        }
        for process in &self.observed {
            match process.exited() {
                Ok(true) => {}
                Ok(false) => {
                    if let Err(error) = process.kill() {
                        eprintln!("could not clean up owned Meow {}: {error}", process.pid);
                    }
                }
                Err(error) => eprintln!("could not check owned Meow {}: {error}", process.pid),
            }
        }
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            if self.child.try_wait().ok().flatten().is_some()
                && self.observed.iter().all(|p| p.exited().unwrap_or(false))
            {
                break;
            }
            thread::sleep(Duration::from_millis(20));
        }
        for process in &self.observed {
            if !process.exited().unwrap_or(false) {
                eprintln!("owned Meow {} did not exit during cleanup", process.pid);
            }
        }
    }
}

#[test]
#[ignore = "requires trusted local MEOW_BIN, Linux pidfds and readable /proc"]
fn owner_exit_stops_managed_core() {
    if std::env::var_os(HELPER).is_some() {
        let binary = PathBuf::from(std::env::var_os("MEOW_BIN").expect("MEOW_BIN"));
        let root = PathBuf::from(std::env::var_os("TMPDIR").expect("TMPDIR"));
        let profile = Profile::parse(
            "mode: direct\nipv6: false\ndns: {enable: false}\nrules: ['MATCH,DIRECT']\n",
        )
        .unwrap();
        let session = Session::launch_with_mixed_port(&binary, &profile, 0, || false).unwrap();
        session.controller.verify().unwrap();
        fs::write(root.join("ready.tmp"), session.mixed_port.to_string()).unwrap();
        fs::rename(root.join("ready.tmp"), root.join("ready")).unwrap();
        // Keep Session and, crucially, its spawning thread alive until SIGKILL.
        let mut byte = [0];
        let _ = io::stdin().read(&mut byte);
        drop(session);
        return;
    }

    let binary = PathBuf::from(std::env::var_os("MEOW_BIN").expect("set trusted MEOW_BIN"))
        .canonicalize()
        .expect("canonicalize MEOW_BIN");
    let root = tempfile::Builder::new()
        .prefix("paw-owner-exit-")
        .tempdir()
        .unwrap();
    let root_path = root.path().canonicalize().unwrap();
    // Fail before launching anything if this kernel disallows pidfds.
    PinnedProcess::open(std::process::id()).expect("pidfd_open required");
    let child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--ignored",
            "--exact",
            TEST_NAME,
            "--nocapture",
            "--test-threads=1",
        ])
        .env(HELPER, "1")
        .env("MEOW_BIN", &binary)
        .env("TMPDIR", &root_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();
    let mut supervisor = Supervisor {
        child,
        binary,
        root: root_path.clone(),
        observed: Vec::new(),
    };
    let deadline = Instant::now() + Duration::from_secs(25);
    let port: u16 = loop {
        supervisor.observe();
        if let Ok(ready) = fs::read_to_string(root_path.join("ready")) {
            break ready.parse().expect("supervisor readiness port");
        }
        assert!(
            supervisor.child.try_wait().unwrap().is_none(),
            "supervisor exited before readiness"
        );
        assert!(Instant::now() < deadline, "supervisor did not become ready");
        thread::sleep(Duration::from_millis(20));
    };
    let addr: SocketAddr = (Ipv4Addr::LOCALHOST, port).into();
    assert!(TcpStream::connect_timeout(&addr, Duration::from_millis(200)).is_ok());
    supervisor.observe();
    let matches: Vec<_> = supervisor
        .observed
        .iter()
        .filter(|p| {
            is_owned(
                p.pid,
                supervisor.child.id(),
                &supervisor.binary,
                &supervisor.root,
            ) && process_stat(p.pid).is_ok_and(|s| s.2 == p.start)
                && owns_listener(p.pid, port).unwrap_or(false)
                && p.exited().is_ok_and(|exited| !exited)
        })
        .collect();
    assert_eq!(
        matches.len(),
        1,
        "expected one identified Meow owning the ready listener"
    );
    let core_index = supervisor
        .observed
        .iter()
        .position(|p| std::ptr::eq(p, matches[0]))
        .unwrap();

    // Child::kill sends SIGKILL: no Session destructor or graceful shutdown.
    supervisor.child.kill().expect("SIGKILL test supervisor");
    let deadline = Instant::now() + Duration::from_secs(8);
    let status = loop {
        if let Some(status) = supervisor.child.try_wait().expect("reap test supervisor") {
            break status;
        }
        assert!(Instant::now() < deadline, "test supervisor did not exit");
        thread::sleep(Duration::from_millis(25));
    };
    use std::os::unix::process::ExitStatusExt;
    assert_eq!(status.signal(), Some(libc::SIGKILL));
    let deadline = Instant::now() + Duration::from_secs(8);
    while !supervisor.observed[core_index]
        .exited()
        .expect("poll Meow pidfd")
    {
        assert!(
            Instant::now() < deadline,
            "owned Meow remained live after supervisor death"
        );
        thread::sleep(Duration::from_millis(25));
    }
    assert!(
        TcpStream::connect_timeout(&addr, Duration::from_millis(200)).is_err(),
        "mixed listener still accepts connections"
    );
}
