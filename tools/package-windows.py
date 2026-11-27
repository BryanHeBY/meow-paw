#!/usr/bin/env python3
"""Create or verify a minimal Windows portable ZIP without running its contents.

The source commits are checked at packaging time; they do not attest that the
supplied executables were built from those commits.
"""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import stat
import subprocess
import sys
import zipfile

NAMES = ("meow-paw.exe", "meow.exe")
FORMAT = "paw-windows-portable-v1"
MAX_MANIFEST = 16384


def git(root, *args):
    return subprocess.run(
        ["git", "--no-optional-locks", "-C", str(root), *args],
        check=True, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
    ).stdout


def source_provenance(root):
    try:
        if Path(os.fsdecode(git(root, "rev-parse", "--show-toplevel").strip())).resolve() != root.resolve():
            raise ValueError("source root is not a repository root")
        if git(root, "status", "--porcelain=v1", "--untracked-files=all", "--ignore-submodules=none"):
            raise ValueError("Paw checkout must be clean")
        spec = importlib.util.spec_from_file_location("build_with_core", Path(__file__).with_name("build-with-core.py"))
        build = importlib.util.module_from_spec(spec)
        previous = sys.dont_write_bytecode
        try:
            sys.dont_write_bytecode = True
            spec.loader.exec_module(build)
        finally:
            sys.dont_write_bytecode = previous
        build.verify_core(root, root / "vendor" / "meow-rs")
        paw = git(root, "rev-parse", "--verify", "HEAD").decode("ascii").strip()
        entry = git(root, "ls-tree", "-z", "HEAD", "--", "vendor/meow-rs")
        core = entry.split(b"\t", 1)[0].split()[2].decode("ascii")
        if not valid_commit(paw) or not valid_commit(core):
            raise ValueError("invalid source commit")
        return {"paw_commit": paw, "core_commit": core, "binary_origin": "not_attested"}
    except (OSError, subprocess.CalledProcessError, UnicodeError, IndexError):
        raise ValueError("cannot inspect source provenance") from None


def valid_commit(value):
    return isinstance(value, str) and re.fullmatch(r"(?:[0-9a-f]{40}|[0-9a-f]{64})", value) is not None


def open_input(path):
    # Check both the directory entry and opened file. O_NOFOLLOW closes the
    # symlink race on platforms that provide it; Windows reparse points are
    # rejected via lstat's file attributes.
    info = path.lstat()
    if not stat.S_ISREG(info.st_mode) or getattr(info, "st_file_attributes", 0) & 0x400:
        raise ValueError("executable inputs must be regular files, not links")
    fd = os.open(path, os.O_RDONLY | getattr(os, "O_BINARY", 0) | getattr(os, "O_NOFOLLOW", 0))
    try:
        opened = os.fstat(fd)
        if not stat.S_ISREG(opened.st_mode) or (info.st_dev, info.st_ino) != (opened.st_dev, opened.st_ino):
            raise ValueError("executable input changed while opening")
        if opened.st_size == 0:
            raise ValueError("executable inputs must not be empty")
        return os.fdopen(fd, "rb")
    except BaseException:
        os.close(fd)
        raise


def zip_info(name):
    entry = zipfile.ZipInfo(name, (1980, 1, 1, 0, 0, 0))
    entry.create_system = 3
    entry.external_attr = (stat.S_IFREG | 0o644) << 16
    entry.compress_type = zipfile.ZIP_STORED
    return entry


def create(root, binaries, output):
    provenance = source_provenance(root)
    # Exclusively create the output: never replace an existing file or link.
    # Open inputs first so failures cannot leave an artifact behind.
    handles = []
    try:
        for name in NAMES:
            handles.append(open_input(binaries / name))
        with open(output, "xb") as raw:
            try:
                with zipfile.ZipFile(raw, "w", allowZip64=True) as archive:
                    files = {}
                    for name, source in zip(NAMES, handles):
                        digest = hashlib.sha256()
                        size = 0
                        info = zip_info(name)
                        info.file_size = os.fstat(source.fileno()).st_size
                        with archive.open(info, "w") as dest:
                            while True:
                                chunk = source.read(1024 * 1024)
                                if not chunk:
                                    break
                                size += len(chunk)
                                digest.update(chunk)
                                dest.write(chunk)
                        if size == 0:
                            raise ValueError("executable inputs must not be empty")
                        files[name] = {"sha256": digest.hexdigest(), "size": size}
                    manifest = {"format": FORMAT, "provenance": provenance, "files": files}
                    encoded = (json.dumps(manifest, sort_keys=True, separators=(",", ":")) + "\n").encode("utf-8")
                    archive.writestr(zip_info("manifest.json"), encoded)
            except BaseException:
                raw.close()
                output.unlink(missing_ok=True)
                raise
    finally:
        for handle in handles:
            handle.close()


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError("duplicate manifest key")
        result[key] = value
    return result


def verify(path):
    with zipfile.ZipFile(path, "r") as archive:
        infos = archive.infolist()
        if len(infos) != 3 or {entry.filename for entry in infos} != {*NAMES, "manifest.json"}:
            raise ValueError("archive must contain exactly the two executables and manifest.json")
        for entry in infos:
            mode = entry.external_attr >> 16
            if (entry.is_dir() or stat.S_IFMT(mode) not in (0, stat.S_IFREG) or entry.flag_bits & 1
                    or entry.compress_type != zipfile.ZIP_STORED or entry.compress_size != entry.file_size):
                raise ValueError("archive entries must be unencrypted, uncompressed regular files")
        manifest_info = archive.getinfo("manifest.json")
        if manifest_info.file_size > MAX_MANIFEST:
            raise ValueError("manifest is too large")
        manifest = json.loads(archive.read(manifest_info).decode("utf-8"), object_pairs_hook=unique_object)
        if not isinstance(manifest, dict) or set(manifest) != {"format", "provenance", "files"} or manifest["format"] != FORMAT:
            raise ValueError("invalid manifest format")
        provenance = manifest["provenance"]
        if (not isinstance(provenance, dict) or set(provenance) != {"paw_commit", "core_commit", "binary_origin"}
                or not valid_commit(provenance["paw_commit"]) or not valid_commit(provenance["core_commit"])
                or provenance["binary_origin"] != "not_attested"):
            raise ValueError("invalid provenance")
        files = manifest["files"]
        if not isinstance(files, dict) or set(files) != set(NAMES):
            raise ValueError("invalid manifest file list")
        for name in NAMES:
            item = files[name]
            if (not isinstance(item, dict) or set(item) != {"sha256", "size"}
                    or not isinstance(item["sha256"], str) or not re.fullmatch(r"[0-9a-f]{64}", item["sha256"])
                    or type(item["size"]) is not int or item["size"] <= 0
                    or archive.getinfo(name).file_size != item["size"]):
                raise ValueError("invalid executable metadata")
            digest = hashlib.sha256()
            with archive.open(name) as source:
                for chunk in iter(lambda: source.read(1024 * 1024), b""):
                    digest.update(chunk)
            if digest.hexdigest() != item["sha256"]:
                raise ValueError("executable hash mismatch")
    return manifest


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    make = sub.add_parser("create", help="package adjacent Windows executables")
    make.add_argument("--source-root", type=Path, default=Path(__file__).resolve().parents[1])
    make.add_argument("--binary-dir", type=Path, help="defaults to SOURCE_ROOT/target/release")
    make.add_argument("--output", type=Path, required=True)
    check = sub.add_parser("verify", help="verify a ZIP without extracting or running it")
    check.add_argument("archive", type=Path)
    args = parser.parse_args()
    try:
        if args.command == "create":
            create(args.source_root, args.binary_dir or args.source_root / "target" / "release", args.output)
            print("Portable package created; source commits do not attest binary origin.")
        else:
            verify(args.archive)
            print("Package contents and manifest hashes verified; binary origin is not attested.")
    except (OSError, ValueError, UnicodeError, json.JSONDecodeError, zipfile.BadZipFile, RuntimeError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
