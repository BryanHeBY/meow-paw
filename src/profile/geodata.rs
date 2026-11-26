//! Local-only warnings about known geodata dependencies. Meow still owns
//! rule parsing and runtime readiness; this is not a config validator.
use super::{Diagnostic, ProfileError};
use serde_json::{Map, Value};

pub(super) fn validate_paths(root: &Map<String, Value>) -> Result<(), ProfileError> {
    if let Some(geodata) = root.get("geodata").and_then(Value::as_object) {
        for key in ["mmdb-path", "asn-path", "geosite-path"] {
            if geodata.get(key).is_some_and(|value| !value.is_null()) {
                return Err(ProfileError::UnsupportedField(format!("geodata.{key}")));
            }
        }
    }
    Ok(())
}

pub(super) fn diagnostics(root: &Map<String, Value>) -> impl Iterator<Item = Diagnostic> {
    let top_level = string_items(root.get("rules"));
    let sub_rules = root
        .get("sub-rules")
        .and_then(Value::as_object)
        .into_iter()
        .flat_map(Map::values)
        .flat_map(|value| string_items(Some(value)));
    let inline_classical = root
        .get("rule-providers")
        .and_then(Value::as_object)
        .into_iter()
        .flat_map(Map::values)
        .filter(|provider| {
            provider.get("type").and_then(Value::as_str) == Some("inline")
                && provider.get("behavior").and_then(Value::as_str) == Some("classical")
        })
        .flat_map(|provider| string_items(provider.get("payload")));

    let mut needs = [false; 3];
    for rule in top_level.chain(sub_rules).chain(inline_classical) {
        let Some((kind, rest)) = rule.split_once(',') else {
            continue;
        };
        if rest
            .split(',')
            .next()
            .is_none_or(|part| part.trim().is_empty())
        {
            continue;
        }
        let kind = kind.trim();
        if ["GEOIP", "SRC-GEOIP"]
            .iter()
            .any(|key| kind.eq_ignore_ascii_case(key))
        {
            needs[0] = true;
        } else if ["IP-ASN", "SRC-IP-ASN"]
            .iter()
            .any(|key| kind.eq_ignore_ascii_case(key))
        {
            needs[1] = true;
        } else if kind.eq_ignore_ascii_case("GEOSITE") {
            needs[2] = true;
        }
    }

    let mut dns_geoip = false;
    let mut dns_geosite = false;
    if let Some(dns) = root
        .get("dns")
        .filter(|dns| dns.get("enable").and_then(Value::as_bool) == Some(true))
    {
        dns_geoip = dns
            .get("fallback")
            .and_then(Value::as_array)
            .is_some_and(|servers| !servers.is_empty())
            && dns
                .get("fallback-filter")
                .and_then(|filter| filter.get("geoip"))
                .and_then(Value::as_bool)
                != Some(false);
        dns_geosite = dns
            .get("nameserver-policy")
            .and_then(Value::as_object)
            .into_iter()
            .flat_map(Map::keys)
            .flat_map(|key| key.split(','))
            .map(str::trim)
            .any(|key| {
                key.split_once(':').is_some_and(|(kind, category)| {
                    kind.eq_ignore_ascii_case("geosite") && !category.trim().is_empty()
                })
            });
    }

    [
        (
            needs[0] || dns_geoip,
            "geodata (GeoIP)",
            "This profile references GeoIP data. A configuration check does not verify availability; missing data may delay startup or applying this profile.",
        ),
        (
            needs[1],
            "geodata (ASN)",
            "This profile references ASN data. A configuration check does not verify availability; missing data may delay startup or applying this profile.",
        ),
        (
            needs[2] || dns_geosite,
            "geodata (GeoSite)",
            "This profile references GeoSite data. A configuration check does not verify availability; matching and DNS selection may differ until the data loads.",
        ),
    ]
    .into_iter()
    .filter(|(needed, _, _)| *needed)
    .map(|(_, path, message)| Diagnostic {
        path: path.into(),
        message,
    })
}

fn string_items(value: Option<&Value>) -> impl Iterator<Item = &str> {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
}

#[cfg(test)]
mod tests {
    use crate::profile::{Profile, ProfileError};

    fn diagnostics(yaml: &str) -> Vec<String> {
        Profile::parse(yaml)
            .unwrap()
            .diagnostics()
            .into_iter()
            .map(|diagnostic| diagnostic.path)
            .collect()
    }

    #[test]
    fn warns_for_local_geodata_references_and_keeps_routing() {
        let yaml = r#"
rules: ["src-geoip,CN,DIRECT", "DOMAIN-KEYWORD,GEOIP,DIRECT"]
sub-rules: {private: [" SRC-IP-ASN,24589,DIRECT"]}
rule-providers:
  private: {type: inline, behavior: classical, payload: ["GEOSITE,category"]}
"#;
        assert_eq!(
            diagnostics(yaml),
            ["geodata (GeoIP)", "geodata (ASN)", "geodata (GeoSite)"]
        );
        let managed = Profile::parse(yaml)
            .unwrap()
            .managed("127.0.0.1:9998".parse().unwrap(), 9999, "test")
            .unwrap();
        assert!(managed.contains("src-geoip,CN,DIRECT"));
        assert!(managed.contains("SRC-IP-ASN,24589,DIRECT"));
        assert!(managed.contains("GEOSITE,category"));
    }

    #[test]
    fn accounts_for_enabled_dns_only_and_does_not_scan_rule_payload_text() {
        assert!(
            diagnostics(
                r#"
rules: ["DOMAIN-KEYWORD,GEOIP,DIRECT", "PROCESS-NAME,GEOSITE,DIRECT"]
dns:
  enable: false
  fallback: ["1.1.1.1"]
  nameserver-policy: {"geosite:example": ["1.1.1.1"]}
"#
            )
            .is_empty()
        );
        assert_eq!(
            diagnostics(
                r#"
dns:
  enable: true
  fallback: ["1.1.1.1"]
  nameserver-policy: {"GEOSITE:example,example2": ["1.1.1.1"]}
"#
            ),
            ["geodata (GeoIP)", "geodata (GeoSite)"]
        );
        assert!(
            diagnostics(
                r#"
dns: {enable: true, fallback: ["1.1.1.1"], fallback-filter: {geoip: false}}
"#
            )
            .is_empty()
        );
    }

    #[test]
    fn rejects_database_path_overrides_in_managed_profiles() {
        for name in ["mmdb-path", "asn-path", "geosite-path"] {
            let yaml = format!("geodata: {{ {name}: '/not-a-session/db' }}");
            let profile = Profile::parse(&yaml).unwrap();
            for https in [false, true] {
                let error = profile
                    .managed_with_https("127.0.0.1:9998".parse().unwrap(), 9999, "test", https)
                    .unwrap_err();
                assert!(matches!(error, ProfileError::UnsupportedField(ref field)
                    if field == &format!("geodata.{name}")));
                assert!(!error.to_string().contains("not-a-session"));
            }
        }
    }
}
