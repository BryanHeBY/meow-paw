use serde::{Deserialize, de::DeserializeOwned};
use std::io::{BufRead, BufReader, Read as _};
use std::{collections::BTreeMap, fmt, net::SocketAddr, time::Duration};
use url::Url;

#[derive(Debug, Default, Clone, Deserialize)]
pub struct Proxy {
    #[serde(rename = "type", default)]
    pub kind: String,
    pub all: Option<Vec<String>>,
    pub now: Option<String>,
    #[serde(default)]
    pub hidden: bool,
    pub alive: Option<bool>,
    pub udp: Option<bool>,
    #[serde(default)]
    pub history: Vec<DelaySample>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DelaySample {
    delay: u16,
}

impl Proxy {
    /// The controller orders samples oldest to newest; zero means a failed check.
    pub fn latest_delay_ms(&self) -> Option<u16> {
        self.history
            .last()
            .map(|sample| sample.delay)
            .filter(|ms| *ms > 0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Origin {
    Local,
    Provider(String),
}

#[derive(Debug, Clone)]
pub struct Node {
    pub origin: Origin,
    pub name: String,
    pub proxy: Proxy,
}

#[derive(Debug, Default, Clone)]
pub struct Inventory {
    pub groups: BTreeMap<String, Proxy>,
    pub nodes: Vec<Node>,
    pub providers: BTreeMap<String, ProviderStatus>,
    /// A failed provider read leaves route data usable but ownership unknown.
    pub provider_error: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderStatus {
    #[serde(default)]
    pub vehicle_type: String,
    pub updated_at: Option<String>,
    pub subscription_info: Option<ProviderUsage>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleProviderStatus {
    #[serde(default)]
    pub behavior: String,
    #[serde(default)]
    pub format: String,
    #[serde(default)]
    pub rule_count: Option<usize>,
    #[serde(default)]
    pub updated_at: Option<String>,
    #[serde(default)]
    pub vehicle_type: String,
}

#[derive(Deserialize)]
struct RuleProvidersResponse {
    providers: BTreeMap<String, RuleProviderStatus>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ProviderUsage {
    pub upload: u64,
    pub download: u64,
    pub total: u64,
    pub expire: u64,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Traffic {
    pub up: u64,
    pub down: u64,
    #[serde(default, rename = "upTotal")]
    pub up_total: u64,
    #[serde(default, rename = "downTotal")]
    pub down_total: u64,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Connection {
    pub id: String,
    #[serde(default, deserialize_with = "optional_u64")]
    pub upload: Option<u64>,
    #[serde(default, deserialize_with = "optional_u64")]
    pub download: Option<u64>,
    #[serde(default)]
    pub metadata: Metadata,
    #[serde(default)]
    pub chains: Vec<String>,
    #[serde(default)]
    pub rule: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Metadata {
    #[serde(default, deserialize_with = "optional_network")]
    pub network: Option<String>,
    #[serde(default)]
    pub host: String,
    #[serde(
        default,
        rename = "destinationIP",
        deserialize_with = "nullable_destination"
    )]
    pub destination_ip: String,
    #[serde(default)]
    pub destination_port: serde_json::Value,
}

fn nullable_destination<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<String, D::Error> {
    Ok(Option::<String>::deserialize(deserializer)?.unwrap_or_default())
}

fn optional_u64<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<u64>, D::Error> {
    Ok(serde_json::Value::deserialize(deserializer)?.as_u64())
}

fn optional_network<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<String>, D::Error> {
    const MAX_NETWORK_BYTES: usize = 64;
    Ok(match serde_json::Value::deserialize(deserializer)? {
        serde_json::Value::String(network)
            if !network.is_empty() && network.len() <= MAX_NETWORK_BYTES =>
        {
            Some(network)
        }
        _ => None,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RoutingMode {
    Rule,
    Global,
    Direct,
}

impl RoutingMode {
    pub const ALL: [Self; 3] = [Self::Rule, Self::Global, Self::Direct];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Rule => "Rule",
            Self::Global => "Global",
            Self::Direct => "Direct",
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Rule {
    #[serde(rename = "type")]
    pub kind: String,
    pub payload: String,
    pub proxy: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct LogEntry {
    pub time: String,
    pub level: String,
    pub message: String,
}

#[derive(Debug)]
pub enum ControllerError {
    InvalidAddress,
    Unreachable,
    Status(u16),
    InvalidResponse,
    NotMeow,
}

impl fmt::Display for ControllerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidAddress => {
                f.write_str("Controller must be a loopback address with a port")
            }
            Self::Unreachable => f.write_str("Controller is unreachable or timed out"),
            Self::Status(code) => write!(f, "Controller returned HTTP {code}"),
            Self::InvalidResponse => f.write_str("Unexpected controller response"),
            Self::NotMeow => f.write_str("The endpoint did not identify itself as Meow"),
        }
    }
}

impl std::error::Error for ControllerError {}

#[derive(Clone)]
pub struct Controller {
    address: Url,
    agent: ureq::Agent,
    secret: String,
}

impl Controller {
    pub fn new(address: SocketAddr, secret: String) -> Result<Self, ControllerError> {
        if !address.ip().is_loopback() || address.port() == 0 {
            return Err(ControllerError::InvalidAddress);
        }
        let address = Url::parse(&format!("http://{address}/"))
            .map_err(|_| ControllerError::InvalidAddress)?;
        let config = ureq::Agent::config_builder()
            .proxy(None)
            .max_redirects(0)
            .timeout_global(Some(Duration::from_secs(3)))
            .build();
        Ok(Self {
            address,
            secret,
            agent: config.into(),
        })
    }

    #[cfg(any(target_os = "linux", windows, test))]
    pub(crate) fn unlock_client(&self) -> crate::unlock::Client {
        crate::unlock::Client::new(self.address.clone(), &self.secret)
    }

    fn endpoint(&self, parts: &[&str]) -> Url {
        let mut address = self.address.clone();
        address
            .path_segments_mut()
            .expect("HTTP URL")
            .pop_if_empty()
            .extend(parts);
        address
    }

    fn get<T: DeserializeOwned>(&self, url: Url) -> Result<T, ControllerError> {
        let mut request = self.agent.get(url.as_str());
        if !self.secret.is_empty() {
            request = request.header("Authorization", format!("Bearer {}", self.secret));
        }
        let mut response = request.call().map_err(http_error)?;
        if response.status().as_u16() != 200 {
            return Err(ControllerError::Status(response.status().as_u16()));
        }
        response
            .body_mut()
            .read_json()
            .map_err(|_| ControllerError::InvalidResponse)
    }

    pub fn verify(&self) -> Result<(), ControllerError> {
        #[derive(Deserialize)]
        struct Hello {
            hello: String,
        }
        let reply: Hello = self.get(self.endpoint(&[]))?;
        if reply.hello != "meow" {
            return Err(ControllerError::NotMeow);
        }
        Ok(())
    }

    pub fn inventory(&self) -> Result<Inventory, ControllerError> {
        #[derive(Deserialize)]
        struct RouteTable {
            proxies: BTreeMap<String, Proxy>,
        }
        #[derive(Deserialize)]
        struct Providers {
            providers: BTreeMap<String, Provider>,
        }
        #[derive(Deserialize)]
        struct Provider {
            proxies: Vec<NamedProxy>,
            #[serde(flatten)]
            status: ProviderStatus,
        }
        #[derive(Deserialize)]
        struct NamedProxy {
            name: String,
            #[serde(flatten)]
            proxy: Proxy,
        }
        let routes: RouteTable = self.get(self.endpoint(&["proxies"]))?;
        let mut inventory = Inventory::default();
        for (name, proxy) in routes.proxies {
            if proxy.all.is_some() {
                if !proxy.hidden {
                    inventory.groups.insert(name, proxy);
                }
            } else {
                inventory.nodes.push(Node {
                    origin: Origin::Local,
                    name,
                    proxy,
                });
            }
        }
        let providers: Providers = match self.get(self.endpoint(&["providers", "proxies"])) {
            Ok(providers) => providers,
            Err(error) => {
                inventory.provider_error = Some(error.to_string());
                return Ok(inventory);
            }
        };
        for (provider, value) in providers.providers {
            // Even a provider with no proxies yet has an independent status.
            inventory.providers.insert(provider.clone(), value.status);
            // Meow exposes membership by name, not by provider-qualified identity.
            inventory
                .nodes
                .extend(value.proxies.into_iter().map(|node| Node {
                    origin: Origin::Provider(provider.clone()),
                    name: node.name,
                    proxy: node.proxy,
                }));
        }
        Ok(inventory)
    }

    /// Refresh one running proxy provider without applying the profile again.
    /// A timeout is not proof the refresh failed; read inventory before retrying.
    pub fn refresh_provider(&self, provider: &str) -> Result<(), ControllerError> {
        let url = self.endpoint(&["providers", "proxies", provider]);
        let mut request = self.agent.put(url.as_str());
        if !self.secret.is_empty() {
            request = request.header("Authorization", format!("Bearer {}", self.secret));
        }
        let reply = request
            .config()
            .timeout_global(Some(Duration::from_secs(20)))
            .build()
            .send_empty()
            .map_err(http_error)?;
        if reply.status().as_u16() != 204 {
            return Err(ControllerError::Status(reply.status().as_u16()));
        }
        Ok(())
    }

    pub fn select(&self, group: &str, member: &str) -> Result<(), ControllerError> {
        let url = self.endpoint(&["proxies", group]);
        let mut request = self.agent.put(url.as_str());
        if !self.secret.is_empty() {
            request = request.header("Authorization", format!("Bearer {}", self.secret));
        }
        let reply = request
            .send_json(serde_json::json!({"name": member}))
            .map_err(http_error)?;
        if reply.status().as_u16() != 204 {
            return Err(ControllerError::Status(reply.status().as_u16()));
        }
        Ok(())
    }

    pub(crate) fn reload_config(&self, path: &std::path::Path) -> Result<(), ControllerError> {
        let path = path.to_str().ok_or(ControllerError::InvalidResponse)?;
        let url = self.endpoint(&["configs"]);
        let reply = self
            .agent
            .put(url.as_str())
            .config()
            .timeout_global(Some(Duration::from_secs(12)))
            .build()
            .header("Authorization", format!("Bearer {}", self.secret))
            .send_json(serde_json::json!({"path": path}))
            .map_err(http_error)?;
        if reply.status().as_u16() != 204 {
            return Err(ControllerError::Status(reply.status().as_u16()));
        }
        Ok(())
    }

    pub fn delay(&self, origin: &Origin, name: &str, probe: &Url) -> Result<u32, ControllerError> {
        #[derive(Deserialize)]
        struct Delay {
            delay: u32,
        }
        let mut url = match origin {
            Origin::Local => self.endpoint(&["proxies", name, "delay"]),
            Origin::Provider(provider) => {
                self.endpoint(&["providers", "proxies", provider, name, "healthcheck"])
            }
        };
        url.query_pairs_mut()
            .append_pair("url", probe.as_str())
            .append_pair("timeout", "2000");
        Ok(self.get::<Delay>(url)?.delay)
    }

    pub fn mode(&self) -> Result<RoutingMode, ControllerError> {
        #[derive(Deserialize)]
        struct Config {
            mode: RoutingMode,
        }
        Ok(self.get::<Config>(self.endpoint(&["configs"]))?.mode)
    }

    pub fn set_mode(&self, mode: RoutingMode) -> Result<(), ControllerError> {
        let url = self.endpoint(&["configs"]);
        let mut request = self.agent.patch(url.as_str());
        if !self.secret.is_empty() {
            request = request.header("Authorization", format!("Bearer {}", self.secret));
        }
        let response = request
            .send_json(serde_json::json!({"mode": mode.label().to_ascii_lowercase()}))
            .map_err(http_error)?;
        if response.status().as_u16() != 204 {
            return Err(ControllerError::Status(response.status().as_u16()));
        }
        Ok(())
    }

    pub fn rules(&self) -> Result<Vec<Rule>, ControllerError> {
        #[derive(Deserialize)]
        struct Rules {
            rules: Vec<Rule>,
        }
        Ok(self.get::<Rules>(self.endpoint(&["rules"]))?.rules)
    }

    pub fn rule_providers(&self) -> Result<BTreeMap<String, RuleProviderStatus>, ControllerError> {
        Ok(self
            .get::<RuleProvidersResponse>(self.endpoint(&["providers", "rules"]))?
            .providers)
    }

    pub fn read_log_stream(
        &self,
        sink: impl FnMut(LogEntry),
        cancelled: impl Fn() -> bool,
    ) -> Result<(), ControllerError> {
        self.read_log_stream_with_connected(sink, cancelled, || {})
    }

    pub(crate) fn read_log_stream_with_connected(
        &self,
        mut sink: impl FnMut(LogEntry),
        cancelled: impl Fn() -> bool,
        mut connected: impl FnMut(),
    ) -> Result<(), ControllerError> {
        // A stream needs an idle read deadline, not the short global command deadline.
        let config = ureq::Agent::config_builder()
            .proxy(None)
            .max_redirects(0)
            .timeout_connect(Some(Duration::from_secs(2)))
            .timeout_per_read(Some(Duration::from_secs(4)))
            .build();
        let agent: ureq::Agent = config.into();
        let mut url = self.endpoint(&["logs"]);
        url.query_pairs_mut().append_pair("format", "structured");
        let mut request = agent.get(url.as_str());
        if !self.secret.is_empty() {
            request = request.header("Authorization", format!("Bearer {}", self.secret));
        }
        let mut response = request.call().map_err(http_error)?;
        if response.status().as_u16() != 200 {
            return Err(ControllerError::Status(response.status().as_u16()));
        }
        connected();

        let mut reader = BufReader::new(response.body_mut().as_reader());
        read_log_frames(&mut reader, &mut sink, &cancelled)
    }

    pub fn traffic(&self) -> Result<Traffic, ControllerError> {
        // This endpoint is an open NDJSON stream in Meow 0.29; do not
        // read the whole body to EOF to sample one frame.
        let url = self.endpoint(&["traffic"]);
        let mut request = self.agent.get(url.as_str());
        if !self.secret.is_empty() {
            request = request.header("Authorization", format!("Bearer {}", self.secret));
        }
        let mut response = request.call().map_err(http_error)?;
        if response.status().as_u16() != 200 {
            return Err(ControllerError::Status(response.status().as_u16()));
        }
        let mut reader = BufReader::new(response.body_mut().as_reader().take(4096));
        let mut frame = Vec::new();
        reader
            .read_until(b'\n', &mut frame)
            .map_err(|_| ControllerError::Unreachable)?;
        serde_json::from_slice(&frame).map_err(|_| ControllerError::InvalidResponse)
    }

    pub fn close_connection(&self, id: &str) -> Result<(), ControllerError> {
        if id.is_empty() {
            return Err(ControllerError::InvalidResponse);
        }
        let url = self.endpoint(&["connections", id]);
        let mut request = self.agent.delete(url.as_str());
        if !self.secret.is_empty() {
            request = request.header("Authorization", format!("Bearer {}", self.secret));
        }
        let response = request.call().map_err(http_error)?;
        if response.status().as_u16() != 204 {
            return Err(ControllerError::Status(response.status().as_u16()));
        }
        Ok(())
    }

    pub fn connections(&self) -> Result<Vec<Connection>, ControllerError> {
        #[derive(Deserialize)]
        struct Snapshot {
            connections: Option<Vec<Connection>>,
        }
        Ok(self
            .get::<Snapshot>(self.endpoint(&["connections"]))?
            .connections
            .unwrap_or_default())
    }
}

fn read_log_frames(
    reader: &mut impl BufRead,
    sink: &mut impl FnMut(LogEntry),
    cancelled: &impl Fn() -> bool,
) -> Result<(), ControllerError> {
    const MAX_FRAME_BYTES: usize = 16 * 1024;
    let mut frame = Vec::new();
    while !cancelled() {
        let remaining = MAX_FRAME_BYTES - frame.len();
        if remaining == 0 {
            return Err(ControllerError::InvalidResponse);
        }
        match reader
            .by_ref()
            .take(remaining as u64)
            .read_until(b'\n', &mut frame)
        {
            Ok(0) if frame.is_empty() => return Ok(()),
            Ok(_) if frame.last() == Some(&b'\n') => {
                if let Ok(log) = serde_json::from_slice(&frame) {
                    sink(log);
                }
                frame.clear();
            }
            Ok(_) => return Err(ControllerError::InvalidResponse),
            Err(error) if is_log_idle_timeout(&error) => continue,
            Err(_) => return Err(ControllerError::Unreachable),
        }
    }
    Ok(())
}

fn is_log_idle_timeout(error: &std::io::Error) -> bool {
    error.kind() == std::io::ErrorKind::TimedOut
        || matches!(
            error
                .get_ref()
                .and_then(|e| e.downcast_ref::<ureq::Error>()),
            Some(ureq::Error::Timeout(ureq::Timeout::PerRead))
        )
}

fn http_error(error: ureq::Error) -> ControllerError {
    match error {
        ureq::Error::StatusCode(code) => ControllerError::Status(code),
        _ => ControllerError::Unreachable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{BufRead, BufReader, Write},
        net::TcpListener,
        thread,
    };

    #[test]
    fn log_frames_survive_idle_timeouts_and_keep_partial_frames() {
        use std::io::{self, Read};
        struct Script {
            step: usize,
        }
        impl Read for Script {
            fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
                self.step += 1;
                let data: &[u8] = match self.step {
                    1 => b"{\"time\":\"now\",\"level\":\"info\",",
                    2 => return Err(io::Error::new(io::ErrorKind::TimedOut, "idle")),
                    3 => return Err(ureq::Error::Timeout(ureq::Timeout::PerRead).into_io()),
                    4 => b"\"message\":\"after idle\"}\n",
                    _ => return Ok(0),
                };
                buf[..data.len()].copy_from_slice(data);
                Ok(data.len())
            }
        }
        let mut reader = BufReader::new(Script { step: 0 });
        let mut logs = Vec::new();
        read_log_frames(&mut reader, &mut |entry| logs.push(entry), &|| false).unwrap();
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].message, "after idle");
        assert!(!is_log_idle_timeout(&io::Error::new(
            io::ErrorKind::ConnectionReset,
            "reset"
        )));
        assert!(!is_log_idle_timeout(
            &ureq::Error::Timeout(ureq::Timeout::RecvBody).into_io()
        ));
    }

    #[test]
    fn log_frames_distinguish_eof_failure_and_cancellation() {
        use std::io::{self, Read};
        struct Failure;
        impl Read for Failure {
            fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
                Err(io::Error::new(io::ErrorKind::ConnectionReset, "reset"))
            }
        }
        assert!(matches!(
            read_log_frames(&mut BufReader::new(Failure), &mut |_| {}, &|| false),
            Err(ControllerError::Unreachable)
        ));
        assert!(read_log_frames(&mut BufReader::new(Failure), &mut |_| {}, &|| true).is_ok());
        assert!(read_log_frames(&mut BufReader::new(&b""[..]), &mut |_| {}, &|| false).is_ok());
        assert!(matches!(
            read_log_frames(&mut BufReader::new(&b"partial"[..]), &mut |_| {}, &|| false),
            Err(ControllerError::InvalidResponse)
        ));
        assert!(matches!(
            read_log_frames(
                &mut BufReader::new(&vec![b'x'; 16 * 1024][..]),
                &mut |_| {},
                &|| false
            ),
            Err(ControllerError::InvalidResponse)
        ));
    }

    #[test]
    fn decodes_connection_details_from_snapshot_fixture() {
        #[derive(Deserialize)]
        struct Snapshot {
            connections: Vec<Connection>,
        }
        let snapshot: Snapshot = serde_json::from_str(
            r#"{"uploadTotal":100,"downloadTotal":200,"connections":[{"id":"one","upload":0,"download":18446744073709551615,"metadata":{"network":"tcp","host":"example.invalid","destinationIP":"192.0.2.1","destinationPort":443},"chains":["DIRECT"],"rule":"MATCH"}]}"#,
        )
        .unwrap();
        let connection = &snapshot.connections[0];
        assert_eq!(connection.id, "one");
        assert_eq!(connection.upload, Some(0));
        assert_eq!(connection.download, Some(u64::MAX));
        assert_eq!(connection.metadata.network.as_deref(), Some("tcp"));
        assert_eq!(connection.metadata.host, "example.invalid");
        assert_eq!(connection.metadata.destination_ip, "192.0.2.1");
        assert_eq!(connection.metadata.destination_port, 443);
        assert_eq!(connection.chains, ["DIRECT"]);
        assert_eq!(connection.rule, "MATCH");
    }

    #[test]
    fn absent_and_null_connection_details_are_none() {
        let connections: Vec<Connection> = serde_json::from_str(
            r#"[{"id":"absent"},{"id":"null","upload":null,"download":null,"metadata":{"network":null,"host":"example.invalid","destinationIP":null}}]"#,
        )
        .unwrap();
        for connection in connections {
            assert_eq!(connection.upload, None);
            assert_eq!(connection.download, None);
            assert_eq!(connection.metadata.network, None);
            assert!(connection.metadata.destination_ip.is_empty());
        }
    }

    #[test]
    fn malformed_connection_details_do_not_discard_rows() {
        let connections: Vec<Connection> = serde_json::from_str(
            r#"[
                {"id":"negative","upload":-1,"download":1.0,"metadata":{"network":""}},
                {"id":"wrong types","upload":"12","download":true,"metadata":{"network":42}},
                {"id":"nested","upload":{},"download":[1],"metadata":{"network":["tcp"]}},
                {"id":"overflow","upload":18446744073709551616,"download":-2,"metadata":{"network":{}}}
            ]"#,
        )
        .unwrap();
        assert_eq!(connections.len(), 4);
        for connection in connections {
            assert_eq!(connection.upload, None, "{}", connection.id);
            assert_eq!(connection.download, None, "{}", connection.id);
            assert_eq!(connection.metadata.network, None, "{}", connection.id);
        }

        for (network, expected) in [("a".repeat(64), true), ("é".repeat(33), false)] {
            let connection: Connection = serde_json::from_value(serde_json::json!({
                "id": "length", "upload": 7, "download": "bad", "metadata": {"network": network}
            }))
            .unwrap();
            assert_eq!(connection.upload, Some(7));
            assert_eq!(connection.download, None);
            assert_eq!(connection.metadata.network.is_some(), expected);
        }
    }

    #[test]
    fn decodes_rule_provider_response_with_empty_and_partial_statuses() {
        let empty: RuleProvidersResponse = serde_json::from_str(r#"{"providers":{}}"#).unwrap();
        assert!(empty.providers.is_empty());
        let response: RuleProvidersResponse = serde_json::from_str(
            r#"{"providers":{"map key":{"name":"ignored","behavior":"domain","format":"yaml","ruleCount":12,"updatedAt":"2026-11-26T04:00:00Z","vehicleType":"HTTP"},"partial":{"behavior":"ipcidr","ruleCount":null}}}"#,
        )
        .unwrap();
        let full = &response.providers["map key"];
        assert_eq!(full.behavior, "domain");
        assert_eq!(full.format, "yaml");
        assert_eq!(full.rule_count, Some(12));
        assert_eq!(full.updated_at.as_deref(), Some("2026-11-26T04:00:00Z"));
        assert_eq!(full.vehicle_type, "HTTP");
        let partial = &response.providers["partial"];
        assert_eq!(partial.behavior, "ipcidr");
        assert_eq!(partial.format, "");
        assert_eq!(partial.rule_count, None);
        assert_eq!(partial.updated_at, None);
        assert_eq!(partial.vehicle_type, "");
        assert!(serde_json::from_str::<RuleProvidersResponse>(r#"{}"#).is_err());
    }

    #[test]
    fn reads_rule_providers_and_reports_unavailable_endpoint() {
        let (controller, worker) = fixture(vec![
            ("GET /providers/rules HTTP/", r#"{"providers":{}}"#),
            ("GET /providers/rules HTTP/", "x"),
        ]);
        assert!(controller.rule_providers().unwrap().is_empty());
        assert!(matches!(
            controller.rule_providers(),
            Err(ControllerError::Status(503))
        ));
        worker.join().unwrap();
    }

    fn fixture(
        responses: Vec<(&'static str, &'static str)>,
    ) -> (Controller, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let controller =
            Controller::new(listener.local_addr().unwrap(), "fake-secret".into()).unwrap();
        let worker = thread::spawn(move || {
            for (expected, body) in responses {
                let (status, body) = match body {
                    "!" => ("204 No Content", ""),
                    "x" => ("503 Service Unavailable", ""),
                    body => ("200 OK", body),
                };
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                let mut reader = BufReader::new(&mut stream);
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                assert!(line.starts_with(expected), "{line:?}");
                let mut authorized = false;
                let mut content_length = 0;
                loop {
                    line.clear();
                    if reader.read_line(&mut line).unwrap() == 0 || line == "\r\n" {
                        break;
                    }
                    if let Some((name, value)) = line.split_once(':')
                        && name.eq_ignore_ascii_case("content-length")
                    {
                        content_length = value.trim().parse::<usize>().unwrap();
                    }
                    authorized |= line
                        .trim()
                        .eq_ignore_ascii_case("authorization: Bearer fake-secret");
                }
                assert!(authorized);
                if content_length != 0 {
                    let mut request_body = vec![0; content_length];
                    std::io::Read::read_exact(&mut reader, &mut request_body).unwrap();
                    if expected.starts_with("PUT /proxies/G ") {
                        let payload: serde_json::Value =
                            serde_json::from_slice(&request_body).unwrap();
                        assert_eq!(payload, serde_json::json!({"name": "B"}));
                    }
                } else {
                    assert!(
                        !expected.starts_with("PUT /proxies/G "),
                        "missing selector body"
                    );
                }
                write!(stream, "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{body}", body.len()).unwrap();
            }
        });
        (controller, worker)
    }

    #[test]
    fn streams_ordered_structured_log_entries() {
        let (c, worker) = fixture(vec![(
            "GET /logs?format=structured HTTP/",
            "{\"time\":\"00:00:00\",\"level\":\"info\",\"message\":\"one\"}\ninvalid\n{\"time\":\"00:00:01\",\"level\":\"warn\",\"message\":\"two\"}\n",
        )]);
        let mut logs = Vec::new();
        c.read_log_stream(|log| logs.push(log), || false).unwrap();
        assert_eq!(logs.len(), 2);
        assert_eq!(logs[1].message, "two");
        worker.join().unwrap();
    }

    #[test]
    fn reads_mode_and_preserves_rule_order() {
        let (c, worker) = fixture(vec![
            (
                "GET /configs HTTP/",
                r#"{"mode":"global","tun":{"enable":false}}"#,
            ),
            (
                "GET /rules HTTP/",
                r#"{"rules":[{"type":"Domain","payload":"example.invalid","proxy":"LOCAL"},{"type":"Match","payload":"","proxy":"DIRECT"}]}"#,
            ),
        ]);
        assert_eq!(c.mode().unwrap(), RoutingMode::Global);
        let rules = c.rules().unwrap();
        assert_eq!(rules[0].kind, "Domain");
        assert_eq!(rules[0].payload, "example.invalid");
        assert_eq!(rules[1].proxy, "DIRECT");
        worker.join().unwrap();
    }

    #[test]
    fn rejects_non_loopback_and_encodes_path_segments() {
        assert!(Controller::new("0.0.0.0:9090".parse().unwrap(), String::new()).is_err());
        let c = Controller::new("127.0.0.1:9090".parse().unwrap(), String::new()).unwrap();
        assert_eq!(
            c.endpoint(&["proxies", "HK #1 / test"]).path(),
            "/proxies/HK%20%231%20%2F%20test"
        );
    }

    #[test]
    fn routes_continue_through_provider_outage_and_recover() {
        let (c, worker) = fixture(vec![
            (
                "GET /proxies HTTP/",
                r#"{"proxies":{"G":{"type":"Selector","all":["A","B"],"now":"A"},"A":{"type":"Direct"}}}"#,
            ),
            ("GET /providers/proxies HTTP/", "x"),
            ("PUT /proxies/G HTTP/", "!"),
            (
                "GET /proxies HTTP/",
                r#"{"proxies":{"G":{"type":"Selector","all":["A","B"],"now":"B"},"A":{"type":"Direct"}}}"#,
            ),
            ("GET /providers/proxies HTTP/", "x"),
            (
                "GET /proxies HTTP/",
                r#"{"proxies":{"G":{"type":"Selector","all":["A","B"],"now":"B"},"A":{"type":"Direct"}}}"#,
            ),
            (
                "GET /providers/proxies HTTP/",
                r#"{"providers":{"Feed":{"vehicleType":"HTTP","proxies":[{"name":"B","type":"Direct"}]}}}"#,
            ),
            (
                "GET /proxies HTTP/",
                r#"{"proxies":{"G":{"type":"Selector","all":["A","B"],"now":"A"},"A":{"type":"Direct"}}}"#,
            ),
            ("GET /providers/proxies HTTP/", "x"),
        ]);
        let first = c.inventory().unwrap();
        assert_eq!(first.groups["G"].now.as_deref(), Some("A"));
        assert!(first.provider_error.is_some());
        assert!(first.providers.is_empty());
        c.select("G", "B").unwrap();
        let changed = c.inventory().unwrap();
        assert_eq!(changed.groups["G"].now.as_deref(), Some("B"));
        assert!(changed.provider_error.is_some());
        let recovered = c.inventory().unwrap();
        assert!(recovered.provider_error.is_none());
        assert!(recovered.providers.contains_key("Feed"));
        assert!(
            recovered
                .nodes
                .iter()
                .any(|n| { n.name == "B" && n.origin == Origin::Provider("Feed".into()) })
        );
        let failed_again = c.inventory().unwrap();
        assert_eq!(failed_again.groups["G"].now.as_deref(), Some("A"));
        assert!(failed_again.provider_error.is_some());
        assert!(failed_again.providers.is_empty());
        assert!(failed_again.nodes.iter().all(|n| n.origin == Origin::Local));
        worker.join().unwrap();
    }

    #[test]
    fn keeps_provider_status_without_nodes_and_refreshes_only_that_provider() {
        let (c, worker) = fixture(vec![
            ("GET /proxies HTTP/", r#"{"proxies":{}}"#),
            (
                "GET /providers/proxies HTTP/",
                r#"{"providers":{"A / #":{"vehicleType":"HTTP","proxies":[],"updatedAt":"2026-11-26T04:00:00Z","subscriptionInfo":{"Upload":1,"Download":2,"Total":3,"Expire":4}}}}"#,
            ),
            ("PUT /providers/proxies/A%20%2F%20%23 HTTP/", "!"),
            ("PUT /providers/proxies/A%20%2F%20%23 HTTP/", "x"),
        ]);
        let inventory = c.inventory().unwrap();
        assert!(inventory.nodes.is_empty());
        let status = &inventory.providers["A / #"];
        assert_eq!(status.vehicle_type, "HTTP");
        assert_eq!(status.updated_at.as_deref(), Some("2026-11-26T04:00:00Z"));
        assert_eq!(
            status.subscription_info,
            Some(ProviderUsage {
                upload: 1,
                download: 2,
                total: 3,
                expire: 4
            })
        );
        c.refresh_provider("A / #").unwrap();
        assert!(matches!(
            c.refresh_provider("A / #"),
            Err(ControllerError::Status(503))
        ));
        worker.join().unwrap();
    }

    #[test]
    fn keeps_same_named_nodes_distinct_and_probes_provider_by_qualified_name() {
        let (c, worker) = fixture(vec![
            ("GET / HTTP/", r#"{"hello":"meow"}"#),
            (
                "GET /proxies HTTP/",
                r#"{"proxies":{"DUP":{"type":"Direct"},"Group":{"type":"Selector","all":["DUP"],"now":"DUP"}}}"#,
            ),
            (
                "GET /providers/proxies HTTP/",
                r#"{"providers":{"A":{"proxies":[{"name":"DUP","type":"Trojan"}]},"B":{"proxies":[{"name":"DUP","type":"Vless"}]}}}"#,
            ),
            (
                "GET /providers/proxies/B/DUP/healthcheck?",
                r#"{"delay":42}"#,
            ),
        ]);
        c.verify().unwrap();
        let data = c.inventory().unwrap();
        assert_eq!(data.nodes.len(), 3);
        assert_eq!(data.nodes[2].origin, Origin::Provider("B".into()));
        assert_eq!(data.groups["Group"].now.as_deref(), Some("DUP"));
        assert_eq!(
            c.delay(
                &data.nodes[2].origin,
                "DUP",
                &Url::parse("https://example.invalid/").unwrap()
            )
            .unwrap(),
            42
        );
        worker.join().unwrap();
    }
}
