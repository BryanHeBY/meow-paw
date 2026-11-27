#!/usr/bin/env python3
"""Run inside dbus-run-session and Xvfb; all inspected PIDs are test-owned."""
import os
from pathlib import Path
import socket
import subprocess as sp
import tempfile
import time

from x11_smoke import activate, child_ids, eventually, item_ready, one_window, output, running_ports, stop, windows, wm_close


def fixture_port():
    with socket.socket() as listener:
        listener.bind(("127.0.0.1", 0))
        return str(listener.getsockname()[1])


with tempfile.TemporaryDirectory(prefix="paw-tray-smoke-") as directory:
    profile = Path(directory) / "isolated.yaml"
    profile.write_text("mode: rule\nipv6: false\ndns: {enable: false}\nrules: ['MATCH,DIRECT']\n")
    native = Path("target/debug/meow-paw")
    fake_tray = Path("target/debug/examples/tray_test_watcher")
    for running in [False, True]:
        if running and not os.environ.get("MEOW_BIN"):
            continue
        watcher = sp.Popen([fake_tray])
        flags = ["--profile", str(profile), "--mixed-port", fixture_port(), "--start"] if running else []
        app = sp.Popen([native, "--no-preferences", *flags])
        try:
            first = eventually(lambda: one_window(app), "initial window")
            service = f"org.kde.StatusNotifierItem-{app.pid}-1"
            eventually(lambda: item_ready(service), "SNI registration")
            assert '"Active"' in output("busctl", "--user", "get-property", service,
                                        "/StatusNotifierItem", "org.kde.StatusNotifierItem", "Status")
            ports = eventually(lambda: running_ports(app), "running managed engine") if running else None
            time.sleep(.4)
            wm_close(first)
            eventually(lambda: not windows(app), "first native window closed")
            assert app.poll() is None, "tray must retain application"
            if running:
                assert running_ports(app) == ports
            activate(service)
            reopened = eventually(lambda: one_window(app), "tray opens window")
            for _ in range(3):
                activate(service)
            time.sleep(.5)
            assert windows(app) == [reopened], "duplicate Paw window"
            if running:
                assert running_ports(app) == ports, "managed listeners changed"
            wm_close(reopened)
            eventually(lambda: not windows(app), "reopened window closed")
            stop(watcher)
            fallback = eventually(lambda: one_window(app), "window recovery after tray disappears")
            wm_close(fallback)
            eventually(lambda: app.poll() is not None, "last-window exit without host")
            assert app.returncode == 0, app.returncode
            print(f"SNI reopen, duplicate guard and host-loss recovery: {'running' if running else 'stopped'}", flush=True)
        finally:
            stop(app)
            stop(watcher)


def menu_call(service, method, *args):
    completed = sp.run(
        ["busctl", "--user", "--", "call", service, "/MenuBar", "com.canonical.dbusmenu", method, *args],
        capture_output=True,
        text=True,
        timeout=3,
    )
    assert completed.returncode == 0, (method, completed.stderr)
    return completed.stdout.strip()


def click_menu(service, item):
    menu_call(service, "Event", "isvu", str(item), "clicked", "s", "", "0")


with tempfile.TemporaryDirectory(prefix="paw-tray-menu-") as directory:
    profile = Path(directory) / "isolated.yaml"
    profile.write_text("mode: rule\nipv6: false\ndns: {enable: false}\nrules: ['MATCH,DIRECT']\n")
    flags = ["--profile", str(profile), "--mixed-port", fixture_port(), "--start"] if os.environ.get("MEOW_BIN") else []
    watcher = sp.Popen(["target/debug/examples/tray_test_watcher"])
    app = sp.Popen(["target/debug/meow-paw", "--no-preferences", "--locale", "zh-CN", *flags])
    try:
        first = eventually(lambda: one_window(app), "initial menu test window")
        service = f"org.kde.StatusNotifierItem-{app.pid}-1"
        eventually(lambda: item_ready(service), "menu item registration")
        assert output("busctl", "--user", "get-property", service, "/StatusNotifierItem",
                      "org.kde.StatusNotifierItem", "Menu") == 'o "/MenuBar"'
        layout = menu_call(service, "GetLayout", "iias", "0", "-1", "0")
        assert layout.startswith("u(ia{sv}av) "), layout
        for label in ("显示", "退出"):
            # busctl's text output can escape non-ASCII bytes regardless of locale.
            escaped = "".join(f"\\{byte:03o}" for byte in label.encode())
            assert label in layout or escaped in layout, layout
        if flags:
            eventually(lambda: running_ports(app), "running engine before menu command")
        children = child_ids(app.pid)
        if flags:
            assert children, "expected a tracked managed engine"
        time.sleep(.4)
        wm_close(first)
        eventually(lambda: not windows(app), "menu test window hidden")
        assert app.poll() is None, "tray must retain app"
        click_menu(service, 1)
        eventually(lambda: one_window(app), "Show via DBusMenu reopens the window")
        if flags:
            assert child_ids(app.pid) == children, "Show should retain the same managed engine"
        click_menu(service, 2)
        assert app.wait(timeout=10) == 0, app.returncode
        eventually(lambda: all(not Path(f"/proc/{pid}").exists() for pid in children),
                   "Quit via DBusMenu reaps the managed engine")
        print("SNI localized menu layout, Show and clean Quit: passed", flush=True)
    finally:
        stop(app)
        stop(watcher)
