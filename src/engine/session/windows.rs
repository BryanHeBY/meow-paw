//! Bind the new process to a private kill-on-close Job before it can run.
use std::{
    ffi::OsStr,
    io::{self, Error},
    mem::{size_of, size_of_val},
    os::windows::{
        ffi::OsStrExt,
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
        process::ExitStatusExt,
    },
    path::Path,
    process::ExitStatus,
    ptr::{null, null_mut},
};

use windows_sys::Win32::{
    Foundation::{HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT},
    System::{
        JobObjects::{
            CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
            SetInformationJobObject, TerminateJobObject,
        },
        Threading::{
            CREATE_NO_WINDOW, CreateProcessW, DeleteProcThreadAttributeList,
            EXTENDED_STARTUPINFO_PRESENT, GetExitCodeProcess, InitializeProcThreadAttributeList,
            LPPROC_THREAD_ATTRIBUTE_LIST, PROC_THREAD_ATTRIBUTE_JOB_LIST, PROCESS_INFORMATION,
            STARTUPINFOEXW, UpdateProcThreadAttribute, WaitForSingleObject,
        },
    },
};

pub(super) struct OwnedChild {
    job: OwnedHandle,
    process: OwnedHandle,
}

// Word-aligned storage for an initialized Win32 attribute list. The referenced
// job HANDLE array must remain live through CreateProcessW.
struct Attributes(Vec<usize>);

impl Attributes {
    fn new() -> io::Result<Self> {
        let mut len = 0;
        // Win32's documented size query returns failure and fills the length.
        unsafe { InitializeProcThreadAttributeList(null_mut(), 1, 0, &mut len) };
        if len == 0 {
            return Err(Error::last_os_error());
        }
        let mut words = vec![0; len.div_ceil(size_of::<usize>())];
        if unsafe { InitializeProcThreadAttributeList(words.as_mut_ptr().cast(), 1, 0, &mut len) }
            == 0
        {
            return Err(Error::last_os_error());
        }
        Ok(Self(words))
    }

    fn ptr(&mut self) -> LPPROC_THREAD_ATTRIBUTE_LIST {
        self.0.as_mut_ptr().cast()
    }
}

impl Drop for Attributes {
    fn drop(&mut self) {
        unsafe { DeleteProcThreadAttributeList(self.ptr()) };
    }
}

impl OwnedChild {
    pub(super) fn spawn(binary: &Path, directory: &Path, args: &[&OsStr]) -> io::Result<Self> {
        // No HANDLE from this launch is inheritable or shared with another run.
        let raw_job = unsafe { CreateJobObjectW(null(), null()) };
        if raw_job.is_null() {
            return Err(Error::last_os_error());
        }
        let job = unsafe { OwnedHandle::from_raw_handle(raw_job) };
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        if unsafe {
            SetInformationJobObject(
                job.as_raw_handle(),
                JobObjectExtendedLimitInformation,
                (&raw const limits).cast(),
                size_of_val(&limits) as u32,
            )
        } == 0
        {
            return Err(Error::last_os_error());
        }

        let jobs: [HANDLE; 1] = [job.as_raw_handle()];
        let mut attributes = Attributes::new()?;
        if unsafe {
            UpdateProcThreadAttribute(
                attributes.ptr(),
                0,
                PROC_THREAD_ATTRIBUTE_JOB_LIST as usize,
                jobs.as_ptr().cast(),
                size_of_val(&jobs),
                null_mut(),
                null(),
            )
        } == 0
        {
            return Err(Error::last_os_error());
        }

        let image = wide(binary.as_os_str())?;
        let working_dir = wide(directory.as_os_str())?;
        let mut cmdline = Vec::new();
        quote(binary.as_os_str(), &mut cmdline)?;
        for arg in args {
            cmdline.push(b' ' as u16);
            quote(arg, &mut cmdline)?;
        }
        cmdline.push(0);
        let mut start = STARTUPINFOEXW::default();
        start.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
        start.lpAttributeList = attributes.ptr();
        let mut info = PROCESS_INFORMATION::default();
        // Atomic assignment: if the inherited job cannot nest this one,
        // CreateProcessW fails rather than running an unowned child.
        if unsafe {
            CreateProcessW(
                image.as_ptr(),
                cmdline.as_mut_ptr(),
                null(),
                null(),
                0,
                EXTENDED_STARTUPINFO_PRESENT | CREATE_NO_WINDOW,
                null(),
                working_dir.as_ptr(),
                &start.StartupInfo,
                &mut info,
            )
        } == 0
        {
            return Err(Error::last_os_error());
        }
        let process = unsafe { OwnedHandle::from_raw_handle(info.hProcess) };
        drop(unsafe { OwnedHandle::from_raw_handle(info.hThread) });
        Ok(Self { job, process })
    }

    pub(super) fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        match unsafe { WaitForSingleObject(self.process.as_raw_handle(), 0) } {
            WAIT_TIMEOUT => Ok(None),
            WAIT_OBJECT_0 => {
                let mut code = 0;
                if unsafe { GetExitCodeProcess(self.process.as_raw_handle(), &mut code) } == 0 {
                    Err(Error::last_os_error())
                } else {
                    Ok(Some(ExitStatus::from_raw(code)))
                }
            }
            _ => Err(Error::last_os_error()),
        }
    }

    pub(super) fn stop(&mut self) -> io::Result<()> {
        if unsafe { TerminateJobObject(self.job.as_raw_handle(), 1) } == 0 {
            return Err(Error::last_os_error());
        }
        match unsafe { WaitForSingleObject(self.process.as_raw_handle(), 2000) } {
            WAIT_OBJECT_0 => Ok(()),
            WAIT_TIMEOUT => Err(Error::new(io::ErrorKind::TimedOut, "Meow did not stop")),
            _ => Err(Error::last_os_error()),
        }
    }
}

fn wide(value: &OsStr) -> io::Result<Vec<u16>> {
    let mut out: Vec<u16> = value.encode_wide().collect();
    if out.contains(&0) {
        return Err(Error::new(
            io::ErrorKind::InvalidInput,
            "NUL in process argument",
        ));
    }
    out.push(0);
    Ok(out)
}

fn quote(value: &OsStr, out: &mut Vec<u16>) -> io::Result<()> {
    // Quote every field, doubling the backslashes before quotes and at the end.
    let (quote, slash) = (b'"' as u16, b'\\' as u16);
    out.push(quote);
    let mut slashes = 0;
    for c in value.encode_wide() {
        if c == 0 {
            return Err(Error::new(
                io::ErrorKind::InvalidInput,
                "NUL in process argument",
            ));
        }
        if c == slash {
            slashes += 1;
            continue;
        }
        out.extend(std::iter::repeat_n(
            slash,
            if c == quote { 2 * slashes + 1 } else { slashes },
        ));
        slashes = 0;
        out.push(c);
    }
    out.extend(std::iter::repeat_n(slash, 2 * slashes));
    out.push(quote);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "helper for killing a session owner; not a standalone test"]
    fn crash_fixture_owner() {
        assert_eq!(std::env::var("PAW_CRASH_FIXTURE").as_deref(), Ok("owner"));
        let binary = std::env::current_exe().unwrap();
        let dir = std::env::current_dir().unwrap();
        let mut child = OwnedChild::spawn(
            &binary,
            &dir,
            &[
                OsStr::new("--exact"),
                OsStr::new("engine::session::windows::tests::crash_fixture_child"),
                OsStr::new("--ignored"),
            ],
        )
        .unwrap();
        let pid = unsafe {
            windows_sys::Win32::System::Threading::GetProcessId(child.process.as_raw_handle())
        };
        std::fs::write("owned.pid", pid.to_string()).unwrap();
        assert!(child.try_wait().unwrap().is_none());
        std::thread::sleep(std::time::Duration::from_secs(15));
    }

    #[test]
    #[ignore = "helper for killing a session owner; not a standalone test"]
    fn crash_fixture_child() {
        assert_eq!(std::env::var("PAW_CRASH_FIXTURE").as_deref(), Ok("owner"));
        std::fs::write("started", "").unwrap();
        std::thread::sleep(std::time::Duration::from_secs(15));
    }

    #[test]
    fn windows_kills_owned_child_after_forced_owner_exit() {
        use std::{
            process::{Command, Stdio},
            thread::sleep,
            time::{Duration, Instant},
        };
        use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_SYNCHRONIZE};

        let dir = tempfile::tempdir().unwrap();
        let mut owner = Command::new(std::env::current_exe().unwrap())
            .current_dir(dir.path())
            .env("PAW_CRASH_FIXTURE", "owner")
            .args([
                "--exact",
                "engine::session::windows::tests::crash_fixture_owner",
                "--ignored",
            ])
            .stdout(Stdio::null())
            .spawn()
            .unwrap();
        let open_owned_child = || {
            let deadline = Instant::now() + Duration::from_secs(5);
            while Instant::now() < deadline {
                if dir.path().join("started").exists()
                    && let Ok(pid) = std::fs::read_to_string(dir.path().join("owned.pid"))
                    && let Ok(pid) = pid.parse()
                {
                    let handle = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
                    if !handle.is_null() {
                        return Some(unsafe { OwnedHandle::from_raw_handle(handle) });
                    }
                }
                sleep(Duration::from_millis(20));
            }
            None
        };
        let child = open_owned_child();
        owner.kill().unwrap();
        owner.wait().unwrap();
        let child = child.expect("owned process did not start");
        assert_eq!(
            unsafe { WaitForSingleObject(child.as_raw_handle(), 2000) },
            WAIT_OBJECT_0
        );
    }

    // This helper holds the Session (and its job and process handles) until the
    // parent closes stdin. The parent instead kills it for the actual test.
    #[test]
    #[ignore = "supervisor helper; run the real-session owner-exit test instead"]
    fn real_session_owner_fixture() {
        use crate::{engine::session::Session, profile::Profile};
        use std::io::Read;

        assert_eq!(
            std::env::var("PAW_REAL_SESSION_FIXTURE").as_deref(),
            Ok("owner")
        );
        let dir = std::path::PathBuf::from(std::env::var_os("PAW_FIXTURE_DIR").unwrap());
        let binary = std::path::PathBuf::from(std::env::var_os("MEOW_BIN").unwrap());
        let profile = Profile::parse("mode: direct\nrules: ['MATCH,DIRECT']\n").unwrap();
        let session = match Session::launch_with_mixed_port(&binary, &profile, 0, || false) {
            Ok(session) => session,
            Err(error) => {
                std::fs::write(dir.join("launch-error"), error.to_string()).unwrap();
                panic!("Session::launch failed: {error}");
            }
        };
        // The handle value belongs to this process. The parent duplicates it
        // using the retained supervisor process handle before terminating us.
        let ready = format!(
            "{} {} {}",
            session.process.process.as_raw_handle() as usize,
            session.mixed_port,
            session.controller_addr.port()
        );
        std::fs::write(dir.join("ready.tmp"), ready).unwrap();
        std::fs::rename(dir.join("ready.tmp"), dir.join("ready")).unwrap();
        let mut byte = [0];
        let _ = std::io::stdin().read(&mut byte);
        session.stop().expect("orderly fixture shutdown failed");
    }

    #[test]
    #[ignore = "requires an explicitly supplied trusted MEOW_BIN on Windows"]
    fn windows_real_session_dies_after_abrupt_owner_exit() {
        use std::{
            net::{Ipv4Addr, SocketAddr, TcpStream},
            os::windows::process::CommandExt,
            path::PathBuf,
            process::{Child, Command, Stdio},
            thread::sleep,
            time::{Duration, Instant},
        };
        use windows_sys::Win32::{
            Foundation::{DUPLICATE_SAME_ACCESS, DuplicateHandle},
            System::Threading::{GetCurrentProcess, TerminateProcess},
        };

        // On any failure, first request an orderly shutdown through stdin, then
        // kill only our supervisor if it does not exit. If necessary, terminate
        // only the core identified by the handle duplicated from that owner.
        struct Fixture {
            owner: Child,
            core: Option<OwnedHandle>,
        }
        impl Drop for Fixture {
            fn drop(&mut self) {
                drop(self.owner.stdin.take());
                let deadline = Instant::now() + Duration::from_secs(3);
                while Instant::now() < deadline {
                    if matches!(self.owner.try_wait(), Ok(Some(_))) {
                        break;
                    }
                    sleep(Duration::from_millis(20));
                }
                if !matches!(self.owner.try_wait(), Ok(Some(_))) {
                    let _ = self.owner.kill();
                    if unsafe { WaitForSingleObject(self.owner.as_raw_handle(), 3000) }
                        == WAIT_OBJECT_0
                    {
                        let _ = self.owner.wait();
                    }
                }
                if let Some(core) = &self.core
                    && unsafe { WaitForSingleObject(core.as_raw_handle(), 3000) } == WAIT_TIMEOUT
                {
                    unsafe { TerminateProcess(core.as_raw_handle(), 1) };
                    unsafe { WaitForSingleObject(core.as_raw_handle(), 3000) };
                }
            }
        }

        let binary = PathBuf::from(std::env::var_os("MEOW_BIN").expect("set trusted MEOW_BIN"))
            .canonicalize()
            .expect("MEOW_BIN must identify an existing binary");
        let dir = tempfile::tempdir().unwrap();
        let temp = dir.path().join("temp");
        std::fs::create_dir(&temp).unwrap();
        let owner = Command::new(std::env::current_exe().unwrap())
            .current_dir(dir.path())
            .env("PAW_REAL_SESSION_FIXTURE", "owner")
            .env("PAW_FIXTURE_DIR", dir.path())
            .env("MEOW_BIN", binary)
            .env("TMP", &temp)
            .env("TEMP", &temp)
            .args([
                "--exact",
                "engine::session::windows::tests::real_session_owner_fixture",
                "--ignored",
            ])
            .creation_flags(CREATE_NO_WINDOW)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let mut fixture = Fixture { owner, core: None };
        // Launch includes capability detection, configuration preflight and
        // listener startup, each with a production six-second deadline.
        let deadline = Instant::now() + Duration::from_secs(30);
        let ready = loop {
            if let Ok(ready) = std::fs::read_to_string(dir.path().join("ready")) {
                break ready;
            }
            if let Ok(error) = std::fs::read_to_string(dir.path().join("launch-error")) {
                panic!("real Session::launch failed (including production preflight): {error}");
            }
            if let Some(status) = fixture.owner.try_wait().unwrap() {
                panic!("real Session supervisor exited before ready: {status}");
            }
            assert!(
                Instant::now() < deadline,
                "real Session supervisor did not become ready within 30 seconds"
            );
            sleep(Duration::from_millis(20));
        };
        let mut fields = ready.split_whitespace();
        let source_handle: usize = fields.next().unwrap().parse().unwrap();
        let mixed: u16 = fields.next().unwrap().parse().unwrap();
        let controller: u16 = fields.next().unwrap().parse().unwrap();
        assert!(
            fields.next().is_none()
                && source_handle != 0
                && mixed != 0
                && controller != 0
                && mixed != controller
        );
        let mut duplicated = null_mut();
        assert_ne!(
            unsafe {
                DuplicateHandle(
                    fixture.owner.as_raw_handle(),
                    source_handle as HANDLE,
                    GetCurrentProcess(),
                    &mut duplicated,
                    0,
                    0,
                    DUPLICATE_SAME_ACCESS,
                )
            },
            0,
            "duplicating the real core handle failed: {}",
            Error::last_os_error()
        );
        fixture.core = Some(unsafe { OwnedHandle::from_raw_handle(duplicated) });
        let core = fixture.core.as_ref().unwrap();
        assert_eq!(
            unsafe { WaitForSingleObject(core.as_raw_handle(), 0) },
            WAIT_TIMEOUT,
            "real core exited before owner termination"
        );
        let reachable = |port| {
            let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
            TcpStream::connect_timeout(&addr, Duration::from_millis(200)).is_ok()
        };
        assert!(reachable(mixed), "mixed listener was not reachable");
        assert!(
            reachable(controller),
            "controller listener was not reachable"
        );

        fixture.owner.kill().expect("kill test-owned supervisor");
        assert_eq!(
            unsafe { WaitForSingleObject(fixture.owner.as_raw_handle(), 5000) },
            WAIT_OBJECT_0,
            "supervisor did not exit"
        );
        fixture.owner.wait().unwrap();
        assert_eq!(
            unsafe { WaitForSingleObject(core.as_raw_handle(), 5000) },
            WAIT_OBJECT_0,
            "job did not terminate the owned real core"
        );
        let deadline = Instant::now() + Duration::from_secs(5);
        while (reachable(mixed) || reachable(controller)) && Instant::now() < deadline {
            sleep(Duration::from_millis(20));
        }
        assert!(!reachable(mixed), "mixed listener remained reachable");
        assert!(
            !reachable(controller),
            "controller listener remained reachable"
        );
    }

    #[test]
    fn child_arguments_preserve_spaces_quotes_and_trailing_slashes() {
        for (arg, expected) in [
            (r"", r#""""#),
            (r"C:\a b\", r#""C:\a b\\""#),
            (r#"a\"b"#, r#""a\\\"b""#),
            ("测试", r#""测试""#),
        ] {
            let mut out = Vec::new();
            quote(OsStr::new(arg), &mut out).unwrap();
            assert_eq!(String::from_utf16(&out).unwrap(), expected);
        }
        assert!(quote(OsStr::new("a\0b"), &mut Vec::new()).is_err());
    }
}
