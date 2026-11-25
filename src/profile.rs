//! Local profiles and the small set of fields owned by a managed Paw session.

use serde_json::{Map, Value, json};
use std::{error::Error, fmt, net::SocketAddrV4};

pub const MAX_PROFILE_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub path: String,
    pub message: &'static str,
}

#[derive(Debug)]
pub enum ProfileError {
    InvalidDocument,
    TooLarge,
    // Parser error strings may contain source snippets and credentials.
    InvalidYaml,
    UnsupportedField(String),
    InvalidSettings,
    Encode,
}

impl fmt::Display for ProfileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidDocument => f.write_str("Expected a YAML mapping"),
            Self::TooLarge => f.write_str("Profile exceeds the size limit"),
            Self::InvalidYaml => f.write_str("Could not parse the YAML profile"),
            Self::UnsupportedField(path) => {
                write!(f, "{path} is not supported in an isolated session")
            }
            Self::InvalidSettings => {
                f.write_str("Managed endpoints or controller secret are invalid")
            }
            Self::Encode => f.write_str("Could not generate the managed profile"),
        }
    }
}

impl Error for ProfileError {}

pub struct Profile {
    root: Map<String, Value>,
}

impl Profile {
    pub fn parse(source: &str) -> Result<Self, ProfileError> {
        if source.len() > MAX_PROFILE_BYTES {
            return Err(ProfileError::TooLarge);
        }
        let mut options = serde_saphyr::Options::default();
        options.strict_booleans = true;
        options.reject_unsupported_tags = true;
        match serde_saphyr::from_str_with_options::<Value>(source, options)
            .map_err(|_| ProfileError::InvalidYaml)?
        {
            Value::Object(root) => Ok(Self { root }),
            _ => Err(ProfileError::InvalidDocument),
        }
    }

    pub fn diagnostics(&self) -> Vec<Diagnostic> {
        self.root
            .get("proxies")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .enumerate()
            .filter(|(_, node)| node.get("type").and_then(Value::as_str) == Some("trojan"))
            .filter(|(_, node)| {
                !node.get("port").is_some_and(|value| {
                    let port = if let Some(text) = value.as_str() {
                        if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
                            return false;
                        }
                        text.parse::<u64>().ok()
                    } else {
                        value.as_u64()
                    };
                    port.is_some_and(|port| (1..=65535).contains(&port))
                })
            })
            .map(|(i, _)| Diagnostic {
                path: format!("proxies[{i}].port"),
                message: "Trojan requires a decimal port from 1 to 65535.",
            })
            .collect()
    }

    pub fn managed(
        &self,
        controller: SocketAddrV4,
        mixed_port: u16,
        secret: &str,
    ) -> Result<String, ProfileError> {
        if !controller.ip().is_loopback()
            || controller.port() == 0
            || mixed_port == 0
            || mixed_port == controller.port()
            || secret.is_empty()
        {
            return Err(ProfileError::InvalidSettings);
        }
        let mut root = self.root.clone();
        // M0 does not opt into script execution, native subscription writers,
        // alternate listeners, or provider-controlled local file access.
        for key in [
            "listeners",
            "subscriptions",
            "proxy-providers",
            "rule-providers",
            "script",
            "post-up",
            "post-down",
        ] {
            if root.get(key).is_some_and(|value| {
                !value.is_null()
                    && !value.as_object().is_some_and(Map::is_empty)
                    && !value.as_array().is_some_and(Vec::is_empty)
            }) {
                return Err(ProfileError::UnsupportedField(key.into()));
            }
            root.remove(key);
        }
        for key in [
            "port",
            "socks-port",
            "redir-port",
            "tproxy-port",
            "external-controller-tls",
            "external-controller-unix",
            "external-controller-pipe",
            "external-controller-cors",
            "external-ui",
            "external-ui-url",
            "external-ui-name",
            "interface-name",
        ] {
            root.remove(key);
        }
        if let Some(dns) = root.get_mut("dns").and_then(Value::as_object_mut) {
            dns.remove("listen");
        }
        root.insert("mixed-port".into(), json!(mixed_port));
        root.insert("allow-lan".into(), json!(false));
        root.insert("bind-address".into(), json!("127.0.0.1"));
        root.insert("external-controller".into(), json!(controller.to_string()));
        root.insert("secret".into(), json!(secret));
        root.insert("tun".into(), json!({"enable": false}));
        serde_saphyr::to_string(&root).map_err(|_| ProfileError::Encode)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn generated(source: &str) -> Value {
        let profile = Profile::parse(source).unwrap();
        let yaml = profile
            .managed("127.0.0.1:19200".parse().unwrap(), 19201, "local-test")
            .unwrap();
        Value::Object(Profile::parse(&yaml).unwrap().root)
    }

    #[test]
    fn resolves_aliases_and_does_not_coerce_plain_country_names() {
        let value = generated(
            "x-custom: &opts { country: NO, enabled-name: on, other: off }\nother: { <<: *opts }\n",
        );
        assert_eq!(value["x-custom"]["country"], "NO");
        assert_eq!(value["other"]["enabled-name"], "on");
        assert_eq!(value["other"]["other"], "off");
    }

    #[test]
    fn accepts_quoted_trojan_ports_and_keeps_hysteria_ranges() {
        let source = "proxies:\n  - {type: trojan, name: a, port: '443'}\n  - {type: hysteria2, name: b, ports: '443,7000-8000'}\n  - {type: trojan, name: c, port: 443}\n  - {type: trojan, name: bad, port: '65536'}\n";
        let diagnostics = Profile::parse(source).unwrap().diagnostics();
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].path, "proxies[3].port");
        assert_eq!(generated(source)["proxies"][0]["port"], "443");
        assert_eq!(generated(source)["proxies"][1]["ports"], "443,7000-8000");
    }

    #[test]
    fn controls_listeners_without_losing_other_dns_keys() {
        let value = generated(
            "dns: { listen: '0.0.0.0:53', enable: true }\nallow-lan: true\ntun: {enable: true}\nport: 80\nexternal-controller-unix: /tmp/a.sock\n",
        );
        assert_eq!(value["mixed-port"], 19201);
        assert_eq!(value["allow-lan"], false);
        assert_eq!(value["tun"]["enable"], false);
        assert_eq!(value["dns"]["enable"], true);
        assert!(value["dns"].get("listen").is_none());
        assert!(value.get("port").is_none());
        assert!(value.get("external-controller-unix").is_none());
    }

    #[test]
    fn rejects_external_file_and_explicit_side_effect_layers_in_m0() {
        for key in [
            "proxy-providers",
            "rule-providers",
            "listeners",
            "subscriptions",
            "post-up",
        ] {
            let profile = Profile::parse(&format!("{key}: {{ x: y }}")).unwrap();
            assert!(matches!(
                profile.managed("127.0.0.1:19200".parse().unwrap(), 19201, "local-test"),
                Err(ProfileError::UnsupportedField(path)) if path == key
            ));
        }
    }
}
