#!/usr/bin/env python3
"""Run inside dbus-run-session and Xvfb; all inspected PIDs are test-owned."""
import ctypes as c
import glob
import os
from pathlib import Path
import re
import subprocess as sp
import tempfile
import time


def output(*argv):
    return sp.run(argv, capture_output=True, text=True, timeout=3).stdout.strip()


def eventually(test, name, seconds=12):
    end = time.monotonic() + seconds
    while time.monotonic() < end:
        if value := test():
            return value
        time.sleep(.1)
    raise AssertionError(name)


class XClientMessage(c.Structure):
    _fields_ = [
        ("type", c.c_int), ("serial", c.c_ulong), ("send_event", c.c_int),
        ("display", c.c_void_p), ("window", c.c_ulong),
        ("message_type", c.c_ulong), ("format", c.c_int), ("data", c.c_long * 5),
    ]


def wm_close(window):
    x = c.CDLL("libX11.so.6")
    x.XOpenDisplay.restype = c.c_void_p
    x.XInternAtom.argtypes = [c.c_void_p, c.c_char_p, c.c_int]
    x.XInternAtom.restype = c.c_ulong
    x.XSendEvent.argtypes = [c.c_void_p, c.c_ulong, c.c_int, c.c_long, c.c_void_p]
    x.XSync.argtypes = [c.c_void_p, c.c_int]
    x.XCloseDisplay.argtypes = [c.c_void_p]
    display = x.XOpenDisplay(None)
    assert display
    try:
        event = XClientMessage(33, 0, 0, display, window, x.XInternAtom(display, b"WM_PROTOCOLS", 0), 32)
        event.data[0] = x.XInternAtom(display, b"WM_DELETE_WINDOW", 0)
        assert x.XSendEvent(display, window, 0, 0, c.byref(event))
        x.XSync(display, 0)
    finally:
        x.XCloseDisplay(display)


def windows(proc):
    result = output("xdotool", "search", "--pid", str(proc.pid), "--onlyvisible")
    return list(map(int, result.splitlines()))


def stop(proc):
    if proc.poll() is None:
        proc.terminate()
        proc.wait(4)


def one_window(proc):
    wins = windows(proc)
    assert len(wins) <= 1, wins
    return wins[0] if wins else None


def child_ids(pid):
    result = set()
    for file in glob.glob(f"/proc/{pid}/task/*/children"):
        try:
            result.update(Path(file).read_text().split())
        except FileNotFoundError:
            pass
    return result


def running_ports(proc):
    pids = child_ids(proc.pid)
    listeners = []
    for line in output("ss", "-Hlnpt", "src", "127.0.0.1").splitlines():
        if pids.intersection(re.findall(r"pid=(\d+)", line)):
            listeners.append(line.split()[3])
    return sorted(listeners) if len(listeners) == 2 else None


def activate(service):
    return sp.run(
        ["busctl", "--user", "call", service, "/StatusNotifierItem", "org.kde.StatusNotifierItem", "Activate", "ii", "0", "0"],
        check=True,
        capture_output=True,
        timeout=3,
    )


def item_ready(service):
    body = output(
        "busctl", "--user", "get-property", "org.kde.StatusNotifierWatcher",
        "/StatusNotifierWatcher", "org.kde.StatusNotifierWatcher",
        "RegisteredStatusNotifierItems",
    )
    return service in body


with tempfile.TemporaryDirectory(prefix="paw-tray-smoke-") as directory:
    profile = Path(directory) / "isolated.yaml"
    profile.write_text("mode: rule\nipv6: false\ndns: {enable: false}\nrules: ['MATCH,DIRECT']\n")
    native = Path("target/debug/meow-paw")
    fake_tray = Path("target/debug/examples/tray_test_watcher")
    for running in [False, True]:
        if running and not os.environ.get("MEOW_BIN"):
            continue
        watcher = sp.Popen([fake_tray])
        flags = ["--profile", str(profile), "--start"] if running else []
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
    flags = ["--profile", str(profile), "--start"] if os.environ.get("MEOW_BIN") else []
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
