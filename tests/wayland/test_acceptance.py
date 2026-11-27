"""Offline tests for the isolated Wayland acceptance harness.

These tests exercise fixture and probe logic only; they do not establish that
Wayland, Fcitx5, or Paw work together on a compositor.
"""
import contextlib
import importlib.util
import io
import os
from pathlib import Path
import tempfile
import types
import unittest
from unittest import mock


SCRIPT = Path(__file__).resolve().parents[2] / "tools/wayland/acceptance.py"
spec = importlib.util.spec_from_file_location("wayland_acceptance", SCRIPT)
acceptance = importlib.util.module_from_spec(spec)
spec.loader.exec_module(acceptance)


class DependencyTests(unittest.TestCase):
    def test_all_missing_dependencies_are_reported(self):
        missing = acceptance.dependency_diagnostics(which=lambda name: None)
        for name in ("sway", "swaymsg", "fcitx5", "fcitx5-remote", "dbus-run-session", "dbus-daemon", "busctl", "unshare"):
            with self.subTest(name=name):
                self.assertTrue(any(name in entry for entry in missing), missing)

    def test_present_dependencies_do_not_generate_missing_diagnostics(self):
        self.assertEqual(acceptance.dependency_diagnostics(which=lambda name: "/fixture/" + name), [])

    def test_one_missing_dependency_is_identified(self):
        missing = acceptance.dependency_diagnostics(
            which=lambda name: None if name == "fcitx5-remote" else "/fixture/" + name
        )
        self.assertEqual(len(missing), 1)
        self.assertIn("fcitx5-remote", missing[0])

    def test_cli_check_reports_missing_tools_without_launching_a_session(self):
        error = io.StringIO()
        with mock.patch("sys.argv", [str(SCRIPT), "--check"]), \
             mock.patch.object(acceptance.shutil, "which", return_value=None) as which, \
             mock.patch.object(acceptance.subprocess, "Popen") as popen, \
             contextlib.redirect_stderr(error):
            self.assertEqual(acceptance.main(), 2)
        self.assertIn("sway", error.getvalue())
        self.assertIn("fcitx5", error.getvalue())
        self.assertTrue(all(call.kwargs.get("path") == "/usr/bin:/bin" for call in which.call_args_list))
        popen.assert_not_called()


class IsolationTests(unittest.TestCase):
    def test_synthetic_profile_is_local_and_network_free(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp) / "fixture"
            root.mkdir()
            profile = acceptance.synthetic_config(root)
            self.assertTrue(profile.is_file())
            self.assertTrue(profile.resolve().is_relative_to(root.resolve()))
            text = profile.read_text(encoding="utf-8")
            self.assertIn("MATCH,DIRECT", text)
            self.assertIn("enable: false", text)
            for forbidden in ("proxy-providers", "proxies:", "external-controller", "secret:", "tun:"):
                self.assertNotIn(forbidden, text)

    def test_existing_home_is_rejected_without_modification(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            home = root / "home"
            home.mkdir()
            sentinel = home / "sentinel"
            sentinel.write_text("untouched", encoding="utf-8")
            with self.assertRaises(FileExistsError):
                acceptance.synthetic_config(root)
            self.assertEqual(sentinel.read_text(encoding="utf-8"), "untouched")
            self.assertFalse((root / "profile.yaml").exists())

    def test_host_session_and_engine_environment_are_not_inherited(self):
        hostile = {
            "HOME": "/host/home", "XDG_CONFIG_HOME": "/host/config",
            "XDG_CACHE_HOME": "/host/cache", "XDG_DATA_HOME": "/host/data",
            "XDG_STATE_HOME": "/host/state", "XDG_CONFIG_DIRS": "/host/system",
            "XDG_RUNTIME_DIR": "/host/runtime",
            "DISPLAY": ":77", "WAYLAND_DISPLAY": "host-wayland",
            "DBUS_SESSION_BUS_ADDRESS": "unix:path=/host/bus", "MEOW_BIN": "/host/meow",
            "HTTP_PROXY": "http://host.invalid:1234", "http_proxy": "http://host.invalid:1234",
        }
        with tempfile.TemporaryDirectory() as tmp, mock.patch.dict(os.environ, hostile):
            root = Path(tmp) / "fixture"
            root.mkdir()
            acceptance.synthetic_config(root)
            env = acceptance.environment(root)
            for key in ("HOME", "XDG_CONFIG_HOME", "XDG_CACHE_HOME", "XDG_DATA_HOME", "XDG_STATE_HOME", "XDG_CONFIG_DIRS", "XDG_RUNTIME_DIR"):
                with self.subTest(key=key):
                    self.assertTrue(Path(env[key]).resolve().is_relative_to(root.resolve()), env)
            for key in ("DISPLAY", "WAYLAND_DISPLAY", "DBUS_SESSION_BUS_ADDRESS", "MEOW_BIN", "HTTP_PROXY", "http_proxy"):
                with self.subTest(key=key):
                    self.assertNotIn(key, env)
            self.assertEqual(os.environ["HOME"], "/host/home")

    def test_inner_rejects_same_namespace_before_spawning(self):
        with tempfile.TemporaryDirectory() as tmp, \
             open("/proc/self/ns/user", "rb") as user_ns, \
             open("/proc/self/ns/net", "rb") as net_ns, \
             mock.patch.object(acceptance, "outer_is_ancestor", return_value=True), \
             mock.patch.object(acceptance.subprocess, "Popen") as popen:
            with self.assertRaisesRegex(RuntimeError, "isolated user/network namespace"):
                acceptance.inner(Path(tmp), Path("paw"), Path("watcher"), 123,
                                 user_ns.fileno(), net_ns.fileno())
            popen.assert_not_called()
            self.assertFalse((Path(tmp) / "sway.conf").exists())

    def test_inner_rejects_missing_namespace_descriptors_before_spawning(self):
        with tempfile.TemporaryDirectory() as tmp, \
             mock.patch.object(acceptance.subprocess, "Popen") as popen:
            with self.assertRaisesRegex(RuntimeError, "namespace descriptors unavailable"):
                acceptance.inner(Path(tmp), Path("paw"), Path("watcher"), 123, -1, -1)
            popen.assert_not_called()
            self.assertFalse((Path(tmp) / "sway.conf").exists())

    def test_inner_rejects_host_bus_before_spawning(self):
        with tempfile.TemporaryDirectory() as tmp, \
             mock.patch.object(acceptance, "outer_is_ancestor", return_value=True), \
             mock.patch.object(acceptance.os, "fstat", return_value=types.SimpleNamespace(st_ino=-1)), \
             mock.patch.dict(os.environ, {"DBUS_SESSION_BUS_ADDRESS": "unix:path=/host/bus"}), \
             mock.patch.object(acceptance.subprocess, "Popen") as popen:
            with self.assertRaisesRegex(RuntimeError, "private runtime"):
                acceptance.inner(Path(tmp), Path("paw"), Path("watcher"), 123, 100, 101)
            popen.assert_not_called()
            self.assertFalse((Path(tmp) / "sway.conf").exists())


class ProbeTests(unittest.TestCase):
    def test_sni_registration_requires_exact_service(self):
        service = "org.kde.StatusNotifierItem-123-1"
        self.assertTrue(acceptance.sni_registered('as 2 "org.other.Item" "' + service + '"', service))
        for output in ('as 0', 'as 1 "org.kde.StatusNotifierItem-123-10"',
                       'as 1 "org.kde.StatusNotifierItem-123-1.extra"',
                       'as 0 "' + service + '"', 'as 1 "' + service, ''):
            with self.subTest(output=output):
                self.assertFalse(acceptance.sni_registered(output, service))

    def test_focus_must_belong_to_the_target_process(self):
        def tree(views):
            return {"nodes": views, "floating_nodes": []}
        target = {"type": "con", "pid": 123, "focused": True, "nodes": [], "floating_nodes": []}
        self.assertTrue(acceptance.focused_paw(tree([target]), 123))
        for views in ([], [{**target, "focused": False}],
                      [{**target, "pid": 456}],
                      [{**target, "focused": False}, {**target, "pid": 456}]):
            with self.subTest(views=views):
                self.assertFalse(acceptance.focused_paw(tree(views), 123))

    def test_duplicate_target_views_are_not_a_successful_focus_probe(self):
        first = {"type": "con", "pid": 123, "focused": True, "nodes": [], "floating_nodes": []}
        second = {"type": "con", "pid": 123, "focused": False, "nodes": [], "floating_nodes": []}
        tree = {"nodes": [first, second], "floating_nodes": []}
        self.assertEqual(len(acceptance.paw_views(tree, 123)), 2)
        self.assertFalse(acceptance.focused_paw(tree, 123))

    def test_focus_probe_traverses_floating_nodes(self):
        root = {"nodes": [], "floating_nodes": [{"type": "floating_con", "pid": 123, "focused": True,
                                                  "nodes": [], "floating_nodes": []}]}
        self.assertTrue(acceptance.focused_paw(root, 123))


if __name__ == "__main__":
    unittest.main()
