#![cfg(any(target_os = "linux", windows))]

use meow_paw::{engine::session::Session, profile::Profile};
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
