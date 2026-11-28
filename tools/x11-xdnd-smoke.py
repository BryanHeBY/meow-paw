#!/usr/bin/env python3
"""Test real X11 file drops using an already-built, trusted Paw binary."""
import json
import os
from pathlib import Path
import re
import subprocess as sp
import sys
import time

from x11_profile_fixture import drop_target, loopback_only, run_in_xvfb
from x11_smoke import eventually, one_window, stop


def main(binary, fixture_mode="--fixture"):
    run_in_xvfb(Path(__file__).resolve(), fixture_mode, binary, isolate_paw=True)


def fixture(binary, sender):
    loopback_only()
    assert re.fullmatch(r":\d+", os.environ["DISPLAY"])
    payload = b"mode: direct\nlog-level: info\n"
    root = Path(os.environ["XDG_CONFIG_HOME"]) / "meow-paw" / "local-profiles"
    modes = ("uri-first", "plain-first", "property-first", "text-only", "invalid-uri")
    app = sp.Popen([binary, "--page", "profiles", "--locale", "en", "--no-tray"],
                   stdout=sp.DEVNULL)
    def files():
        return sorted(path.name for path in root.glob("*/*.yaml"))
    try:
        window = str(eventually(lambda: one_window(app), "isolated Paw window"))
        pid = sp.check_output(["xdotool", "getwindowpid", window], text=True)
        assert int(pid) == app.pid, (pid, app.pid)
        drop_x, drop_y = drop_target(window)
        expected = []
        for i, mode in enumerate(modes):
            source = Path(os.environ["HOME"]) / f"{mode}.yaml"
            source.write_bytes(payload)
            uri = "https://invalid.example/file.yaml" if mode == "invalid-uri" else source.as_uri() + "\r\n"
            result = sp.check_output(
                [sender, window, drop_x, drop_y, mode, uri, source],
                env={**os.environ, "PAW_TEST_XVFB": os.environ["DISPLAY"]}, text=True, timeout=8,
            )
            selection = "plain" if mode == "text-only" else "uri-list"
            assert f"selection={selection}" in result and "finished=1" in result, (mode, result)
            assert source.read_bytes() == payload, mode
            if i < 3:
                expected.append(source.name)
                eventually(lambda: files() == sorted(expected), f"managed copy: {mode}", seconds=5)
                assert next(root.glob(f"*/{source.name}")).read_bytes() == payload, mode
            else:
                # Re-check through an event-loop settling window to catch a deferred file import.
                deadline = time.monotonic() + 1
                while time.monotonic() < deadline:
                    assert files() == sorted(expected), (mode, files())
                    time.sleep(0.05)
            print(f"PASS {mode}")
    finally:
        stop(app)


def switch_fixture(binary, sender):
    """Exercise the real card selection UI; no controller or core is started."""
    loopback_only()
    assert re.fullmatch(r":\d+", os.environ["DISPLAY"])
    home = Path(os.environ["HOME"])
    config_home = Path(os.environ["XDG_CONFIG_HOME"])
    assert config_home == home / "config" and home.name.startswith("paw-xdnd-"), home
    assert "MEOW_BIN" not in os.environ and not binary.with_name("meow").exists()
    preferences = config_home / "meow-paw" / "preferences.json"
    assert not preferences.exists(), "selection fixture needs fresh preferences"
    managed = preferences.with_name("local-profiles")
    # The content differs as well as the name: one saved path must never be
    # mistaken for the other even if the native file drop order regresses.
    inputs = (
        ("fixture-alpha.yaml", b"mode: direct\nlog-level: info\n", "uri-first", (305, 390)),
        ("fixture-beta.yaml", b"mode: rule\nlog-level: warning\n", "plain-first", (752, 390)),
    )
    app = sp.Popen([binary, "--page", "profiles", "--locale", "en", "--no-tray"],
                   stdout=sp.DEVNULL)

    def saved():
        try:
            return json.loads(preferences.read_text(encoding="utf-8"))
        except FileNotFoundError:
            return {}

    try:
        window = str(eventually(lambda: one_window(app), "isolated Paw window"))
        assert int(sp.check_output(["xdotool", "getwindowpid", window], timeout=5)) == app.pid
        drop_x, drop_y = drop_target(window)
        registered = []
        for index, (name, payload, offer, _) in enumerate(inputs, 1):
            source = home / name
            source.write_bytes(payload)
            result = sp.check_output(
                [sender, window, drop_x, drop_y, offer, source.as_uri() + "\r\n", source],
                env={**os.environ, "PAW_TEST_XVFB": os.environ["DISPLAY"]}, text=True, timeout=8,
            )
            assert "selection=uri-list" in result and "finished=1" in result, (name, result)
            eventually(lambda: len(saved().get("profiles", [])) == index,
                       f"persistently registered {name}", seconds=5)
            path = Path(saved()["profiles"][-1])
            assert path.name == name and path.resolve().is_relative_to(managed.resolve()), path
            assert path.read_bytes() == payload == source.read_bytes(), name
            registered.append(str(path))
        assert saved().get("profile") is None, "importing YAML copies must not select one"

        # Real English 1120×738 Paw screenshot: two cards share y~327..420;
        # their "Use on next start" controls are at x233..377 and x681..825.
        # Don't silently click around if a future layout moves either control.
        for expected, (name, payload, _, (x, y)) in zip(registered, inputs, strict=True):
            sp.run(["xdotool", "mousemove", "--window", window, str(x), str(y), "click", "1"],
                   check=True, timeout=5)
            eventually(lambda: saved().get("profile") == expected,
                       f"select {name} at {x},{y}; prefer updating the native fixture over retries",
                       seconds=5)
            assert saved()["profiles"] == registered and saved().get("engine") is None, saved()
            assert Path(expected).read_bytes() == payload == (home / name).read_bytes()
            print(f"PASS {name}: chosen managed copy saved for next start")
    finally:
        stop(app)


if __name__ == "__main__":
    if len(sys.argv) == 4 and sys.argv[1] in ("--fixture", "--switch-fixture"):
        run = fixture if sys.argv[1] == "--fixture" else switch_fixture
        run(Path(sys.argv[2]).resolve(strict=True), Path(sys.argv[3]).resolve(strict=True))
    elif len(sys.argv) == 3 and sys.argv[1] == "--switch-profiles":
        main(Path(sys.argv[2]).resolve(strict=True), "--switch-fixture")
    elif len(sys.argv) == 2:
        main(Path(sys.argv[1]).resolve(strict=True))
    else:
        raise SystemExit("usage: x11-xdnd-smoke.py [--switch-profiles] /absolute/path/to/meow-paw")
