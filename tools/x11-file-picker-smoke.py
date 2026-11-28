#!/usr/bin/env python3
"""Exercise Profiles → Import YAML copies through a private portal."""
import json
import os
from pathlib import Path
import re
import shlex
import shutil
import subprocess as sp
import sys
from tempfile import TemporaryDirectory
import time
from xml.sax.saxutils import escape

from x11_profile_fixture import HEIGHT, WIDTH, app_window, loopback_only, xdotool_properties
from x11_smoke import eventually, stop

# English 1120x738 fixture: button on the dedicated local import row.
# The portal verifies the exact action so a layout drift cannot click a
# different file-picker path and still appear to pass.
IMPORT_COPIES = (999, 168)
NO_BUS = "unix:path=/nonexistent/paw-no-system-bus"


def run(binary):
    loopback_only()
    env = {"PATH": os.environ["PATH"], "LANG": "C.UTF-8",
           "PYTHONDONTWRITEBYTECODE": "1", "DBUS_SYSTEM_BUS_ADDRESS": NO_BUS}
    script = Path(__file__).resolve()
    with TemporaryDirectory(prefix="paw-picker-") as name:
        home = Path(name)
        env["HOME"] = name
        for key in ("CONFIG", "RUNTIME", "CACHE", "DATA", "STATE"):
            directory = home / key.lower()
            directory.mkdir(mode=0o700)
            env[f"XDG_{key}_HOME" if key != "RUNTIME" else "XDG_RUNTIME_DIR"] = str(directory)

        isolated = home / "meow-paw"
        try:
            os.link(binary, isolated)
        except OSError:
            shutil.copy2(binary, isolated)

        portal = home / "portal-fixture"
        flags = sp.check_output(["pkg-config", "--cflags", "--libs", "gio-2.0"], text=True,
                                env=env, timeout=8)
        sp.run(["cc", "-std=c11", "-Wall", "-Wextra", "-Werror",
                script.with_name("x11-file-chooser-portal.c"), *shlex.split(flags), "-o", portal],
               check=True, env=env, timeout=20)

        # A hand-built config has no service directories or activation files.
        # Neither an inherited session bus nor the host XDG portal is used.
        address = f"unix:path={home / 'runtime' / 'bus'}"
        config = home / "bus.xml"
        config.write_text(f"""<busconfig>
 <type>session</type>
 <listen>{escape(address)}</listen>
 <auth>EXTERNAL</auth>
 <policy context="default">
  <allow send_destination="*"/><allow receive_sender="*"/><allow own="*"/>
 </policy>
</busconfig>
""", encoding="utf-8")
        with (home / "bus.log").open("w+") as bus_log:
            bus = sp.Popen(["dbus-daemon", "--nofork", "--nopidfile", "--print-address",
                            f"--config-file={config}"], stdout=bus_log, stderr=sp.STDOUT, env=env)
            try:
                eventually(lambda: (home / "runtime" / "bus").is_socket(),
                           "private D-Bus socket was not created")
                env["DBUS_SESSION_BUS_ADDRESS"] = address
                sp.run(["xvfb-run", "-a", "-s", "-screen 0 1280x900x24 -nolisten tcp",
                        sys.executable, script, "--fixture", isolated, portal],
                       check=True, env=env, timeout=60)
            finally:
                stop(bus)


def fixture(binary, portal):
    loopback_only()
    home = Path(os.environ["HOME"])
    config = Path(os.environ["XDG_CONFIG_HOME"])
    runtime = Path(os.environ["XDG_RUNTIME_DIR"])
    assert home.name.startswith("paw-picker-") and config == home / "config"
    assert runtime == home / "runtime" and (runtime / "bus").is_socket()
    assert os.environ["DBUS_SESSION_BUS_ADDRESS"] == f"unix:path={runtime / 'bus'}"
    assert os.environ["DBUS_SYSTEM_BUS_ADDRESS"] == NO_BUS
    assert re.fullmatch(r":\d+", os.environ["DISPLAY"]) and Path(os.environ["XAUTHORITY"]).is_file()
    assert "WAYLAND_DISPLAY" not in os.environ and "MEOW_BIN" not in os.environ
    assert binary == home / "meow-paw" and not (home / "meow").exists()
    assert portal == home / "portal-fixture"

    inputs = (
        (home / "fixture alpha.yaml", b"mode: direct\nlog-level: info\n"),
        (home / "fixture beta.yml", b"mode: rule\nlog-level: warning\n"),
    )
    for path, payload in inputs:
        path.write_bytes(payload)
    preferences = config / "meow-paw" / "preferences.json"
    managed = preferences.with_name("local-profiles")

    def saved():
        return json.loads(preferences.read_text(encoding="utf-8")) if preferences.exists() else {}

    def no_imports():
        state = saved()
        assert not state.get("profiles") and not state.get("profile") and not state.get("engine")
        assert not any(p.is_file() for p in managed.rglob("*"))

    portal_log = home / "portal.log"

    def observed(expected_count):
        text = portal_log.read_text(encoding="utf-8")
        assert "Unexpected" not in text, text
        events = re.findall(r"^OpenFile (\d+) ([\w-]+) (\S+)$", text, flags=re.MULTILINE)
        assert len(events) <= expected_count, text
        return events if len(events) == expected_count else None

    with portal_log.open("w") as dbus_log:
        server = sp.Popen([portal, *(path.as_uri() for path, _ in inputs)],
                          stdout=dbus_log, stderr=sp.STDOUT, cwd=home)
        try:
            eventually(lambda: portal_log.read_text(encoding="utf-8").startswith("READY\n"),
                       "synthetic FileChooser did not own the test bus")
            with (home / "app.log").open("w") as app_log:
                app = sp.Popen([binary, "--page", "profiles", "--locale", "en", "--no-tray"],
                               stdout=app_log, stderr=sp.STDOUT, cwd=home)
                try:
                    window = app_window(app)
                    sp.run(["xdotool", "windowsize", "--sync", window, str(WIDTH), str(HEIGHT)],
                           check=True, timeout=5)
                    geometry = xdotool_properties("getwindowgeometry", "--shell", window)
                    assert (int(geometry["WIDTH"]), int(geometry["HEIGHT"])) == (WIDTH, HEIGHT), geometry
                    sp.run(["xdotool", "windowfocus", "--sync", window], check=True, timeout=5)

                    wanted = ["cancel", "non-local", "local"]
                    for index, kind in enumerate(wanted, 1):
                        no_imports()
                        x, y = IMPORT_COPIES
                        sp.run(["xdotool", "mousemove", "--window", window, str(x), str(y),
                                "click", "1"], check=True, timeout=5)
                        events = eventually(lambda: observed(index),
                                            f"OpenFile {kind}: check fixture coordinates and D-Bus log",
                                            seconds=5)
                        assert [event[1] for event in events] == wanted[:index], events
                        assert len({event[2] for event in events}) == index, events
                        if kind != "local":
                            # The server logs after queuing its response, before
                            # Paw receives and paints the now-enabled button.
                            time.sleep(.3)
                            no_imports()
                            # The next real click must open another portal request,
                            # proving this response cleared the app's pending state.
                        print(f"PASS portal round trip: {kind}", flush=True)

                    profile_paths = eventually(lambda: saved().get("profiles")
                                               if len(saved().get("profiles", [])) == len(inputs)
                                               else None,
                                               "two persistent managed YAML copies", seconds=6)
                    assert not saved().get("profile") and not saved().get("engine"), saved()
                    by_name = {Path(path).name: Path(path) for path in profile_paths}
                    assert set(by_name) == {source.name for source, _ in inputs}, by_name
                    for source, payload in inputs:
                        copy = by_name[source.name]
                        assert copy.resolve().is_relative_to(managed.resolve()), copy
                        assert not copy.is_symlink() and not copy.samefile(source), copy
                        assert copy.read_bytes() == source.read_bytes() == payload
                    assert observed(3) is not None
                    print("PASS managed YAML copies; original bytes and selection unchanged", flush=True)
                finally:
                    stop(app)
        except BaseException:
            print(portal_log.read_text(encoding="utf-8"), file=sys.stderr)
            if (home / "app.log").is_file():
                print((home / "app.log").read_text(encoding="utf-8"), file=sys.stderr)
            raise
        finally:
            stop(server)


if __name__ == "__main__":
    if len(sys.argv) == 4 and sys.argv[1] == "--fixture":
        fixture(*(Path(arg).resolve(strict=True) for arg in sys.argv[2:]))
    elif len(sys.argv) == 2:
        run(Path(sys.argv[1]).resolve(strict=True))
    else:
        raise SystemExit("usage: x11-file-picker-smoke.py /absolute/path/to/trusted/meow-paw")
