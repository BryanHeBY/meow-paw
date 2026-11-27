//! Window-independent application state for the first managed desktop workflow.

mod system_proxy;
pub use system_proxy::ProxyState;
use system_proxy::{ProxyAction, ProxyControl};

use crate::subscription_store::{SubscriptionId, SubscriptionStore};
use crate::{
    engine::{
        controller::{
            Connection, ControllerError, Inventory, LogEntry, Origin, RoutingMode, Rule,
            RuleProviderStatus, Traffic,
        },
        session::{ApplyError, Session},
    },
    profile::{Diagnostic, MAX_PROFILE_BYTES, Profile},
    unlock,
};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fs::File,
    io::{self, Error, ErrorKind, Read},
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU16, Ordering},
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

pub const CLOSED_CONNECTION_LIMIT: usize = 100;

/// An opaque, in-memory identity for one observed closed connection.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ClosedConnectionKey {
    generation: u64,
    sequence: u128,
}

/// Closed connections observed in the current profile generation, in memory only.
#[derive(Clone, Debug, Default)]
pub struct ConnectionHistory {
    previous: Option<Arc<Vec<Connection>>>,
    closed: Arc<VecDeque<(ClosedConnectionKey, Connection)>>,
    generation: u64,
    next_sequence: u128,
}

impl ConnectionHistory {
    /// Returns the last observed metadata for closed connections, newest first.
    /// Closures detected in the same sample are ordered by descending ID.
    pub fn recent(&self) -> impl ExactSizeIterator<Item = &Connection> + '_ {
        self.closed.iter().rev().map(|(_, connection)| connection)
    }

    /// Returns the same rows and order as `recent`, with stable per-closure keys.
    /// Keys are valid only while the entry remains in this generation's history.
    pub fn recent_keyed(
        &self,
    ) -> impl ExactSizeIterator<Item = (ClosedConnectionKey, &Connection)> + '_ {
        self.closed.iter().rev().map(|(key, row)| (*key, row))
    }

    fn record(&mut self, sample: Option<&Arc<Vec<Connection>>>) {
        let Some(sample) = sample else { return };
        if let Some(previous) = &self.previous {
            let live: BTreeSet<_> = sample.iter().map(|c| c.id.as_str()).collect();
            // A duplicate ID represents one connection; its last row wins.
            let mut disappeared = BTreeMap::new();
            for connection in previous.iter() {
                if !connection.id.is_empty() && !live.contains(connection.id.as_str()) {
                    disappeared.insert(connection.id.as_str(), connection);
                }
            }
            if !disappeared.is_empty() {
                let closed = Arc::make_mut(&mut self.closed);
                for connection in disappeared.into_values() {
                    if closed.len() == CLOSED_CONNECTION_LIMIT {
                        closed.pop_front();
                    }
                    let key = ClosedConnectionKey {
                        generation: self.generation,
                        sequence: self.next_sequence,
                    };
                    self.next_sequence = self
                        .next_sequence
                        .checked_add(1)
                        .expect("closed connection sequence exhausted");
                    closed.push_back((key, connection.clone()));
                }
            }
        }
        self.previous = Some(Arc::clone(sample));
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

const PROBE_RESULT_LIMIT: usize = 128;

#[derive(Clone, Debug, Default)]
pub struct ProxyProbeResults {
    url: Option<url::Url>,
    running: Option<ProxyProbeTarget>,
    settled: Arc<VecDeque<(ProxyProbeTarget, ProxyProbeStatus)>>,
}

impl ProxyProbeResults {
    pub fn is_running(&self) -> bool {
        self.running.is_some()
    }

    pub fn status(&self, url: &url::Url, target: &ProxyProbeTarget) -> Option<ProxyProbeStatus> {
        if self.url.as_ref() != Some(url) {
            return None;
        }
        if self.running.as_ref() == Some(target) {
            return Some(ProxyProbeStatus::Running);
        }
        self.settled
            .iter()
            .find(|(key, _)| key == target)
            .map(|(_, status)| status.clone())
    }

    fn begin(&mut self, url: url::Url, target: ProxyProbeTarget) {
        if self.url.as_ref() != Some(&url) {
            self.url = Some(url);
            self.settled = Arc::default();
        }
        self.running = Some(target);
    }

    fn finish(&mut self, target: ProxyProbeTarget, status: ProxyProbeStatus) {
        if self.running.as_ref() != Some(&target) {
            return;
        }
        self.running = None;
        let rows = Arc::make_mut(&mut self.settled);
        rows.retain(|(key, _)| key != &target);
        if rows.len() >= PROBE_RESULT_LIMIT {
            rows.pop_front();
        }
        rows.push_back((target, status));
    }
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
    if inventory.provider_error.is_some() || inventory.groups.contains_key(name) {
        return None;
    }
    let mut candidates = inventory.nodes.iter().filter(|node| node.name == name);
    let first = candidates.next()?;
    candidates.next().is_none().then(|| first.origin.clone())
}

/// Limits both preparation work and the number of requests a future caller can schedule.
pub const GROUP_PROBE_TARGET_LIMIT: usize = 128;
const GROUP_PROBE_INVENTORY_LIMIT: usize = 4096;
const GROUP_PROBE_MEMBER_LIMIT: usize = 4096;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GroupProbePreparationError {
    ProviderUnavailable,
    MissingGroup,
    InventoryTooLarge,
    MembershipTooLarge,
    TooManyTargets,
}

/// Resolve nested membership in first-seen order. Unknown and ambiguous names are
/// omitted; a group/node name collision is never followed or measured. Limits
/// fail the whole preparation instead of silently returning an incomplete set.
pub fn group_probe_targets(
    inventory: &Inventory,
    group: &str,
) -> Result<Vec<ProxyProbeTarget>, GroupProbePreparationError> {
    if inventory.provider_error.is_some() {
        return Err(GroupProbePreparationError::ProviderUnavailable);
    }
    if !inventory.groups.contains_key(group) {
        return Err(GroupProbePreparationError::MissingGroup);
    }
    if inventory.nodes.len() > GROUP_PROBE_INVENTORY_LIMIT {
        return Err(GroupProbePreparationError::InventoryTooLarge);
    }
    let mut nodes = BTreeMap::new();
    for node in &inventory.nodes {
        nodes
            .entry(node.name.as_str())
            .and_modify(|origin| *origin = None)
            .or_insert(Some(&node.origin));
    }
    let mut visited = BTreeSet::new();
    let mut seen = BTreeSet::new();
    let mut pending = vec![group];
    let mut examined = 0;
    let mut targets = Vec::new();
    while let Some(name) = pending.pop() {
        if !inventory.groups.contains_key(name) {
            if let Some(Some(origin)) = nodes.get(name)
                && seen.insert(name)
            {
                if targets.len() == GROUP_PROBE_TARGET_LIMIT {
                    return Err(GroupProbePreparationError::TooManyTargets);
                }
                targets.push(ProxyProbeTarget {
                    name: name.to_owned(),
                    origin: (*origin).clone(),
                });
            }
            continue;
        }
        if nodes.contains_key(name) || !visited.insert(name) {
            continue;
        }
        let Some(members) = inventory.groups.get(name).and_then(|g| g.all.as_ref()) else {
            continue;
        };
        examined += members.len();
        if examined > GROUP_PROBE_MEMBER_LIMIT {
            return Err(GroupProbePreparationError::MembershipTooLarge);
        }
        // Reversed stack preserves the controller's member order, including nested groups.
        for member in members.iter().rev() {
            pending.push(member);
        }
    }
    Ok(targets)
}

/// Local comparison state only. The caller owns scheduling and must use a new
/// run_id for each run, even when the engine generation and URL are unchanged.
/// Per-node delay checks update engine history and may affect automatic routes.
#[derive(Clone, Debug)]
pub struct GroupProbeResults {
    epoch: u64,
    run_id: u64,
    url: url::Url,
    rows: Vec<(ProxyProbeTarget, Option<ProxyProbeStatus>)>,
}

impl GroupProbeResults {
    pub fn new(
        epoch: u64,
        run_id: u64,
        url: url::Url,
        targets: Vec<ProxyProbeTarget>,
    ) -> Option<Self> {
        if targets.len() > GROUP_PROBE_TARGET_LIMIT
            || parse_probe_url(url.as_str()).as_ref() != Some(&url)
        {
            return None;
        }
        let mut seen = BTreeSet::new();
        if targets.iter().any(|target| !seen.insert(&target.name)) {
            return None;
        }
        Some(Self {
            epoch,
            run_id,
            url,
            rows: targets.into_iter().map(|target| (target, None)).collect(),
        })
    }

    pub fn rows(&self) -> &[(ProxyProbeTarget, Option<ProxyProbeStatus>)] {
        &self.rows
    }

    pub fn url(&self) -> &url::Url {
        &self.url
    }

    /// Only one target may be in flight; completed entries cannot be restarted.
    pub fn begin(&mut self, index: usize) -> bool {
        if self
            .rows
            .iter()
            .any(|(_, status)| *status == Some(ProxyProbeStatus::Running))
        {
            return false;
        }
        let Some((_, status @ None)) = self.rows.get_mut(index) else {
            return false;
        };
        *status = Some(ProxyProbeStatus::Running);
        true
    }

    pub fn finish(
        &mut self,
        epoch: u64,
        run_id: u64,
        index: usize,
        target: &ProxyProbeTarget,
        status: ProxyProbeStatus,
    ) -> bool {
        if epoch != self.epoch || run_id != self.run_id || status == ProxyProbeStatus::Running {
            return false;
        }
        let Some((expected, current)) = self.rows.get_mut(index) else {
            return false;
        };
        if expected != target || *current != Some(ProxyProbeStatus::Running) {
            return false;
        }
        *current = Some(status);
        true
    }
}

#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub revision: u64,
    pub engine: EngineStatus,
    pub system_proxy: ProxyState,
    pub system_proxy_error: Option<String>,
    pub stop_result: Option<bool>,
    pub profile: Option<PathBuf>,
    pub applying_profile: Option<PathBuf>,
    // Submission is reserved before the worker confirms that an apply has begun.
    apply_reservation: Option<ApplyReservation>,
    next_apply_id: u64,
    pub diagnostics: Vec<Diagnostic>,
    pub inventory: Option<Arc<Inventory>>,
    pub inventory_stale: bool,
    pub mode: Option<RoutingMode>,
    pub rules: Option<Arc<Vec<Rule>>>,
    pub rule_providers: Option<Arc<BTreeMap<String, RuleProviderStatus>>>,
    pub rule_provider_error: Option<String>,
    /// A requested read is running or waiting for an older session's read to finish.
    pub rule_provider_pending: bool,
    pub logs: Option<LogHistory>,
    pub traffic: Option<Traffic>,
    pub traffic_history: TrafficHistory,
    pub connections: Option<Arc<Vec<Connection>>>,
    pub connection_history: ConnectionHistory,
    pub last_update: Option<Instant>,
    pub action_error: Option<String>,
    pub refreshing_provider: Option<String>,
    pub provider_error: Option<String>,
    pub probe_epoch: u64,
    pub probes: ProxyProbeResults,
    pub probe_busy: bool,
    pub group_probe: Option<GroupComparison>,
    group_run_id: u64,
    pub unlock: unlock::State,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ApplyReservation {
    id: u64,
    generation: u64,
}

#[derive(Clone, Debug)]
pub struct GroupComparison {
    pub group: String,
    pub results: GroupProbeResults,
    pub active: bool,
    pub interrupted: bool,
    mode: Option<RoutingMode>,
}

impl GroupComparison {
    /// Whether this completed comparison still belongs to the current snapshot.
    pub fn is_current_completed(
        &self,
        snapshot: &Snapshot,
        group: &str,
        url: Option<&url::Url>,
    ) -> bool {
        self.group == group
            && url == Some(self.results.url())
            && !self.active
            && !self.interrupted
            && self.results.epoch == snapshot.probe_epoch
            && self.results.run_id == snapshot.group_run_id
            && self.mode == snapshot.mode
            && self.results.rows.iter().all(|(_, status)| {
                matches!(
                    status,
                    Some(ProxyProbeStatus::Measured(_) | ProxyProbeStatus::Unavailable)
                )
            })
    }

    fn valid_target(&self, snapshot: &Snapshot, index: usize, target: &ProxyProbeTarget) -> bool {
        self.active
            && !snapshot.inventory_stale
            && self.results.epoch == snapshot.probe_epoch
            && self.results.run_id == snapshot.group_run_id
            && parse_probe_url(self.results.url.as_str()).as_ref() == Some(&self.results.url)
            && self.mode == snapshot.mode
            && self
                .results
                .rows
                .get(index)
                .is_some_and(|(t, _)| t == target)
            && snapshot
                .inventory
                .as_ref()
                .and_then(|i| unique_proxy(i, &target.name))
                == Some(target.origin.clone())
    }
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
    SystemProxy(ProxyAction),
    Start(ProfileInput),
    ApplyProfile(ProfileInput, ApplyReservation),
    Select {
        group: String,
        member: String,
    },
    SetMode(RoutingMode),
    ReadRules,
    ReadRuleProviders,
    RefreshProvider(String),
    ProbeProxy {
        epoch: u64,
        target: ProxyProbeTarget,
        url: url::Url,
    },
    RunUnlock(u64, unlock::Job),
    CloseConnection(String),
}

pub struct Desktop {
    command: SyncSender<Command>,
    snapshot: Arc<Mutex<Snapshot>>,
    stop: Arc<AtomicBool>,
    quit: Arc<AtomicBool>,
    next_mixed_port: Arc<AtomicU16>,
    worker: Option<JoinHandle<()>>,
}

impl Desktop {
    pub fn new(meow: PathBuf) -> io::Result<Self> {
        Self::with_mixed_port(meow, crate::engine::session::DEFAULT_MIXED_PORT)
    }

    pub fn with_mixed_port(meow: PathBuf, mixed_port: u16) -> io::Result<Self> {
        Self::with_desktop_proxy(Some(meow), mixed_port, None)
    }

    pub fn with_desktop_proxy(
        meow: Option<PathBuf>,
        mixed_port: u16,
        proxy_dir: Option<PathBuf>,
    ) -> io::Result<Self> {
        let (command, receiver) = mpsc::sync_channel(8);
        let snapshot = Arc::new(Mutex::new(Snapshot::default()));
        let stop = Arc::new(AtomicBool::new(false));
        let quit = Arc::new(AtomicBool::new(false));
        let next_mixed_port = Arc::new(AtomicU16::new(mixed_port));
        let worker = {
            let (snapshot, stop, quit) = (snapshot.clone(), stop.clone(), quit.clone());
            let next_mixed_port = Arc::clone(&next_mixed_port);
            thread::Builder::new()
                .name("paw-meow-owner".into())
                .spawn(move || {
                    let proxy = ProxyControl::discover(proxy_dir.as_deref());
                    run(
                        meow,
                        proxy,
                        &next_mixed_port,
                        receiver,
                        &snapshot,
                        &stop,
                        &quit,
                    );
                })?
        };
        Ok(Self {
            command,
            snapshot,
            stop,
            quit,
            next_mixed_port,
            worker: Some(worker),
        })
    }

    /// Change the port for future starts without touching the running session.
    pub fn set_next_mixed_port(&self, port: std::num::NonZeroU16) {
        self.next_mixed_port.store(port.get(), Ordering::Release);
    }

    pub fn snapshot(&self) -> Snapshot {
        self.snapshot.lock().expect("snapshot poisoned").clone()
    }

    pub fn start(&self, path: PathBuf) -> io::Result<()> {
        self.send(Command::Start(ProfileInput::Local(path)))
    }

    pub fn apply_profile(&self, path: PathBuf) -> io::Result<()> {
        self.submit_apply(ProfileInput::Local(path))
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
        self.submit_apply(ProfileInput::Subscription { store, id })
    }

    pub fn select(&self, group: String, member: String) -> io::Result<()> {
        self.send(Command::Select { group, member })
    }

    pub fn set_mode(&self, mode: RoutingMode) -> io::Result<()> {
        self.invalidate_group_probe();
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

    /// Starts only an explicit comparison; the worker dispatches one request at a time.
    pub fn compare_group(&self, epoch: u64, group: &str, value: &str) -> io::Result<()> {
        let url = parse_probe_url(value)
            .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "Enter a valid HTTPS test URL"))?;
        let mut s = self.snapshot.lock().expect("snapshot poisoned");
        if self.stop.load(Ordering::Acquire)
            || self.quit.load(Ordering::Acquire)
            || s.probe_epoch != epoch
            || !matches!(s.engine, EngineStatus::Running { .. })
            || s.probe_busy
            || s.inventory_stale
            || s.group_probe.as_ref().is_some_and(|g| g.active)
        {
            return Err(Error::new(
                ErrorKind::WouldBlock,
                "Probe is busy or the session changed",
            ));
        }
        let targets = group_probe_targets(
            s.inventory
                .as_deref()
                .ok_or_else(|| Error::other("No proxy inventory"))?,
            group,
        )
        .map_err(|e| Error::other(format!("Cannot prepare comparison: {e:?}")))?;
        if targets.is_empty() {
            return Err(Error::other("No unambiguous nodes to compare"));
        }
        s.group_run_id = s.group_run_id.wrapping_add(1);
        let results = GroupProbeResults::new(epoch, s.group_run_id, url, targets)
            .ok_or_else(|| Error::other("Cannot prepare comparison"))?;
        s.group_probe = Some(GroupComparison {
            group: group.to_owned(),
            results,
            active: true,
            interrupted: false,
            mode: s.mode,
        });
        s.revision = s.revision.wrapping_add(1);
        Ok(())
    }

    pub fn cancel_group_probe(&self) {
        change(&self.snapshot, interrupt_group_probe);
    }

    pub fn invalidate_group_probe(&self) {
        change(&self.snapshot, |s| s.group_probe = None);
    }

    pub fn run_unlock(&self, epoch: u64, group: &str) -> io::Result<()> {
        let mut s = self.snapshot.lock().expect("snapshot poisoned");
        if self.stop.load(Ordering::Acquire)
            || self.quit.load(Ordering::Acquire)
            || s.probe_epoch != epoch
            || !matches!(s.engine, EngineStatus::Running { .. })
            || s.apply_reservation.is_some()
            || s.inventory_stale
        {
            return Err(Error::new(ErrorKind::WouldBlock, "Unlock session changed"));
        }
        let inventory = s
            .inventory
            .as_deref()
            .filter(|inventory| inventory.groups.contains_key(group))
            .ok_or_else(|| {
                Error::new(ErrorKind::NotFound, "Unlock group is no longer available")
            })?;
        let routes = unlock::routes(inventory, group);
        let job = s
            .unlock
            .begin(routes)
            .ok_or_else(|| Error::new(ErrorKind::WouldBlock, "Unlock is already checking"))?;
        let failure = job.without_checks(unlock::Phase::Unavailable);
        let result = self.send(Command::RunUnlock(epoch, job));
        if result.is_err() {
            s.unlock.finish(failure);
        }
        s.revision = s.revision.wrapping_add(1);
        result
    }

    pub fn cancel_unlock(&self) {
        change(&self.snapshot, |s| s.unlock.cancel());
    }

    pub fn refresh_provider(&self, name: String) -> io::Result<()> {
        self.send(Command::RefreshProvider(name))
    }

    pub fn read_rules(&self) -> io::Result<()> {
        self.send(Command::ReadRules)
    }

    pub fn read_rule_providers(&self) -> io::Result<()> {
        self.send(Command::ReadRuleProviders)
    }

    pub fn close_connection(&self, id: String) -> io::Result<()> {
        self.send(Command::CloseConnection(id))
    }

    pub fn enable_system_proxy(&self, consent: crate::system_proxy::Consent) -> io::Result<()> {
        self.send(Command::SystemProxy(ProxyAction::Enable(consent)))
    }

    pub fn restore_system_proxy(&self) -> io::Result<()> {
        self.send(Command::SystemProxy(ProxyAction::Restore))
    }

    pub fn refresh_system_proxy(&self) -> io::Result<()> {
        self.send(Command::SystemProxy(ProxyAction::Refresh))
    }

    pub fn stop(&self) {
        change(&self.snapshot, |s| {
            self.stop.store(true, Ordering::Release);
            s.stop_result = None;
            s.apply_reservation = None;
            s.group_probe = None;
            s.unlock.reset();
            clear_rule_provider_state(s);
        });
    }

    fn submit_apply(&self, input: ProfileInput) -> io::Result<()> {
        let mut s = self.snapshot.lock().expect("snapshot poisoned");
        if self.stop.load(Ordering::Acquire)
            || self.quit.load(Ordering::Acquire)
            || !matches!(s.engine, EngineStatus::Running { .. })
        {
            return Err(Error::new(
                ErrorKind::NotConnected,
                "Start Meow before applying a live profile",
            ));
        }
        if s.apply_reservation.is_some() {
            return Err(Error::new(
                ErrorKind::WouldBlock,
                "A live profile apply is already pending",
            ));
        }
        s.next_apply_id = s.next_apply_id.wrapping_add(1);
        let reservation = ApplyReservation {
            id: s.next_apply_id,
            generation: s.probe_epoch,
        };
        s.apply_reservation = Some(reservation);
        if let Err(error) = self.send(Command::ApplyProfile(input, reservation)) {
            s.apply_reservation = None;
            return Err(error);
        }
        s.group_probe = None;
        s.unlock.reset();
        s.revision = s.revision.wrapping_add(1);
        Ok(())
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
        change(&self.snapshot, |s| {
            self.quit.store(true, Ordering::Release);
            s.apply_reservation = None;
            s.unlock.reset();
        });
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

enum ProbeCompletion {
    Single(u64, ProxyProbeTarget, ProxyProbeStatus),
    Group {
        epoch: u64,
        run_id: u64,
        url: url::Url,
        index: usize,
        target: ProxyProbeTarget,
        status: ProxyProbeStatus,
    },
}

fn run(
    binary: Option<PathBuf>,
    proxy: ProxyControl,
    next_mixed_port: &AtomicU16,
    receiver: Receiver<Command>,
    snapshot: &Mutex<Snapshot>,
    stop: &AtomicBool,
    quit: &AtomicBool,
) {
    proxy.publish(snapshot, None);
    let mut logs = None;
    let mut session: Option<Session> = None;
    let mut next_sample = Instant::now();
    // Do not block Stop/Quit on a slow remote provider fetch.
    let (refresh_tx, refresh_rx) = mpsc::channel();
    let mut generation = 0_u64;
    let mut refresh_inflight = false;
    let (rule_provider_tx, rule_provider_rx) = mpsc::channel();
    let mut rule_provider_inflight = None;
    let (probe_tx, probe_rx) = mpsc::channel();
    let (unlock_tx, unlock_rx) = mpsc::channel();
    let mut probe_inflight = false;
    let cancelled = || stop.load(Ordering::Acquire) || quit.load(Ordering::Acquire);
    loop {
        if quit.load(Ordering::Acquire) {
            break;
        }
        let stopping = {
            let mut s = snapshot.lock().expect("snapshot poisoned");
            if stop.swap(false, Ordering::AcqRel) {
                s.apply_reservation = None;
                s.engine = EngineStatus::Stopping;
                s.revision = s.revision.wrapping_add(1);
                true
            } else {
                false
            }
        };
        if stopping {
            if !proxy.restore_current(snapshot) {
                change(snapshot, |s| {
                    s.stop_result = Some(false);
                    s.action_error = s.system_proxy_error.clone();
                    if let Some(active) = &session {
                        s.engine = EngineStatus::Running {
                            mixed_port: active.mixed_port,
                        };
                    } else {
                        s.engine = EngineStatus::Stopped;
                    }
                });
                continue;
            }
            generation = generation.wrapping_add(1);
            // Retire already queued actions from the old session.
            while let Ok(command) = receiver.try_recv() {
                if let Command::RunUnlock(_, job) = command {
                    change(snapshot, |s| {
                        s.unlock
                            .finish(job.without_checks(unlock::Phase::Cancelled));
                    });
                }
            }
            let engine = if let Some(active) = session.take() {
                match active.stop() {
                    Ok(()) => EngineStatus::Stopped,
                    Err(error) => EngineStatus::Error(error.to_string()),
                }
            } else {
                EngineStatus::Stopped
            };
            logs.take();
            let stopped = matches!(engine, EngineStatus::Stopped);
            clear_telemetry(snapshot, generation, engine);
            change(snapshot, |s| s.stop_result = Some(stopped));
            continue;
        }

        if let Some(active) = session.as_mut() {
            match active.status() {
                Ok(None) => {}
                Ok(Some(code)) => {
                    proxy.restore_current(snapshot);
                    generation = generation.wrapping_add(1);
                    session.take();
                    logs.take();
                    clear_telemetry(
                        snapshot,
                        generation,
                        EngineStatus::Error(format!("Meow exited ({code})")),
                    );
                }
                Err(error) => {
                    proxy.restore_current(snapshot);
                    generation = generation.wrapping_add(1);
                    session.take();
                    logs.take();
                    clear_telemetry(snapshot, generation, EngineStatus::Error(error.to_string()));
                }
            }
        }

        match receiver.recv_timeout(Duration::from_millis(75)) {
            Ok(Command::RunUnlock(epoch, job)) => {
                let result = match &session {
                    Some(active) if epoch == generation && !cancelled() => {
                        job.dispatch(active.controller.unlock_client(), unlock_tx.clone())
                    }
                    _ => Err(job.without_checks(unlock::Phase::Unavailable)),
                };
                if let Err(completion) = result {
                    change(snapshot, |s| s.unlock.finish(completion));
                }
            }
            Ok(_) if cancelled() => continue,
            Ok(Command::SystemProxy(action)) => {
                proxy.handle(action, snapshot, || {
                    let active = session.as_mut()?;
                    active.ready_mixed_listener().filter(|_| !cancelled())
                });
            }
            Ok(Command::Start(input)) => {
                let Some(binary) = &binary else {
                    change(snapshot, |s| {
                        s.action_error = Some("Choose a Meow executable before starting.".into())
                    });
                    continue;
                };
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
                    s.probes = ProxyProbeResults::default();
                    s.group_probe = None;
                    clear_rule_provider_state(s);
                    clear_connection_history(s);
                    s.profile = Some(path.clone());
                    s.diagnostics.clear();
                    s.action_error = None;
                });
                let mixed_port = next_mixed_port.load(Ordering::Acquire);
                let result = input.load().and_then(|profile| {
                    change(snapshot, |s| s.diagnostics = profile.diagnostics());
                    Session::launch_with_mixed_port(binary, &profile, mixed_port, cancelled)
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
            Ok(Command::ApplyProfile(input, reservation)) => {
                let path = input.key();
                {
                    let mut s = snapshot.lock().expect("snapshot poisoned");
                    if !begin_apply(
                        &mut s,
                        reservation,
                        generation,
                        session.is_some(),
                        cancelled(),
                        &path,
                    ) {
                        continue;
                    }
                }
                // A pending provider response belongs to the previous profile.
                generation = generation.wrapping_add(1);
                change(snapshot, |s| {
                    s.refreshing_provider = None;
                    s.provider_error = None;
                    clear_rule_provider_state(s);
                    s.probe_epoch = generation;
                    s.probes = ProxyProbeResults::default();
                    s.group_probe = None;
                    clear_connection_history(s);
                });
                let active = session.as_ref().expect("apply requires a session");
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
                    finish_apply(s, reservation, path, mode, result);
                });
                next_sample = Instant::now();
            }
            Ok(Command::Select { group, member }) => {
                if let Some(active) = &session {
                    if snapshot.lock().expect("snapshot poisoned").inventory_stale {
                        change(snapshot, |s| {
                            s.action_error = Some("Route inventory is stale".into())
                        });
                        continue;
                    }
                    let result = active.controller.select(&group, &member);
                    change(snapshot, |s| {
                        s.action_error = result.err().map(|e| e.to_string())
                    });
                    next_sample = Instant::now();
                }
            }
            Ok(Command::SetMode(mode)) => {
                change(snapshot, |s| s.group_probe = None);
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
                    let available = {
                        let s = snapshot.lock().expect("snapshot poisoned");
                        !s.inventory_stale
                            && s.inventory
                                .as_ref()
                                .filter(|inv| inv.provider_error.is_none())
                                .and_then(|inv| inv.providers.get(&name))
                                .is_some_and(|p| p.vehicle_type.eq_ignore_ascii_case("http"))
                    };
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
                    let mut s = snapshot.lock().expect("snapshot poisoned");
                    let resolved = s
                        .inventory
                        .as_ref()
                        .and_then(|inventory| unique_proxy(inventory, &target.name));
                    if s.group_probe.as_ref().is_some_and(|g| g.active)
                        || cancelled()
                        || s.inventory_stale
                        || resolved != Some(target.origin.clone())
                    {
                        continue;
                    }
                    let controller = active.controller.clone();
                    let tx = probe_tx.clone();
                    let requested = target.clone();
                    s.probes.begin(url.clone(), target.clone());
                    s.probe_busy = true;
                    probe_inflight = true;
                    if thread::Builder::new()
                        .name("paw-proxy-probe".into())
                        .spawn(move || {
                            let status = controller
                                .delay(&requested.origin, &requested.name, &url)
                                .ok()
                                .filter(|ms| *ms > 0)
                                .map_or(ProxyProbeStatus::Unavailable, ProxyProbeStatus::Measured);
                            let _ = tx.send(ProbeCompletion::Single(epoch, requested, status));
                        })
                        .is_err()
                    {
                        probe_inflight = false;
                        s.probe_busy = false;
                        s.probes.finish(target, ProxyProbeStatus::Unavailable);
                    }
                    s.revision = s.revision.wrapping_add(1);
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
            Ok(Command::ReadRuleProviders) => {
                if session.is_some() {
                    let mut s = snapshot.lock().expect("snapshot poisoned");
                    if !cancelled() && begin_rule_provider_read(&mut s, generation) {
                        s.revision = s.revision.wrapping_add(1);
                    }
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

        while let Ok(completion) = unlock_rx.try_recv() {
            change(snapshot, |s| s.unlock.finish(completion));
        }

        while let Ok((epoch, result)) = rule_provider_rx.try_recv() {
            if release_rule_provider_read(&mut rule_provider_inflight, epoch) && !cancelled() {
                change(snapshot, |s| {
                    finish_rule_provider_read(s, epoch, result);
                });
            }
        }

        // Keep at most one read worker, including across profile changes. A read
        // requested for the current profile waits here for the old worker to exit.
        if let Some(active) = &session
            && rule_provider_inflight.is_none()
        {
            let mut s = snapshot.lock().expect("snapshot poisoned");
            if !cancelled()
                && s.probe_epoch == generation
                && s.rule_provider_pending
                && matches!(s.engine, EngineStatus::Running { .. })
            {
                let controller = active.controller.clone();
                let tx = rule_provider_tx.clone();
                let epoch = generation;
                rule_provider_inflight = Some(epoch);
                if thread::Builder::new()
                    .name("paw-rule-provider-read".into())
                    .spawn(move || {
                        let _ = tx.send((epoch, controller.rule_providers()));
                    })
                    .is_err()
                {
                    rule_provider_inflight = None;
                    fail_rule_provider_spawn(&mut s, epoch);
                    s.revision = s.revision.wrapping_add(1);
                }
            }
        }

        while let Ok(completion) = probe_rx.try_recv() {
            probe_inflight = false;
            change(snapshot, |s| {
                s.probe_busy = false;
                match completion {
                    ProbeCompletion::Single(epoch, target, status) => {
                        finish_proxy_probe(s, epoch, target, status)
                    }
                    ProbeCompletion::Group {
                        epoch,
                        run_id,
                        url,
                        index,
                        target,
                        status,
                    } => {
                        finish_group_probe(s, epoch, run_id, &url, index, &target, status);
                    }
                }
            });
            next_sample = Instant::now();
        }

        // The lock is held through the dispatch decision and thread creation. URL,
        // mode and session invalidation use the same lock and prevent later dispatches.
        if let Some(active) = &session
            && !probe_inflight
            && !cancelled()
        {
            let mut s = snapshot.lock().expect("snapshot poisoned");
            let next = s.group_probe.as_ref().filter(|g| g.active).and_then(|g| {
                g.results
                    .rows
                    .iter()
                    .position(|(_, status)| status.is_none())
                    .map(|index| (index, g.results.rows[index].0.clone()))
            });
            if let Some((index, target)) = next {
                let valid = !cancelled()
                    && s.group_probe
                        .as_ref()
                        .is_some_and(|g| g.valid_target(&s, index, &target));
                if !valid {
                    if let Some(g) = &mut s.group_probe {
                        g.active = false;
                        g.interrupted = true;
                    }
                } else {
                    let g = s.group_probe.as_mut().unwrap();
                    let (epoch, run_id, url) =
                        (g.results.epoch, g.results.run_id, g.results.url.clone());
                    if g.results.begin(index) {
                        let controller = active.controller.clone();
                        let tx = probe_tx.clone();
                        let requested = target.clone();
                        let request_url = url.clone();
                        if thread::Builder::new()
                            .name("paw-group-probe".into())
                            .spawn(move || {
                                let status = controller
                                    .delay(&requested.origin, &requested.name, &request_url)
                                    .ok()
                                    .filter(|ms| *ms > 0)
                                    .map_or(
                                        ProxyProbeStatus::Unavailable,
                                        ProxyProbeStatus::Measured,
                                    );
                                let _ = tx.send(ProbeCompletion::Group {
                                    epoch,
                                    run_id,
                                    url: request_url,
                                    index,
                                    target: requested,
                                    status,
                                });
                            })
                            .is_ok()
                        {
                            probe_inflight = true;
                            s.probe_busy = true;
                        } else {
                            let g = s.group_probe.as_mut().unwrap();
                            g.results.finish(
                                epoch,
                                run_id,
                                index,
                                &target,
                                ProxyProbeStatus::Unavailable,
                            );
                            g.active = false;
                            g.interrupted = true;
                        }
                    }
                }
                s.revision = s.revision.wrapping_add(1);
            }
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
                s.connection_history.record(connections.as_ref());
                s.connections = connections;
                s.traffic = traffic;
            });
            next_sample = Instant::now() + Duration::from_millis(250);
        }
    }
    // Destruction (including tray/window quit) joins this worker. Attempt
    // exact restoration while the owned core still listens, never restore a
    // stale journal encountered at launch without a user action.
    proxy.restore_current(snapshot);
    change(snapshot, |s| {
        s.apply_reservation = None;
        s.applying_profile = None;
    });
}

fn release_apply(snapshot: &mut Snapshot, reservation: ApplyReservation) {
    if snapshot.apply_reservation == Some(reservation) {
        snapshot.apply_reservation = None;
    }
}

fn finish_apply(
    snapshot: &mut Snapshot,
    reservation: ApplyReservation,
    path: PathBuf,
    mode: Option<RoutingMode>,
    result: Result<(), ApplyError>,
) {
    snapshot.applying_profile = None;
    match result {
        Ok(()) => {
            snapshot.profile = Some(path);
            snapshot.mode = mode;
            snapshot.inventory = None;
            snapshot.inventory_stale = false;
            snapshot.rules = None;
        }
        Err(error) => {
            if !error.rejected() {
                snapshot.profile = None;
                snapshot.mode = None;
                snapshot.rules = None;
            }
            snapshot.action_error = Some(error.message());
        }
    }
    release_apply(snapshot, reservation);
}

fn begin_apply(
    snapshot: &mut Snapshot,
    reservation: ApplyReservation,
    generation: u64,
    has_session: bool,
    cancelled: bool,
    path: &std::path::Path,
) -> bool {
    if snapshot.apply_reservation != Some(reservation)
        || reservation.generation != generation
        || snapshot.probe_epoch != generation
        || !has_session
        || cancelled
        || !matches!(snapshot.engine, EngineStatus::Running { .. })
    {
        release_apply(snapshot, reservation);
        return false;
    }
    snapshot.applying_profile = Some(path.to_path_buf());
    snapshot.action_error = None;
    snapshot.diagnostics.clear();
    snapshot.revision = snapshot.revision.wrapping_add(1);
    true
}

fn retain_inventory(snapshot: &mut Snapshot, fresh: Option<Arc<Inventory>>) {
    snapshot.inventory_stale = fresh.is_none();
    if fresh.is_some() {
        snapshot.inventory = fresh;
    }
}

fn clear_connection_history(snapshot: &mut Snapshot) {
    snapshot.connection_history = ConnectionHistory {
        generation: snapshot.probe_epoch,
        ..ConnectionHistory::default()
    };
}

fn clear_rule_provider_state(snapshot: &mut Snapshot) {
    snapshot.rule_providers = None;
    snapshot.rule_provider_error = None;
    snapshot.rule_provider_pending = false;
}

fn begin_rule_provider_read(snapshot: &mut Snapshot, epoch: u64) -> bool {
    if epoch != snapshot.probe_epoch
        || !matches!(snapshot.engine, EngineStatus::Running { .. })
        || snapshot.rule_provider_pending
    {
        return false;
    }
    snapshot.rule_provider_pending = true;
    snapshot.rule_provider_error = None;
    true
}

fn release_rule_provider_read(inflight: &mut Option<u64>, epoch: u64) -> bool {
    if *inflight != Some(epoch) {
        return false;
    }
    *inflight = None;
    true
}

fn fail_rule_provider_spawn(snapshot: &mut Snapshot, epoch: u64) {
    if epoch == snapshot.probe_epoch && snapshot.rule_provider_pending {
        snapshot.rule_provider_pending = false;
        snapshot.rule_provider_error = Some("Could not start rule provider read".into());
    }
}

fn finish_rule_provider_read(
    snapshot: &mut Snapshot,
    epoch: u64,
    result: Result<BTreeMap<String, RuleProviderStatus>, ControllerError>,
) {
    if epoch != snapshot.probe_epoch
        || !matches!(snapshot.engine, EngineStatus::Running { .. })
        || !snapshot.rule_provider_pending
    {
        return;
    }
    snapshot.rule_provider_pending = false;
    match result {
        Ok(providers) => {
            snapshot.rule_providers = Some(Arc::new(providers));
            snapshot.rule_provider_error = None;
        }
        Err(error) => {
            snapshot.rule_provider_error = Some(format!("Could not read rule providers: {error}"));
        }
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

fn interrupt_group_probe(s: &mut Snapshot) {
    if let Some(g) = &mut s.group_probe
        && g.active
    {
        g.active = false;
        g.interrupted = true;
    }
}

fn finish_group_probe(
    s: &mut Snapshot,
    epoch: u64,
    run_id: u64,
    url: &url::Url,
    index: usize,
    target: &ProxyProbeTarget,
    status: ProxyProbeStatus,
) {
    let valid = s.group_probe.as_ref().is_some_and(|g| {
        g.results.epoch == epoch
            && g.results.run_id == run_id
            && &g.results.url == url
            && g.valid_target(s, index, target)
    });
    if valid {
        let g = s.group_probe.as_mut().unwrap();
        if g.results.finish(epoch, run_id, index, target, status)
            && g.results.rows.iter().all(|(_, status)| status.is_some())
        {
            g.active = false;
        }
    } else if let Some(g) = &mut s.group_probe
        && g.results.run_id == run_id
        && g.active
    {
        g.active = false;
        g.interrupted = true;
    }
}

fn finish_proxy_probe(
    snapshot: &mut Snapshot,
    epoch: u64,
    target: ProxyProbeTarget,
    status: ProxyProbeStatus,
) {
    if epoch == snapshot.probe_epoch {
        snapshot.probes.finish(target, status);
    }
}

fn clear_telemetry(snapshot: &Mutex<Snapshot>, generation: u64, engine: EngineStatus) {
    change(snapshot, |s| {
        s.engine = engine;
        s.apply_reservation = None;
        s.applying_profile = None;
        s.probe_epoch = generation;
        s.probes = ProxyProbeResults::default();
        s.group_probe = None;
        s.unlock.reset();
        s.inventory = None;
        s.inventory_stale = false;
        s.refreshing_provider = None;
        s.provider_error = None;
        s.mode = None;
        s.rules = None;
        clear_rule_provider_state(s);
        s.logs = None;
        s.connections = None;
        clear_connection_history(s);
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

    fn apply_test_desktop(capacity: usize) -> (Desktop, Receiver<Command>) {
        let (command, receiver) = mpsc::sync_channel(capacity);
        let desktop = Desktop {
            command,
            snapshot: Arc::new(Mutex::new(Snapshot {
                engine: EngineStatus::Running { mixed_port: 1234 },
                profile: Some(PathBuf::from("a.yaml")),
                probe_epoch: 7,
                ..Default::default()
            })),
            stop: Arc::new(AtomicBool::new(false)),
            quit: Arc::new(AtomicBool::new(false)),
            next_mixed_port: Arc::new(AtomicU16::new(0)),
            worker: None,
        };
        (desktop, receiver)
    }

    fn queued_apply(receiver: &Receiver<Command>) -> (ProfileInput, ApplyReservation) {
        match receiver.try_recv().expect("queued apply") {
            Command::ApplyProfile(input, reservation) => (input, reservation),
            _ => panic!("expected apply command"),
        }
    }

    #[test]
    fn apply_reservation_blocks_rapid_local_and_subscription_commands() {
        let (desktop, receiver) = apply_test_desktop(8);
        let id = "0123456789abcdef0123456789abcdef".parse().unwrap();
        desktop.apply_profile("b.yaml".into()).unwrap();
        assert_eq!(
            desktop.apply_profile("c.yaml".into()).unwrap_err().kind(),
            ErrorKind::WouldBlock
        );
        assert_eq!(
            desktop
                .apply_subscription(SubscriptionStore::new("subscriptions".into()), id)
                .unwrap_err()
                .kind(),
            ErrorKind::WouldBlock
        );
        let pending = desktop.snapshot();
        assert_eq!(pending.profile, Some("a.yaml".into()));
        assert!(pending.applying_profile.is_none());
        let (input, reservation) = queued_apply(&receiver);
        assert_eq!(input.key(), PathBuf::from("b.yaml"));
        assert!(matches!(
            receiver.try_recv(),
            Err(mpsc::TryRecvError::Empty)
        ));

        {
            let mut s = desktop.snapshot.lock().unwrap();
            assert!(begin_apply(
                &mut s,
                reservation,
                7,
                true,
                false,
                &input.key()
            ));
            assert_eq!(s.applying_profile, Some("b.yaml".into()));
        }
        assert_eq!(
            desktop.apply_profile("c.yaml".into()).unwrap_err().kind(),
            ErrorKind::WouldBlock
        );
        {
            let mut s = desktop.snapshot.lock().unwrap();
            finish_apply(&mut s, reservation, input.key(), None, Ok(()));
        }
        assert_eq!(desktop.snapshot().profile, Some("b.yaml".into()));
        desktop
            .apply_subscription(SubscriptionStore::new("subscriptions".into()), id)
            .unwrap();
        assert_eq!(
            desktop.apply_profile("c.yaml".into()).unwrap_err().kind(),
            ErrorKind::WouldBlock
        );
        assert!(matches!(
            queued_apply(&receiver).0,
            ProfileInput::Subscription { .. }
        ));
        assert!(matches!(
            receiver.try_recv(),
            Err(mpsc::TryRecvError::Empty)
        ));
    }

    #[test]
    fn apply_failure_and_enqueue_errors_release_reservation() {
        let (desktop, receiver) = apply_test_desktop(1);
        desktop.apply_profile("b.yaml".into()).unwrap();
        let (input, reservation) = queued_apply(&receiver);
        {
            let mut s = desktop.snapshot.lock().unwrap();
            assert!(begin_apply(
                &mut s,
                reservation,
                7,
                true,
                false,
                &input.key()
            ));
            finish_apply(
                &mut s,
                reservation,
                input.key(),
                None,
                Err(ApplyError::Prepare(Error::other("invalid profile"))),
            );
            assert_eq!(s.profile, Some("a.yaml".into()));
            assert!(s.applying_profile.is_none());
            assert_eq!(s.action_error.as_deref(), Some("invalid profile"));
        }
        desktop.read_rules().unwrap();
        assert_eq!(
            desktop.apply_profile("c.yaml".into()).unwrap_err().kind(),
            ErrorKind::WouldBlock
        );
        assert!(desktop.snapshot().apply_reservation.is_none());
        assert!(matches!(receiver.try_recv(), Ok(Command::ReadRules)));
        desktop.apply_profile("c.yaml".into()).unwrap();
        let (_, reservation) = queued_apply(&receiver);
        release_apply(&mut desktop.snapshot.lock().unwrap(), reservation);
        drop(receiver);
        assert_eq!(
            desktop.apply_profile("d.yaml".into()).unwrap_err().kind(),
            ErrorKind::BrokenPipe
        );
        assert!(desktop.snapshot().apply_reservation.is_none());
    }

    #[test]
    fn stale_stopped_and_cancelled_applies_cannot_keep_or_steal_reservations() {
        let (desktop, receiver) = apply_test_desktop(8);
        desktop.apply_profile("b.yaml".into()).unwrap();
        let (input, old) = queued_apply(&receiver);
        {
            let mut s = desktop.snapshot.lock().unwrap();
            assert!(!begin_apply(&mut s, old, 8, true, false, &input.key()));
            assert!(s.apply_reservation.is_none());
        }
        desktop.apply_profile("c.yaml".into()).unwrap();
        let (input, current) = queued_apply(&receiver);
        {
            let mut s = desktop.snapshot.lock().unwrap();
            assert!(!begin_apply(&mut s, old, 7, true, false, &input.key()));
            assert_eq!(s.apply_reservation, Some(current));
            assert!(!begin_apply(&mut s, current, 7, false, false, &input.key()));
            assert!(s.apply_reservation.is_none());
        }
        desktop.apply_profile("d.yaml".into()).unwrap();
        let (input, current) = queued_apply(&receiver);
        {
            let mut s = desktop.snapshot.lock().unwrap();
            assert!(!begin_apply(&mut s, current, 7, true, true, &input.key()));
            assert!(s.apply_reservation.is_none());
            s.engine = EngineStatus::Stopped;
        }
        assert_eq!(
            desktop.apply_profile("e.yaml".into()).unwrap_err().kind(),
            ErrorKind::NotConnected
        );
        {
            let mut s = desktop.snapshot.lock().unwrap();
            s.engine = EngineStatus::Running { mixed_port: 1234 };
        }
        desktop.apply_profile("f.yaml".into()).unwrap();
        let (input, stopped) = queued_apply(&receiver);
        desktop.stop();
        assert!(desktop.snapshot().apply_reservation.is_none());
        assert!(!begin_apply(
            &mut desktop.snapshot.lock().unwrap(),
            stopped,
            7,
            true,
            true,
            &input.key()
        ));
        assert_eq!(
            desktop.apply_profile("g.yaml".into()).unwrap_err().kind(),
            ErrorKind::NotConnected
        );

        let (desktop, _receiver) = apply_test_desktop(8);
        desktop.apply_profile("b.yaml".into()).unwrap();
        clear_telemetry(&desktop.snapshot, 8, EngineStatus::Error("exited".into()));
        assert!(desktop.snapshot().apply_reservation.is_none());
        assert_eq!(
            desktop.apply_profile("c.yaml".into()).unwrap_err().kind(),
            ErrorKind::NotConnected
        );

        let (desktop, _receiver) = apply_test_desktop(8);
        desktop.apply_profile("b.yaml".into()).unwrap();
        let snapshot = Arc::clone(&desktop.snapshot);
        drop(desktop);
        assert!(snapshot.lock().unwrap().apply_reservation.is_none());
    }

    fn connection(id: &str, host: &str) -> Connection {
        Connection {
            id: id.into(),
            metadata: crate::engine::controller::Metadata {
                host: host.into(),
                ..Default::default()
            },
            chains: vec!["proxy".into()],
            rule: "rule".into(),
            ..Default::default()
        }
    }

    fn connection_sample(history: &mut ConnectionHistory, rows: Vec<Connection>) {
        history.record(Some(&Arc::new(rows)));
    }

    #[test]
    fn connection_history_uses_last_successful_sample_and_metadata() {
        let mut history = ConnectionHistory::default();
        connection_sample(
            &mut history,
            vec![connection("a", "old"), connection("b", "b")],
        );
        history.record(None);
        assert_eq!(history.recent().len(), 0);
        connection_sample(
            &mut history,
            vec![connection("a", "latest"), connection("b", "b")],
        );
        connection_sample(&mut history, vec![connection("b", "b")]);
        let closed = history.recent().collect::<Vec<_>>();
        assert_eq!(closed.len(), 1);
        assert_eq!(closed[0].id, "a");
        assert_eq!(closed[0].metadata.host, "latest");
        assert_eq!(closed[0].chains, ["proxy"]);
        assert_eq!(closed[0].rule, "rule");
        connection_sample(&mut history, vec![connection("b", "b")]);
        assert_eq!(history.recent().len(), 1);
        connection_sample(&mut history, vec![]);
        assert_eq!(
            history.recent().map(|c| c.id.as_str()).collect::<Vec<_>>(),
            ["b", "a"]
        );
    }

    #[test]
    fn connection_history_deduplicates_ids_and_allows_reuse() {
        let mut history = ConnectionHistory::default();
        connection_sample(
            &mut history,
            vec![
                connection("", "unknown"),
                connection("a", "first"),
                connection("a", "last"),
                connection("b", "b"),
            ],
        );
        connection_sample(
            &mut history,
            vec![connection("b", "first"), connection("b", "last")],
        );
        assert_eq!(
            history
                .recent()
                .map(|c| (&*c.id, &*c.metadata.host))
                .collect::<Vec<_>>(),
            [("a", "last")]
        );
        connection_sample(&mut history, vec![connection("a", "reused")]);
        connection_sample(&mut history, vec![]);
        assert_eq!(
            history
                .recent()
                .map(|c| (&*c.id, &*c.metadata.host))
                .collect::<Vec<_>>(),
            [("a", "reused"), ("b", "last"), ("a", "last")]
        );
    }

    #[test]
    fn connection_history_is_bounded_and_has_deterministic_batch_order() {
        let mut history = ConnectionHistory::default();
        connection_sample(
            &mut history,
            (0..103)
                .rev()
                .map(|i| connection(&format!("{i:03}"), "host"))
                .collect(),
        );
        connection_sample(&mut history, vec![]);
        let ids = history.recent().map(|c| c.id.as_str()).collect::<Vec<_>>();
        assert_eq!(ids.len(), CLOSED_CONNECTION_LIMIT);
        assert_eq!(ids[0], "102");
        assert_eq!(ids[99], "003");
    }

    #[test]
    fn closed_keys_survive_new_closures_and_distinguish_reused_ids() {
        let mut history = ConnectionHistory::default();
        connection_sample(&mut history, vec![connection("a", "original")]);
        connection_sample(&mut history, vec![]);
        let captured = history.clone();
        let original_key = captured.recent_keyed().next().unwrap().0;

        connection_sample(
            &mut history,
            vec![connection("a", "reused"), connection("b", "other")],
        );
        connection_sample(&mut history, vec![]);
        let rows = history.recent_keyed().collect::<Vec<_>>();
        assert_eq!(
            rows.iter().map(|(_, c)| c.id.as_str()).collect::<Vec<_>>(),
            ["b", "a", "a"]
        );
        assert_eq!(rows[2].0, original_key);
        assert_ne!(rows[1].0, original_key);
        assert_ne!(rows[0].0, rows[1].0);
        assert_eq!(captured.recent_keyed().next().unwrap().0, original_key);
        assert_eq!(rows[2].1.metadata.host, "original");
        assert_eq!(rows[1].1.metadata.host, "reused");
    }

    #[test]
    fn closed_keys_remain_stable_until_truncated() {
        let mut history = ConnectionHistory::default();
        connection_sample(&mut history, vec![connection("old", "old")]);
        connection_sample(&mut history, vec![]);
        let old_key = history.recent_keyed().next().unwrap().0;
        connection_sample(
            &mut history,
            (0..CLOSED_CONNECTION_LIMIT - 1)
                .map(|i| connection(&format!("{i:03}"), "host"))
                .collect(),
        );
        connection_sample(&mut history, vec![]);
        assert_eq!(history.recent_keyed().len(), CLOSED_CONNECTION_LIMIT);
        assert_eq!(history.recent_keyed().last().unwrap().0, old_key);
        let retained_key = history.recent_keyed().next().unwrap().0;
        connection_sample(&mut history, vec![connection("new", "new")]);
        connection_sample(&mut history, vec![]);
        assert_eq!(history.recent_keyed().len(), CLOSED_CONNECTION_LIMIT);
        assert!(!history.recent_keyed().any(|(key, _)| key == old_key));
        assert!(history.recent_keyed().any(|(key, _)| key == retained_key));
    }

    #[test]
    fn closed_keys_change_on_session_reset_and_baseline_is_cleared() {
        let mut snapshot = Snapshot {
            probe_epoch: 1,
            ..Snapshot::default()
        };
        clear_connection_history(&mut snapshot);
        connection_sample(
            &mut snapshot.connection_history,
            vec![connection("a", "old")],
        );
        connection_sample(&mut snapshot.connection_history, vec![]);
        let old_key = snapshot.connection_history.recent_keyed().next().unwrap().0;
        connection_sample(
            &mut snapshot.connection_history,
            vec![connection("live", "live")],
        );

        let snapshot = Mutex::new(snapshot);
        clear_telemetry(&snapshot, 2, EngineStatus::Stopped);
        let mut snapshot = snapshot.lock().unwrap();
        assert_eq!(snapshot.connection_history.recent_keyed().len(), 0);
        connection_sample(
            &mut snapshot.connection_history,
            vec![connection("a", "new")],
        );
        assert_eq!(snapshot.connection_history.recent_keyed().len(), 0);
        connection_sample(&mut snapshot.connection_history, vec![]);
        let (new_key, row) = snapshot.connection_history.recent_keyed().next().unwrap();
        assert_eq!(row.id, "a");
        assert_ne!(new_key, old_key);
    }

    #[test]
    fn clearing_telemetry_discards_connection_history_and_baseline() {
        let mut snapshot = Snapshot::default();
        connection_sample(
            &mut snapshot.connection_history,
            vec![connection("closed", "c"), connection("live", "l")],
        );
        connection_sample(
            &mut snapshot.connection_history,
            vec![connection("live", "l")],
        );
        snapshot.connections = Some(Arc::new(vec![connection("live", "l")]));
        let snapshot = Mutex::new(snapshot);
        clear_telemetry(&snapshot, 2, EngineStatus::Stopped);
        let mut snapshot = snapshot.lock().unwrap();
        assert!(snapshot.connections.is_none());
        assert_eq!(snapshot.connection_history.recent().len(), 0);
        connection_sample(&mut snapshot.connection_history, vec![]);
        assert_eq!(snapshot.connection_history.recent().len(), 0);
    }

    #[test]
    fn profile_generation_reset_discards_history_without_changing_active_list() {
        let mut snapshot = Snapshot::default();
        connection_sample(
            &mut snapshot.connection_history,
            vec![connection("old", "old"), connection("live", "live")],
        );
        connection_sample(
            &mut snapshot.connection_history,
            vec![connection("live", "live")],
        );
        let active = Arc::new(vec![connection("live", "live")]);
        snapshot.connections = Some(Arc::clone(&active));
        clear_connection_history(&mut snapshot);
        assert!(Arc::ptr_eq(snapshot.connections.as_ref().unwrap(), &active));
        assert_eq!(snapshot.connection_history.recent().len(), 0);
        connection_sample(&mut snapshot.connection_history, vec![]);
        assert_eq!(snapshot.connection_history.recent().len(), 0);
    }

    #[test]
    fn rule_provider_reads_keep_unknown_empty_errors_and_sessions_distinct() {
        let mut snapshot = Snapshot {
            engine: EngineStatus::Running { mixed_port: 1234 },
            probe_epoch: 7,
            inventory: Some(Arc::new(Inventory::default())),
            rules: Some(Arc::new(Vec::new())),
            action_error: Some("other action".into()),
            provider_error: Some("proxy provider".into()),
            ..Default::default()
        };
        assert!(snapshot.rule_providers.is_none());
        assert!(begin_rule_provider_read(&mut snapshot, 7));
        finish_rule_provider_read(&mut snapshot, 7, Err(ControllerError::Status(404)));
        assert!(!snapshot.rule_provider_pending);
        assert!(snapshot.rule_providers.is_none());
        assert_eq!(
            snapshot.rule_provider_error.as_deref(),
            Some("Could not read rule providers: Controller returned HTTP 404")
        );
        assert!(begin_rule_provider_read(&mut snapshot, 7));
        finish_rule_provider_read(&mut snapshot, 7, Ok(BTreeMap::new()));
        let empty = Arc::clone(snapshot.rule_providers.as_ref().unwrap());
        assert!(empty.is_empty());
        assert!(snapshot.rule_provider_error.is_none());
        assert!(begin_rule_provider_read(&mut snapshot, 7));
        finish_rule_provider_read(&mut snapshot, 7, Err(ControllerError::InvalidResponse));
        assert!(Arc::ptr_eq(
            &empty,
            snapshot.rule_providers.as_ref().unwrap()
        ));
        assert_eq!(snapshot.action_error.as_deref(), Some("other action"));
        assert_eq!(snapshot.provider_error.as_deref(), Some("proxy provider"));
        assert!(snapshot.inventory.is_some());
        assert!(snapshot.rules.is_some());

        let snapshot = Mutex::new(snapshot);
        clear_telemetry(&snapshot, 8, EngineStatus::Stopped);
        let mut state = snapshot.lock().unwrap();
        assert!(state.rule_providers.is_none());
        assert!(state.rule_provider_error.is_none());
        finish_rule_provider_read(&mut state, 7, Ok(BTreeMap::new()));
        finish_rule_provider_read(&mut state, 7, Err(ControllerError::Status(503)));
        assert!(state.rule_providers.is_none());
        assert!(state.rule_provider_error.is_none());
        state.engine = EngineStatus::Stopped;
        finish_rule_provider_read(&mut state, 8, Ok(BTreeMap::new()));
        assert!(state.rule_providers.is_none());
    }

    #[test]
    fn rule_provider_pending_deduplicates_and_retains_last_good_on_failure() {
        let providers = Arc::new(BTreeMap::new());
        let mut snapshot = Snapshot {
            engine: EngineStatus::Running { mixed_port: 1234 },
            probe_epoch: 7,
            rule_providers: Some(Arc::clone(&providers)),
            rule_provider_error: Some("previous failure".into()),
            ..Default::default()
        };
        assert!(begin_rule_provider_read(&mut snapshot, 7));
        assert!(snapshot.rule_provider_pending);
        assert!(snapshot.rule_provider_error.is_none());
        assert!(!begin_rule_provider_read(&mut snapshot, 7));
        assert!(snapshot.rule_provider_pending);
        assert!(snapshot.rule_provider_error.is_none());
        finish_rule_provider_read(&mut snapshot, 7, Err(ControllerError::InvalidResponse));
        assert!(!snapshot.rule_provider_pending);
        assert!(snapshot.rule_provider_error.is_some());
        assert!(Arc::ptr_eq(
            &providers,
            snapshot.rule_providers.as_ref().unwrap()
        ));
        assert!(begin_rule_provider_read(&mut snapshot, 7));
        assert!(snapshot.rule_provider_error.is_none());
        finish_rule_provider_read(&mut snapshot, 7, Ok(BTreeMap::new()));
        assert!(!snapshot.rule_provider_pending);
        assert!(snapshot.rule_provider_error.is_none());
    }

    #[test]
    fn old_rule_provider_completion_cannot_cancel_new_profile_request() {
        let mut snapshot = Snapshot {
            engine: EngineStatus::Running { mixed_port: 1234 },
            probe_epoch: 7,
            ..Default::default()
        };
        assert!(begin_rule_provider_read(&mut snapshot, 7));
        let mut inflight = Some(7);
        clear_rule_provider_state(&mut snapshot);
        snapshot.probe_epoch = 8;
        assert!(begin_rule_provider_read(&mut snapshot, 8));
        assert!(!begin_rule_provider_read(&mut snapshot, 8));
        assert_eq!(inflight, Some(7)); // The new request waits for the sole worker.
        assert!(release_rule_provider_read(&mut inflight, 7));
        finish_rule_provider_read(&mut snapshot, 7, Err(ControllerError::Status(503)));
        assert!(snapshot.rule_provider_pending);
        assert!(snapshot.rule_provider_error.is_none());
        assert!(snapshot.rule_providers.is_none());
        assert_eq!(inflight, None);
        inflight = Some(8);
        assert!(!release_rule_provider_read(&mut inflight, 7));
        assert_eq!(inflight, Some(8));
        assert!(release_rule_provider_read(&mut inflight, 8));
        finish_rule_provider_read(&mut snapshot, 8, Ok(BTreeMap::new()));
        assert!(!snapshot.rule_provider_pending);
        assert!(snapshot.rule_providers.is_some());
    }

    #[test]
    fn rule_provider_spawn_failure_and_stop_clear_only_current_pending() {
        let providers = Arc::new(BTreeMap::new());
        let mut snapshot = Snapshot {
            engine: EngineStatus::Running { mixed_port: 1234 },
            probe_epoch: 8,
            rule_providers: Some(Arc::clone(&providers)),
            ..Default::default()
        };
        assert!(begin_rule_provider_read(&mut snapshot, 8));
        fail_rule_provider_spawn(&mut snapshot, 7);
        assert!(snapshot.rule_provider_pending);
        fail_rule_provider_spawn(&mut snapshot, 8);
        assert!(!snapshot.rule_provider_pending);
        assert_eq!(
            snapshot.rule_provider_error.as_deref(),
            Some("Could not start rule provider read")
        );
        assert!(Arc::ptr_eq(
            &providers,
            snapshot.rule_providers.as_ref().unwrap()
        ));
        assert!(begin_rule_provider_read(&mut snapshot, 8));

        let (command, _receiver) = mpsc::sync_channel(1);
        let desktop = Desktop {
            command,
            snapshot: Arc::new(Mutex::new(snapshot)),
            stop: Arc::new(AtomicBool::new(false)),
            quit: Arc::new(AtomicBool::new(false)),
            next_mixed_port: Arc::new(AtomicU16::new(0)),
            worker: None,
        };
        desktop.stop();
        let mut state = desktop.snapshot.lock().unwrap();
        assert!(!state.rule_provider_pending);
        assert!(state.rule_providers.is_none());
        assert!(state.rule_provider_error.is_none());
        finish_rule_provider_read(&mut state, 8, Ok(BTreeMap::new()));
        assert!(state.rule_providers.is_none());
        assert!(!state.rule_provider_pending);
    }

    #[test]
    fn group_targets_are_ordered_deduplicated_and_skip_ambiguous_members() {
        use crate::engine::controller::{Node, Proxy};
        let mut inventory = Inventory::default();
        let group = |members: &[&str]| Proxy {
            all: Some(members.iter().map(|s| (*s).to_owned()).collect()),
            ..Default::default()
        };
        inventory.groups.insert(
            "root".into(),
            group(&["a", "nested", "a", "ambiguous", "collision", "missing", "b"]),
        );
        inventory
            .groups
            .insert("nested".into(), group(&["b", "root", "a"]));
        inventory.groups.insert("collision".into(), group(&["c"]));
        for (name, origin) in [
            ("a", Origin::Local),
            ("b", Origin::Provider("P".into())),
            ("c", Origin::Local),
            ("ambiguous", Origin::Local),
            ("ambiguous", Origin::Provider("P".into())),
            ("collision", Origin::Local),
        ] {
            inventory.nodes.push(Node {
                name: name.into(),
                origin,
                proxy: Proxy::default(),
            });
        }
        assert_eq!(
            group_probe_targets(&inventory, "root").unwrap(),
            vec![
                ProxyProbeTarget {
                    name: "a".into(),
                    origin: Origin::Local
                },
                ProxyProbeTarget {
                    name: "b".into(),
                    origin: Origin::Provider("P".into())
                },
            ]
        );
        assert_eq!(
            group_probe_targets(&inventory, "absent"),
            Err(GroupProbePreparationError::MissingGroup)
        );
    }

    #[test]
    fn group_target_limits_fail_without_returning_partial_results() {
        use crate::engine::controller::{Node, Proxy};
        let mut inventory = Inventory::default();
        let names: Vec<_> = (0..=GROUP_PROBE_TARGET_LIMIT)
            .map(|i| format!("n{i}"))
            .collect();
        inventory.groups.insert(
            "root".into(),
            Proxy {
                all: Some(names.clone()),
                ..Default::default()
            },
        );
        inventory.nodes = names
            .into_iter()
            .map(|name| Node {
                name,
                origin: Origin::Local,
                proxy: Proxy::default(),
            })
            .collect();
        assert_eq!(
            group_probe_targets(&inventory, "root"),
            Err(GroupProbePreparationError::TooManyTargets)
        );
        inventory.groups.get_mut("root").unwrap().all =
            Some(vec!["unknown".into(); GROUP_PROBE_MEMBER_LIMIT + 1]);
        assert_eq!(
            group_probe_targets(&inventory, "root"),
            Err(GroupProbePreparationError::MembershipTooLarge)
        );
        inventory
            .nodes
            .resize_with(GROUP_PROBE_INVENTORY_LIMIT + 1, || Node {
                name: "x".into(),
                origin: Origin::Local,
                proxy: Proxy::default(),
            });
        assert_eq!(
            group_probe_targets(&inventory, "root"),
            Err(GroupProbePreparationError::InventoryTooLarge)
        );
    }

    #[test]
    fn group_results_reject_stale_or_mismatched_completions() {
        let target = ProxyProbeTarget {
            name: "a".into(),
            origin: Origin::Local,
        };
        let other = ProxyProbeTarget {
            name: "b".into(),
            origin: Origin::Local,
        };
        let url = parse_probe_url("https://example.invalid/").unwrap();
        assert!(
            GroupProbeResults::new(4, 1, url.clone(), vec![target.clone(), target.clone()])
                .is_none()
        );
        let mut results =
            GroupProbeResults::new(4, 2, url.clone(), vec![target.clone(), other.clone()]).unwrap();
        assert!(results.begin(0));
        assert!(!results.begin(1));
        assert!(!results.finish(3, 2, 0, &target, ProxyProbeStatus::Measured(10)));
        assert!(!results.finish(4, 1, 0, &target, ProxyProbeStatus::Measured(10)));
        assert!(!results.finish(4, 2, 0, &other, ProxyProbeStatus::Measured(10)));
        assert!(!results.finish(4, 2, 0, &target, ProxyProbeStatus::Running));
        assert!(results.finish(4, 2, 0, &target, ProxyProbeStatus::Measured(10)));
        assert!(!results.begin(0));
        assert!(results.begin(1));
        assert!(results.finish(4, 2, 1, &other, ProxyProbeStatus::Unavailable));
        assert_eq!(results.url, url);
        assert_eq!(results.rows()[0].1, Some(ProxyProbeStatus::Measured(10)));
        assert_eq!(results.rows()[1].1, Some(ProxyProbeStatus::Unavailable));
    }

    #[test]
    fn comparison_fixture_rejects_stale_context_and_completes_sequentially() {
        use crate::engine::controller::{Node, Proxy};
        let a = ProxyProbeTarget {
            name: "a".into(),
            origin: Origin::Provider("provider".into()),
        };
        let b = ProxyProbeTarget {
            name: "b".into(),
            origin: Origin::Local,
        };
        let url = parse_probe_url("https://example.invalid/check").unwrap();
        let mut inventory = Inventory::default();
        for target in [&a, &b] {
            inventory.nodes.push(Node {
                name: target.name.clone(),
                origin: target.origin.clone(),
                proxy: Proxy::default(),
            });
        }
        let mut s = Snapshot {
            probe_epoch: 7,
            group_run_id: 42,
            mode: Some(RoutingMode::Rule),
            inventory: Some(Arc::new(inventory)),
            ..Default::default()
        };
        let results =
            GroupProbeResults::new(7, 42, url.clone(), vec![a.clone(), b.clone()]).unwrap();
        s.group_probe = Some(GroupComparison {
            group: "group".into(),
            results,
            active: true,
            interrupted: false,
            mode: s.mode,
        });
        assert!(s.group_probe.as_ref().unwrap().valid_target(&s, 0, &a));
        assert!(
            !s.group_probe
                .as_ref()
                .unwrap()
                .is_current_completed(&s, "group", Some(&url))
        );
        assert!(s.group_probe.as_mut().unwrap().results.begin(0));
        finish_group_probe(&mut s, 7, 41, &url, 0, &a, ProxyProbeStatus::Measured(1));
        assert!(s.group_probe.as_ref().unwrap().active);
        finish_group_probe(&mut s, 7, 42, &url, 0, &a, ProxyProbeStatus::Measured(15));
        assert!(s.group_probe.as_mut().unwrap().results.begin(1));
        finish_group_probe(&mut s, 7, 42, &url, 1, &b, ProxyProbeStatus::Unavailable);
        let g = s.group_probe.as_ref().unwrap();
        assert!(!g.active && !g.interrupted);
        assert_eq!(g.results.rows()[0].1, Some(ProxyProbeStatus::Measured(15)));
        assert_eq!(g.results.rows()[1].1, Some(ProxyProbeStatus::Unavailable));
        assert!(g.is_current_completed(&s, "group", Some(&url)));
        assert!(!g.is_current_completed(&s, "other", Some(&url)));
        assert!(!g.is_current_completed(&s, "group", None));
        let other_url = parse_probe_url("https://example.invalid/other").unwrap();
        assert!(!g.is_current_completed(&s, "group", Some(&other_url)));
        let mut stale = s.clone();
        stale.probe_epoch += 1;
        assert!(!g.is_current_completed(&stale, "group", Some(&url)));
        stale = s.clone();
        stale.group_run_id += 1;
        assert!(!g.is_current_completed(&stale, "group", Some(&url)));
        stale = s.clone();
        stale.mode = Some(RoutingMode::Direct);
        assert!(!g.is_current_completed(&stale, "group", Some(&url)));
        let mut interrupted = g.clone();
        interrupted.interrupted = true;
        assert!(!interrupted.is_current_completed(&s, "group", Some(&url)));

        for change in 0..5 {
            let results = GroupProbeResults::new(7, 43, url.clone(), vec![a.clone()]).unwrap();
            s.group_probe = Some(GroupComparison {
                group: "group".into(),
                results,
                active: true,
                interrupted: false,
                mode: Some(RoutingMode::Rule),
            });
            s.probe_epoch = 7;
            s.group_run_id = 43;
            s.mode = Some(RoutingMode::Rule);
            let mut inventory = Inventory::default();
            inventory.nodes.push(Node {
                name: a.name.clone(),
                origin: a.origin.clone(),
                proxy: Proxy::default(),
            });
            s.inventory = Some(Arc::new(inventory));
            assert!(s.group_probe.as_mut().unwrap().results.begin(0));
            let mut completion_url = url.clone();
            match change {
                0 => interrupt_group_probe(&mut s),
                1 => s.probe_epoch += 1,
                2 => s.mode = Some(RoutingMode::Direct),
                3 => Arc::make_mut(s.inventory.as_mut().unwrap()).nodes[0].origin = Origin::Local,
                _ => completion_url = parse_probe_url("https://example.invalid/other").unwrap(),
            }
            finish_group_probe(
                &mut s,
                7,
                43,
                &completion_url,
                0,
                &a,
                ProxyProbeStatus::Measured(99),
            );
            let g = s.group_probe.as_ref().unwrap();
            assert!(!g.active && g.interrupted);
            assert_eq!(g.results.rows()[0].1, Some(ProxyProbeStatus::Running));
        }
    }

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
        let url = parse_probe_url("https://example.invalid/ping").unwrap();
        let mut s = Snapshot {
            probe_epoch: 12,
            ..Default::default()
        };
        s.probes.begin(url.clone(), target.clone());
        finish_proxy_probe(&mut s, 11, target.clone(), ProxyProbeStatus::Measured(10));
        assert_eq!(
            s.probes.status(&url, &target),
            Some(ProxyProbeStatus::Running)
        );
        finish_proxy_probe(&mut s, 12, target.clone(), ProxyProbeStatus::Measured(20));
        assert_eq!(
            s.probes.status(&url, &target),
            Some(ProxyProbeStatus::Measured(20))
        );
    }

    #[test]
    fn probe_results_are_bounded_by_recency_and_bound_to_the_test_url() {
        let first_url = parse_probe_url("https://example.invalid/a").unwrap();
        let second_url = parse_probe_url("https://example.invalid/b").unwrap();
        let target = |i: usize| ProxyProbeTarget {
            name: format!("Node {i}"),
            origin: Origin::Provider("Provider".into()),
        };
        let mut results = ProxyProbeResults::default();
        for i in 0..PROBE_RESULT_LIMIT {
            results.begin(first_url.clone(), target(i));
            results.finish(target(i), ProxyProbeStatus::Measured(20));
        }
        results.begin(first_url.clone(), target(0));
        results.finish(target(0), ProxyProbeStatus::Unavailable);
        results.begin(first_url.clone(), target(PROBE_RESULT_LIMIT));
        assert!(results.is_running());
        assert_eq!(
            results.status(&first_url, &target(0)),
            Some(ProxyProbeStatus::Unavailable)
        );
        assert_eq!(results.status(&second_url, &target(0)), None);
        results.finish(target(PROBE_RESULT_LIMIT), ProxyProbeStatus::Measured(5));
        assert!(!results.is_running());
        assert_eq!(results.settled.len(), PROBE_RESULT_LIMIT);
        assert_eq!(results.status(&first_url, &target(1)), None);
        assert_eq!(
            results.status(&first_url, &target(0)),
            Some(ProxyProbeStatus::Unavailable)
        );
        results.begin(second_url.clone(), target(2));
        assert_eq!(results.status(&first_url, &target(0)), None);
        assert_eq!(results.status(&second_url, &target(0)), None);
        assert_eq!(
            results.status(&second_url, &target(2)),
            Some(ProxyProbeStatus::Running)
        );
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
    fn provider_outage_blocks_identity_resolution_and_recovery_restores_it() {
        use crate::engine::controller::{Node, Proxy};
        let mut inventory = Inventory::default();
        inventory.groups.insert(
            "G".into(),
            Proxy {
                all: Some(vec!["A".into()]),
                ..Default::default()
            },
        );
        inventory.nodes.push(Node {
            name: "A".into(),
            origin: Origin::Local,
            proxy: Proxy::default(),
        });
        let target = ProxyProbeTarget {
            name: "A".into(),
            origin: Origin::Local,
        };
        let mut snapshot = Snapshot {
            inventory: Some(Arc::new(inventory)),
            probe_epoch: 1,
            group_run_id: 1,
            ..Default::default()
        };
        let results = GroupProbeResults::new(
            1,
            1,
            parse_probe_url("https://example.invalid/").unwrap(),
            vec![target.clone()],
        )
        .unwrap();
        snapshot.group_probe = Some(GroupComparison {
            group: "G".into(),
            results,
            active: true,
            interrupted: false,
            mode: None,
        });
        assert!(
            snapshot
                .group_probe
                .as_ref()
                .unwrap()
                .valid_target(&snapshot, 0, &target)
        );
        Arc::make_mut(snapshot.inventory.as_mut().unwrap()).provider_error = Some("503".into());
        let degraded = snapshot.inventory.as_ref().unwrap();
        assert_eq!(unique_proxy(degraded, "A"), None);
        assert_eq!(
            group_probe_targets(degraded, "G"),
            Err(GroupProbePreparationError::ProviderUnavailable)
        );
        assert!(
            !snapshot
                .group_probe
                .as_ref()
                .unwrap()
                .valid_target(&snapshot, 0, &target)
        );
        let mut recovered = (**degraded).clone();
        recovered.provider_error = None;
        retain_inventory(&mut snapshot, Some(Arc::new(recovered)));
        assert_eq!(
            unique_proxy(snapshot.inventory.as_ref().unwrap(), "A"),
            Some(Origin::Local)
        );
        assert_eq!(
            group_probe_targets(snapshot.inventory.as_ref().unwrap(), "G").unwrap(),
            vec![target.clone()]
        );
        assert!(
            snapshot
                .group_probe
                .as_ref()
                .unwrap()
                .valid_target(&snapshot, 0, &target)
        );
        retain_inventory(&mut snapshot, None);
        assert!(snapshot.inventory_stale);
        assert!(
            !snapshot
                .group_probe
                .as_ref()
                .unwrap()
                .valid_target(&snapshot, 0, &target)
        );
        retain_inventory(&mut snapshot, Some(Arc::new(Inventory::default())));
        assert!(!snapshot.inventory_stale);
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
