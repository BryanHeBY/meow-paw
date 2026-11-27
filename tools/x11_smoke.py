#!/usr/bin/env python3
"""Shared window-manager events for isolated Xvfb smoke tests."""
import ctypes as c
import glob
import os
from pathlib import Path
import re
import subprocess as sp
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
