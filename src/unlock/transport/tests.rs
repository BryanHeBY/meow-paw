use super::*;
use crate::engine::controller::Controller;
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    fmt,
    io::{Cursor, Read},
    sync::{
        Mutex,
        mpsc::{self, Receiver, Sender},
    },
    time::Instant,
};
use ureq::{
    Agent, Error,
    config::Config,
    http::Uri,
    unversioned::{
        resolver::{ResolvedSocketAddrs, Resolver},
        transport::{Buffers, ConnectionDetails, Connector, LazyBuffers, NextTimeout, Transport},
    },
};

const WAIT: Duration = Duration::from_secs(5);
const SECRET: &str = "synthetic-loopback-secret";
const LOCALHOST: &str = "127.0.0.1:32189";
const NO_CAPABILITY: &str = r#"{"version":"v0.29.0","meta":true}"#;
const GENERATION: u64 = 27;
const CAPABILITY: &str = r#"{"capabilities":{"route-http-check-v1":{"generation":27}}}"#;

type Reply = Box<dyn FnOnce(&Value, NextTimeout) -> Result<Vec<u8>, Error> + Send>;
type Captured = (String, Value);

// Exercise ureq's HTTP implementation on the controller's loopback URL without
// opening a socket. No test URI can resolve to a real destination.
struct Fixture {
    connector: FixtureConnector,
    requests: Receiver<Captured>,
}

#[derive(Clone)]
struct FixtureConnector {
    replies: Arc<Mutex<VecDeque<Reply>>>,
    requests: Sender<Captured>,
}

#[derive(Debug)]
struct Loopback;

impl Resolver for Loopback {
    fn resolve(&self, uri: &Uri, _: &Config, _: NextTimeout) -> Result<ResolvedSocketAddrs, Error> {
        assert_eq!(uri.scheme_str(), Some("http"));
        assert_eq!(uri.authority().unwrap().as_str(), LOCALHOST);
        let mut addresses = self.empty();
        addresses.push(LOCALHOST.parse().unwrap());
        Ok(addresses)
    }
}

impl Connector for FixtureConnector {
    type Out = FixtureTransport;

    fn connect(
        &self,
        details: &ConnectionDetails,
        _: Option<()>,
    ) -> Result<Option<Self::Out>, Error> {
        assert!(details.config.proxy().is_none());
        assert_eq!(details.config.max_redirects(), 0);
        assert_eq!(details.config.max_response_header_size(), MAX_HEADER_BYTES);
        Ok(Some(FixtureTransport {
            reply: Mutex::new(self.replies.lock().unwrap().pop_front()),
            requests: self.requests.clone(),
            buffers: LazyBuffers::new(
                details.config.input_buffer_size(),
                details.config.output_buffer_size(),
            ),
            request: Vec::new(),
            response: Cursor::default(),
        }))
    }
}

struct FixtureTransport {
    reply: Mutex<Option<Reply>>,
    requests: Sender<Captured>,
    buffers: LazyBuffers,
    request: Vec<u8>,
    response: Cursor<Vec<u8>>,
}

impl fmt::Debug for FixtureConnector {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("FixtureConnector")
    }
}

impl fmt::Debug for FixtureTransport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("FixtureTransport")
    }
}

impl Transport for FixtureTransport {
    fn buffers(&mut self) -> &mut dyn Buffers {
        &mut self.buffers
    }

    fn transmit_output(&mut self, amount: usize, _: NextTimeout) -> Result<(), Error> {
        self.request
            .extend_from_slice(&self.buffers.output()[..amount]);
        Ok(())
    }

    fn await_input(&mut self, timeout: NextTimeout) -> Result<bool, Error> {
        if !self.request.is_empty() {
            let request = std::str::from_utf8(&self.request).unwrap();
            let (headers, body) = request.split_once("\r\n\r\n").unwrap();
            let mut headers = headers.lines();
            let path = headers.next().unwrap().to_owned();
            assert!(
                headers
                    .any(|line| line
                        .eq_ignore_ascii_case(&format!("authorization: Bearer {SECRET}")))
            );
            let body = if body.is_empty() {
                Value::Null
            } else {
                serde_json::from_str(body).unwrap()
            };
            self.requests.send((path, body.clone())).unwrap();
            self.request.clear();
            let reply = self
                .reply
                .get_mut()
                .unwrap()
                .take()
                .expect("unexpected controller request");
            self.response = Cursor::new(reply(&body, timeout.for_read())?);
        }
        let count = self.response.read(self.buffers.input_append_buf())?;
        self.buffers.input_appended(count);
        Ok(count != 0)
    }

    fn is_open(&mut self) -> bool {
        false
    }
}

impl Fixture {
    fn new(replies: Vec<Reply>) -> Self {
        let (sender, requests) = mpsc::channel();
        Self {
            connector: FixtureConnector {
                replies: Arc::new(Mutex::new(replies.into())),
                requests: sender,
            },
            requests,
        }
    }

    fn client(&self) -> Client {
        self.client_with_timeout(REQUEST_TIMEOUT)
    }

    fn client_with_timeout(&self, timeout: Duration) -> Client {
        let mut client = Controller::new(LOCALHOST.parse().unwrap(), SECRET.into())
            .unwrap()
            .unlock_client();
        client.agent = Agent::with_parts(agent_config(timeout), self.connector.clone(), Loopback);
        client
    }

    fn next(&self, path: &str) -> Value {
        let (actual, body) = self.requests.recv_timeout(WAIT).unwrap();
        assert_eq!(actual, path);
        body
    }

    fn finish(self) {
        assert!(self.requests.try_recv().is_err());
        assert!(self.connector.replies.lock().unwrap().is_empty());
    }
}

fn response(status: u16, headers: &str, body: &str) -> Vec<u8> {
    format!(
        "HTTP/1.1 {status} Fixture\r\nContent-Length: {}\r\nConnection: close\r\n{headers}\r\n{body}",
        body.len()
    )
    .into_bytes()
}

fn reply(status: u16, body: &'static str) -> Reply {
    Box::new(move |_, _| Ok(response(status, "", body)))
}

fn checked(status: u16, elapsed_ms: u32) -> Reply {
    Box::new(move |request, _| {
        let body = json!({
            "route": request["route"],
            "check": request["check"],
            "status": status,
            "elapsed_ms": elapsed_ms,
        });
        Ok(response(200, "", &body.to_string()))
    })
}

fn named(name: &str) -> Route {
    Route::new(name.into(), Origin::Local).unwrap()
}

fn run(client: &Client, routes: Vec<Route>) -> State {
    let mut state = State::default();
    let job = state.begin(routes).unwrap();
    state.finish(job.run(client));
    state
}

#[test]
fn legacy_core_cannot_dispatch_checks() {
    let fixture = Fixture::new(vec![reply(200, NO_CAPABILITY)]);
    let client = fixture.client();
    let state = run(&client, vec![named("Fixture route")]);
    fixture.next("GET /version HTTP/1.1");
    assert_eq!(state.phase, Phase::Unsupported);
    assert!(state.rows.is_empty());
    assert!(!state.is_busy());
    fixture.finish();
}

#[test]
fn capability_requires_a_route_generation() {
    let fixture = Fixture::new(vec![reply(
        200,
        r#"{"capabilities":{"route-http-check-v1":{}}}"#,
    )]);
    let state = run(&fixture.client(), vec![named("Fixture route")]);
    assert_eq!(state.phase, Phase::Unavailable);
    assert!(state.rows.is_empty());
    fixture.next("GET /version HTTP/1.1");
    fixture.finish();
}

#[test]
fn selected_routes_are_ranked_using_only_verified_http_results() {
    let fixture = Fixture::new(vec![
        reply(200, CAPABILITY),
        checked(204, 20),
        checked(204, 25),
        checked(204, 3),
        checked(204, 7),
        checked(204, 2),
        checked(451, 4),
        reply(502, ""),
        reply(504, ""),
    ]);
    let remote = Route::new("Fast".into(), Origin::Provider("Fixture provider".into())).unwrap();
    let routes = vec![named("Slow"), remote, named("Partial"), named("Failed")];
    let mut state = State::default();
    assert!(!state.is_busy());
    let job = state.begin(routes.clone()).unwrap();
    assert!(state.is_busy());
    assert!(state.begin(vec![named("Overlap")]).is_none());
    state.finish(job.run(&fixture.client()));

    fixture.next("GET /version HTTP/1.1");
    for route in routes {
        for check in CHECKS {
            assert_eq!(
                fixture.next("POST /api/unlock/check HTTP/1.1"),
                json!({
                    "generation": GENERATION,
                    "route": route,
                    "check": check,
                    "timeout_ms": CORE_TIMEOUT_MS,
                })
            );
        }
    }
    assert_eq!(state.phase, Phase::Complete);
    assert_eq!(
        state
            .rows
            .iter()
            .map(|row| row.route.as_str())
            .collect::<Vec<_>>(),
        ["Fast", "Slow", "Partial", "Failed"]
    );
    assert_eq!(
        state.rows[0].checks,
        [
            Status::Passed { elapsed_ms: 3 },
            Status::Passed { elapsed_ms: 7 }
        ]
    );
    assert_eq!(
        state.rows[2].checks,
        [Status::Passed { elapsed_ms: 2 }, Status::Http(451)]
    );
    assert_eq!(state.rows[3].checks, [Status::Failed, Status::TimedOut]);
    fixture.finish();
}

#[test]
fn invalid_reply_or_redirect_stops_without_following_or_retrying() {
    let redirect = |_: &Value, _| Ok(response(307, "Location: /unexpected\r\n", ""));
    let wrong_route = |_: &Value, _| {
        let body = json!({
            "route": named("Different"),
            "check": CHECKS[0],
            "status": 204,
            "elapsed_ms": 1,
        });
        Ok(response(200, "", &body.to_string()))
    };
    let oversized_body = |_: &Value, _| {
        let body = format!("{{}}{}", " ".repeat(MAX_BODY_BYTES as usize));
        Ok(response(200, "", &body))
    };
    let oversized_headers = |_: &Value, _| {
        let headers = format!("X-Fixture: {}\r\n", "a".repeat(MAX_HEADER_BYTES));
        Ok(response(200, &headers, "{}"))
    };
    let first_replies: Vec<Reply> = vec![
        Box::new(redirect),
        Box::new(wrong_route),
        Box::new(oversized_body),
        Box::new(oversized_headers),
        reply(404, ""),
        reply(409, ""),
    ];
    for first in first_replies {
        let fixture = Fixture::new(vec![reply(200, CAPABILITY), first]);
        let state = run(&fixture.client(), vec![named("Chosen")]);
        assert_eq!(state.phase, Phase::Unavailable);
        assert_eq!(state.rows[0].checks, [Status::Failed, Status::NotRun]);
        fixture.next("GET /version HTTP/1.1");
        fixture.next("POST /api/unlock/check HTTP/1.1");
        fixture.finish();
    }
}

#[test]
fn client_timeout_stops_the_run_and_preserves_the_timeout_status() {
    let (release, wait) = mpsc::channel::<()>();
    let fixture = Fixture::new(vec![
        reply(200, CAPABILITY),
        Box::new(move |_, timeout| {
            assert_eq!(
                wait.recv_timeout(*timeout.after),
                Err(mpsc::RecvTimeoutError::Timeout)
            );
            Err(Error::Timeout(timeout.reason))
        }),
    ]);
    let client = fixture.client_with_timeout(Duration::from_millis(250));
    let start = Instant::now();
    let state = run(&client, vec![named("Slow")]);
    assert!(start.elapsed() < Duration::from_secs(2));
    assert_eq!(state.phase, Phase::Unavailable);
    assert_eq!(state.rows[0].checks, [Status::TimedOut, Status::NotRun]);
    fixture.next("GET /version HTTP/1.1");
    fixture.next("POST /api/unlock/check HTTP/1.1");
    drop(release);
    fixture.finish();
}

#[test]
fn cancellation_discards_inflight_reply_and_prevents_further_checks() {
    let (release, wait) = mpsc::channel::<()>();
    let fixture = Fixture::new(vec![
        reply(200, CAPABILITY),
        checked(204, 3),
        Box::new(move |request, timeout| {
            wait.recv_timeout(WAIT).unwrap();
            checked(204, 7)(request, timeout)
        }),
    ]);
    let mut state = State::default();
    let job = state.begin(vec![named("One"), named("Two")]).unwrap();
    let (send, receive) = mpsc::channel();
    assert!(job.dispatch(fixture.client(), send).is_ok());
    fixture.next("GET /version HTTP/1.1");
    fixture.next("POST /api/unlock/check HTTP/1.1");
    fixture.next("POST /api/unlock/check HTTP/1.1");
    state.cancel();
    assert_eq!(state.phase, Phase::Cancelling);
    assert!(state.begin(vec![named("Overlap")]).is_none());
    release.send(()).unwrap();
    state.finish(receive.recv_timeout(WAIT).unwrap());
    assert_eq!(state.phase, Phase::Cancelled);
    assert_eq!(
        state.rows[0].checks,
        [Status::Passed { elapsed_ms: 3 }, Status::Cancelled]
    );
    assert_eq!(state.rows[1].checks, [Status::Cancelled; 2]);
    assert!(!state.is_busy());
    fixture.finish();
}

#[test]
fn session_reset_retires_the_job_and_releases_its_slot_when_it_drains() {
    let mut state = State::default();
    let job = state.begin(vec![named("Old route")]).unwrap();
    state.reset();
    assert!(job.ticket.cancelled());
    assert!(state.begin(vec![named("New route")]).is_none());
    state.finish(job.without_checks(Phase::Cancelled));
    assert_eq!(state.phase, Phase::Idle);
    assert!(state.rows.is_empty());
    assert!(state.begin(vec![named("New route")]).is_some());
}

#[cfg(any(target_os = "linux", windows))]
#[test]
fn plan_excludes_builtins_and_ambiguous_routes_before_applying_the_cap() {
    use crate::engine::controller::{Node, Proxy};

    let mut inventory = Inventory::default();
    let mut group = Proxy::default();
    let mut members = Vec::new();
    for (name, kind, origin) in [
        ("direct alias", "Direct", Origin::Local),
        ("ambiguous", "Socks5", Origin::Local),
        ("ambiguous", "Socks5", Origin::Provider("fixture".into())),
        ("one", "Socks5", Origin::Local),
        ("two", "Socks5", Origin::Local),
        ("three", "Socks5", Origin::Local),
        ("four", "Socks5", Origin::Local),
        ("five", "Socks5", Origin::Local),
    ] {
        members.push(name.into());
        inventory.nodes.push(Node {
            name: name.into(),
            origin,
            proxy: Proxy {
                kind: kind.into(),
                ..Proxy::default()
            },
        });
    }
    group.all = Some(members);
    inventory.groups.insert("Choice".into(), group);
    assert_eq!(
        routes(&inventory, "Choice"),
        vec![named("one"), named("two"), named("three"), named("four")]
    );
}

#[cfg(any(target_os = "linux", windows))]
#[test]
fn manual_check_requires_an_explicitly_started_session() {
    let desktop = crate::app::Desktop::new("unused-synthetic-core".into()).unwrap();
    let initial = desktop.snapshot();
    let error = desktop
        .run_unlock(initial.probe_epoch, "Fixture")
        .unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::WouldBlock);
    assert!(matches!(
        desktop.snapshot().engine,
        crate::app::EngineStatus::Stopped
    ));
    assert_eq!(desktop.snapshot().unlock.phase, Phase::Idle);
}

#[cfg(target_os = "linux")]
#[test]
#[ignore = "requires an isolated synthetic Meow TCP controller and proxy witness"]
fn real_meow_controller_route_check_contract() {
    let address = std::env::var("PAW_TEST_ROUTE_CONTROLLER").expect("synthetic controller URL");
    let address = Url::parse(&address).unwrap();
    assert_eq!(address.scheme(), "http");
    assert_eq!(address.host_str(), Some("127.0.0.1"));
    let secret = "paw-route-fixture";
    let route = Route::new("selected".to_owned(), Origin::Local).unwrap();
    let direct = Route::new("DIRECT".to_owned(), Origin::Local).unwrap();

    let client = Client::new(address.clone(), secret);
    let generation = client.capability().unwrap().expect("route-http-check-v1");
    let unauthorized = Client::new(address, "wrong-fixture-token");
    assert!(unauthorized.capability().is_err());
    assert!(
        unauthorized
            .check(generation, &route, Check::Gstatic204)
            .is_err()
    );
    assert!(
        client
            .check(generation.wrapping_sub(1), &route, Check::Gstatic204)
            .is_err()
    );
    assert!(matches!(
        client
            .check(generation, &direct, Check::Gstatic204)
            .unwrap(),
        Status::Failed
    ));
    // The synthetic selected SOCKS5 adapter captures the hostname and rejects
    // the connection. The runner independently asserts exactly one such dial.
    assert!(matches!(
        client.check(generation, &route, Check::Gstatic204).unwrap(),
        Status::Failed
    ));
}
