"""Offline contract checks for the visual capture wrapper; no GUI is launched."""
import importlib.util
import os
from pathlib import Path
import struct
import subprocess
import tempfile
import unittest
from unittest import mock

spec = importlib.util.spec_from_file_location("capture_fixture", Path(__file__).resolve().parents[1] / "tools/capture-zh-fixture.py")
fixture = importlib.util.module_from_spec(spec)
spec.loader.exec_module(fixture)
PNG = fixture.PNG_SIGNATURE + b"\x00\x00\x00\rIHDR" + struct.pack(">II", 1120, 738)


class CaptureTest(unittest.TestCase):
    def test_isolated_arguments_and_publication(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            binary = root / "paw.exe"
            binary.write_bytes(b"fixture")
            calls = []

            def run(args, **kwargs):
                calls.append(args)
                self.assertNotEqual(Path(args[0]).parent, binary.parent)
                self.assertEqual(Path(args[0]).read_bytes(), b"fixture")
                self.assertNotIn("MEOW_BIN", kwargs["env"])
                self.assertEqual(kwargs["env"]["LOCALAPPDATA"], str(Path(args[0]).parent / "home"))
                self.assertNotIn("--start", args)
                self.assertNotIn("--profile", args)
                self.assertEqual(args[1:5], ["--no-preferences", "--no-tray", "--locale", "zh-CN"])
                Path(args[-1]).write_bytes(PNG)

            with mock.patch.dict(os.environ, {"MEOW_BIN": "must-not-pass"}), mock.patch.object(fixture.subprocess, "run", side_effect=run):
                fixture.capture(binary, root / "evidence")
            self.assertEqual([call[6] for call in calls], list(fixture.PAGES))
            self.assertEqual(sorted(p.name for p in (root / "evidence").iterdir()), ["overview.png", "proxies.png", "settings.png"])
            with self.assertRaises(ValueError):
                fixture.capture(binary, root / "evidence")

    def test_failure_does_not_publish(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            binary = root / "paw"
            binary.touch()
            with mock.patch.object(fixture.subprocess, "run", side_effect=subprocess.TimeoutExpired("paw", 30)):
                with self.assertRaises(subprocess.TimeoutExpired):
                    fixture.capture(binary, root / "evidence")
            self.assertFalse((root / "evidence").exists())
            with mock.patch.object(fixture.subprocess, "run", side_effect=lambda args, **kw: Path(args[-1]).write_bytes(b"bad")):
                with self.assertRaises(ValueError):
                    fixture.capture(binary, root / "evidence")
            self.assertFalse((root / "evidence").exists())


if __name__ == "__main__":
    unittest.main()
