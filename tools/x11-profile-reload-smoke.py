#!/usr/bin/env python3
"""Apply two local YAML copies to a live Meow core through Paw's X11 Profiles UI."""
import http.client
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
import socket
import subprocess as sp
import sys
from threading import Thread
import time

from x11_profile_fixture import app_window, drop_target, guarded_home, offer_file, run_in_xvfb
from x11_smoke import child_ids, eventually, stop, wm_close

ORIGIN_BODY = b"paw-hot-reload-origin"
# The active-session cards differ from stopped cards (additional status and
# actions). These two centers are from the English 1120x738 live Profiles view.
ALPHA_APPLY = (731, 424)
BETA_APPLY = (281, 531)
# The frontend polls the engine snapshot every 300 ms. Give the card positions
# time to reflect the acknowledged state without retrying any UI click.
UI_SETTLE_SECONDS = 0.7


def config(group, route):
    return (
        "mode: rule\nlog-level: info\nallow-lan: false\nbind-address: 127.0.0.1\n"
        "dns: {enable: false}\ntun: {enable: false}\n"
        f"proxy-groups:\n  - name: {group}\n    type: select\n    proxies: [{route}]\n"
        f"rules:\n  - IP-CIDR,127.0.0.0/8,{group},no-resolve\n  - MATCH,REJECT\n"
    ).encode()


def free_port():
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        return sock.getsockname()[1]


def core_identity(app, core):
    matches = []
    for pid in child_ids(app.pid):
        proc = Path("/proc") / pid
        try:
            if (proc / "exe").samefile(core):
                # Field 22 is process start time. Use the last close-paren so
                # process names containing spaces do not change field indices.
                start_time = (proc / "stat").read_text().rpartition(") ")[2].split()[19]
                matches.append((pid, start_time))
        except FileNotFoundError:
            # An unrelated, short-lived child (e.g. a view helper) has exited.
            continue
    assert len(matches) <= 1, "expected a single managed Meow child"
    return matches[0] if matches else None


def local_request(proxy, origin, marker):
    connection = http.client.HTTPConnection("127.0.0.1", proxy, timeout=1)
    try:
        connection.request("GET", f"http://127.0.0.1:{origin}/{marker}")
        reply = connection.getresponse()
        return reply.status, reply.read()
    except (OSError, http.client.HTTPException):
        return 0, b""
    finally:
        connection.close()


def click(window, point):
    sp.run(["xdotool", "mousemove", "--window", window, *map(str, point), "click", "1"],
           check=True, timeout=5)


def import_copies(home, sender, window, point):
    preferences = home / "config" / "meow-paw" / "preferences.json"
    managed = preferences.with_name("local-profiles")

    def find_copy(name):
        try:
            registered = json.loads(preferences.read_text(encoding="utf-8")).get("profiles", [])
        except FileNotFoundError:
            return None
        candidates = [Path(path) for path in registered if Path(path).name == name]
        assert len(candidates) <= 1, (name, candidates)
        return candidates[0] if candidates else None

    copies = []
    for name, group, route, mode in (
        ("hot-alpha.yaml", "Live-Alpha", "REJECT", "uri-first"),
        ("hot-beta.yaml", "Live-Beta", "DIRECT", "plain-first"),
    ):
        payload = config(group, route)
        source = home / name
        source.write_bytes(payload)
        result = offer_file(sender, window, point, mode, source.as_uri() + "\r\n", source)
        assert "selection=uri-list" in result and "finished=1" in result, (name, result)
        copy = eventually(lambda: find_copy(name), f"saved YAML copy: {name}", seconds=5)
        assert copy.resolve().is_relative_to(managed.resolve()), copy
        assert copy.read_bytes() == payload == source.read_bytes(), name
        copies.append((copy, source, payload))
    # Files exist and preferences are persisted before clicking; allow GPUI to
    # present the committed card stack. Never retry a click on a stale row.
    return copies


def hot_reload(binary, sender, core):
    home = guarded_home()
    assert "MEOW_BIN" not in os.environ and not binary.with_name("meow").exists()
    prefs = home / "config" / "meow-paw" / "preferences.json"
    assert not prefs.exists(), "hot-reload fixture needs fresh preferences"
    initial = home / "live-initial.yaml"
    initial.write_bytes(config("Initial-Harness", "DIRECT"))
    # --profile applies to this run only. Seed a real saved choice as well
    # so the drop verifies a nonempty selection is neither cleared nor changed.
    prefs.parent.mkdir(mode=0o700)
    prefs.write_text(json.dumps({"profile": str(initial), "profiles": [str(initial)]}),
                     encoding="utf-8")
    port = free_port()
    seen = set()

    class Origin(BaseHTTPRequestHandler):
        def do_GET(self):
            seen.add(self.path)
            self.send_response(200)
            self.send_header("Content-Length", str(len(ORIGIN_BODY)))
            self.end_headers()
            self.wfile.write(ORIGIN_BODY)

        def log_message(self, *_args):
            pass

    with ThreadingHTTPServer(("127.0.0.1", 0), Origin) as origin:
        thread = Thread(target=origin.serve_forever, daemon=True)
        thread.start()
        try:
            app = sp.Popen(
                [binary, "--page", "profiles", "--locale", "en", "--no-tray",
                 "--profile", initial, "--mixed-port", str(port), "--start"],
                env={**os.environ, "MEOW_BIN": str(core)},
            )
            window = None
            try:
                window = app_window(app)
                point = drop_target(window)

                def request(marker):
                    return local_request(port, origin.server_port, marker)

                eventually(lambda: request("initial") == (200, ORIGIN_BODY),
                           "initial DIRECT response via Meow", seconds=8)
                assert "/initial" in seen
                identity = eventually(lambda: core_identity(app, core),
                                      "trusted Meow child executable", seconds=5)
                copies = import_copies(home, sender, window, point)
                assert json.loads(prefs.read_text(encoding="utf-8")).get("profile") == str(initial)
                assert request("after-import") == (200, ORIGIN_BODY)
                assert core_identity(app, core) == identity, "import alone restarted Meow"
                time.sleep(UI_SETTLE_SECONDS)

                click(window, ALPHA_APPLY)
                eventually(lambda: request("alpha-transition")[0] == 502,
                           "Apply live for Alpha did not switch to REJECT", seconds=7)
                for index in range(3):
                    marker = f"alpha-proof-{index}"
                    assert request(marker)[0] == 502, marker
                    assert f"/{marker}" not in seen, "a rejected request reached the fixture origin"
                assert core_identity(app, core) == identity, "Alpha restarted the Meow process"
                time.sleep(UI_SETTLE_SECONDS)

                click(window, BETA_APPLY)
                eventually(lambda: request("beta") == (200, ORIGIN_BODY),
                           "Apply live for Beta did not restore DIRECT", seconds=7)
                assert "/beta" in seen
                assert core_identity(app, core) == identity, "Beta restarted the Meow process"
                assert json.loads(prefs.read_text(encoding="utf-8")).get("profile") == str(initial)
                for copy, source, payload in copies:
                    assert copy.read_bytes() == payload == source.read_bytes(), source.name
                print("PASS live Apply: DIRECT → REJECT → DIRECT; same managed Meow process", flush=True)
            finally:
                try:
                    if window is not None and app.poll() is None:
                        try:
                            wm_close(int(window))
                            app.wait(timeout=5)
                        except sp.TimeoutExpired:
                            pass
                finally:
                    stop(app)
        finally:
            origin.shutdown()
            thread.join(timeout=3)


def executable(value):
    path = Path(value).resolve(strict=True)
    if not path.is_file() or not os.access(path, os.X_OK):
        raise SystemExit(f"Expected a trusted executable: {path}")
    return path


def main(args):
    if len(args) == 4 and args[0] == "--fixture":
        hot_reload(*(executable(arg) for arg in args[1:]))
    elif len(args) == 2:
        paw, core = map(executable, args)
        if paw.samefile(core):
            raise SystemExit("Paw and Meow must be distinct trusted executables")
        run_in_xvfb(Path(__file__).resolve(), "--fixture", paw, core, isolate_paw=True, timeout=80)
    else:
        raise SystemExit("usage: x11-profile-reload-smoke.py /path/to/meow-paw /path/to/meow")


if __name__ == "__main__":
    main(sys.argv[1:])
