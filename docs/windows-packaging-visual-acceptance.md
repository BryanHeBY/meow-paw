# Windows packaging and visual acceptance

This is a local, opt-in runbook. The CI workflow remains the formatting,
Clippy and headless test matrix on `ubuntu-24.04` and `windows-2025`; it does
not initialize or build the core, create a release package, or capture a GUI.
Building requires local prerequisites; GUI checks also require a Windows GUI
session. These instructions are not evidence of a successful Windows run.

## Portable package and provenance

The packaging tool creates a **local baseline** ZIP with exactly
`meow-paw.exe`, `meow.exe`, `manifest.json`, `THIRD-PARTY-NOTICES.txt`, and
the required files under `LICENSES/`. This includes the exact MIT notices at
the committed Paw root and pinned Meow gitlink, GPUI Kit and GPUI pre notices,
the complete Lucide/Feather notice, the known Paw MPL/CDLA notices, and the
official Wintun 0.14.1 Prebuilt Binaries License. The core remains a separate
executable. The package declares `notice_coverage: "baseline_only"`; **do not
publish or distribute the baseline ZIP**.

The manifest identifies the clean Paw commit and its core gitlink and records
the size and SHA-256 of each executable and notice. No build paths,
preferences, subscriptions, credentials, or signing keys are included.
Source notices are read directly from committed regular Git blobs, so line
endings are preserved. The third-party originals are pinned byte for byte;
the Wintun text preserves the CRLF of the official ZIP.

On a Windows build machine, start from a clean, committed Paw checkout with
an initialized core at its committed gitlink. With prerequisites and locked
dependencies already available, build the pinned pair and create the local
package:

```powershell
git rev-parse HEAD
git ls-tree HEAD vendor/meow-rs
git status --porcelain
git -C vendor/meow-rs rev-parse HEAD
git -C vendor/meow-rs status --porcelain
python tools/build-with-core.py --jobs 1
python tools/package-windows.py create --output .paw-local/meow-paw-portable.zip
python tools/package-windows.py verify .paw-local/meow-paw-portable.zip
python tools/package-windows.py verify --source-root . .paw-local/meow-paw-portable.zip
```

Create the output parent first; the output ZIP must not already exist. Use a
location outside the checkout or ignored by Git. The Python standard library
is sufficient. Verification checks the exact member list and hashes without
extracting or running the executables. Optional `--source-root` checks the
archive's two source commits and matches **every notice** against the clean
committed sources. It also detects a replacement Paw/Meow notice with an
updated manifest. Verification without a trusted source checkout cannot
independently authenticate those notices or the authored index.

Run the synthetic contract tests with
`python -m unittest discover -s tests -p "test_package_windows.py"` on Windows
or a neutral host. The format is `paw-windows-portable-v2`; legacy notice-free
archives do not pass this verifier.

The manifest cannot prove that the supplied executables were built from the
listed revisions, that they are Windows executables, that they contain no
secrets, or that their license obligations have been met. No signature or
trust chain is supplied, and reproducible executables are not established.
The offline fixture tests do not establish a native Windows build or launch.
There is no installer, signing, upload, delivery, or public release job.

### Required license check before any Windows distribution

1. Record the **actual** Windows target triple, locked Paw and Meow commits,
   feature sets, Cargo/Rust/toolchain versions, artifact hashes, and any
   patched/embedded inputs. The default build script uses Paw's desktop
   feature and Meow app's defaults (Full). Do not use a Linux inventory, the
   unfiltered lockfile, or a different feature set as the release evidence.
2. From those same sources, resolve both target-specific dependency graphs
   and review the licenses and copyrights from the exact crate/source
   archives. For an ordinary x86-64 MSVC default build start with:

   ```powershell
   cargo tree --locked --target x86_64-pc-windows-msvc --features desktop -e normal
   cargo tree --locked --manifest-path vendor/meow-rs/Cargo.toml -p meow-app --target x86_64-pc-windows-msvc -e normal
   ```

   Use the same target and features as the actual build, including any
   dependency overrides. Check build-script-generated content separately.
   Reconcile the graph with both final binaries and the archive, including
   native libraries, embedded or runtime-extracted DLLs, certificates and
   data/asset copies. Record the final license choices and source
   availability for everything redistributed. The baseline only verifies a
   subset of the GPUI and Paw x86-64 target graph and does not resolve
   Meow's complete graph. If `self_cell 1.3.0` is present, elect and record
   Apache-2.0, not its alternate GPL-2.0-only terms.
3. Preserve the required original text and notices. For MPL-2.0
   `option-ext`, verify the corresponding exact covered source (including
   local modifications, if any) is available to recipients and tell them how
   to obtain it under the terms of [MPL-2.0 §3.2](https://www.mozilla.org/en-US/MPL/2.0/).
   Retain the CDLA-Permissive-2.0 text for `webpki-roots` certificate data
   as applicable to the shipped snapshot. See the
   [baseline inventory and source references](../licenses/windows/THIRD-PARTY-NOTICES.txt).
4. For a default/full Windows Meow build, inventory Wintun even when the
   outer ZIP has no separate DLL. Use the exact matching, signed official
   binary and its Prebuilt Binaries License from [wintun.net](https://www.wintun.net/);
   the separately available Wintun source has different GPL-2.0 terms. The
   prebuilt terms refer to redistribution alongside software using the
   permitted API. Do not assume that an embedded-only payload satisfies
   this condition. Document a verified packaging/runtime strategy under
   the applicable terms or keep that artifact out of distribution.
5. Have the artifact-specific inventory and all notices/source offers
   checked and ship them with the real package. The baseline tool does not
   accept or certify a release notice set. Until the audited notice set and
   Wintun disposition are reflected in the actual release artifact, keep
   this tool's ZIP local. Rerun the audit when target, dependency locks, pin,
   features, native code or asset inputs change.

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
