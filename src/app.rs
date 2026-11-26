//! Window-independent application state for the first managed desktop workflow.

use crate::subscription_store::{SubscriptionId, SubscriptionStore};
use crate::{
    engine::{
        controller::{Connection, Inventory, LogEntry, Origin, RoutingMode, Rule, Traffic},
        session::{ApplyError, Session},
    },
    profile::{Diagnostic, MAX_PROFILE_BYTES, Profile},
};
use std::{
    collections::VecDeque,
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

pub const TRAFFIC_WINDOW_SECS: usize = 40;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TrafficRate {
    pub up: u64,
    pub down: u64,
}

#[derive(Clone, Debug, Default)]
pub struct TrafficHistory {
    // One bucket per elapsed second. Missing samples stay unknown, not zero.
    entries: VecDeque<Option<TrafficRate>>,
    origin: Option<Instant>,
    last_second: u64,
}

impl TrafficHistory {
    pub fn recent(&self) -> impl ExactSizeIterator<Item = Option<TrafficRate>> + '_ {
        self.entries.iter().copied()
    }

    fn record(&mut self, now: Instant, traffic: Option<TrafficRate>) {
        let origin = *self.origin.get_or_insert(now);
        let second = now.saturating_duration_since(origin).as_secs();
        if second == self.last_second
            && let Some(current) = self.entries.back_mut()
        {
            if traffic.is_some() {
                *current = traffic;
            }
            return;
        }
        for _ in 0..second
            .saturating_sub(self.last_second + 1)
            .min(TRAFFIC_WINDOW_SECS as u64)
        {
            self.entries.push_back(None);
        }
        self.entries.push_back(traffic);
        while self.entries.len() > TRAFFIC_WINDOW_SECS {
            self.entries.pop_front();
        }
        self.last_second = second;
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProxyProbeTarget {
    pub name: String,
    pub origin: Origin,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProxyProbeStatus {
    Running,
    Measured(u32),
    Unavailable,
}

pub fn parse_probe_url(value: &str) -> Option<url::Url> {
    let value = value.trim();
    let (scheme, remainder) = value.split_once("://")?;
    if !scheme.eq_ignore_ascii_case("https")
        || remainder.starts_with(['/', '\\'])
        || remainder.split(['/', '?', '#']).next()?.contains('@')
    {
        return None;
    }
    let url = url::Url::parse(value).ok()?;
    (url.scheme() == "https"
        && url.host_str().is_some()
        && url.username().is_empty()
        && url.password().is_none()
        && url.fragment().is_none())
    .then_some(url)
}

pub fn unique_proxy(inventory: &Inventory, name: &str) -> Option<Origin> {
    if inventory.groups.contains_key(name) {
        return None;
    }
    let mut candidates = inventory.nodes.iter().filter(|node| node.name == name);
    let first = candidates.next()?;
    candidates.next().is_none().then(|| first.origin.clone())
}

#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub revision: u64,
    pub engine: EngineStatus,
    pub profile: Option<PathBuf>,
    pub applying_profile: Option<PathBuf>,
    pub diagnostics: Vec<Diagnostic>,
    pub inventory: Option<Arc<Inventory>>,
    pub mode: Option<RoutingMode>,
    pub rules: Option<Arc<Vec<Rule>>>,
    pub logs: Option<LogHistory>,
    pub traffic: Option<Traffic>,
    pub traffic_history: TrafficHistory,
    pub connections: Option<Arc<Vec<Connection>>>,
    pub last_update: Option<Instant>,
    pub action_error: Option<String>,
    pub refreshing_provider: Option<String>,
    pub provider_error: Option<String>,
    pub probe_epoch: u64,
    pub probe: Option<(ProxyProbeTarget, ProxyProbeStatus)>,
}

const LOG_CAPACITY: usize = 256;

#[derive(Clone, Debug, Default)]
pub struct LogHistory(Arc<Mutex<VecDeque<LogEntry>>>);

impl LogHistory {
    pub fn recent(&self) -> Vec<LogEntry> {
        self.0
            .lock()
            .expect("log history poisoned")
            .iter()
            .rev()
            .cloned()
            .collect()
    }

    pub fn clear(&self) {
        self.0.lock().expect("log history poisoned").clear();
    }

    fn push(&self, entry: LogEntry) {
        let mut history = self.0.lock().expect("log history poisoned");
        if history.len() == LOG_CAPACITY {
            history.pop_front();
        }
        history.push_back(entry);
    }
}

struct LogWorker {
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl LogWorker {
    fn start(controller: crate::engine::controller::Controller, history: LogHistory) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let worker = thread::Builder::new()
            .name("paw-meow-logs".into())
            .spawn({
                let stop = stop.clone();
                move || {
                    let cancelled = || stop.load(Ordering::Acquire);
                    while !cancelled() {
                        let result =
                            controller.read_log_stream(|entry| history.push(entry), cancelled);
                        if matches!(
                            result,
                            Err(crate::engine::controller::ControllerError::Status(404))
                        ) {
                            break;
                        }
                        if !cancelled() {
                            thread::park_timeout(Duration::from_millis(250));
                        }
                    }
                }
            })
            .ok();
        Self { stop, worker }
    }
}

impl Drop for LogWorker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            worker.thread().unpark();
            let _ = worker.join();
        }
    }
}

enum ProfileInput {
    Local(PathBuf),
    Subscription {
        store: SubscriptionStore,
        id: SubscriptionId,
    },
}

impl ProfileInput {
    fn key(&self) -> PathBuf {
        match self {
            Self::Local(path) => path.clone(),
            Self::Subscription { store, id } => store.record_path(*id),
        }
    }

    fn load(&self) -> io::Result<Profile> {
        let profile = match self {
            Self::Local(path) => load_profile(path),
            Self::Subscription { store, id } => {
                let record = store.read(*id)?;
                Profile::parse(record.yaml()).map_err(Error::other)
            }
        }?;
        Ok(profile.with_cache_scope(&self.key()))
    }
}

enum Command {
    Start(ProfileInput),
    ApplyProfile(ProfileInput),
    Select {
        group: String,
        member: String,
    },
    SetMode(RoutingMode),
    ReadRules,
    RefreshProvider(String),
    ProbeProxy {
        epoch: u64,
        target: ProxyProbeTarget,
        url: url::Url,
    },
    CloseConnection(String),
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
        self.send(Command::Start(ProfileInput::Local(path)))
    }

    pub fn apply_profile(&self, path: PathBuf) -> io::Result<()> {
        self.send(Command::ApplyProfile(ProfileInput::Local(path)))
    }

    pub fn start_subscription(
        &self,
        store: SubscriptionStore,
        id: SubscriptionId,
    ) -> io::Result<()> {
        self.send(Command::Start(ProfileInput::Subscription { store, id }))
    }

    pub fn apply_subscription(
        &self,
        store: SubscriptionStore,
        id: SubscriptionId,
    ) -> io::Result<()> {
        self.send(Command::ApplyProfile(ProfileInput::Subscription {
            store,
            id,
        }))
    }

    pub fn select(&self, group: String, member: String) -> io::Result<()> {
        self.send(Command::Select { group, member })
    }

    pub fn set_mode(&self, mode: RoutingMode) -> io::Result<()> {
        self.send(Command::SetMode(mode))
    }

    pub fn probe_proxy(&self, epoch: u64, target: ProxyProbeTarget, value: &str) -> io::Result<()> {
        let url = parse_probe_url(value).ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidInput,
                "Enter a valid HTTPS test URL without credentials or a fragment",
            )
        })?;
        self.send(Command::ProbeProxy { epoch, target, url })
    }

    pub fn refresh_provider(&self, name: String) -> io::Result<()> {
        self.send(Command::RefreshProvider(name))
    }

    pub fn read_rules(&self) -> io::Result<()> {
        self.send(Command::ReadRules)
    }

    pub fn close_connection(&self, id: String) -> io::Result<()> {
        self.send(Command::CloseConnection(id))
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
    let mut logs = None;
    let mut session: Option<Session> = None;
    let mut next_sample = Instant::now();
    // Do not block Stop/Quit on a slow remote provider fetch.
    let (refresh_tx, refresh_rx) = mpsc::channel();
    let mut generation = 0_u64;
    let mut refresh_inflight = false;
    let (probe_tx, probe_rx) = mpsc::channel();
    let mut probe_inflight = false;
    let cancelled = || stop.load(Ordering::Acquire) || quit.load(Ordering::Acquire);
    loop {
        if quit.load(Ordering::Acquire) {
            break;
        }
        if stop.swap(false, Ordering::AcqRel) {
            generation = generation.wrapping_add(1);
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
            logs.take();
            clear_telemetry(snapshot, generation);
            continue;
        }

        if let Some(active) = session.as_mut() {
            match active.status() {
                Ok(None) => {}
                Ok(Some(code)) => {
                    generation = generation.wrapping_add(1);
                    session.take();
                    logs.take();
                    change(snapshot, |s| {
                        s.engine = EngineStatus::Error(format!("Meow exited ({code})"))
                    });
                    clear_telemetry(snapshot, generation);
                }
                Err(error) => {
                    generation = generation.wrapping_add(1);
                    session.take();
                    logs.take();
                    change(snapshot, |s| {
                        s.engine = EngineStatus::Error(error.to_string())
                    });
                    clear_telemetry(snapshot, generation);
                }
            }
        }

        match receiver.recv_timeout(Duration::from_millis(75)) {
            Ok(_) if cancelled() => continue,
            Ok(Command::Start(input)) => {
                let path = input.key();
                if session.is_some() {
                    change(snapshot, |s| {
                        s.action_error =
                            Some("Stop the current profile before starting another".into())
                    });
                    continue;
                }
                generation = generation.wrapping_add(1);
                change(snapshot, |s| {
                    s.engine = EngineStatus::Starting;
                    s.probe_epoch = generation;
                    s.probe = None;
                    s.profile = Some(path.clone());
                    s.diagnostics.clear();
                    s.action_error = None;
                });
                let result = input.load().and_then(|profile| {
                    change(snapshot, |s| s.diagnostics = profile.diagnostics());
                    Session::launch(&binary, &profile, cancelled)
                });
                match result {
                    Ok(active) => {
                        let mode = if cancelled() {
                            None
                        } else {
                            active.controller.mode().ok()
                        };
                        let history = LogHistory::default();
                        logs = Some(LogWorker::start(active.controller.clone(), history.clone()));
                        change(snapshot, |s| {
                            s.logs = Some(history);
                            s.mode = mode;
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
            Ok(Command::ApplyProfile(input)) => {
                // A pending provider response belongs to the previous profile.
                generation = generation.wrapping_add(1);
                change(snapshot, |s| {
                    s.refreshing_provider = None;
                    s.provider_error = None;
                    s.probe_epoch = generation;
                    s.probe = None;
                });
                let path = input.key();
                let Some(active) = &session else {
                    change(snapshot, |s| {
                        s.action_error = Some("Start Meow before applying a live profile".into())
                    });
                    continue;
                };
                change(snapshot, |s| {
                    s.applying_profile = Some(path.clone());
                    s.action_error = None;
                    s.diagnostics.clear();
                });
                let result = input
                    .load()
                    .map_err(ApplyError::Prepare)
                    .and_then(|profile| {
                        change(snapshot, |s| s.diagnostics = profile.diagnostics());
                        active.apply_profile(&profile)
                    });
                let mode = if result.is_ok() && !cancelled() {
                    active.controller.mode().ok()
                } else {
                    None
                };
                change(snapshot, |s| {
                    s.applying_profile = None;
                    match result {
                        Ok(()) => {
                            s.profile = Some(path);
                            s.mode = mode;
                            s.inventory = None;
                            s.rules = None;
                        }
                        Err(error) => {
                            if !error.rejected() {
                                s.profile = None;
                                s.mode = None;
                                s.rules = None;
                            }
                            s.action_error = Some(error.message());
                        }
                    }
                });
                next_sample = Instant::now();
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
            Ok(Command::SetMode(mode)) => {
                if let Some(active) = &session {
                    let result = active.controller.set_mode(mode);
                    let actual = if result.is_ok() && !cancelled() {
                        active.controller.mode().ok().or(Some(mode))
                    } else {
                        None
                    };
                    change(snapshot, |s| {
                        if actual.is_some() {
                            s.mode = actual;
                        }
                        s.action_error = result.err().map(|e| e.to_string());
                    });
                    next_sample = Instant::now();
                }
            }
            Ok(Command::RefreshProvider(name)) => {
                if let Some(active) = &session {
                    let already_busy = snapshot
                        .lock()
                        .expect("snapshot poisoned")
                        .refreshing_provider
                        .is_some();
                    if already_busy || refresh_inflight {
                        change(snapshot, |s| {
                            s.provider_error =
                                Some("A previous provider refresh is still finishing".into());
                        });
                        continue;
                    }
                    let available = snapshot
                        .lock()
                        .expect("snapshot poisoned")
                        .inventory
                        .as_ref()
                        .and_then(|inv| inv.providers.get(&name))
                        .is_some_and(|p| p.vehicle_type.eq_ignore_ascii_case("http"));
                    if !available {
                        change(snapshot, |s| {
                            s.provider_error = Some("Provider is no longer refreshable".into())
                        });
                        continue;
                    }
                    let controller = active.controller.clone();
                    let tx = refresh_tx.clone();
                    let epoch = generation;
                    change(snapshot, |s| {
                        s.refreshing_provider = Some(name.clone());
                        s.provider_error = None;
                    });
                    refresh_inflight = true;
                    if thread::Builder::new()
                        .name("paw-provider-refresh".into())
                        .spawn(move || {
                            let result = controller.refresh_provider(&name);
                            let _ = tx.send((epoch, result));
                        })
                        .is_err()
                    {
                        refresh_inflight = false;
                        change(snapshot, |s| {
                            s.refreshing_provider = None;
                            s.provider_error = Some("Could not start provider refresh".into());
                        });
                    }
                }
            }
            Ok(Command::ProbeProxy { epoch, target, url }) => {
                if let Some(active) = &session
                    && epoch == generation
                    && !probe_inflight
                {
                    let resolved = snapshot
                        .lock()
                        .expect("snapshot poisoned")
                        .inventory
                        .as_ref()
                        .and_then(|inventory| unique_proxy(inventory, &target.name));
                    if resolved != Some(target.origin.clone()) {
                        continue;
                    }
                    let controller = active.controller.clone();
                    let tx = probe_tx.clone();
                    let requested = target.clone();
                    change(snapshot, |s| {
                        s.probe = Some((target.clone(), ProxyProbeStatus::Running))
                    });
                    probe_inflight = true;
                    if thread::Builder::new()
                        .name("paw-proxy-probe".into())
                        .spawn(move || {
                            let status = controller
                                .delay(&requested.origin, &requested.name, &url)
                                .ok()
                                .filter(|ms| *ms > 0)
                                .map_or(ProxyProbeStatus::Unavailable, ProxyProbeStatus::Measured);
                            let _ = tx.send((epoch, requested, status));
                        })
                        .is_err()
                    {
                        probe_inflight = false;
                        change(snapshot, |s| {
                            s.probe = Some((target, ProxyProbeStatus::Unavailable))
                        });
                    }
                }
            }
            Ok(Command::ReadRules) => {
                if let Some(active) = &session {
                    let result = active.controller.rules();
                    change(snapshot, |s| match result {
                        Ok(rules) => {
                            s.rules = Some(Arc::new(rules));
                            s.action_error = None;
                        }
                        Err(error) => s.action_error = Some(error.to_string()),
                    });
                }
            }
            Ok(Command::CloseConnection(id)) => {
                if let Some(active) = &session {
                    let result = active.controller.close_connection(&id);
                    change(snapshot, |s| {
                        s.action_error = result.err().map(|e| e.to_string())
                    });
                    next_sample = Instant::now();
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }

        while let Ok((epoch, result)) = refresh_rx.try_recv() {
            refresh_inflight = false;
            if epoch != generation {
                continue;
            }
            change(snapshot, |s| finish_provider_refresh(s, result));
            next_sample = Instant::now();
        }

        while let Ok((epoch, target, status)) = probe_rx.try_recv() {
            probe_inflight = false;
            change(snapshot, |s| finish_proxy_probe(s, epoch, target, status));
            next_sample = Instant::now();
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
                let now = Instant::now();
                s.last_update = (inventory.is_some() || connections.is_some() || traffic.is_some())
                    .then_some(now);
                s.traffic_history.record(
                    now,
                    traffic.as_ref().map(|t| TrafficRate {
                        up: t.up,
                        down: t.down,
                    }),
                );
                // A temporary controller failure must not erase the last good list.
                retain_inventory(s, inventory);
                s.connections = connections;
                s.traffic = traffic;
            });
            next_sample = Instant::now() + Duration::from_millis(250);
        }
    }
}

fn retain_inventory(snapshot: &mut Snapshot, fresh: Option<Arc<Inventory>>) {
    if fresh.is_some() {
        snapshot.inventory = fresh;
    }
}

fn finish_provider_refresh(
    snapshot: &mut Snapshot,
    result: Result<(), crate::engine::controller::ControllerError>,
) {
    snapshot.refreshing_provider = None;
    snapshot.provider_error = result.err().map(|error| match error {
        crate::engine::controller::ControllerError::Unreachable =>
            "Provider refresh timed out or disconnected; result is unknown. Check its status before retrying.".into(),
        error => format!("Provider refresh failed: {error}"),
    });
}

fn finish_proxy_probe(
    snapshot: &mut Snapshot,
    epoch: u64,
    target: ProxyProbeTarget,
    status: ProxyProbeStatus,
) {
    if epoch == snapshot.probe_epoch
        && snapshot.probe.as_ref() == Some(&(target.clone(), ProxyProbeStatus::Running))
    {
        snapshot.probe = Some((target, status));
    }
}

fn clear_telemetry(snapshot: &Mutex<Snapshot>, generation: u64) {
    change(snapshot, |s| {
        s.probe_epoch = generation;
        s.probe = None;
        s.inventory = None;
        s.refreshing_provider = None;
        s.provider_error = None;
        s.mode = None;
        s.rules = None;
        s.logs = None;
        s.connections = None;
        s.traffic = None;
        s.traffic_history = TrafficHistory::default();
        s.last_update = None;
    });
}

fn change(snapshot: &Mutex<Snapshot>, apply: impl FnOnce(&mut Snapshot)) {
    let mut snapshot = snapshot.lock().expect("snapshot poisoned");
    apply(&mut snapshot);
    snapshot.revision = snapshot.revision.wrapping_add(1);
}

fn load_profile(path: &std::path::Path) -> io::Result<Profile> {
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
    fn proxy_probe_rejects_ambiguous_targets_and_stale_completions() {
        use crate::engine::controller::{Node, Proxy};
        let node = |origin| Node {
            origin,
            name: "Example".into(),
            proxy: Proxy::default(),
        };
        let mut inventory = Inventory::default();
        inventory.nodes.push(node(Origin::Provider("One".into())));
        let origin = unique_proxy(&inventory, "Example").unwrap();
        inventory.nodes.push(node(Origin::Provider("Two".into())));
        assert!(unique_proxy(&inventory, "Example").is_none());
        inventory.nodes.pop();
        inventory.groups.insert("Example".into(), Proxy::default());
        assert!(unique_proxy(&inventory, "Example").is_none());

        let target = ProxyProbeTarget {
            origin,
            name: "Example".into(),
        };
        let mut s = Snapshot {
            probe_epoch: 12,
            probe: Some((target.clone(), ProxyProbeStatus::Running)),
            ..Default::default()
        };
        finish_proxy_probe(&mut s, 11, target.clone(), ProxyProbeStatus::Measured(10));
        assert_eq!(s.probe, Some((target.clone(), ProxyProbeStatus::Running)));
        finish_proxy_probe(&mut s, 12, target.clone(), ProxyProbeStatus::Measured(20));
        assert_eq!(s.probe, Some((target, ProxyProbeStatus::Measured(20))));
    }

    #[test]
    fn probe_urls_are_explicit_https_without_credentials_or_fragments() {
        assert!(parse_probe_url(" https://127.0.0.1:1234/check?x=1 ").is_some());
        for value in [
            "",
            "example.invalid",
            "http://127.0.0.1",
            "https:example.invalid",
            "https://a:b@example.invalid",
            "https://@example.invalid",
            "https://example.invalid/#x",
        ] {
            assert!(parse_probe_url(value).is_none(), "accepted {value}");
        }
    }

    #[test]
    fn failed_provider_refresh_and_lost_sample_preserve_last_inventory() {
        let mut snapshot = Snapshot {
            inventory: Some(Arc::new(Inventory::default())),
            refreshing_provider: Some("one".into()),
            ..Default::default()
        };
        let before = Arc::clone(snapshot.inventory.as_ref().unwrap());
        finish_provider_refresh(
            &mut snapshot,
            Err(crate::engine::controller::ControllerError::Status(503)),
        );
        retain_inventory(&mut snapshot, None);
        assert!(Arc::ptr_eq(&before, snapshot.inventory.as_ref().unwrap()));
        assert!(snapshot.refreshing_provider.is_none());
        assert!(snapshot.provider_error.as_deref().unwrap().contains("503"));
        finish_provider_refresh(
            &mut snapshot,
            Err(crate::engine::controller::ControllerError::Unreachable),
        );
        assert!(
            snapshot
                .provider_error
                .as_deref()
                .unwrap()
                .contains("unknown")
        );
        snapshot.action_error = Some("unrelated error".into());
        finish_provider_refresh(&mut snapshot, Ok(()));
        assert!(snapshot.provider_error.is_none());
        assert_eq!(snapshot.action_error.as_deref(), Some("unrelated error"));
    }

    #[test]
    fn log_history_is_bounded_and_newest_first() {
        let history = LogHistory::default();
        for i in 0..LOG_CAPACITY + 3 {
            history.push(LogEntry {
                time: String::new(),
                level: "info".into(),
                message: i.to_string(),
            });
        }
        let entries = history.recent();
        assert_eq!(entries.len(), LOG_CAPACITY);
        assert_eq!(entries[0].message, (LOG_CAPACITY + 2).to_string());
        assert_eq!(entries[LOG_CAPACITY - 1].message, "3");
        history.clear();
        assert!(history.recent().is_empty());
    }

    #[test]
    fn traffic_history_bounds_gaps_and_preserves_zero() {
        let base = Instant::now();
        let mut series = TrafficHistory::default();
        let rate = TrafficRate { up: 42, down: 7 };
        let zero = TrafficRate { up: 0, down: 0 };
        series.record(base, Some(rate));
        series.record(base + Duration::from_millis(700), None);
        series.record(base + Duration::from_secs(2), Some(zero));
        assert_eq!(
            series.recent().collect::<Vec<_>>(),
            vec![Some(rate), None, Some(zero)]
        );
        series.record(base + Duration::from_secs(200), Some(rate));
        let entries = series.recent().collect::<Vec<_>>();
        assert_eq!(entries.len(), TRAFFIC_WINDOW_SECS);
        assert!(
            entries[..TRAFFIC_WINDOW_SECS - 1]
                .iter()
                .all(Option::is_none)
        );
        assert_eq!(entries.last(), Some(&Some(rate)));
    }

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
