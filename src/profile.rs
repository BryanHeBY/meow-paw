//! Local profiles and the small set of fields owned by a managed Paw session.

use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use std::{error::Error, fmt, net::SocketAddrV4};
use url::Url;

mod geodata;

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
    cache_scope: Option<Vec<u8>>,
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
            Value::Object(root) => Ok(Self {
                root,
                cache_scope: None,
            }),
            _ => Err(ProfileError::InvalidDocument),
        }
    }

    /// Identify a saved document independently of its changing contents.
    pub(crate) fn with_cache_scope(mut self, path: &std::path::Path) -> Self {
        self.cache_scope = Some(path.as_os_str().as_encoded_bytes().to_vec());
        self
    }

    pub(crate) fn has_subscription_content(&self) -> bool {
        self.root
            .get("proxies")
            .and_then(Value::as_array)
            .is_some_and(|nodes| !nodes.is_empty() && nodes.iter().all(Value::is_object))
            || self
                .root
                .get("proxy-providers")
                .and_then(Value::as_object)
                .is_some_and(|providers| {
                    !providers.is_empty() && providers.values().all(Value::is_object)
                })
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
            .chain(geodata::diagnostics(&self.root))
            .collect()
    }

    pub fn managed(
        &self,
        controller: SocketAddrV4,
        mixed_port: u16,
        secret: &str,
    ) -> Result<String, ProfileError> {
        self.managed_with_https(controller, mixed_port, secret, false)
    }

    /// The caller must launch an engine with enforced HTTPS provider downloads.
    pub(crate) fn managed_with_https(
        &self,
        controller: SocketAddrV4,
        mixed_port: u16,
        secret: &str,
        allow_https: bool,
    ) -> Result<String, ProfileError> {
        if !controller.ip().is_loopback()
            || controller.port() == 0
            || mixed_port == 0
            || mixed_port == controller.port()
            || secret.is_empty()
        {
            return Err(ProfileError::InvalidSettings);
        }
        geodata::validate_paths(&self.root)?;
        let mut root = self.root.clone();
        // M0 does not opt into script execution, native subscription writers,
        // alternate listeners, or provider-controlled local file access.
        for key in [
            "listeners",
            "subscriptions",
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
        // Inline and guarded HTTPS providers are allowed. File providers stay
        // disabled and remote cache paths are always owned by Paw.
        // Anonymous callers conservatively use content identity. Saved
        // documents retain identity through updates and across hot switches.
        let fallback;
        let scope = match self.cache_scope.as_deref() {
            Some(scope) => scope,
            None => {
                fallback = serde_json::to_vec(&root).map_err(|_| ProfileError::Encode)?;
                &fallback
            }
        };
        let identity = Sha256::digest(scope);
        allow_providers(&mut root, "proxy-providers", true, allow_https, &identity)?;
        allow_providers(&mut root, "rule-providers", false, allow_https, &identity)?;
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

fn allow_providers(
    root: &mut Map<String, Value>,
    key: &str,
    proxies: bool,
    allow_https: bool,
    profile_identity: &[u8],
) -> Result<(), ProfileError> {
    let Some(value) = root.get_mut(key) else {
        return Ok(());
    };
    if value.is_null() || value.as_array().is_some_and(Vec::is_empty) {
        root.remove(key);
        return Ok(());
    }
    let providers = value
        .as_object_mut()
        .ok_or_else(|| ProfileError::UnsupportedField(key.into()))?;
    for (index, (name, entry)) in providers.iter_mut().enumerate() {
        // Use only an index in diagnostics, never log provider-supplied names.
        let path = || ProfileError::UnsupportedField(format!("{key}[{index}]"));
        let provider = entry.as_object_mut().ok_or_else(path)?;
        let kind = provider.get("type").and_then(Value::as_str);
        if kind == Some("inline") {
            if ["url", "path", "header"]
                .iter()
                .any(|field| provider.contains_key(*field))
                || !provider
                    .get("payload")
                    .and_then(Value::as_array)
                    .is_some_and(|items| {
                        if proxies {
                            items.iter().all(Value::is_object)
                        } else {
                            items.iter().all(Value::is_string)
                        }
                    })
            {
                return Err(path());
            }
        } else if allow_https && kind == Some("http") {
            // Never honour profile-supplied paths, including absolute paths
            // and symbolic-link traversals. Only Paw chooses the cache name.
            if provider.contains_key("payload") {
                return Err(path());
            }
            let url = provider
                .get("url")
                .and_then(Value::as_str)
                .and_then(|u| Url::parse(u).ok())
                .ok_or_else(path)?;
            if url.scheme() != "https"
                || url.host_str().is_none()
                || !url.username().is_empty()
                || url.password().is_some()
            {
                return Err(path());
            }
            let mut digest = Sha256::new();
            digest.update(profile_identity);
            digest.update(key.as_bytes());
            digest.update([0]);
            digest.update(name.as_bytes());
            digest.update([0]);
            digest.update(url.as_str().as_bytes());
            // Fetch-affecting fields distinguish sources; user path and
            // refresh cadence do not.
            let mut source = provider.clone();
            source.remove("path");
            source.remove("interval");
            digest.update(serde_json::to_vec(&source).map_err(|_| ProfileError::Encode)?);
            let cache = digest.finalize().iter().fold(String::new(), |mut text, b| {
                use std::fmt::Write as _;
                let _ = write!(text, "{b:02x}");
                text
            });
            provider.insert("path".into(), json!(format!("paw-provider-{cache}.yaml")));
        } else {
            return Err(path());
        }
        if proxies {
            provider.insert("allow-external-plugin".into(), json!(false));
        }
    }
    Ok(())
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
    fn managed_https_providers_use_private_stable_scoped_caches() {
        let source = "proxy-providers: {same: {type: http, url: 'https://example.test/feed?secret=private'}}\nrule-providers: {same: {type: http, url: 'https://example.test/feed?secret=private', behavior: domain}}\n";
        let build = |input: &str| {
            let profile = Profile::parse(input)
                .unwrap()
                .with_cache_scope(std::path::Path::new("/saved/one.json"));
            let rendered = profile
                .managed_with_https("127.0.0.1:19200".parse().unwrap(), 19201, "test", true)
                .unwrap();
            Profile::parse(&rendered).unwrap().root
        };
        let first = build(source);
        let second = build(source);
        let p = first["proxy-providers"]["same"]["path"].as_str().unwrap();
        let r = first["rule-providers"]["same"]["path"].as_str().unwrap();
        assert_eq!(p, second["proxy-providers"]["same"]["path"]);
        assert_ne!(p, r);
        assert!(p.starts_with("paw-provider-"));
        assert!(!p.contains("private"));
        assert_ne!(
            p,
            build(&source.replace("secret=private", "secret=changed"))["proxy-providers"]["same"]["path"]
        );
        assert_eq!(
            p,
            build(&(source.to_owned() + "mode: direct\nrules: ['MATCH,DIRECT']\n"))["proxy-providers"]
                ["same"]["path"]
        );
        let another = Profile::parse(source)
            .unwrap()
            .with_cache_scope(std::path::Path::new("/saved/two.json"))
            .managed_with_https("127.0.0.1:19200".parse().unwrap(), 19201, "test", true)
            .unwrap();
        assert_ne!(
            p,
            Profile::parse(&another).unwrap().root["proxy-providers"]["same"]["path"]
        );
        for bad in [
            "proxy-providers: {p: {type: http, url: 'http://example.test/feed'}}",
            "proxy-providers: {p: {type: http, url: 'https://user:pass@example.test/feed'}}",
            "proxy-providers: {p: {type: file, path: /tmp/no}}",
        ] {
            assert!(
                Profile::parse(bad)
                    .unwrap()
                    .managed_with_https("127.0.0.1:19200".parse().unwrap(), 19201, "test", true)
                    .is_err()
            );
        }
        // Common subscription-provided paths are ignored, never followed.
        let supplied = build(
            "proxy-providers: {same: {type: http, url: 'https://example.test/feed', path: '/tmp/no'}}",
        );
        let safe = supplied["proxy-providers"]["same"]["path"]
            .as_str()
            .unwrap();
        assert!(safe.starts_with("paw-provider-"));
        assert!(!safe.contains("/tmp"));

        // The public non-managed API must not silently enable downloads.
        assert!(
            Profile::parse(source)
                .unwrap()
                .managed("127.0.0.1:19200".parse().unwrap(), 19201, "test")
                .is_err()
        );
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
    fn permits_inline_payloads_but_not_provider_paths_or_downloads() {
        let value = generated(
            "proxy-providers: { local: {type: inline, allow-external-plugin: true, payload: [{name: Node, type: direct}]}}\n\
             rule-providers: { local: {type: inline, behavior: domain, payload: [example.test]}}\n",
        );
        assert_eq!(
            value["proxy-providers"]["local"]["allow-external-plugin"],
            false
        );
        assert_eq!(
            value["rule-providers"]["local"]["payload"][0],
            "example.test"
        );
        for key in ["proxy-providers", "rule-providers"] {
            for fields in [
                "type: file, path: /tmp/secret",
                "type: http, url: 'https://example.test/token'",
                "type: inline, path: ../../escape, payload: []",
                "type: inline, payload: invalid",
            ] {
                let profile = Profile::parse(&format!("{key}: {{hidden: {{{fields}}}}}")).unwrap();
                assert!(matches!(
                    profile.managed("127.0.0.1:19200".parse().unwrap(), 19201, "local-test"),
                    Err(ProfileError::UnsupportedField(path)) if path == format!("{key}[0]")
                ));
            }
        }
    }

    #[test]
    fn rejects_external_file_and_explicit_side_effect_layers_in_m0() {
        for key in ["listeners", "subscriptions", "post-up"] {
            let profile = Profile::parse(&format!("{key}: {{ x: y }}")).unwrap();
            assert!(matches!(
                profile.managed("127.0.0.1:19200".parse().unwrap(), 19201, "local-test"),
                Err(ProfileError::UnsupportedField(path)) if path == key
            ));
        }
    }
}
