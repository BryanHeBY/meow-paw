#!/usr/bin/env python3
"""Build Paw and its pinned engine and put them in the same directory."""
import argparse
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

def verify_core(root, core):
    """Verify the committed gitlink and the checked-out core without changing either."""
    def git(directory, *args):
        return subprocess.run(
            ["git", "--no-optional-locks", "-C", str(directory), *args], check=True,
            stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
        ).stdout

    try:
        if Path(os.fsdecode(git(root, "rev-parse", "--show-toplevel").strip())).resolve() != root.resolve():
            raise ValueError
        entry = git(root, "ls-tree", "-z", "HEAD", "--", "vendor/meow-rs")
        metadata, path = entry.rstrip(b"\0").split(b"\t", 1)
        mode, kind, pin = metadata.split()
        if mode != b"160000" or kind != b"commit" or path != b"vendor/meow-rs":
            raise ValueError
        if Path(os.fsdecode(git(core, "rev-parse", "--show-toplevel").strip())).resolve() != core.resolve():
            raise ValueError
        if git(core, "rev-parse", "--verify", "HEAD").strip() != pin:
            raise ValueError
        if not (core / "Cargo.toml").is_file():
            raise ValueError
        if git(core, "status", "--porcelain=v1", "--untracked-files=all", "--ignore-submodules=none"):
            raise ValueError
    except (OSError, subprocess.CalledProcessError, ValueError):
        raise ValueError("vendor/meow-rs must be an initialized, clean checkout of the committed core gitlink") from None

def cargo(cwd, *args):
    subprocess.run(["cargo", "build", "--locked", "--release", *args], cwd=cwd, check=True)

def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--jobs", type=int, default=2)
    ap.add_argument("--core-target-dir", type=Path)
    args = ap.parse_args()
    if args.jobs < 1:
        ap.error("--jobs must be positive")
    root = Path(__file__).resolve().parents[1]
    core = root / "vendor" / "meow-rs"
    try:
        verify_core(root, core)
    except ValueError as error:
        ap.error(str(error))
    dest = root / "target" / "release"
    engine_target = (args.core_target_dir or root / "target" / "bundled-core").resolve()
    jobs = ("--jobs", str(args.jobs))
    cargo(core, "--target-dir", str(engine_target), "-p", "meow-app", "--bin", "meow", *jobs)
    cargo(root, "--target-dir", str(dest.parent), "--features", "desktop", "--bin", "meow-paw", *jobs)
    filename = "meow.exe" if os.name == "nt" else "meow"
    built = engine_target / "release" / filename
    with tempfile.TemporaryDirectory(dir=dest) as temp:
        pending = Path(temp) / filename
        shutil.copy2(built, pending)
        os.replace(pending, dest / filename)
    print(dest / filename)

if __name__ == "__main__":
    main()
