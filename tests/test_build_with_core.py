"""Offline tests for the bundled engine build and publication."""
import contextlib
import importlib.util
import io
from pathlib import Path
import subprocess
import tempfile
import types
import unittest
from unittest import mock


spec = importlib.util.spec_from_file_location(
    "build_with_core", Path(__file__).resolve().parents[1] / "tools/build-with-core.py"
)
build = importlib.util.module_from_spec(spec)
spec.loader.exec_module(build)


class BuildWithCoreTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.core = self.root / "vendor/meow-rs"
        self.core.mkdir(parents=True)
        self.git(self.root, "init", "-q")
        self.git(self.core, "init", "-q")
        (self.core / "Cargo.toml").write_text("[package]\nname = 'core'\n")
        self.git(self.core, "add", "Cargo.toml")
        self.git(self.core, "commit", "-qm", "core")
        self.pin = self.git(self.core, "rev-parse", "HEAD").strip()
        self.git(self.root, "update-index", "--add", "--cacheinfo", "160000", self.pin, "vendor/meow-rs")
        self.git(self.root, "commit", "-qm", "parent")
        (self.root / "tools").mkdir()
        self.dest = self.root / "target/release"
        self.dest.mkdir(parents=True)

    @staticmethod
    def git(cwd, *args):
        return subprocess.run(
            ["git", "-c", "user.name=Test", "-c", "user.email=test@example.invalid",
             "-C", str(cwd), *args], check=True, stdout=subprocess.PIPE,
            stderr=subprocess.PIPE, text=True,
        ).stdout

    def run_main(self, platform, argv, run, replace=None):
        # Override only this module's platform view, leaving pathlib and tempfile intact.
        platform_os = types.SimpleNamespace(name=platform, replace=replace or build.os.replace,
                                            fsdecode=build.os.fsdecode)
        with mock.patch.object(build, "__file__", str(self.root / "tools/build-with-core.py")), \
             mock.patch.object(build, "os", platform_os), \
             mock.patch.object(build, "cargo", side_effect=lambda cwd, *args: run(
                 ["cargo", "build", "--locked", "--release", *args], cwd=cwd, check=True)), \
             mock.patch("sys.argv", ["build-with-core.py", *argv]), \
             contextlib.redirect_stdout(io.StringIO()) as output:
            build.main()
        return output.getvalue()

    def assert_preflight_fails(self):
        with mock.patch.object(build, "cargo") as cargo, \
             mock.patch.object(build, "__file__", str(self.root / "tools/build-with-core.py")), \
             mock.patch("sys.argv", ["build-with-core.py"]), \
             contextlib.redirect_stderr(io.StringIO()) as error:
            with self.assertRaises(SystemExit):
                build.main()
        cargo.assert_not_called()
        self.assertIn("initialized, clean checkout", error.getvalue())
        self.assertNotIn(str(self.root), error.getvalue())

    def test_clean_committed_pin_passes(self):
        build.verify_core(self.root, self.core)

    def test_wrong_pin_even_if_index_is_staged_to_match(self):
        (self.core / "Cargo.toml").write_text("changed\n")
        self.git(self.core, "add", "Cargo.toml")
        self.git(self.core, "commit", "-qm", "other")
        other = self.git(self.core, "rev-parse", "HEAD").strip()
        self.git(self.root, "update-index", "--cacheinfo", "160000", other, "vendor/meow-rs")
        self.assert_preflight_fails()

    def test_uninitialized_and_wrong_top_level(self):
        import shutil
        shutil.rmtree(self.core / ".git")
        self.assert_preflight_fails()

    def test_dirty_core(self):
        for state in ("modified", "staged", "untracked"):
            with self.subTest(state=state):
                if state == "untracked":
                    path = self.core / "new-source.rs"
                    path.write_text("source")
                else:
                    path = self.core / "Cargo.toml"
                    original = path.read_text()
                    path.write_text("modified")
                    if state == "staged":
                        self.git(self.core, "add", "Cargo.toml")
                self.assert_preflight_fails()
                if state == "staged":
                    self.git(self.core, "reset", "-q", "HEAD", "--", "Cargo.toml")
                if state == "untracked":
                    path.unlink()
                else:
                    path.write_text(original)

    def test_success_publishes_exact_core_atomically_for_platform_names(self):
        for platform, filename in (("posix", "meow"), ("nt", "meow.exe")):
            with self.subTest(platform=platform):
                custom_target = self.root / ("custom-" + platform)
                adjacent = self.dest / filename
                adjacent.write_bytes(b"old engine")
                calls = []
                replacements = []

                def cargo_run(command, *, cwd, check):
                    calls.append((command, cwd, check))
                    self.assertEqual(adjacent.read_bytes(), b"old engine")
                    if len(calls) == 1:
                        built = custom_target / "release" / filename
                        built.parent.mkdir(parents=True)
                        built.write_bytes(b"newly built core")
                    else:
                        # A tempting unrelated artifact must never be selected.
                        (self.dest / ("other-" + filename)).write_bytes(b"wrong core")

                def replace(source, destination):
                    source, destination = Path(source), Path(destination)
                    replacements.append((source, destination))
                    self.assertEqual(source.parent.parent, self.dest)
                    self.assertEqual(source.name, filename)
                    self.assertEqual(source.read_bytes(), b"newly built core")
                    self.assertEqual(destination, adjacent)
                    self.assertEqual(destination.read_bytes(), b"old engine")
                    build_os_replace(source, destination)

                build_os_replace = build.os.replace
                output = self.run_main(platform, ["--jobs", "3", "--core-target-dir", str(custom_target)], cargo_run, replace)
                self.assertEqual(calls, [
                    (["cargo", "build", "--locked", "--release", "--target-dir", str(custom_target), "-p", "meow-app", "--bin", "meow", "--jobs", "3"], self.root / "vendor/meow-rs", True),
                    (["cargo", "build", "--locked", "--release", "--target-dir", str(self.root / "target"), "--features", "desktop", "--bin", "meow-paw", "--jobs", "3"], self.root, True),
                ])
                self.assertEqual(len(replacements), 1)
                self.assertFalse(replacements[0][0].exists())
                self.assertEqual(adjacent.read_bytes(), b"newly built core")
                self.assertEqual(output.strip(), str(adjacent))

    def test_failed_build_never_publishes_engine(self):
        for platform, filename in (("posix", "meow"), ("nt", "meow.exe")):
            for failed_call in (1, 2):
                with self.subTest(platform=platform, failed_call=failed_call):
                    adjacent = self.dest / filename
                    adjacent.write_bytes(b"existing engine")
                    calls = []

                    def cargo_run(command, *, cwd, check):
                        calls.append(command)
                        if len(calls) == failed_call:
                            raise subprocess.CalledProcessError(1, command)
                        built = self.root / "target/bundled-core/release" / filename
                        built.parent.mkdir(parents=True, exist_ok=True)
                        built.write_bytes(b"new core")

                    with mock.patch.object(build.shutil, "copy2") as copy, \
                         mock.patch.object(build.os, "replace") as replace:
                        with self.assertRaises(subprocess.CalledProcessError):
                            self.run_main(platform, [], cargo_run)
                    self.assertEqual(len(calls), failed_call)
                    self.assertEqual(adjacent.read_bytes(), b"existing engine")
                    copy.assert_not_called()
                    replace.assert_not_called()
                    self.assertEqual(calls[0][0:4], ["cargo", "build", "--locked", "--release"])
                    self.assertIn(str(self.root / "target/bundled-core"), calls[0])
                    self.assertEqual(calls[0][-2:], ["--jobs", "2"])


    def test_copy_failure_keeps_existing_engine_and_cleans_temporary_directory(self):
        adjacent = self.dest / "meow"
        adjacent.write_bytes(b"existing engine")

        def cargo_run(command, *, cwd, check):
            if cwd == self.root / "vendor/meow-rs":
                built = self.root / "target/bundled-core/release/meow"
                built.parent.mkdir(parents=True)
                built.write_bytes(b"new core")

        def fail_copy(source, destination):
            Path(destination).write_bytes(b"partial")
            raise OSError("copy failed")

        with mock.patch.object(build.shutil, "copy2", side_effect=fail_copy), \
             mock.patch.object(build.os, "replace") as replace:
            with self.assertRaisesRegex(OSError, "copy failed"):
                self.run_main("posix", [], cargo_run)
        replace.assert_not_called()
        self.assertEqual(adjacent.read_bytes(), b"existing engine")
        self.assertEqual(list(self.dest.iterdir()), [adjacent])


if __name__ == "__main__":
    unittest.main()
