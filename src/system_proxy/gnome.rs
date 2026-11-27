//! GNOME's session proxy back end through GSettings/dconf.
//! Other desktops (including Hyprland) are not presumed to implement it.
use super::{Backend, NonZeroU16, ProxyError, Snapshot};
use std::{env, io, process::Command};

const ROOT: &str = "org.gnome.system.proxy";
const HTTP: &str = "org.gnome.system.proxy.http";
const HTTPS: &str = "org.gnome.system.proxy.https";
const FTP: &str = "org.gnome.system.proxy.ftp";
const SOCKS: &str = "org.gnome.system.proxy.socks";
const KEYS: &[(&str, &str)] = &[
    (ROOT, "mode"),
    (HTTP, "host"),
    (HTTP, "port"),
    (HTTPS, "host"),
    (HTTPS, "port"),
    (ROOT, "use-same-proxy"),
    // Read-only guards; preserve bypass list/PAC and refuse incompatible
    // SOCKS/FTP/authentication rather than silently routing around Paw.
    (ROOT, "ignore-hosts"),
    (ROOT, "autoconfig-url"),
    (HTTP, "use-authentication"),
    (FTP, "host"),
    (FTP, "port"),
    (SOCKS, "host"),
    (SOCKS, "port"),
];
const MANAGED: usize = 6;

pub(super) struct Gnome;
impl Gnome {
    pub(super) fn discover() -> Result<Self, ProxyError> {
        let gnome = env::var("XDG_CURRENT_DESKTOP")
            .unwrap_or_default()
            .split(':')
            .any(|name| name.eq_ignore_ascii_case("GNOME"));
        if !gnome
            || env::var_os("DBUS_SESSION_BUS_ADDRESS").is_none()
            || env::var_os("GSETTINGS_BACKEND").is_some()
            || env::var_os("DCONF_PROFILE").is_some()
            || KEYS.iter().take(MANAGED).any(|&(schema, key)| {
                call("gsettings", &["writable", schema, key])
                    .as_deref()
                    .ok()
                    != Some("true")
            })
        {
            return Err(ProxyError::Unsupported);
        }
        let desktop = Self;
        desktop.read().map_err(|_| ProxyError::Unsupported)?;
        Ok(desktop)
    }
}
fn path(schema: &str, key: &str) -> String {
    format!(
        "/{}/{key}",
        schema.trim_start_matches("org.gnome.").replace('.', "/")
    )
}
fn call(program: &str, args: &[&str]) -> Result<String, ProxyError> {
    let p = Command::new(program)
        .args(args)
        .env("LC_ALL", "C")
        .output()?;
    // GLib can emit a dconf commit failure as a warning with exit 0.
    if !p.status.success() || !p.stderr.is_empty() {
        return Err(io::Error::other(format!("{program} failed")).into());
    }
    let out = String::from_utf8(p.stdout)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "Invalid GSettings output"))?;
    Ok(out.trim_end_matches('\n').to_owned())
}
impl Backend for Gnome {
    fn id(&self) -> &'static str {
        "gnome-dconf-v1"
    }
    fn read(&self) -> Result<Snapshot, ProxyError> {
        let mut values = Vec::new();
        for (i, &(schema, key)) in KEYS.iter().enumerate() {
            let effective = call("gsettings", &["get", schema, key])?;
            if effective.is_empty() {
                return Err(ProxyError::Unsupported);
            }
            let stored = if i < MANAGED {
                let path = path(schema, key);
                // dconf's normal read includes policy/default sources. A
                // nonempty lower-tier value makes set-vs-unset ambiguous.
                if !call("dconf", &["read", "-d", &path])?.is_empty() {
                    return Err(ProxyError::Unsupported);
                }
                let raw = call("dconf", &["read", &path])?;
                if !raw.is_empty() && raw != effective {
                    return Err(ProxyError::Unsupported);
                }
                (!raw.is_empty()).then_some(raw)
            } else {
                None
            };
            values.extend([Some(effective), stored]);
        }
        Ok(Snapshot(values))
    }
    fn desired(&self, original: &Snapshot, port: NonZeroU16) -> Result<Snapshot, ProxyError> {
        let mut state = original.clone();
        let fields = &mut state.0;
        if fields.len() != 2 * KEYS.len()
            || !matches!(fields[0].as_deref(), Some("'none'" | "'manual'" | "'auto'"))
            || fields[16].as_deref() != Some("false")
            || fields[18].as_deref() != Some("''")
            || fields[22].as_deref() != Some("''")
            || fields.as_chunks::<2>().0.iter().any(|pair| {
                pair[0].is_none()
                    || pair[1]
                        .as_ref()
                        .is_some_and(|raw| pair[0].as_ref() != Some(raw))
            })
        {
            return Err(ProxyError::Unsupported);
        }
        for (i, value) in [
            "'manual'".to_owned(),
            "'127.0.0.1'".to_owned(),
            port.to_string(),
            "'127.0.0.1'".to_owned(),
            port.to_string(),
            "false".to_owned(),
        ]
        .into_iter()
        .enumerate()
        {
            fields[i * 2] = Some(value.clone());
            fields[i * 2 + 1] = Some(value);
        }
        Ok(state)
    }

    fn apply(&self, from: &Snapshot, to: &Snapshot) -> Result<(), ProxyError> {
        let mut observed = from.clone();
        // Enable after writing hosts/ports; for non-manual restoration,
        // leave managed mode before restoring those now-unused keys.
        let order = if to.0[0].as_deref() == Some("'manual'") {
            [1, 2, 3, 4, 5, 0]
        } else {
            [0, 1, 2, 3, 4, 5]
        };
        for i in order {
            let managed = 2 * i..2 * i + 2;
            if observed.0[managed.clone()] == to.0[managed.clone()] {
                continue;
            }
            if self.read()? != observed {
                return Err(ProxyError::Conflict);
            }
            let (schema, key) = KEYS[i];
            if call("gsettings", &["writable", schema, key])? != "true" {
                return Err(ProxyError::Unsupported);
            }
            if let Some(value) = &to.0[2 * i + 1] {
                call("gsettings", &["set", schema, key, value])?;
            } else {
                call("gsettings", &["reset", schema, key])?;
            }
            observed.0[managed.clone()].clone_from_slice(&to.0[managed]);
        }
        if self.read()? != *to {
            return Err(ProxyError::Conflict);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn desktop() -> Snapshot {
        let effective = [
            "'none'",
            "'original-proxy'",
            "8080",
            "''",
            "0",
            "true",
            "['localhost']",
            "'https://configuration.example/proxy.pac'",
            "false",
            "''",
            "0",
            "''",
            "0",
        ];
        let mut fields = effective
            .into_iter()
            .flat_map(|value| [Some(value.into()), None])
            .collect::<Vec<_>>();
        fields[3] = Some("'original-proxy'".into());
        Snapshot(fields)
    }

    #[test]
    fn desired_proxy_only_changes_http_and_https_and_preserves_read_only_guards() {
        let before = desktop();
        let after = Gnome
            .desired(&before, NonZeroU16::new(7890).unwrap())
            .unwrap();
        assert_eq!(after.0[0].as_deref(), Some("'manual'"));
        assert_eq!(after.0[2].as_deref(), Some("'127.0.0.1'"));
        assert_eq!(after.0[4].as_deref(), Some("7890"));
        assert_eq!(after.0[6].as_deref(), Some("'127.0.0.1'"));
        assert_eq!(after.0[8].as_deref(), Some("7890"));
        assert_eq!(after.0[12..], before.0[12..]);
        assert_eq!(path(HTTP, "host"), "/system/proxy/http/host");
    }

    #[test]
    fn existing_socks_or_authentication_is_not_reconfigured() {
        for (key, value) in [(16, "true"), (22, "'my-socks'")] {
            let mut before = desktop();
            before.0[key] = Some(value.into());
            assert!(matches!(
                Gnome.desired(&before, NonZeroU16::MIN),
                Err(ProxyError::Unsupported)
            ));
        }
    }
}
