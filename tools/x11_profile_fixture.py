"""Shared, local-only Xvfb and XDND fixture primitives for Paw."""
import os
from pathlib import Path
import re
import shutil
import subprocess as sp
import sys
from tempfile import TemporaryDirectory

from x11_smoke import eventually, one_window


WIDTH, HEIGHT = 1120, 738
# Coordinates are window-local, not desktop-global. This English fixture's
# persistent Profiles view has the attached-file row above the dedicated managed-copy area.
DROP_X, DROP_Y = 470, 168


def loopback_only():
    # A local X display alone is not enough: the app also starts local services.
    interfaces = sp.check_output(["ip", "-o", "link", "show"], text=True)
    if not re.fullmatch(r"1: lo: [^\n]+\n", interfaces):
        raise SystemExit("Run inside a network namespace with only loopback")


def run_in_xvfb(script, fixture_mode, binary, *extra, isolate_paw=False, timeout=65):
    loopback_only()
    env = {"PATH": os.environ["PATH"], "LANG": "C.UTF-8",
           "PYTHONDONTWRITEBYTECODE": "1",
           "DBUS_SESSION_BUS_ADDRESS": "unix:path=/nonexistent/paw-no-session-bus",
           "DBUS_SYSTEM_BUS_ADDRESS": "unix:path=/nonexistent/paw-no-system-bus"}
    with TemporaryDirectory(prefix="paw-xdnd-") as name:
        home = Path(name)
        env["HOME"] = name
        for key in ("CONFIG", "RUNTIME", "CACHE", "DATA", "STATE"):
            directory = home / key.lower()
            directory.mkdir(mode=0o700)
            env[f"XDG_{key}_HOME" if key != "RUNTIME" else "XDG_RUNTIME_DIR"] = str(directory)
        if isolate_paw:
            # Paw auto-discovers a Meow executable beside itself. Use a fresh
            # directory with no sibling engine, even if the trusted input
            # binary came from a complete release package.
            isolated = home / "meow-paw"
            try:
                os.link(binary, isolated)
            except OSError:
                shutil.copy2(binary, isolated)
            binary = isolated
        sender = home / "sender"
        sp.run(["cc", "-std=c11", "-Wall", "-Wextra", "-Werror",
                script.with_name("x11-xdnd-fixture.c"), "-lX11", "-o", sender],
               check=True, env=env, timeout=20)
        sp.run(["xvfb-run", "-a", "-s", "-screen 0 1280x900x24 -nolisten tcp",
                sys.executable, script, fixture_mode, binary, sender, *extra],
               check=True, env=env, timeout=timeout)


def guarded_home():
    loopback_only()
    home = Path(os.environ["HOME"])
    assert home.name.startswith("paw-xdnd-")
    assert Path(os.environ["XDG_CONFIG_HOME"]) == home / "config"
    assert re.fullmatch(r":\d+", os.environ["DISPLAY"])
    assert "WAYLAND_DISPLAY" not in os.environ and Path(os.environ["XAUTHORITY"]).is_file()
    assert (os.environ["DBUS_SESSION_BUS_ADDRESS"], os.environ["DBUS_SYSTEM_BUS_ADDRESS"]) == (
        "unix:path=/nonexistent/paw-no-session-bus",
        "unix:path=/nonexistent/paw-no-system-bus",
    )
    return home


def app_window(app):
    window = str(eventually(lambda: one_window(app), "isolated Paw window"))
    assert int(sp.check_output(["xdotool", "getwindowpid", window], timeout=5)) == app.pid
    return window


def offer_file(sender, window, point, mode, uri, source):
    return sp.check_output(
        [sender, window, *point, mode, uri, source],
        env={**os.environ, "PAW_TEST_XVFB": os.environ["DISPLAY"]},
        text=True, timeout=8,
    )


def xdotool_properties(*args):
    text = sp.check_output(["xdotool", *args], text=True, timeout=5)
    return dict(line.split("=", 1) for line in text.splitlines())


def drop_target(window):
    sp.run(["xdotool", "windowsize", "--sync", window, str(WIDTH), str(HEIGHT)],
           check=True, timeout=5)
    geometry = xdotool_properties("getwindowgeometry", "--shell", window)
    assert (int(geometry["WIDTH"]), int(geometry["HEIGHT"])) == (WIDTH, HEIGHT), geometry
    sp.run(["xdotool", "mousemove", "--window", window, str(DROP_X), str(DROP_Y)],
           check=True, timeout=5)
    pointer = xdotool_properties("getmouselocation", "--shell")
    assert (int(pointer["X"]) - int(geometry["X"]), int(pointer["Y"]) - int(geometry["Y"])) == (
        DROP_X, DROP_Y
    ), (geometry, pointer)
    return pointer["X"], pointer["Y"]
