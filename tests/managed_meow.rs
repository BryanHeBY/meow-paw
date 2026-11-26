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
    let session = Session::launch(&binary, &profile, || false).unwrap();
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
    let session = Session::launch(&binary, &profile, || false).unwrap();
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

    let desktop = Desktop::new(PathBuf::from(std::env::var_os("MEOW_BIN").unwrap())).unwrap();
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
    let desktop = Desktop::new(binary).unwrap();
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
    let session = Session::launch(&binary, &profile, || false).unwrap();
    let inventory = session.controller.inventory().unwrap();
    assert!(inventory.nodes.iter().any(|node| {
        node.name == "ProviderNode" && node.origin == Origin::Provider("Embedded".to_owned())
    }));
    let rules = session.controller.rules().unwrap();
    assert_eq!(rules[0].payload, "Domains");
    assert_eq!(rules[0].proxy, "Select");
}
