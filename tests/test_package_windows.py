"""Offline packaging tests using only synthetic repositories and executable bytes."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import stat
import subprocess
import sys
import tempfile
import unittest
import zipfile


TOOL = Path(__file__).resolve().parents[1] / "tools" / "package-windows.py"


class WindowsPackageTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.base = Path(self.temp.name)
        self.source = self.base / "source"
        self.core = self.source / "vendor" / "meow-rs"
        self.core.mkdir(parents=True)
        self.binaries = self.base / "binaries"
        self.binaries.mkdir()
        self.output = self.base / "paw.zip"
        self.env = os.environ.copy()
        self.home = self.base / "home"
        self.home.mkdir()
        self.env.update({"HOME": str(self.home), "USERPROFILE": str(self.home),
                         "GIT_CONFIG_NOSYSTEM": "1", "GIT_CONFIG_GLOBAL": os.devnull})
        # Trust only fixture repositories, without touching global Git configuration.
        count = int(self.env.get("GIT_CONFIG_COUNT", "0"))
        self.env["GIT_CONFIG_COUNT"] = str(count + 2)
        for index, directory in enumerate((self.source, self.core), count):
            self.env[f"GIT_CONFIG_KEY_{index}"] = "safe.directory"
            self.env[f"GIT_CONFIG_VALUE_{index}"] = directory.resolve().as_posix()
        self.git(self.source, "init", "-q")
        self.git(self.core, "init", "-q")
        (self.core / "Cargo.toml").write_text("[package]\nname = 'fixture-core'\n", encoding="utf-8")
        self.git(self.core, "add", "Cargo.toml")
        self.git(self.core, "commit", "-qm", "core fixture")
        self.core_commit = self.git(self.core, "rev-parse", "HEAD").strip()
        (self.source / "README.md").write_text("fixture source\n", encoding="utf-8")
        self.git(self.source, "add", "README.md")
        self.git(self.source, "update-index", "--add", "--cacheinfo", "160000", self.core_commit, "vendor/meow-rs")
        self.git(self.source, "commit", "-qm", "paw fixture")
        self.paw_commit = self.git(self.source, "rev-parse", "HEAD").strip()
        self.payloads = {"meow-paw.exe": b"synthetic paw executable\n", "meow.exe": b"synthetic core executable\n"}
        for name, data in self.payloads.items():
            (self.binaries / name).write_bytes(data)

    def git(self, cwd, *args):
        return subprocess.run(
            ["git", "-c", "user.name=Test", "-c", "user.email=test@example.invalid", "-C", str(cwd), *args],
            check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, env=self.env,
        ).stdout

    def run_tool(self, *args):
        return subprocess.run([sys.executable, str(TOOL), *map(str, args)],
                              stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, env=self.env)

    def create(self, output=None):
        return self.run_tool("create", "--source-root", self.source, "--binary-dir", self.binaries,
                             "--output", output or self.output)

    def test_exact_reproducible_package_with_provenance_and_no_secrets(self):
        secret = "fixture-secret-do-not-ship"
        (self.binaries / "preferences.json").write_text(secret, encoding="utf-8")
        self.env["PAW_FIXTURE_SECRET"] = secret
        first = self.create()
        self.assertEqual(first.returncode, 0, first.stderr)
        second_path = self.base / "second.zip"
        second = self.create(second_path)
        self.assertEqual(second.returncode, 0, second.stderr)
        self.assertEqual(self.output.read_bytes(), second_path.read_bytes())
        with zipfile.ZipFile(self.output) as archive:
            self.assertEqual(set(archive.namelist()), {"manifest.json", *self.payloads})
            self.assertEqual(len(archive.namelist()), 3)
            manifest_bytes = archive.read("manifest.json")
            self.assertNotIn(secret.encode(), manifest_bytes)
            self.assertNotIn(str(self.base).encode(), manifest_bytes)
            manifest = json.loads(manifest_bytes)
            self.assertEqual(manifest["format"], "paw-windows-portable-v1")
            self.assertEqual(manifest["provenance"], {
                "paw_commit": self.paw_commit,
                "core_commit": self.core_commit,
                "binary_origin": "not_attested",
            })
            self.assertEqual(set(manifest["files"]), set(self.payloads))
            for name, data in self.payloads.items():
                self.assertEqual(archive.read(name), data)
                self.assertEqual(manifest["files"][name], {
                    "size": len(data), "sha256": hashlib.sha256(data).hexdigest(),
                })
        before = sorted(str(path.relative_to(self.base)) for path in self.base.rglob("*"))
        verified = self.run_tool("verify", self.output)
        self.assertEqual(verified.returncode, 0, verified.stderr)
        self.assertEqual(before, sorted(str(path.relative_to(self.base)) for path in self.base.rglob("*")))

    def test_verify_rejects_tampering_and_unexpected_archive_contents(self):
        created = self.create()
        self.assertEqual(created.returncode, 0, created.stderr)
        with zipfile.ZipFile(self.output) as archive:
            entries = {name: archive.read(name) for name in archive.namelist()}
        manifest = json.loads(entries["manifest.json"])
        manifest["files"]["meow.exe"]["sha256"] = "0" * 64
        cases = {
            "modified binary": [(name, b"tampered" if name == "meow.exe" else data) for name, data in entries.items()],
            "wrong hash": [(name, json.dumps(manifest).encode() if name == "manifest.json" else data) for name, data in entries.items()],
            "extra file": [*entries.items(), ("preferences.json", b"secret")],
            "traversal": [*entries.items(), ("../escape", b"secret")],
            "missing executable": [(name, data) for name, data in entries.items() if name != "meow.exe"],
            "duplicate executable": [*entries.items(), ("meow.exe", b"second")],
            "bad manifest": [(name, b"not json" if name == "manifest.json" else data) for name, data in entries.items()],
            "duplicate manifest key": [(name, b'{"format":1,"format":2}' if name == "manifest.json" else data) for name, data in entries.items()],
        }
        for label, modify in (
            ("extra manifest field", lambda value: value.update({"private_path": "secret"})),
            ("invalid commit", lambda value: value["provenance"].update({"core_commit": "123"})),
            ("boolean size", lambda value: value["files"]["meow.exe"].update({"size": True})),
        ):
            value = json.loads(entries["manifest.json"])
            modify(value)
            cases[label] = [(name, json.dumps(value).encode() if name == "manifest.json" else data)
                            for name, data in entries.items()]
        for label, members in cases.items():
            with self.subTest(label=label):
                path = self.base / (label.replace(" ", "-") + ".zip")
                # Duplicate names are intentionally used to model hostile archives.
                import warnings
                with warnings.catch_warnings():
                    warnings.simplefilter("ignore", UserWarning)
                    with zipfile.ZipFile(path, "w") as archive:
                        for name, data in members:
                            archive.writestr(name, data)
                result = self.run_tool("verify", path)
                self.assertNotEqual(result.returncode, 0, label)
                self.assertFalse((self.base.parent / "escape").exists())
        broken = self.base / "truncated.zip"
        broken.write_bytes(self.output.read_bytes()[:20])
        self.assertNotEqual(self.run_tool("verify", broken).returncode, 0)
        link_archive = self.base / "link.zip"
        with zipfile.ZipFile(link_archive, "w") as archive:
            for name, data in entries.items():
                info = zipfile.ZipInfo(name)
                if name == "meow.exe":
                    info.create_system = 3
                    info.external_attr = (stat.S_IFLNK | 0o777) << 16
                archive.writestr(info, data)
        self.assertNotEqual(self.run_tool("verify", link_archive).returncode, 0)
        compressed = self.base / "compressed.zip"
        with zipfile.ZipFile(compressed, "w", compression=zipfile.ZIP_DEFLATED) as archive:
            for name, data in entries.items():
                archive.writestr(name, data)
        self.assertNotEqual(self.run_tool("verify", compressed).returncode, 0)

    def test_create_does_not_write_to_source_tool_or_home_directories(self):
        tool_dir = self.base / "tools"
        tool_dir.mkdir()
        for name in ("package-windows.py", "build-with-core.py"):
            shutil.copyfile(TOOL.parent / name, tool_dir / name)

        def snapshot(directory):
            return {str(path.relative_to(directory)): path.read_bytes() if path.is_file() else None
                    for path in directory.rglob("*") if ".git" not in path.parts}

        before = {directory: snapshot(directory) for directory in (self.source, tool_dir, self.home)}
        # Invoke without -B so the script itself must avoid writing bytecode.
        env = self.env.copy()
        env.pop("PYTHONDONTWRITEBYTECODE", None)
        result = subprocess.run(
            [sys.executable, str(tool_dir / TOOL.name), "create", "--source-root", str(self.source),
             "--binary-dir", str(self.binaries), "--output", str(self.output)],
            stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, env=env,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(before, {directory: snapshot(directory) for directory in before})

    def test_dirty_paw_and_core_fail_before_creating_output(self):
        paw_file = self.source / "README.md"
        core_file = self.core / "Cargo.toml"
        for path, staged in ((paw_file, False), (paw_file, True), (core_file, False), (core_file, True)):
            with self.subTest(path=path.name, staged=staged):
                old = path.read_bytes()
                path.write_bytes(old + b"dirty\n")
                repo = self.core if path == core_file else self.source
                if staged:
                    self.git(repo, "add", path.name)
                try:
                    self.assertNotEqual(self.create().returncode, 0)
                    self.assertFalse(self.output.exists())
                finally:
                    if staged:
                        self.git(repo, "reset", "-q", "HEAD", "--", path.name)
                    path.write_bytes(old)
        for directory in (self.source, self.core):
            with self.subTest(directory=directory):
                untracked = directory / "untracked-fixture"
                untracked.write_text("fixture", encoding="utf-8")
                try:
                    self.assertNotEqual(self.create().returncode, 0)
                    self.assertFalse(self.output.exists())
                finally:
                    untracked.unlink()

    def test_wrong_or_uninitialized_core_fails(self):
        (self.core / "Cargo.toml").write_text("changed\n", encoding="utf-8")
        self.git(self.core, "add", "Cargo.toml")
        self.git(self.core, "commit", "-qm", "different core")
        self.assertNotEqual(self.create().returncode, 0)
        self.assertFalse(self.output.exists())
        self.git(self.core, "checkout", "-q", self.core_commit)
        metadata = self.core / ".git"
        hidden = self.core / ".git-fixture-hidden"
        metadata.rename(hidden)
        try:
            self.assertNotEqual(self.create().returncode, 0)
            self.assertFalse(self.output.exists())
        finally:
            hidden.rename(metadata)

    def test_missing_or_empty_input_and_existing_output_fail_safely(self):
        input_file = self.binaries / "meow.exe"
        input_file.unlink()
        self.assertNotEqual(self.create().returncode, 0)
        self.assertFalse(self.output.exists())
        input_file.write_bytes(b"")
        self.assertNotEqual(self.create().returncode, 0)
        self.assertFalse(self.output.exists())
        input_file.write_bytes(self.payloads["meow.exe"])
        self.output.write_bytes(b"existing package")
        self.assertNotEqual(self.create().returncode, 0)
        self.assertEqual(self.output.read_bytes(), b"existing package")

    def test_write_failure_removes_partial_output(self):
        script = (
            "import runpy, sys; from unittest.mock import patch; "
            "sys.argv = sys.argv[1:]; "
            "patcher = patch('zipfile.ZipFile.writestr', side_effect=OSError('fixture failure')); "
            "patcher.start(); runpy.run_path(sys.argv[0], run_name='__main__')"
        )
        result = subprocess.run(
            [sys.executable, "-c", script, str(TOOL), "create", "--source-root", str(self.source),
             "--binary-dir", str(self.binaries), "--output", str(self.output)],
            stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, env=self.env,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse(self.output.exists())

    def test_symbolic_link_input_is_rejected_when_supported(self):
        input_file = self.binaries / "meow.exe"
        input_file.unlink()
        target = self.base / "other.exe"
        target.write_bytes(self.payloads["meow.exe"])
        try:
            input_file.symlink_to(target)
        except (OSError, NotImplementedError):
            self.skipTest("host does not permit symbolic link creation")
        self.assertNotEqual(self.create().returncode, 0)
        self.assertFalse(self.output.exists())


if __name__ == "__main__":
    unittest.main()
