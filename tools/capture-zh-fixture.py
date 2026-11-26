#!/usr/bin/env python3
"""Capture Paw's own window in three isolated, stopped synthetic states."""
import argparse
import os
from pathlib import Path
import shutil
import struct
import subprocess
import tempfile

PAGES = ("overview", "proxies", "settings")
PNG_SIGNATURE = b"\x89PNG\r\n\x1a\n"


def check_png(path):
    # Check the header and dimensions; this is not a visual comparison.
    with path.open("rb") as image:
        header = image.read(24)
    if (len(header) != 24 or header[:8] != PNG_SIGNATURE or
            header[8:16] != b"\x00\x00\x00\rIHDR" or
            not all(struct.unpack(">II", header[16:24]))):
        raise ValueError("capture is not a PNG with nonzero dimensions")


def capture(binary, output):
    binary = binary.resolve(strict=True)
    if not binary.is_file():
        raise ValueError("binary must be a file")
    if output.exists():
        raise ValueError("output directory must not already exist")
    with tempfile.TemporaryDirectory(prefix="paw-visual-fixture-") as directory:
        work = Path(directory)
        isolated = work / binary.name
        shutil.copy2(binary, isolated)
        env = os.environ.copy()
        env.pop("MEOW_BIN", None)
        # Do not discover real preferences or subscription storage even if flags regress.
        for name in ("HOME", "USERPROFILE", "APPDATA", "LOCALAPPDATA", "XDG_CONFIG_HOME", "XDG_DATA_HOME", "XDG_CACHE_HOME"):
            env[name] = str(work / "home")
        for page in PAGES:
            image = work / (page + ".png")
            subprocess.run([str(isolated), "--no-preferences", "--no-tray", "--locale", "zh-CN",
                            "--page", page, "--screenshot", str(image)],
                           env=env, cwd=work, check=True, timeout=30)
            check_png(image)
        # Publishing only after every capture succeeds avoids partial evidence sets.
        output.mkdir(parents=False, exist_ok=False)
        for page in PAGES:
            shutil.copyfile(work / (page + ".png"), output / (page + ".png"))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path, help="locally built visual-test Paw executable")
    parser.add_argument("output", type=Path, help="new output directory under an existing parent")
    args = parser.parse_args()
    try:
        capture(args.binary, args.output)
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        parser.exit(1, f"capture failed: {error}\n")
    print("Captured overview, proxies and settings. Inspect the images manually.")


if __name__ == "__main__":
    main()
