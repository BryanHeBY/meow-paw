use super::{Phase, ROUTE_LIMIT, Row, State, Status};
use crate::engine::controller::{Inventory, Origin};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{
    cmp::Reverse,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::Sender,
    },
    thread,
    time::Duration,
};
use ureq::{Body, http::Response};
use url::Url;

const CORE_TIMEOUT_MS: u32 = 2_000;
const REQUEST_TIMEOUT: Duration = Duration::from_millis(2_500);
const MAX_HEADER_BYTES: usize = 4 * 1024;
const MAX_BODY_BYTES: u64 = 2 * 1024;
const MAX_ROUTE_BYTES: usize = 256;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) struct Route {
    name: String,
    provider: Option<String>,
}

impl Route {
    fn new(name: String, origin: Origin) -> Option<Self> {
        let provider = match origin {
            Origin::Local => None,
            Origin::Provider(provider) => Some(provider),
        };
        (valid_label(&name) && provider.as_deref().is_none_or(valid_label))
            .then_some(Self { name, provider })
    }
}

#[cfg(any(target_os = "linux", windows))]
pub(crate) fn routes(inventory: &Inventory, group: &str) -> Vec<Route> {
    crate::app::group_probe_targets(inventory, group)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|target| {
            let node = inventory
                .nodes
                .iter()
                .find(|node| node.name == target.name)?;
            let kind = node.proxy.kind.to_ascii_lowercase();
            if kind.is_empty()
                || matches!(
                    kind.as_str(),
                    "direct" | "reject" | "rejectdrop" | "pass" | "compatible"
                )
            {
                return None;
            }
            Route::new(target.name, target.origin)
        })
        .take(ROUTE_LIMIT)
        .collect()
}

fn valid_label(label: &str) -> bool {
    !label.is_empty() && label.len() <= MAX_ROUTE_BYTES && !label.chars().any(char::is_control)
}

#[derive(Clone, Debug)]
pub(super) struct Ticket(Arc<AtomicBool>);

impl Ticket {
    fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    fn cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

pub(crate) struct Job {
    ticket: Ticket,
    routes: Vec<Route>,
}

pub(crate) struct Completion {
    ticket: Ticket,
    phase: Phase,
    rows: Vec<Row>,
}

impl State {
    pub(crate) fn begin(&mut self, routes: Vec<Route>) -> Option<Job> {
        if self.is_busy() || routes.len() > ROUTE_LIMIT {
            return None;
        }
        let ticket = Ticket(Arc::default());
        self.active = Some(ticket.clone());
        self.phase = Phase::Checking;
        self.rows = pending_rows(&routes);
        Some(Job { ticket, routes })
    }

    pub(crate) fn cancel(&mut self) {
        if self.phase == Phase::Checking
            && let Some(ticket) = &self.active
        {
            ticket.cancel();
            self.phase = Phase::Cancelling;
        }
    }

    pub(crate) fn reset(&mut self) {
        // Keep the slot reserved until the cancelled job drains, even across sessions.
        self.cancel();
        self.phase = Phase::Idle;
        self.rows.clear();
    }

    pub(crate) fn finish(&mut self, result: Completion) {
        if self
            .active
            .as_ref()
            .is_none_or(|active| !Arc::ptr_eq(&active.0, &result.ticket.0))
        {
            return;
        }
        self.active = None;
        if self.phase == Phase::Idle {
            return;
        }
        self.rows = result.rows;
        self.phase = if self.phase == Phase::Cancelling {
            self.rows
                .iter_mut()
                .flat_map(|row| &mut row.checks)
                .filter(|status| **status == Status::Pending)
                .for_each(|status| *status = Status::Cancelled);
            Phase::Cancelled
        } else {
            result.phase
        };
    }
}

fn pending_rows(routes: &[Route]) -> Vec<Row> {
    routes
        .iter()
        .map(|route| Row {
            route: route.name.clone(),
            checks: [Status::Pending; 2],
        })
        .collect()
}

impl Job {
    pub(crate) fn dispatch(
        self,
        client: Client,
        sender: Sender<Completion>,
    ) -> Result<(), Completion> {
        let failure = self.without_checks(Phase::Unavailable);
        thread::Builder::new()
            .name("paw-unlock".into())
            .spawn(move || {
                let _ = sender.send(self.run(&client));
            })
            .map(|_| ())
            .map_err(|_| failure)
    }

    pub(crate) fn without_checks(&self, phase: Phase) -> Completion {
        Completion {
            ticket: self.ticket.clone(),
            phase,
            rows: Vec::new(),
        }
    }

    fn run(self, client: &Client) -> Completion {
        if self.ticket.cancelled() {
            return self.without_checks(Phase::Cancelled);
        }
        let capability = client.capability();
        if self.ticket.cancelled() {
            return self.without_checks(Phase::Cancelled);
        }
        let generation = match capability {
            Ok(Some(generation)) => generation,
            Ok(None) => return self.without_checks(Phase::Unsupported),
            Err(_) => return self.without_checks(Phase::Unavailable),
        };
        if self.routes.is_empty() {
            return self.without_checks(Phase::NoRoutes);
        }

        let mut rows = pending_rows(&self.routes);
        let mut phase = Phase::Complete;
        'routes: for (route, row) in self.routes.iter().zip(&mut rows) {
            for (check, status) in CHECKS.into_iter().zip(&mut row.checks) {
                if self.ticket.cancelled() {
                    phase = Phase::Cancelled;
                    break 'routes;
                }
                let result = client.check(generation, route, check);
                if self.ticket.cancelled() {
                    phase = Phase::Cancelled;
                    break 'routes;
                }
                match result {
                    Ok(value) => *status = value,
                    Err(error) => {
                        *status = match error {
                            RequestError::Timeout => Status::TimedOut,
                            RequestError::Invalid => Status::Failed,
                        };
                        phase = Phase::Unavailable;
                        break 'routes;
                    }
                }
            }
        }
        if phase == Phase::Complete {
            rows.sort_by_key(rank);
        } else if phase == Phase::Unavailable {
            rows.iter_mut()
                .flat_map(|row| &mut row.checks)
                .filter(|status| **status == Status::Pending)
                .for_each(|status| *status = Status::NotRun);
        }
        Completion {
            ticket: self.ticket,
            phase,
            rows,
        }
    }
}

fn rank(row: &Row) -> (Reverse<usize>, u32) {
    let (passed, latency) =
        row.checks
            .iter()
            .fold((0, 0), |(passed, latency), status| match *status {
                Status::Passed { elapsed_ms } => (passed + 1, latency + elapsed_ms),
                _ => (passed, latency),
            });
    (Reverse(passed), latency)
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
enum Check {
    Gstatic204,
    Cloudflare204,
}

const CHECKS: [Check; 2] = [Check::Gstatic204, Check::Cloudflare204];

pub(crate) struct Client {
    address: Url,
    authorization: String,
    agent: ureq::Agent,
}

#[derive(Deserialize)]
struct Version {
    #[serde(default)]
    capabilities: Capabilities,
}

#[derive(Default, Deserialize)]
struct Capabilities {
    #[serde(default, rename = "route-http-check-v1")]
    route_http_check_v1: Option<RouteCapability>,
}

#[derive(Deserialize)]
struct RouteCapability {
    generation: u64,
}

#[derive(Serialize)]
struct CheckRequest<'a> {
    generation: u64,
    route: &'a Route,
    check: Check,
    timeout_ms: u32,
}

#[derive(Deserialize)]
struct CheckReply {
    route: Route,
    check: Check,
    status: u16,
    elapsed_ms: u32,
}

#[derive(Debug)]
enum RequestError {
    Timeout,
    Invalid,
}

impl From<ureq::Error> for RequestError {
    fn from(error: ureq::Error) -> Self {
        match error {
            ureq::Error::Timeout(_) => Self::Timeout,
            _ => Self::Invalid,
        }
    }
}

impl Client {
    // Only an already validated, authenticated loopback Controller constructs this.
    pub(crate) fn new(address: Url, secret: &str) -> Self {
        Self {
            address,
            authorization: format!("Bearer {secret}"),
            agent: agent_config(REQUEST_TIMEOUT).into(),
        }
    }

    // route-http-check-v1 is an opt-in core contract. A check is a single status-only
    // HTTPS GET on the exact concrete, non-direct adapter identified by name and
    // provider. The core must atomically pin that adapter in the advertised generation
    // and reject a stale generation, absent, ambiguous, or direct route before dialing.
    // The core maps these IDs to these endpoints only:
    //   gstatic204    https://www.gstatic.com/generate_204
    //   cloudflare204 https://cp.cloudflare.com/generate_204
    // It verifies TLS and sends the target through the adapter, including target
    // name resolution. It permits no redirect, retry, body read, or health/selection
    // update, caps upstream headers at 4 KiB, and includes queue time in timeout_ms.
    // Credentials and local request metadata must stay on loopback.
    fn capability(&self) -> Result<Option<u64>, RequestError> {
        let url = self.address.join("version").expect("controller URL");
        let reply = self
            .agent
            .get(url.as_str())
            .header("Authorization", &self.authorization)
            .call()?;
        Ok(match reply.status().as_u16() {
            200 => decode::<Version>(reply)?
                .capabilities
                .route_http_check_v1
                .map(|capability| capability.generation),
            404 | 405 => None,
            _ => return Err(RequestError::Invalid),
        })
    }

    fn check(&self, generation: u64, route: &Route, check: Check) -> Result<Status, RequestError> {
        let url = self
            .address
            .join("api/unlock/check")
            .expect("controller URL");
        let reply = self
            .agent
            .post(url.as_str())
            .header("Authorization", &self.authorization)
            .send_json(CheckRequest {
                generation,
                route,
                check,
                timeout_ms: CORE_TIMEOUT_MS,
            })?;
        Ok(match reply.status().as_u16() {
            200 => {
                let result = decode::<CheckReply>(reply)?;
                if &result.route != route
                    || result.check != check
                    || result.elapsed_ms > CORE_TIMEOUT_MS
                {
                    return Err(RequestError::Invalid);
                }
                match result.status {
                    204 => Status::Passed {
                        elapsed_ms: result.elapsed_ms,
                    },
                    code @ 200..=599 => Status::Http(code),
                    _ => return Err(RequestError::Invalid),
                }
            }
            422 | 502 | 503 => Status::Failed,
            504 => Status::TimedOut,
            _ => return Err(RequestError::Invalid),
        })
    }
}

fn agent_config(timeout: Duration) -> ureq::config::Config {
    ureq::Agent::config_builder()
        .proxy(None)
        .max_redirects(0)
        .max_response_header_size(MAX_HEADER_BYTES)
        .http_status_as_error(false)
        .accept_encoding("identity")
        .max_idle_connections(0)
        .timeout_global(Some(timeout))
        .build()
}

fn decode<T: DeserializeOwned>(mut response: Response<Body>) -> Result<T, RequestError> {
    if response.headers().contains_key("content-encoding") {
        return Err(RequestError::Invalid);
    }
    let body = response
        .body_mut()
        .with_config()
        .limit(MAX_BODY_BYTES)
        .read_to_vec()?;
    serde_json::from_slice(&body).map_err(|_| RequestError::Invalid)
}

#[cfg(test)]
mod tests;
