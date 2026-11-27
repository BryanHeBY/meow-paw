#![cfg(target_os = "linux")]

use meow_paw::{
    app::{Desktop, EngineStatus, ProxyState, Snapshot},
    system_proxy::Consent,
};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    thread,
    time::{Duration, Instant},
};

#[test]
#[ignore = "requires a local Meow binary and /usr/bin/python3; uses an isolated fake desktop"]
fn managed_proxy_worker_uses_only_fixture_desktop() {
    if let Some(path) = std::env::var_os("PAW_TEST_PROXY_FIXTURE") {
        fixture_child(&PathBuf::from(path));
        return;
    }
    let meow = fs::canonicalize(std::env::var_os("MEOW_BIN").expect("set MEOW_BIN")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let bin = dir.path().join("bin");
    fs::create_dir(&bin).unwrap();
    for name in ["gsettings", "dconf"] {
        use std::os::unix::fs::PermissionsExt;
        let executable = bin.join(name);
        fs::write(&executable, include_str!("fixtures/fake_gsettings.py")).unwrap();
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
    }
    let fake_state = dir.path().join("gsettings.json");
    fs::write(&fake_state, "{}").unwrap();
    // Only the two mock binaries are on PATH. The child cannot accidentally
    // touch the host desktop even when running under a real GNOME session.
    let output = Command::new(std::env::current_exe().unwrap())
        .args([
            "--ignored",
            "--exact",
            "managed_proxy_worker_uses_only_fixture_desktop",
            "--nocapture",
        ])
        .env("PATH", &bin)
        .env("MEOW_BIN", meow)
        .env("PAW_TEST_PROXY_FIXTURE", dir.path())
        .env("PAW_TEST_GSETTINGS_STATE", fake_state)
        .env("XDG_CURRENT_DESKTOP", "GNOME")
        .env(
            "DBUS_SESSION_BUS_ADDRESS",
            "unix:path=/not-a-real-desktop-bus",
        )
        .env_remove("DCONF_PROFILE")
        .env_remove("GSETTINGS_BACKEND")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn fixture_child(dir: &Path) {
    assert!(dir.join("bin/gsettings").is_file() && dir.join("bin/dconf").is_file());
    assert_eq!(std::env::var_os("PATH").unwrap(), dir.join("bin"));
    let worker = Desktop::with_desktop_proxy(
        std::env::var_os("MEOW_BIN").map(PathBuf::from),
        0,
        Some(dir.to_owned()),
    )
    .unwrap();
    let state = || fs::read_to_string(dir.join("gsettings.json")).unwrap();
    wait(&worker, |s| s.system_proxy == ProxyState::Off);
    assert_eq!(state(), "{}");
    assert!(!dir.join("system-proxy").exists());
    worker.enable_system_proxy(Consent::GrantedByUser).unwrap();
    wait(&worker, |s| s.system_proxy_error.is_some());
    assert_eq!(state(), "{}");

    let profile = dir.join("profile.yaml");
    fs::write(&profile, "mode: rule\nrules: ['MATCH,DIRECT']\n").unwrap();
    worker.start(profile).unwrap();
    let EngineStatus::Running { mixed_port } = wait(&worker, |s| {
        matches!(s.engine, EngineStatus::Running { .. })
    })
    .engine
    else {
        unreachable!();
    };
    worker.enable_system_proxy(Consent::GrantedByUser).unwrap();
    wait(
        &worker,
        |s| matches!(s.system_proxy, ProxyState::On(port) if port.get() == mixed_port),
    );
    let expected = state();
    let values: serde_json::Value = serde_json::from_str(&expected).unwrap();
    assert_eq!(values["https/host"], "'127.0.0.1'");
    assert_eq!(values["http/port"], mixed_port.to_string());

    // A foreign operator changes the desktop. A normal Stop must not
    // clobber that change and must keep the owned listener alive.
    let mut changed = values;
    changed["http/port"] = serde_json::Value::String("3128".into());
    fs::write(dir.join("gsettings.json"), changed.to_string()).unwrap();
    worker.stop();
    let blocked = wait(&worker, |s| s.stop_result == Some(false));
    assert!(matches!(blocked.engine, EngineStatus::Running { .. }));
    assert_eq!(blocked.system_proxy, ProxyState::Recovery);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&state()).unwrap()["http/port"],
        "3128"
    );
    assert!(dir.join("system-proxy/recovery.json").is_file());

    // Simulate the operator returning the settings to the journal's exact
    // match. Clean Stop can now restore the original state and stop Meow.
    fs::write(dir.join("gsettings.json"), expected).unwrap();
    worker.stop();
    let stopped = wait(&worker, |s| s.stop_result == Some(true));
    assert!(matches!(stopped.engine, EngineStatus::Stopped));
    assert_eq!(stopped.system_proxy, ProxyState::Off);
    assert_eq!(state(), "{}");
    assert!(!dir.join("system-proxy/recovery.json").exists());
}

fn wait(worker: &Desktop, pred: impl Fn(&Snapshot) -> bool) -> Snapshot {
    let deadline = Instant::now() + Duration::from_secs(25);
    loop {
        let snap = worker.snapshot();
        if pred(&snap) {
            return snap;
        }
        assert!(!matches!(snap.engine, EngineStatus::Error(_)), "{snap:?}");
        assert!(Instant::now() < deadline, "{snap:?}");
        thread::sleep(Duration::from_millis(30));
    }
}
