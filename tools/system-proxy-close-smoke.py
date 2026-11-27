#!/usr/bin/env python3
"""Drive real WM_DELETE_WINDOW in a private 1300x1024 Xvfb and fake desktop."""
import json
import os
from pathlib import Path
import socket
import subprocess as sp
import tempfile
import time

from x11_smoke import eventually, one_window, running_ports, stop, wm_close


def fixture(binary: Path, meow: Path):
    with tempfile.TemporaryDirectory(prefix="paw-proxy-native-close-") as directory:
        root = Path(directory)
        bin_dir = root / "bin"
        bin_dir.mkdir()
        fake = Path(__file__).resolve().parent.parent / "tests/fixtures/fake_gsettings.py"
        for name in ("gsettings", "dconf"):
            command = bin_dir / name
            command.write_bytes(fake.read_bytes())
            command.chmod(0o700)
        state = root / "settings.json"
        state.write_text("{}")
        profile = root / "profile.yaml"
        profile.write_text("mode: rule\nipv6: false\ndns: {enable: false}\nrules: ['MATCH,DIRECT']\n")
        config = root / "config"
        config.mkdir()
        journal = config / "meow-paw/system-proxy/recovery.json"
        with socket.socket() as listener:
            listener.bind(("127.0.0.1", 0))
            port = listener.getsockname()[1]
        env = os.environ.copy()
        for name in ("GSETTINGS_BACKEND", "DCONF_PROFILE", "WAYLAND_DISPLAY"):
            env.pop(name, None)
        env.update(
            PATH=f"{bin_dir}:/usr/bin:/bin",
            MEOW_BIN=str(meow),
            XDG_CONFIG_HOME=str(config),
            XDG_CACHE_HOME=str(root / "cache"),
            XDG_DATA_HOME=str(root / "data"),
            XDG_CURRENT_DESKTOP="GNOME",
            DBUS_SESSION_BUS_ADDRESS="unix:path=/not-a-real-desktop-bus",
            PAW_TEST_GSETTINGS_STATE=str(state),
        )
        app = sp.Popen(
            [binary, "--locale", "en", "--page", "settings", "--no-tray",
             "--mixed-port", str(port), "--profile", profile, "--start"],
            env=env,
        )

        def read():
            # A subprocess may be between truncate and write.
            try:
                return json.loads(state.read_text())
            except json.JSONDecodeError:
                return {}

        try:
            win = eventually(lambda: one_window(app), "visible Settings window")
            ports = eventually(lambda: running_ports(app), "owned Meow listeners")
            assert read() == {} and not journal.exists(), "startup changed desktop proxy"
            # Fixed viewport/locale for this visual smoke fixture. Scroll
            # Settings to its bottom, then explicitly click Enable.
            sp.run(["xdotool", "mousemove", "--window", str(win), "555", "673",
                    "click", "--repeat", "8", "5"], check=True)
            time.sleep(.8)
            sp.run(["xdotool", "mousemove", "--window", str(win), "430", "692",
                    "click", "1"], check=True)
            eventually(lambda: read().get("mode") == "'manual'", "explicit enable", 18)
            expected = read()
            assert expected["http/port"] == expected["https/port"] == str(port)
            assert journal.is_file()
            time.sleep(.8)

            # External edit only affects HTTP; HTTPS still needs live Paw.
            changed = {**expected, "http/port": "3128"}
            state.write_text(json.dumps(changed))
            wm_close(win)
            time.sleep(2)
            assert app.poll() is None, "native close abandoned the live proxy"
            assert one_window(app) == win, "recovery controls disappeared"
            assert running_ports(app) == ports, "owned listener did not survive conflict"
            assert read() == changed and journal.is_file(), "native close overwrote foreign state"

            # Put the expected state back in the fake desktop, then repeat
            # the actual native close. This time the process must clean up.
            state.write_text(json.dumps(expected))
            wm_close(win)
            assert app.wait(20) == 0, app.returncode
            assert read() == {} and not journal.exists(), "clean native close did not restore"
            print("WM_DELETE_WINDOW: conflict retained window & core; exact retry restored settings.")
        finally:
            stop(app)


if __name__ == "__main__":
    fixture(Path("target/debug/meow-paw").resolve(strict=True),
            Path(os.environ["MEOW_BIN"]).resolve(strict=True))
