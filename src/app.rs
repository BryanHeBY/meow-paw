//! Window-independent application state for the first managed desktop workflow.

use crate::{
    engine::{
        controller::{Connection, Inventory, Traffic},
        session::Session,
    },
    profile::{Diagnostic, MAX_PROFILE_BYTES, Profile},
};
use std::{
    fs::File,
    io::{self, Error, ErrorKind, Read},
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

#[derive(Clone, Debug, Default)]
pub enum EngineStatus {
    #[default]
    Stopped,
    Starting,
    Running {
        mixed_port: u16,
    },
    Stopping,
    Error(String),
}

#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub revision: u64,
    pub engine: EngineStatus,
    pub profile: Option<PathBuf>,
    pub diagnostics: Vec<Diagnostic>,
    pub inventory: Option<Arc<Inventory>>,
    pub traffic: Option<Traffic>,
    pub connections: Option<Arc<Vec<Connection>>>,
    pub last_update: Option<Instant>,
    pub action_error: Option<String>,
}

enum Command {
    Start(PathBuf),
    Select { group: String, member: String },
}

pub struct Desktop {
    command: SyncSender<Command>,
    snapshot: Arc<Mutex<Snapshot>>,
    stop: Arc<AtomicBool>,
    quit: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl Desktop {
    pub fn new(meow: PathBuf) -> io::Result<Self> {
        let (command, receiver) = mpsc::sync_channel(8);
        let snapshot = Arc::new(Mutex::new(Snapshot::default()));
        let stop = Arc::new(AtomicBool::new(false));
        let quit = Arc::new(AtomicBool::new(false));
        let worker = {
            let (snapshot, stop, quit) = (snapshot.clone(), stop.clone(), quit.clone());
            thread::Builder::new()
                .name("paw-meow-owner".into())
                .spawn(move || {
                    run(meow, receiver, &snapshot, &stop, &quit);
                })?
        };
        Ok(Self {
            command,
            snapshot,
            stop,
            quit,
            worker: Some(worker),
        })
    }

    pub fn snapshot(&self) -> Snapshot {
        self.snapshot.lock().expect("snapshot poisoned").clone()
    }

    pub fn start(&self, path: PathBuf) -> io::Result<()> {
        self.send(Command::Start(path))
    }

    pub fn select(&self, group: String, member: String) -> io::Result<()> {
        self.send(Command::Select { group, member })
    }

    pub fn stop(&self) {
        self.stop.store(true, Ordering::Release);
    }

    fn send(&self, command: Command) -> io::Result<()> {
        self.command.try_send(command).map_err(|error| match error {
            mpsc::TrySendError::Full(_) => {
                Error::new(ErrorKind::WouldBlock, "Meow command queue is busy")
            }
            mpsc::TrySendError::Disconnected(_) => {
                Error::new(ErrorKind::BrokenPipe, "Meow worker stopped")
            }
        })
    }
}

impl Drop for Desktop {
    fn drop(&mut self) {
        self.quit.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn run(
    binary: PathBuf,
    receiver: Receiver<Command>,
    snapshot: &Mutex<Snapshot>,
    stop: &AtomicBool,
    quit: &AtomicBool,
) {
    let mut session: Option<Session> = None;
    let mut next_sample = Instant::now();
    let cancelled = || stop.load(Ordering::Acquire) || quit.load(Ordering::Acquire);
    loop {
        if quit.load(Ordering::Acquire) {
            break;
        }
        if stop.swap(false, Ordering::AcqRel) {
            // Retire already queued actions from the old session.
            while receiver.try_recv().is_ok() {}
            if let Some(active) = session.take() {
                change(snapshot, |s| s.engine = EngineStatus::Stopping);
                let result = active.stop();
                change(snapshot, |s| {
                    s.engine = match result {
                        Ok(()) => EngineStatus::Stopped,
                        Err(error) => EngineStatus::Error(error.to_string()),
                    }
                });
            } else {
                change(snapshot, |s| s.engine = EngineStatus::Stopped);
            }
            clear_telemetry(snapshot);
            continue;
        }

        if let Some(active) = session.as_mut() {
            match active.status() {
                Ok(None) => {}
                Ok(Some(code)) => {
                    session.take();
                    change(snapshot, |s| {
                        s.engine = EngineStatus::Error(format!("Meow exited ({code})"))
                    });
                    clear_telemetry(snapshot);
                }
                Err(error) => {
                    session.take();
                    change(snapshot, |s| {
                        s.engine = EngineStatus::Error(error.to_string())
                    });
                    clear_telemetry(snapshot);
                }
            }
        }

        match receiver.recv_timeout(Duration::from_millis(75)) {
            Ok(_) if cancelled() => continue,
            Ok(Command::Start(path)) => {
                if session.is_some() {
                    change(snapshot, |s| {
                        s.action_error =
                            Some("Stop the current profile before starting another".into())
                    });
                    continue;
                }
                change(snapshot, |s| {
                    s.engine = EngineStatus::Starting;
                    s.profile = Some(path.clone());
                    s.diagnostics.clear();
                    s.action_error = None;
                });
                let result = load_profile(&path).and_then(|profile| {
                    change(snapshot, |s| s.diagnostics = profile.diagnostics());
                    Session::launch(&binary, &profile, cancelled)
                });
                match result {
                    Ok(active) => {
                        change(snapshot, |s| {
                            s.engine = EngineStatus::Running {
                                mixed_port: active.mixed_port,
                            }
                        });
                        session = Some(active);
                        next_sample = Instant::now();
                    }
                    Err(error) if error.kind() == ErrorKind::Interrupted => {}
                    Err(error) => change(snapshot, |s| {
                        s.engine = EngineStatus::Error(error.to_string())
                    }),
                }
            }
            Ok(Command::Select { group, member }) => {
                if let Some(active) = &session {
                    let result = active.controller.select(&group, &member);
                    change(snapshot, |s| {
                        s.action_error = result.err().map(|e| e.to_string())
                    });
                    next_sample = Instant::now();
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }

        if let Some(active) = &session
            && Instant::now() >= next_sample
            && !cancelled()
        {
            let inventory = active.controller.inventory().ok().map(Arc::new);
            if cancelled() {
                continue;
            }
            let connections = active.controller.connections().ok().map(Arc::new);
            if cancelled() {
                continue;
            }
            let traffic = active.controller.traffic().ok();
            if cancelled() {
                continue;
            }
            change(snapshot, |s| {
                s.last_update = (inventory.is_some() || connections.is_some() || traffic.is_some())
                    .then(Instant::now);
                s.inventory = inventory;
                s.connections = connections;
                s.traffic = traffic;
            });
            next_sample = Instant::now() + Duration::from_millis(250);
        }
    }
}

fn clear_telemetry(snapshot: &Mutex<Snapshot>) {
    change(snapshot, |s| {
        s.inventory = None;
        s.connections = None;
        s.traffic = None;
        s.last_update = None;
    });
}

fn change(snapshot: &Mutex<Snapshot>, apply: impl FnOnce(&mut Snapshot)) {
    let mut snapshot = snapshot.lock().expect("snapshot poisoned");
    apply(&mut snapshot);
    snapshot.revision = snapshot.revision.wrapping_add(1);
}

fn load_profile(path: &PathBuf) -> io::Result<Profile> {
    let file = File::open(path)?;
    if !file.metadata()?.is_file() {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "Select a regular YAML file",
        ));
    }
    let mut source = String::new();
    file.take(MAX_PROFILE_BYTES as u64 + 1)
        .read_to_string(&mut source)?;
    Profile::parse(&source).map_err(Error::other)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bad_profile_reports_a_revision_without_launching_an_engine() {
        let d = tempfile::tempdir().unwrap();
        let profile = d.path().join("bad.yaml");
        std::fs::write(&profile, "- not-a-profile").unwrap();
        let app = Desktop::new(d.path().join("does-not-exist")).unwrap();
        app.start(profile).unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        let state = loop {
            let state = app.snapshot();
            if matches!(state.engine, EngineStatus::Error(_)) || Instant::now() >= deadline {
                break state;
            }
            thread::sleep(Duration::from_millis(10));
        };
        assert!(matches!(state.engine, EngineStatus::Error(_)));
        assert!(state.revision >= 2);
        assert!(state.traffic.is_none());
    }
}
