# Windows packaging and visual acceptance

This is a local, opt-in runbook. The CI workflow remains the formatting,
Clippy and headless test matrix on `ubuntu-24.04` and `windows-2025`; it does
not initialize or build the core, create a release package, or capture a GUI.
Building requires local prerequisites; GUI checks also require a Windows GUI
session. These instructions are not evidence of a successful Windows run.

## Portable package and provenance

The packaging tool creates a ZIP containing exactly `meow-paw.exe`, `meow.exe`,
and `manifest.json`. The manifest identifies the clean Paw commit and its core
gitlink, and records the size and SHA-256 of each executable. It contains no
build paths, preferences, profiles, subscriptions, credentials, or signing keys.
The core remains a separate executable; this workflow does not copy its source or
assets into Paw. Check the applicable licenses and distribution obligations of
both binaries and their dependencies before any eventual distribution.

On a Windows build machine, start from a clean, committed Paw checkout with an
initialized core checkout at its committed gitlink. Check both worktrees and
record the commits independently. The baseline for this work is Paw
`5e5fbc4645c009c5d30491d6ca5a6eb983d22044`, whose core gitlink is
`e3bfb51ea8b07935cb95126dbf755581470eb2cd`; a checkout containing the packaging
tool must have its own committed Paw revision. With prerequisites and locked
dependencies already available, build the pinned pair and create the package:

```powershell
git rev-parse HEAD
git ls-tree HEAD vendor/meow-rs
git status --porcelain
git -C vendor/meow-rs rev-parse HEAD
git -C vendor/meow-rs status --porcelain
python tools/build-with-core.py --jobs 1
python tools/package-windows.py create --output .paw-local/meow-paw-portable.zip
python tools/package-windows.py verify .paw-local/meow-paw-portable.zip
```

Create the output parent before invoking the packaging tool; the output ZIP
must not already exist. The build script checks that the core is initialized, clean, and matches the *committed* gitlink;
the packaging tool also requires a clean Paw checkout. Use an output location
outside the checkout or ignored by Git. The Python standard library is sufficient for the packaging tool. Its verification
command checks package contents and hashes without extracting or running either
executable. Run the
synthetic contract tests with `python -m unittest discover -s tests -p
"test_package_windows.py"` on Windows or a neutral host.

The manifest binds the supplied bytes to their recorded hashes and records source
revisions; it cannot prove that the binaries were built from those revisions,
that they are Windows executables, or that they contain no secrets. Build inputs,
toolchain and environment need independent review. No signature or trust chain
is supplied, and byte-for-byte reproducible builds are not established. Do not
include local paths or private data in evidence. The offline contract tests
do not establish a native Windows build or GUI launch. The package is local
only: no installer, signing,
upload, delivery, or release job is provided.

## Clean-machine and first-launch validation

Verification above is an offline, no-launch path that changes no system settings. It
checks the ZIP as an artifact, not whether the programs work on a clean Windows
machine. A clean-machine run is still pending. For a future authorized test, use a
disposable Windows VM with a fresh user and no production configuration, secrets,
or connectivity, and compare the VM state before and after. Verify the ZIP,
extract it into a disposable user-owned directory, and keep the two executables
adjacent. In a process-local PowerShell environment, remove `MEOW_BIN`; use
`meow-paw.exe --no-preferences --no-tray` without `--profile` or `--start`, and
close the window. Confirm a stopped, empty state, adjacent engine discovery, and
process exit. Do not start an engine, import a profile, connect to a subscription,
or change proxy or network settings. Record the OS, commits, package and binary
hashes, commands, results, and unexpected changes without paths or private data.
The flags prevent Paw preference and tray use; they do not prove that Windows or
other dependencies make no changes. Normal launches may read saved preferences
from `%LOCALAPPDATA%\meow-paw` and a saved engine or `MEOW_BIN` overrides the
adjacent executable. Do not use a real user profile for this test.

## Installer and upgrade status

There is currently no installer to validate. Installation, uninstall, upgrade,
rollback, interrupted upgrade, locked/running executable, and prior-version
migration behavior have not been implemented or tested. If an installer is later
proposed, validate it in disposable Windows environments with synthetic data,
including fresh install, first launch, upgrade and uninstall, preservation of
user data, and process cleanup. It must not self-elevate or modify registry,
system proxy, Clash, DNS, VPN, routes or other machine-wide network state under
this scope. Treat installer and upgrade validation as pending until an actual
implementation and evidence exist; a portable ZIP verification is not that
validation.

## Synthetic Chinese window captures

Build a separate visual-test executable only when resources permit, for example
`cargo build --locked --features visual-test --bin meow-paw`. This feature is
required for `--screenshot`; the release packaging command above enables only
`desktop`. With Python and a functioning Windows GUI, run:

```powershell
python tools/capture-zh-fixture.py target/debug/meow-paw.exe .paw-local/zh-evidence
```

Create `.paw-local` first; the final output directory must not exist. The
wrapper copies the executable alone into a temporary directory, clears
`MEOW_BIN`, redirects user configuration directories, and invokes the
window-render capture with `--no-preferences --no-tray --locale zh-CN` for
`overview`, `proxies`, and `settings`. It supplies no profile or `--start`,
so the expected fixture is an empty, stopped state with no engine. The
application itself disables preferences and tray for screenshots and captures
its own window after a 1.4-second delay; this is not desktop-wide capture.
The wrapper checks PNG headers and nonzero dimensions, not full image integrity,
content, translation, pixel determinism or privacy. It times out each launch
and only publishes after all three header checks pass. The copied executable
must be independently runnable; missing adjacent DLLs or GUI support can make
capture fail. Do not substitute a desktop screenshot or bypass isolation.

Manually inspect all three files for the intended page, Chinese navigation and
shell controls, readable CJK glyphs, clipping, and absence of personal data.
Page content is only partly localized. Record OS, display scaling, font,
commit, binary hash, feature and results without real paths or secrets. Rendering
may vary with fonts, scaling and timing; no baseline or pixel comparison is
provided. Delete local evidence if it contains unexpected data. The offline
wrapper contract can be checked with `python -m unittest discover -s tests -p
"test_capture_zh_fixture.py"` without launching Paw.

## Operational checks

On a disposable Windows test environment, verify the clean package starts and
exits, and that the adjacent core is discovered with no override or saved
engine path. For a managed engine check, use only a synthetic local profile and
a test-owned core. The ignored test
`isolated_session_proxies_loopback_traffic_and_cleans_up` in
`tests/managed_meow.rs` exercises a synthetic DIRECT/REJECT group, loopback
traffic, mode selection and listener cleanup; it requires `MEOW_BIN` and a
local core and binds local ports. Run that named test alone when authorized
and dependencies are available; other ignored tests have separate requirements.
Verify process cleanup and tray/window behavior separately on Windows; the
existing `tools/tray-smoke.py` is Linux/X11-specific. Do not change system
proxy, Clash, DNS, VPN, routes or external network settings, and do not use
live configurations. A stopped-state screenshot does not prove managed-core,
tray, installer or release behavior.
