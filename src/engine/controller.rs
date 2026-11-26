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
    #[serde(default)]
    pub host: String,
    #[serde(default, rename = "destinationIP")]
    pub destination_ip: String,
    #[serde(default)]
    pub destination_port: serde_json::Value,
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
        let providers: Providers = self.get(self.endpoint(&["providers", "proxies"]))?;
        for (provider, value) in providers.providers {
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

    fn fixture(
        responses: Vec<(&'static str, &'static str)>,
    ) -> (Controller, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let controller =
            Controller::new(listener.local_addr().unwrap(), "fake-secret".into()).unwrap();
        let worker = thread::spawn(move || {
            for (expected, body) in responses {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                let mut reader = BufReader::new(&mut stream);
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                assert!(line.starts_with(expected), "{line:?}");
                let mut authorized = false;
                loop {
                    line.clear();
                    if reader.read_line(&mut line).unwrap() == 0 || line == "\r\n" {
                        break;
                    }
                    authorized |= line
                        .trim()
                        .eq_ignore_ascii_case("authorization: Bearer fake-secret");
                }
                assert!(authorized);
                write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{body}", body.len()).unwrap();
            }
        });
        (controller, worker)
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
