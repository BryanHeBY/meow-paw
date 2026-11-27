#![cfg(any(target_os = "linux", windows))]

use meow_paw::{
    engine::{controller::RoutingMode, session::Session},
    profile::Profile,
};
use std::{
    io::{Read, Write},
    net::{Ipv4Addr, TcpListener, TcpStream},
    path::PathBuf,
    sync::mpsc,
    thread,
    time::Duration,
};

#[test]
#[ignore = "requires a local Meow binary: MEOW_BIN=/path/to/meow"]
fn isolated_session_proxies_loopback_traffic_and_cleans_up() {
    let binary = PathBuf::from(std::env::var_os("MEOW_BIN").expect("set MEOW_BIN"));
    let profile = Profile::parse(
        "mode: rule\nipv6: false\ndns: {enable: false}\nproxy-groups:\n  - {name: Test, type: select, proxies: [DIRECT, REJECT]}\nrules: ['MATCH,Test']\n",
    ).unwrap();
    let session = Session::launch_with_mixed_port(&binary, &profile, 0, || false).unwrap();
    assert_eq!(session.controller.mode().unwrap(), RoutingMode::Rule);
    for mode in [RoutingMode::Global, RoutingMode::Direct, RoutingMode::Rule] {
        session.controller.set_mode(mode).unwrap();
        assert_eq!(session.controller.mode().unwrap(), mode);
    }
    let rules = session.controller.rules().unwrap();
    assert_eq!(rules.len(), 1);
    assert_eq!(rules[0].kind, "Match");
    assert_eq!(rules[0].proxy, "Test");
    assert_eq!(
        session.controller.inventory().unwrap().groups["Test"]
            .now
            .as_deref(),
        Some("DIRECT")
    );
    session.controller.select("Test", "REJECT").unwrap();
    assert_eq!(
        session.controller.inventory().unwrap().groups["Test"]
            .now
            .as_deref(),
        Some("REJECT")
    );
    session.controller.select("Test", "DIRECT").unwrap();
    let origin = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let endpoint = origin.local_addr().unwrap();
    let (started, reached) = mpsc::channel();
    let (response_ready, release_response) = mpsc::channel();
    let server = thread::spawn(move || {
        let (mut stream, _) = origin.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let mut request = [0; 2048];
        let count = stream.read(&mut request).unwrap();
        assert!(
            std::str::from_utf8(&request[..count])
                .unwrap()
                .starts_with("GET /proof ")
        );
        started.send(()).unwrap();
        release_response
            .recv_timeout(Duration::from_secs(3))
            .unwrap();
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\nConnection: close\r\n\r\nPaw!")
            .unwrap();
    });
    let mixed = (Ipv4Addr::LOCALHOST, session.mixed_port);
    let mut client = TcpStream::connect(mixed).unwrap();
    client
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    write!(
        client,
        "GET http://{endpoint}/proof HTTP/1.1\r\nHost: {endpoint}\r\nConnection: close\r\n\r\n"
    )
    .unwrap();
    reached.recv_timeout(Duration::from_secs(3)).unwrap();
    assert!(
        session
            .controller
            .connections()
            .unwrap()
            .iter()
            .any(|c| c.chains.iter().any(|chain| chain == "DIRECT"))
    );
    response_ready.send(()).unwrap();
    let mut received = String::new();
    client.read_to_string(&mut received).unwrap();
    assert!(received.ends_with("Paw!"));
    server.join().unwrap();
    assert!(session.controller.traffic().unwrap().down_total > 0);
    session.stop().unwrap();
    assert!(TcpStream::connect_timeout(&mixed.into(), Duration::from_millis(100)).is_err());
}

#[test]
#[ignore = "requires a local Meow binary: MEOW_BIN=/path/to/meow"]
fn closes_only_the_selected_loopback_connection() {
    let binary = PathBuf::from(std::env::var_os("MEOW_BIN").expect("set MEOW_BIN"));
    let profile = Profile::parse("mode: direct\nrules: ['MATCH,DIRECT']\n").unwrap();
    let session = Session::launch_with_mixed_port(&binary, &profile, 0, || false).unwrap();
    let origin = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let endpoint = origin.local_addr().unwrap();
    let mixed = (Ipv4Addr::LOCALHOST, session.mixed_port);

    let tunnel = || {
        let mut client = TcpStream::connect(mixed).unwrap();
        client
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        write!(
            client,
            "CONNECT {endpoint} HTTP/1.1\r\nHost: {endpoint}\r\n\r\n"
        )
        .unwrap();
        let (server, _) = origin.accept().unwrap();
        let mut reply = Vec::new();
        while !reply.ends_with(b"\r\n\r\n") {
            let mut byte = [0];
            client.read_exact(&mut byte).unwrap();
            reply.push(byte[0]);
        }
        assert!(std::str::from_utf8(&reply).unwrap().contains(" 200 "));
        (client, server)
    };

    let (mut first, _first_origin) = tunnel();
    let [first_connection] = session
        .controller
        .connections()
        .unwrap()
        .try_into()
        .unwrap();
    let (mut second, mut second_origin) = tunnel();
    session
        .controller
        .close_connection(&first_connection.id)
        .unwrap();
    match first.read(&mut [0]) {
        Ok(0) => {}
        Err(error) if error.kind() == std::io::ErrorKind::ConnectionReset => {}
        outcome => panic!("closed connection should not remain readable: {outcome:?}"),
    }
    second_origin.write_all(b"P").unwrap();
    let mut marker = [0];
    second.read_exact(&mut marker).unwrap();
    assert_eq!(&marker, b"P");
    assert_eq!(session.controller.connections().unwrap().len(), 1);
}

#[test]
#[ignore = "requires a local Meow binary: MEOW_BIN=/path/to/meow"]
fn applies_local_profile_without_rebinding_and_preserves_it_on_rejection() {
    use meow_paw::app::{Desktop, EngineStatus, Snapshot};
    use std::{fs, time::Instant};

    fn wait(desktop: &Desktop, check: impl Fn(&Snapshot) -> bool) -> Snapshot {
        let deadline = Instant::now() + Duration::from_secs(25);
        loop {
            let snapshot = desktop.snapshot();
            assert!(
                !matches!(snapshot.engine, EngineStatus::Error(_)),
                "{:?}",
                snapshot.engine
            );
            if check(&snapshot) {
                return snapshot;
            }
            assert!(Instant::now() < deadline, "Timed out: {snapshot:?}");
            thread::sleep(Duration::from_millis(60));
        }
    }

    let desktop =
        Desktop::with_mixed_port(PathBuf::from(std::env::var_os("MEOW_BIN").unwrap()), 0).unwrap();
    let fixtures = tempfile::tempdir().unwrap();
    let original = fixtures.path().join("original.yaml");
    let candidate = fixtures.path().join("candidate.yaml");
    let rejected = fixtures.path().join("rejected.yaml");
    let original_yaml = "mode: rule\nrules: ['MATCH,DIRECT']\n";
    let candidate_yaml = "mode: global\nrules: ['MATCH,REJECT']\n";
    fs::write(&original, original_yaml).unwrap();
    fs::write(&candidate, candidate_yaml).unwrap();
    // The Paw parser accepts this mapping; the Meow reload API rejects the typed field.
    fs::write(&rejected, "mode: direct\nlog-level: [bad, shape]\n").unwrap();
    desktop.start(original.clone()).unwrap();
    let starting = wait(&desktop, |s| {
        matches!(s.engine, EngineStatus::Running { .. })
    });
    let EngineStatus::Running { mixed_port } = starting.engine else {
        unreachable!()
    };

    let origin = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let endpoint = origin.local_addr().unwrap();
    let mut client = TcpStream::connect((Ipv4Addr::LOCALHOST, mixed_port)).unwrap();
    client
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    write!(
        client,
        "CONNECT {endpoint} HTTP/1.1\r\nHost: {endpoint}\r\n\r\n"
    )
    .unwrap();
    let mut reply = Vec::new();
    while !reply.ends_with(b"\r\n\r\n") {
        let mut byte = [0];
        client.read_exact(&mut byte).unwrap();
        reply.push(byte[0]);
        assert!(reply.len() < 4096);
    }
    assert!(std::str::from_utf8(&reply).unwrap().contains(" 200 "));
    let (mut server, _) = origin.accept().unwrap();
    server.write_all(b"X").unwrap();
    let mut marker = [0];
    client.read_exact(&mut marker).unwrap();
    assert_eq!(marker, *b"X");

    desktop.apply_profile(candidate.clone()).unwrap();
    let applied = wait(&desktop, |s| s.profile.as_ref() == Some(&candidate));
    assert!(
        matches!(applied.engine, EngineStatus::Running { mixed_port: port } if port == mixed_port)
    );
    assert_eq!(applied.mode, Some(RoutingMode::Global));
    // A real change keeps the listener bound, not the established TCP stream.
    match client.read(&mut marker) {
        Ok(0) => {}
        Err(error) if error.kind() == std::io::ErrorKind::ConnectionReset => {}
        outcome => panic!("changed profile should close the tracked connection: {outcome:?}"),
    }
    desktop.read_rules().unwrap();
    let rules = wait(&desktop, |s| s.rules.is_some()).rules.unwrap();
    assert_eq!(rules[0].proxy, "REJECT");

    desktop.apply_profile(rejected).unwrap();
    let rejected = wait(&desktop, |s| s.action_error.is_some());
    assert!(
        rejected
            .action_error
            .as_deref()
            .unwrap()
            .starts_with("Meow rejected")
    );
    assert_eq!(rejected.profile.as_ref(), Some(&candidate));
    assert_eq!(rejected.mode, Some(RoutingMode::Global));
    assert!(
        matches!(rejected.engine, EngineStatus::Running { mixed_port: port } if port == mixed_port)
    );
    assert_eq!(fs::read_to_string(original).unwrap(), original_yaml);
    assert_eq!(fs::read_to_string(candidate).unwrap(), candidate_yaml);
}

#[test]
#[ignore = "requires a local Meow binary: MEOW_BIN=/path/to/meow"]
fn subscription_refresh_only_changes_disk_until_the_user_applies() {
    use meow_paw::{
        app::{Desktop, EngineStatus, Snapshot},
        subscription::Download,
        subscription_store::SubscriptionStore,
    };
    use std::time::Instant;

    fn wait_for(desktop: &Desktop, mut done: impl FnMut(&Snapshot) -> bool) -> Snapshot {
        let limit = Instant::now() + Duration::from_secs(15);
        loop {
            let snapshot = desktop.snapshot();
            assert!(
                !matches!(snapshot.engine, EngineStatus::Error(_)),
                "{:?}",
                snapshot.engine,
            );
            if done(&snapshot) {
                return snapshot;
            }
            assert!(Instant::now() < limit, "Timed out: {snapshot:?}");
            thread::sleep(Duration::from_millis(40));
        }
    }

    let binary = PathBuf::from(std::env::var_os("MEOW_BIN").unwrap());
    let desktop = Desktop::with_mixed_port(binary, 0).unwrap();
    let fixtures = tempfile::tempdir().unwrap();
    let store_dir = fixtures.path().to_path_buf();
    let mut store = SubscriptionStore::new(store_dir.clone());
    let download = |group: &str| Download {
        yaml: format!(
            "mode: rule\nproxies: [{{name: Local, type: direct}}]\n\
             proxy-groups: [{{name: {group}, type: select, proxies: [Local]}}]\n\
             rules: ['MATCH,{group}']\n"
        ),
        suggested_interval_minutes: Some(120),
    };
    let id = store
        .import(
            "https://example.test/local",
            download("Before"),
            Default::default(),
        )
        .unwrap();
    desktop
        .start_subscription(SubscriptionStore::new(store_dir.clone()), id)
        .unwrap();
    let started = wait_for(&desktop, |s| {
        matches!(s.engine, EngineStatus::Running { .. })
    });
    assert_eq!(
        started.profile.as_deref(),
        Some(store.record_path(id).as_path())
    );
    store.refresh(id, download("After")).unwrap();
    desktop.read_rules().unwrap();
    let before = wait_for(&desktop, |s| s.rules.is_some());
    assert_eq!(before.rules.unwrap()[0].proxy, "Before");

    desktop
        .apply_subscription(SubscriptionStore::new(store_dir), id)
        .unwrap();
    desktop.read_rules().unwrap();
    let after = wait_for(&desktop, |s| {
        s.rules
            .as_ref()
            .is_some_and(|rules| rules[0].proxy == "After")
    });
    let EngineStatus::Running { mixed_port } = started.engine else {
        unreachable!()
    };
    assert!(
        matches!(after.engine, EngineStatus::Running { mixed_port: port } if port == mixed_port)
    );
    assert_eq!(after.profile, started.profile);
}

#[test]
#[ignore = "requires a local Meow binary: MEOW_BIN=/path/to/meow"]
fn isolated_session_loads_inline_proxy_and_rule_providers() {
    use meow_paw::engine::controller::Origin;

    let binary = PathBuf::from(std::env::var_os("MEOW_BIN").expect("set MEOW_BIN"));
    let profile = Profile::parse(
        "strict: true\n\
         mode: rule\n\
         proxy-providers: {Embedded: {type: inline, payload: [{name: ProviderNode, type: socks5, server: 127.0.0.1, port: 9}]}}\n\
         proxy-groups: [{name: Select, type: select, use: [Embedded]}]\n\
         rule-providers: {Domains: {type: inline, behavior: domain, payload: [example.test]}}\n\
         rules: ['RULE-SET,Domains,Select', 'MATCH,DIRECT']\n",
    )
    .unwrap();
    let session = Session::launch_with_mixed_port(&binary, &profile, 0, || false).unwrap();
    let inventory = session.controller.inventory().unwrap();
    assert!(inventory.nodes.iter().any(|node| {
        node.name == "ProviderNode" && node.origin == Origin::Provider("Embedded".to_owned())
    }));
    let rules = session.controller.rules().unwrap();
    assert_eq!(rules[0].payload, "Domains");
    assert_eq!(rules[0].proxy, "Select");
}

#[test]
#[ignore = "requires new MEOW_BIN and previous MEOW_OLD_BIN"]
fn provider_policy_is_compatible_with_older_engines_without_unsafe_fallback() {
    let new = PathBuf::from(std::env::var_os("MEOW_BIN").unwrap());
    let old = PathBuf::from(std::env::var_os("MEOW_OLD_BIN").unwrap());
    let local = Profile::parse("mode: direct\nrules: ['MATCH,DIRECT']\n").unwrap();
    Session::launch_with_mixed_port(&old, &local, 0, || false)
        .unwrap()
        .stop()
        .unwrap();
    Session::launch_with_mixed_port(&new, &local, 0, || false)
        .unwrap()
        .stop()
        .unwrap();
    let remote = Profile::parse(
        "mode: direct\nproxy-providers: {P: {type: http, url: 'https://127.0.0.1:9/nodes', path: /tmp/unsafe.yaml}}\nrules: ['MATCH,DIRECT']\n",
    ).unwrap();
    let error = match Session::launch_with_mixed_port(&old, &remote, 0, || false) {
        Ok(_) => panic!("old engine must reject HTTPS provider profiles"),
        Err(error) => error,
    };
    assert!(error.to_string().contains("not supported"));
    // No server listens at this endpoint. A new engine may start with an
    // initially empty provider, but it must not fall back to an old engine.
    let session = Session::launch_with_mixed_port(&new, &remote, 0, || false).unwrap();
    session.stop().unwrap();
}

#[test]
#[ignore = "requires patched MEOW_BIN"]
fn slow_provider_does_not_block_guarded_engine_startup() {
    use std::time::Instant;
    let binary = PathBuf::from(std::env::var_os("MEOW_BIN").unwrap());
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    listener.set_nonblocking(true).unwrap();
    let address = listener.local_addr().unwrap();
    let (seen, connected) = mpsc::channel();
    let (stop, finish) = mpsc::channel();
    let server = thread::spawn(move || {
        loop {
            if finish.try_recv().is_ok() {
                break;
            }
            match listener.accept() {
                Ok((stream, _)) => {
                    let _ = seen.send(());
                    // Stall TLS without using the host network or touching routes.
                    let _stream = stream;
                    let _ = finish.recv_timeout(Duration::from_secs(8));
                    break;
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(10))
                }
                Err(e) => panic!("{e}"),
            }
        }
    });
    let profile = Profile::parse(&format!(
        "mode: direct\nproxy-providers: {{P: {{type: http, url: 'https://{address}/nodes'}}}}\nrules: ['MATCH,DIRECT']\n"
    )).unwrap();
    let start = Instant::now();
    let session = Session::launch_with_mixed_port(&binary, &profile, 0, || false).unwrap();
    assert!(start.elapsed() < Duration::from_secs(5));
    connected.recv_timeout(Duration::from_secs(3)).unwrap();
    assert!(
        session
            .controller
            .inventory()
            .unwrap()
            .providers
            .contains_key("P")
    );
    session.stop().unwrap();
    let _ = stop.send(());
    server.join().unwrap();
}

#[test]
#[ignore = "requires patched MEOW_BIN"]
fn slow_provider_does_not_block_live_profile_apply() {
    use meow_paw::app::{Desktop, EngineStatus};
    use std::time::Instant;
    let binary = PathBuf::from(std::env::var_os("MEOW_BIN").unwrap());
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    listener.set_nonblocking(true).unwrap();
    let address = listener.local_addr().unwrap();
    let (seen, connected) = mpsc::channel();
    let (stop, finish) = mpsc::channel();
    let server = thread::spawn(move || {
        loop {
            if finish.try_recv().is_ok() {
                break;
            }
            match listener.accept() {
                Ok((stream, _)) => {
                    let _ = seen.send(());
                    let _stream = stream;
                    let _ = finish.recv_timeout(Duration::from_secs(8));
                    break;
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(10))
                }
                Err(e) => panic!("{e}"),
            }
        }
    });
    let dir = tempfile::tempdir().unwrap();
    let before = dir.path().join("before.yaml");
    let after = dir.path().join("after.yaml");
    std::fs::write(&before, "mode: direct\nrules: ['MATCH,DIRECT']\n").unwrap();
    std::fs::write(&after, format!(
        "mode: direct\nproxy-providers: {{P: {{type: http, url: 'https://{address}/nodes'}}}}\nrules: ['MATCH,DIRECT']\n"
    )).unwrap();
    let desktop = Desktop::with_mixed_port(binary, 0).unwrap();
    desktop.start(before).unwrap();
    let end = Instant::now() + Duration::from_secs(8);
    while !matches!(desktop.snapshot().engine, EngineStatus::Running { .. }) {
        assert!(Instant::now() < end, "engine start timed out");
        thread::sleep(Duration::from_millis(30));
    }
    let start = Instant::now();
    desktop.apply_profile(after.clone()).unwrap();
    loop {
        let snapshot = desktop.snapshot();
        assert!(
            snapshot.action_error.is_none(),
            "{:?}",
            snapshot.action_error
        );
        if snapshot.profile.as_ref() == Some(&after) && snapshot.applying_profile.is_none() {
            break;
        }
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "live apply timed out"
        );
        thread::sleep(Duration::from_millis(30));
    }
    connected.recv_timeout(Duration::from_secs(3)).unwrap();
    drop(desktop);
    let _ = stop.send(());
    server.join().unwrap();
}

#[test]
#[ignore = "requires test-only CA Meow and MEOW_PROVIDER_TEST_URL"]
fn guarded_https_provider_fetches_and_refreshes_over_trusted_local_tls() {
    use meow_paw::engine::controller::Origin;
    use std::time::Instant;
    let binary = PathBuf::from(std::env::var_os("MEOW_BIN").unwrap());
    let url = std::env::var("MEOW_PROVIDER_TEST_URL").unwrap();
    let profile = Profile::parse(&format!(
        "mode: direct\nproxy-providers: {{P: {{type: http, url: '{url}'}}}}\nproxy-groups: [{{name: G, type: select, use: [P]}}]\nrules: ['MATCH,DIRECT']\n"
    )).unwrap();
    let session = Session::launch_with_mixed_port(&binary, &profile, 0, || false).unwrap();
    let end = Instant::now() + Duration::from_secs(8);
    loop {
        let inventory = session.controller.inventory().unwrap();
        if inventory
            .nodes
            .iter()
            .any(|n| n.name == "FixtureNode" && n.origin == Origin::Provider("P".into()))
        {
            break;
        }
        assert!(Instant::now() < end, "HTTPS provider did not populate");
        thread::sleep(Duration::from_millis(30));
    }
    session.controller.refresh_provider("P").unwrap();
    let inventory = session.controller.inventory().unwrap();
    assert!(inventory.nodes.iter().any(|n| n.name == "FixtureNode"));
    assert_eq!(
        inventory.providers["P"]
            .subscription_info
            .as_ref()
            .unwrap()
            .upload,
        1
    );
    session.stop().unwrap();
}

#[test]
#[ignore = "requires test-only CA Meow and local fixture state"]
fn saved_profile_provider_cache_is_stable_and_isolated_on_live_apply() {
    use meow_paw::app::{Desktop, EngineStatus, Snapshot};
    use meow_paw::engine::controller::Origin;
    use std::time::Instant;

    fn wait_for(desktop: &Desktop, predicate: impl Fn(&Snapshot) -> bool) {
        let end = Instant::now() + Duration::from_secs(10);
        loop {
            let snapshot = desktop.snapshot();
            assert!(
                snapshot.action_error.is_none(),
                "{:?}",
                snapshot.action_error
            );
            assert!(
                !matches!(snapshot.engine, EngineStatus::Error(_)),
                "{:?}",
                snapshot.engine
            );
            if predicate(&snapshot) {
                return;
            }
            assert!(Instant::now() < end, "state timed out: {snapshot:?}");
            thread::sleep(Duration::from_millis(30));
        }
    }
    fn has_fixture(snapshot: &Snapshot) -> bool {
        snapshot.inventory.as_ref().is_some_and(|inv| {
            inv.nodes.iter().any(|node| {
                node.name == "FixtureNode" && node.origin == Origin::Provider("P".into())
            })
        })
    }
    let binary = PathBuf::from(std::env::var_os("MEOW_BIN").unwrap());
    let url = std::env::var("MEOW_PROVIDER_TEST_URL").unwrap();
    let state = PathBuf::from(std::env::var_os("MEOW_PROVIDER_TEST_STATE").unwrap());
    let dir = tempfile::tempdir().unwrap();
    let first = dir.path().join("first.yaml");
    let second = dir.path().join("second.yaml");
    let profile = format!(
        "proxy-providers: {{P: {{type: http, url: '{url}'}}}}\nproxy-groups: [{{name: G, type: select, use: [P]}}]\nrules: ['MATCH,DIRECT']\n"
    );
    std::fs::write(&state, "online").unwrap();
    std::fs::write(&first, format!("mode: rule\n{profile}")).unwrap();
    std::fs::write(&second, format!("mode: direct\n{profile}")).unwrap();
    let desktop = Desktop::with_mixed_port(binary, 0).unwrap();
    desktop.start(first.clone()).unwrap();
    wait_for(&desktop, |s| {
        matches!(s.engine, EngineStatus::Running { .. }) && has_fixture(s)
    });
    // An unrelated rules/mode edit and different caller-supplied cache path
    // must not change the provider identity or discard the last good nodes.
    std::fs::write(&state, "offline").unwrap();
    std::fs::write(&first, format!("mode: direct\n{profile}")).unwrap();
    desktop.apply_profile(first.clone()).unwrap();
    wait_for(&desktop, |s| {
        s.profile.as_ref() == Some(&first) && s.applying_profile.is_none() && has_fixture(s)
    });
    // A second saved profile with the same provider label and URL must not
    // inherit the first profile's previously cached nodes.
    desktop.apply_profile(second.clone()).unwrap();
    wait_for(&desktop, |s| {
        s.profile.as_ref() == Some(&second)
            && s.applying_profile.is_none()
            && s.inventory
                .as_ref()
                .is_some_and(|i| i.providers.contains_key("P"))
            && !has_fixture(s)
    });
    drop(desktop);
}
