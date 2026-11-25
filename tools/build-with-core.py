#!/usr/bin/env python3
"""Build Paw and its pinned engine and put them in the same directory."""
import argparse
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

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
    if not (core / "Cargo.toml").is_file():
        ap.error("initialize vendor/meow-rs: git submodule update --init --recursive")
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
