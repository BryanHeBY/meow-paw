#![cfg(target_os = "linux")]

use meow_paw::{
    app::{
        Desktop, EngineStatus, GROUP_PROBE_TARGET_LIMIT, ProviderRefreshOutcome, ProxyProbeStatus,
        Snapshot,
    },
    engine::controller::Origin,
};
use std::{
    fs::{self, OpenOptions},
    io::{BufRead, BufReader, Write},
    net::{Ipv4Addr, TcpListener, TcpStream},
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    sync::Mutex,
    thread,
    time::{Duration, Instant},
};

const PROBE_URL: &str = "https://example.invalid/check";
const STALE_URL: &str = "https://example.invalid/stale";
static REQUEST_LOG: Mutex<()> = Mutex::new(());

fn wait_for(desktop: &Desktop, predicate: impl Fn(&Snapshot) -> bool) -> Snapshot {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let snapshot = desktop.snapshot();
        assert!(
            !matches!(snapshot.engine, EngineStatus::Error(_)),
            "{snapshot:?}"
        );
        if predicate(&snapshot) {
            return snapshot;
        }
        assert!(Instant::now() < deadline, "timed out: {snapshot:?}");
        thread::sleep(Duration::from_millis(20));
    }
}

fn requests(dir: &Path) -> Vec<String> {
    fs::read_to_string(dir.join("requests"))
        .unwrap_or_default()
        .lines()
        .map(str::to_owned)
        .collect()
}

fn wait_for_request(dir: &Path, needle: &str) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !requests(dir).iter().any(|line| line.contains(needle)) {
        assert!(
            Instant::now() < deadline,
            "missing {needle}: {:?}",
            requests(dir)
        );
        thread::sleep(Duration::from_millis(10));
    }
}

fn shell_quote(path: &Path) -> String {
    format!("'{}'", path.to_str().unwrap().replace('\'', "'\\''"))
}

fn setup_fixture(dir: &Path) -> (Desktop, PathBuf) {
    let binary = dir.join("fake-meow");
    let executable = std::env::current_exe().unwrap();
    // The managed launcher performs two preflight invocations before the actual child.
    let script = format!(
        "#!/bin/sh\nconfig=\nwhile [ $# -gt 0 ]; do\n  case \"$1\" in\n    -t) exit 0 ;;\n    -f) shift; config=$1 ;;\n  esac\n  shift\ndone\nPAW_FAKE_CONFIG=\"$config\" PAW_FAKE_DIR={} exec {} --ignored --exact fake_controller_child\n",
        shell_quote(dir),
        shell_quote(&executable)
    );
    fs::write(&binary, script).unwrap();
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o700)).unwrap();
    let profile = dir.join("fixture.yaml");
    fs::write(&profile, "mode: direct\nrules: ['MATCH,DIRECT']\n").unwrap();
    let desktop = Desktop::with_mixed_port(binary, 0).unwrap();
    (desktop, profile)
}

#[test]
fn provider_batch_is_explicit_serial_partial_and_session_fenced() {
    let dir = tempfile::tempdir().unwrap();
    let (desktop, profile) = setup_fixture(dir.path());
    fs::write(dir.path().join("provider-batch"), "enabled").unwrap();
    desktop.start(profile.clone()).unwrap();
    let epoch = wait_for(&desktop, |s| {
        matches!(s.engine, EngineStatus::Running { .. }) && s.inventory.is_some()
    })
    .probe_epoch;
    assert!(
        desktop
            .refresh_all_http_providers(epoch.wrapping_sub(1))
            .is_err()
    );
    assert!(
        !requests(dir.path())
            .iter()
            .any(|line| line.starts_with("PUT "))
    );
    desktop.refresh_all_http_providers(epoch).unwrap();
    wait_for_request(dir.path(), "PUT /providers/proxies/A%20HTTP ");
    assert!(desktop.refresh_provider(epoch, "B".into()).is_err());
    desktop.apply_profile(profile.clone()).unwrap();
    wait_for(&desktop, |s| {
        s.action_error
            .as_deref()
            .is_some_and(|e| e.contains("provider refresh is still finishing"))
    });
    assert!(
        !requests(dir.path())
            .iter()
            .any(|line| line.contains("PUT /providers/proxies/B "))
    );
    fs::write(dir.path().join("release-helpers"), "go").unwrap();
    let done = wait_for(&desktop, |s| {
        s.provider_batch.as_ref().is_some_and(|job| !job.active())
    });
    assert_eq!(done.probe_epoch, epoch);
    let results = done.provider_batch.unwrap().results;
    assert_eq!(results.len(), 2);
    assert_eq!(results["A HTTP"], ProviderRefreshOutcome::Refreshed);
    assert_eq!(results["B"], ProviderRefreshOutcome::Failed);
    let writes: Vec<_> = requests(dir.path())
        .into_iter()
        .filter(|line| line.starts_with("PUT "))
        .collect();
    assert_eq!(
        writes,
        [
            "PUT /providers/proxies/A%20HTTP HTTP/1.1",
            "PUT /providers/proxies/B HTTP/1.1"
        ]
    );

    fs::remove_file(dir.path().join("release-helpers")).unwrap();
    desktop.refresh_all_http_providers(epoch).unwrap();
    wait_for(&desktop, |s| {
        s.refreshing_provider.as_deref() == Some("A HTTP")
    });
    desktop.stop();
    let stopped = wait_for(&desktop, |s| matches!(s.engine, EngineStatus::Stopped));
    assert!(stopped.provider_batch.is_none());
    assert!(desktop.refresh_all_http_providers(epoch).is_err());
    assert_eq!(
        requests(dir.path())
            .iter()
            .filter(|line| line.contains("PUT /providers/proxies/B "))
            .count(),
        1
    );
}

#[test]
fn uncertain_provider_write_fences_refresh_and_apply_until_fresh_controller() {
    let dir = tempfile::tempdir().unwrap();
    let (desktop, profile) = setup_fixture(dir.path());
    fs::write(dir.path().join("provider-batch"), "enabled").unwrap();
    fs::write(dir.path().join("disconnect-provider"), "enabled").unwrap();
    desktop.start(profile.clone()).unwrap();
    let epoch = wait_for(&desktop, |s| {
        matches!(s.engine, EngineStatus::Running { .. }) && s.inventory.is_some()
    })
    .probe_epoch;
    desktop.refresh_all_http_providers(epoch).unwrap();
    let fenced = wait_for(&desktop, |s| s.provider_refresh_uncertain);
    assert!(dir.path().join("server-write-still-running").exists());
    assert!(!dir.path().join("server-write-finished").exists());
    let job = fenced.provider_batch.unwrap();
    assert!(!job.active());
    assert_eq!(job.results["A HTTP"], ProviderRefreshOutcome::Unknown);
    assert_eq!(job.results["B"], ProviderRefreshOutcome::NotAttempted);
    for error in [
        desktop.refresh_provider(epoch, "B".into()).unwrap_err(),
        desktop.refresh_all_http_providers(epoch).unwrap_err(),
        desktop.apply_profile(profile.clone()).unwrap_err(),
    ] {
        assert!(
            error.to_string().contains("Stop Meow and start it again"),
            "{error}"
        );
    }
    assert!(
        !requests(dir.path())
            .iter()
            .any(|line| line.contains("PUT /providers/proxies/B "))
    );
    assert!(
        !requests(dir.path())
            .iter()
            .any(|line| line.starts_with("PUT /configs"))
    );
    let stop = Instant::now();
    desktop.stop();
    let stopped = wait_for(&desktop, |s| matches!(s.engine, EngineStatus::Stopped));
    assert!(stop.elapsed() < Duration::from_secs(4));
    assert!(!dir.path().join("server-write-finished").exists());
    assert!(!stopped.provider_refresh_uncertain);
    fs::remove_file(dir.path().join("disconnect-provider")).unwrap();
    fs::write(dir.path().join("release-helpers"), "go").unwrap();
    desktop.start(profile).unwrap();
    let next = wait_for(&desktop, |s| {
        matches!(s.engine, EngineStatus::Running { .. }) && s.inventory.is_some()
    })
    .probe_epoch;
    assert_ne!(next, epoch);
    assert!(desktop.refresh_all_http_providers(epoch).is_err());
    desktop.refresh_all_http_providers(next).unwrap();
    let resumed = wait_for(&desktop, |s| {
        s.provider_batch.as_ref().is_some_and(|job| !job.active())
    });
    assert!(!resumed.provider_refresh_uncertain);
    assert_eq!(
        resumed.provider_batch.unwrap().results["A HTTP"],
        ProviderRefreshOutcome::Refreshed
    );
    assert_eq!(
        requests(dir.path())
            .iter()
            .filter(|line| line.contains("PUT /providers/proxies/B "))
            .count(),
        1
    );
}

#[test]
fn explicit_group_comparison_uses_bounded_unambiguous_requests_and_ignores_stale_results() {
    let dir = tempfile::tempdir().unwrap();
    let (desktop, profile) = setup_fixture(dir.path());
    desktop.start(profile).unwrap();
    let started = wait_for(&desktop, |s| {
        matches!(s.engine, EngineStatus::Running { .. }) && s.inventory.is_some()
    });
    let epoch = started.probe_epoch;

    // Preparation rejects the whole oversized group before any delay is dispatched.
    let error = desktop
        .compare_group(epoch, "Large", PROBE_URL)
        .unwrap_err();
    assert!(error.to_string().contains("TooManyTargets"), "{error}");
    assert!(
        !requests(dir.path())
            .iter()
            .any(|line| line.contains("/delay?") || line.contains("/healthcheck?"))
    );

    desktop.compare_group(epoch, "Root", PROBE_URL).unwrap();
    let completed = wait_for(&desktop, |s| {
        s.group_probe.as_ref().is_some_and(|g| !g.active)
    });
    let comparison = completed.group_probe.unwrap();
    assert!(!comparison.interrupted);
    let rows = comparison.results.rows();
    assert_eq!(rows.len(), 4);
    assert_eq!(
        rows.iter()
            .map(|(target, status)| (target.name.as_str(), &target.origin, status.clone()))
            .collect::<Vec<_>>(),
        vec![
            (
                "Alpha",
                &Origin::Local,
                Some(ProxyProbeStatus::Measured(17))
            ),
            (
                "Provider Only",
                &Origin::Provider("Feed".into()),
                Some(ProxyProbeStatus::Measured(29))
            ),
            (
                "Failure",
                &Origin::Local,
                Some(ProxyProbeStatus::Unavailable)
            ),
            ("Zero", &Origin::Local, Some(ProxyProbeStatus::Unavailable)),
        ]
    );

    // Hold the first request, invalidate its run, then deliver its late response.
    desktop.compare_group(epoch, "Root", STALE_URL).unwrap();
    wait_for_request(dir.path(), "example.invalid%2Fstale");
    // Give the worker time to dispatch another request if it violates the
    // one-in-flight bound, while the controller still holds the first reply.
    thread::sleep(Duration::from_millis(350));
    let held = desktop.snapshot();
    let held_rows = held.group_probe.as_ref().unwrap().results.rows();
    assert_eq!(held_rows[0].1, Some(ProxyProbeStatus::Running));
    assert!(held_rows[1..].iter().all(|(_, status)| status.is_none()));
    assert_eq!(
        requests(dir.path())
            .iter()
            .filter(|line| line.contains("/delay?") || line.contains("/healthcheck?"))
            .count(),
        5
    );
    desktop.invalidate_group_probe();
    fs::write(dir.path().join("release"), "go").unwrap();
    let after_stale = wait_for(&desktop, |s| !s.probe_busy && s.group_probe.is_none());
    assert_eq!(after_stale.probe_epoch, epoch);
    desktop.compare_group(epoch, "Root", PROBE_URL).unwrap();
    let again = wait_for(&desktop, |s| {
        s.group_probe.as_ref().is_some_and(|g| !g.active)
    });
    assert_eq!(
        again.group_probe.unwrap().results.rows()[0].1,
        Some(ProxyProbeStatus::Measured(17))
    );
    drop(desktop);

    let lines = requests(dir.path());
    let delay: Vec<_> = lines
        .iter()
        .filter(|line| line.contains("/delay?") || line.contains("/healthcheck?"))
        .collect();
    assert_eq!(delay.len(), 9, "{delay:?}");
    let paths: Vec<_> = delay
        .iter()
        .map(|line| line.split_whitespace().nth(1).unwrap())
        .collect();
    for chunk in [&paths[..4], &paths[5..]] {
        assert!(chunk[0].starts_with("/proxies/Alpha/delay?"), "{chunk:?}");
        assert!(
            chunk[1].starts_with("/providers/proxies/Feed/Provider%20Only/healthcheck?"),
            "{chunk:?}"
        );
        assert!(chunk[2].starts_with("/proxies/Failure/delay?"), "{chunk:?}");
        assert!(chunk[3].starts_with("/proxies/Zero/delay?"), "{chunk:?}");
    }
    assert!(paths[4].starts_with("/proxies/Alpha/delay?"));
    for (index, path) in paths.iter().enumerate() {
        let url = url::Url::parse(&format!("http://127.0.0.1{path}")).unwrap();
        let query: Vec<_> = url.query_pairs().collect();
        assert_eq!(query.len(), 2);
        assert!(query.iter().any(|(k, v)| k == "timeout" && v == "2000"));
        let expected_url = if index == 4 { STALE_URL } else { PROBE_URL };
        assert!(query.iter().any(|(k, v)| k == "url" && v == expected_url));
    }
    assert!(
        lines.iter().all(|line| !line.starts_with("PUT ")
            && !line.starts_with("PATCH ")
            && !line.starts_with("DELETE ")),
        "{lines:?}"
    );
}

// Executed only as the synthetic managed child, never as part of the normal suite.
#[test]
#[ignore = "test-owned fake Meow subprocess"]
fn fake_controller_child() {
    let Some(config_path) = std::env::var_os("PAW_FAKE_CONFIG") else {
        return;
    };
    let config = fs::read_to_string(config_path).unwrap();
    let config: serde_json::Value = serde_saphyr::from_str(&config).unwrap();
    let controller = TcpListener::bind(config["external-controller"].as_str().unwrap()).unwrap();
    let mixed = TcpListener::bind((
        Ipv4Addr::LOCALHOST,
        config["mixed-port"].as_u64().unwrap() as u16,
    ))
    .unwrap();
    thread::spawn(move || {
        for connection in mixed.incoming() {
            drop(connection.unwrap());
        }
    });
    let secret = config["secret"].as_str().unwrap().to_owned();
    let dir = PathBuf::from(std::env::var_os("PAW_FAKE_DIR").unwrap());
    for connection in controller.incoming() {
        let stream = connection.unwrap();
        let secret = secret.clone();
        let dir = dir.clone();
        thread::spawn(move || serve(stream, &secret, &dir));
    }
}

fn serve(mut stream: TcpStream, secret: &str, dir: &Path) {
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let mut reader = BufReader::new(&mut stream);
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    let request = line.trim_end().to_owned();
    let mut authorized = false;
    loop {
        line.clear();
        if reader.read_line(&mut line).unwrap() == 0 || line == "\r\n" {
            break;
        }
        authorized |= line
            .trim()
            .eq_ignore_ascii_case(&format!("authorization: Bearer {secret}"));
    }
    assert!(authorized, "{request}");
    {
        let _guard = REQUEST_LOG.lock().unwrap();
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join("requests"))
            .unwrap()
            .write_all(format!("{request}\n").as_bytes())
            .unwrap();
    }
    let path = request.split_whitespace().nth(1).unwrap();
    let (status, body) = if path == "/" {
        ("200 OK", serde_json::json!({"hello":"meow"}))
    } else if path == "/configs" {
        ("200 OK", serde_json::json!({"mode":"direct"}))
    } else if path == "/proxies" {
        let mut routes = serde_json::Map::new();
        for name in ["Alpha", "Failure", "Zero", "Ambiguous"] {
            routes.insert(name.into(), serde_json::json!({"type":"Direct"}));
        }
        routes.insert("Root".into(), serde_json::json!({"type":"Selector", "now":"Alpha", "all":["Alpha", "Nested", "Alpha", "Ambiguous", "Collision", "Missing"]}));
        routes.insert("Nested".into(), serde_json::json!({"type":"Selector", "all":["Provider Only", "Failure", "Zero", "Root"]}));
        routes.insert(
            "Collision".into(),
            serde_json::json!({"type":"Selector", "all":["Alpha"]}),
        );
        let names: Vec<_> = (0..=GROUP_PROBE_TARGET_LIMIT)
            .map(|i| format!("Large{i}"))
            .collect();
        for name in &names {
            routes.insert(name.clone(), serde_json::json!({"type":"Direct"}));
        }
        routes.insert(
            "Large".into(),
            serde_json::json!({"type":"Selector", "all":names}),
        );
        ("200 OK", serde_json::json!({"proxies":routes}))
    } else if path == "/providers/proxies" && dir.join("provider-batch").exists() {
        (
            "200 OK",
            serde_json::json!({"providers":{
                "A HTTP":{"vehicleType":"HTTP","proxies":[]},
                "B":{"vehicleType":"http","proxies":[]},
                "Local":{"vehicleType":"File","proxies":[]}
            }}),
        )
    } else if request.starts_with("PUT ") && path == "/providers/proxies/A%20HTTP" {
        if dir.join("disconnect-provider").exists() {
            fs::write(dir.join("server-write-still-running"), "held").unwrap();
            stream.shutdown(std::net::Shutdown::Both).unwrap();
            // Simulate the server keeping work alive after the client has lost its reply.
            thread::sleep(Duration::from_secs(8));
            fs::write(dir.join("server-write-finished"), "done").unwrap();
            return;
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        while !dir.join("release-helpers").exists() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
        let _ = write!(
            stream,
            "HTTP/1.1 204 No Content\r\nConnection: close\r\nContent-Length: 0\r\n\r\n"
        );
        return;
    } else if request.starts_with("PUT ") && path == "/providers/proxies/B" {
        ("503 Service Unavailable", serde_json::json!({}))
    } else if path == "/providers/proxies" {
        (
            "200 OK",
            serde_json::json!({"providers":{"Feed":{"proxies":[{"name":"Provider Only","type":"Direct"},{"name":"Ambiguous","type":"Direct"},{"name":"Collision","type":"Direct"}]}}}),
        )
    } else if path == "/connections" {
        ("200 OK", serde_json::json!({"connections":[]}))
    } else if path == "/traffic" {
        ("200 OK", serde_json::json!({"up":0,"down":0}))
    } else if path.starts_with("/logs?") {
        ("404 Not Found", serde_json::json!({}))
    } else if path.starts_with("/proxies/Alpha/delay?") {
        if path.contains("example.invalid%2Fstale") {
            let deadline = Instant::now() + Duration::from_secs(5);
            while !dir.join("release").exists() && Instant::now() < deadline {
                thread::sleep(Duration::from_millis(10));
            }
        }
        ("200 OK", serde_json::json!({"delay":17}))
    } else if path.starts_with("/providers/proxies/Feed/Provider%20Only/healthcheck?") {
        ("200 OK", serde_json::json!({"delay":29}))
    } else if path.starts_with("/proxies/Failure/delay?") {
        ("503 Service Unavailable", serde_json::json!({}))
    } else if path.starts_with("/proxies/Zero/delay?") {
        ("200 OK", serde_json::json!({"delay":0}))
    } else {
        ("404 Not Found", serde_json::json!({}))
    };
    let body = format!("{body}\n");
    let _ = write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    );
}
