#!/usr/bin/env python3
"""Create or verify a minimal Windows portable ZIP without running its contents.

The source commits are checked at packaging time; they do not attest that the
supplied executables were built from those commits. The ZIP carries baseline
notices; a separate artifact-specific audit is required for distribution.
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
FORMAT = "paw-windows-portable-v2"
NOTICE_COVERAGE = "baseline_only"
MAX_MANIFEST = 16384
MAX_NOTICE = 65536

# ZIP path -> (source commit key, repository path, immutable upstream digest).
# Paw/Core MIT texts and the authored index are sourced from the manifest's
# committed trees. Upstream notices are pinned to their exact original bytes.
NOTICES = {
    "LICENSES/Paw-MIT.txt": ("paw_commit", "LICENSE", None),
    "LICENSES/Meow-MIT.txt": ("core_commit", "LICENSE", None),
    "THIRD-PARTY-NOTICES.txt": ("paw_commit", "licenses/windows/THIRD-PARTY-NOTICES.txt", None),
    "LICENSES/gpui-kit-0.11.0-apache.txt": (
        "paw_commit", "licenses/windows/gpui-kit-0.11.0-apache.txt",
        "d1b0449e5478c574ba4f686c2656df7fe77d66821a61f8b6ed3378a58ed9a811",
    ),
    "LICENSES/gpui-pre-0.3.14-apache.txt": (
        "paw_commit", "licenses/windows/gpui-pre-0.3.14-apache.txt",
        "752daf2fb234ca4a1fa372c073fe127f44b7b90fd2529ae44273a64f9d53da7a",
    ),
    "LICENSES/gpui-kit-assets-0.11.0-lucide-feather.txt": (
        "paw_commit", "licenses/windows/gpui-kit-assets-0.11.0-lucide-feather.txt",
        "b495047bd93a9b06913511076f504daba17d5bbeb3e0650f3bb53a4220329c57",
    ),
    "LICENSES/option-ext-0.2.0-mpl.txt": (
        "paw_commit", "licenses/windows/option-ext-0.2.0-mpl.txt",
        "66a3107d5ad6a058aab753eaac2047ccb2ed0e39465dd0fe5844da3e300d5172",
    ),
    "LICENSES/webpki-roots-1.0.11-cdla.txt": (
        "paw_commit", "licenses/windows/webpki-roots-1.0.11-cdla.txt",
        "e271993808fec50ab29350b39539cdec611a9103f827e0aa26d61da70e2d33f8",
    ),
    "LICENSES/wintun-0.14.1-prebuilt.txt": (
        "paw_commit", "licenses/windows/wintun-0.14.1-prebuilt.txt",
        "183adac21e7d96c508c8fd34d394b7b6708bc81564ad1bad61ab66143a008cd2",
    ),
}
PAYLOADS = (*NAMES, *NOTICES)


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


def validate_notice(name, data, digest):
    if not 0 < len(data) <= MAX_NOTICE:
        raise ValueError(f"invalid notice size: {name}")
    text = data.decode("utf-8")
    if not text.strip() or re.search(r"[\x00-\x08\x0b\x0c\x0e-\x1f\x7f]", text):
        raise ValueError(f"invalid notice text: {name}")
    if name in ("LICENSES/Paw-MIT.txt", "LICENSES/Meow-MIT.txt") and not (
        "Copyright" in text and "Permission is hereby granted" in text
        and "THE SOFTWARE IS PROVIDED" in text
    ):
        raise ValueError(f"invalid MIT notice: {name}")
    if name == "THIRD-PARTY-NOTICES.txt" and not text.startswith(
        "Meow Paw Windows portable bundle: baseline third-party notices"
    ):
        raise ValueError(f"invalid notice index: {name}")
    if (expected := NOTICES[name][2]) is not None and digest != expected:
        raise ValueError(f"upstream notice does not match its pinned bytes: {name}")


def source_notices(root, provenance):
    """Use committed regular blobs, preserving bytes across Git EOL settings."""
    notices = {}
    for name, (key, path, _) in NOTICES.items():
        repository = root if key == "paw_commit" else root / "vendor" / "meow-rs"
        commit = provenance[key]
        try:
            entry = git(repository, "ls-tree", "-z", commit, "--", path)
            metadata, tree_path = entry.rstrip(b"\0").split(b"\t", 1)
            mode, kind, blob = metadata.split()
            if kind != b"blob" or mode not in (b"100644", b"100755") or tree_path != path.encode("ascii"):
                raise ValueError
            oid = blob.decode("ascii")
            size = int(git(repository, "cat-file", "-s", oid))
            if not 0 < size <= MAX_NOTICE:
                raise ValueError
            data = git(repository, "cat-file", "blob", oid)
            if len(data) != size:
                raise ValueError
        except (OSError, ValueError, subprocess.CalledProcessError):
            raise ValueError(f"cannot read required committed notice: {name}") from None
        digest = hashlib.sha256(data).hexdigest()
        validate_notice(name, data, digest)
        notices[name] = (data, {"sha256": digest, "size": size})
    return notices


def create(root, binaries, output):
    provenance = source_provenance(root)
    notices = source_notices(root, provenance)
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
                    for name, (data, metadata) in notices.items():
                        archive.writestr(zip_info(name), data)
                        files[name] = metadata
                    manifest = {"format": FORMAT, "provenance": provenance, "files": files,
                                "notice_coverage": NOTICE_COVERAGE}
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


def verify(path, root=None):
    with zipfile.ZipFile(path, "r") as archive:
        infos = archive.infolist()
        expected = {*PAYLOADS, "manifest.json"}
        if len(infos) != len(expected) or {entry.filename for entry in infos} != expected:
            raise ValueError("archive must contain exactly the executables, required notices and manifest.json")
        for entry in infos:
            mode = entry.external_attr >> 16
            if (entry.is_dir() or stat.S_IFMT(mode) not in (0, stat.S_IFREG) or entry.flag_bits & 1
                    or entry.compress_type != zipfile.ZIP_STORED or entry.compress_size != entry.file_size):
                raise ValueError("archive entries must be unencrypted, uncompressed regular files")
        manifest_info = archive.getinfo("manifest.json")
        if manifest_info.file_size > MAX_MANIFEST:
            raise ValueError("manifest is too large")
        manifest = json.loads(archive.read(manifest_info).decode("utf-8"), object_pairs_hook=unique_object)
        if (not isinstance(manifest, dict)
                or set(manifest) != {"format", "provenance", "files", "notice_coverage"}
                or manifest["format"] != FORMAT or manifest["notice_coverage"] != NOTICE_COVERAGE):
            raise ValueError("invalid manifest format or notice coverage")
        provenance = manifest["provenance"]
        if (not isinstance(provenance, dict) or set(provenance) != {"paw_commit", "core_commit", "binary_origin"}
                or not valid_commit(provenance["paw_commit"]) or not valid_commit(provenance["core_commit"])
                or provenance["binary_origin"] != "not_attested"):
            raise ValueError("invalid provenance")
        files = manifest["files"]
        if not isinstance(files, dict) or set(files) != set(PAYLOADS):
            raise ValueError("invalid manifest file list")
        if root is not None:
            if source_provenance(root) != provenance:
                raise ValueError("source checkout does not match archived commits")
            trusted_notices = source_notices(root, provenance)
        else:
            trusted_notices = None
        for name in PAYLOADS:
            item = files[name]
            if (not isinstance(item, dict) or set(item) != {"sha256", "size"}
                    or not isinstance(item["sha256"], str) or not re.fullmatch(r"[0-9a-f]{64}", item["sha256"])
                    or type(item["size"]) is not int or item["size"] <= 0
                    or archive.getinfo(name).file_size != item["size"]):
                raise ValueError(f"invalid file metadata: {name}")
            if name in NOTICES:
                if item["size"] > MAX_NOTICE:
                    raise ValueError(f"invalid notice size: {name}")
                data = archive.read(name)
                digest = hashlib.sha256(data).hexdigest()
                validate_notice(name, data, digest)
                if trusted_notices is not None and data != trusted_notices[name][0]:
                    raise ValueError(f"notice does not match its committed source: {name}")
            else:
                hashed = hashlib.sha256()
                with archive.open(name) as source:
                    for chunk in iter(lambda: source.read(1024 * 1024), b""):
                        hashed.update(chunk)
                digest = hashed.hexdigest()
            if digest != item["sha256"]:
                raise ValueError(f"file hash mismatch: {name}")
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
    check.add_argument("--source-root", type=Path, help="check notices against matching clean committed sources")
    args = parser.parse_args()
    try:
        if args.command == "create":
            create(args.source_root, args.binary_dir or args.source_root / "target" / "release", args.output)
            print("Portable package created with baseline notices only; not approved for distribution.")
        else:
            verify(args.archive, args.source_root)
            print("Package contents and hashes verified; notice coverage is baseline only and binary origin is not attested.")
    except (OSError, ValueError, UnicodeError, json.JSONDecodeError, zipfile.BadZipFile, RuntimeError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
