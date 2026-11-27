# Meow Paw for Arch Linux ARM (AArch64)

The [Arch Linux ARM package workflow](../.github/workflows/arch-arm64.yml) builds
both Paw and its pinned Meow submodule on the GitHub-hosted `ubuntu-24.04-arm`
runner, inside the signed official Arch Linux ARM userland. This is an Arch
`aarch64` package, not an Ubuntu binary wrapped in an archive. The workflow
uploads an Actions artifact only; it does not publish a Release or an AUR
package. The rootfs and system packages follow Arch Linux ARM's rolling
repository; the build records the rootfs SHA-256 and both source commits.

## Build and download

In the repository's **Actions → Arch Linux ARM package**, select **Run
workflow** on `main`. Or, with the GitHub CLI authenticated:

```sh
gh workflow run arch-arm64.yml --ref main --repo BryanHeBY/meow-paw
```

After the run succeeds, download and unpack **meow-paw-arch-aarch64**. It contains
one `.pkg.tar.zst`, its SHA-256 checksum, and the two Git revisions in
`BUILD-INFO.txt`. Treat Actions artifacts as packages you trust from your
repository; the checksum detects a damaged download, not a signature or
a substitute for your local package trust policy.

## Install on Omarchy / Arch Linux ARM

On a compatible, up-to-date `aarch64` system with its ordinary package
repositories configured, run these commands from the unpacked artifact:

```sh
uname -m                          # should print aarch64
sha256sum -c SHA256SUMS
sudo pacman -U ./meow-paw-*-aarch64.pkg.tar.zst
```

Start **Meow Paw** from the application launcher, or run `meow-paw`.
The package includes the pinned Meow binary next to Paw under
`/usr/lib/meow-paw`; it does not install a system-wide `meow` command.
An explicit `MEOW_BIN` or already saved custom engine path still wins.
Installing the package does not start an app or a service, change system
proxy settings, or install any profiles or secrets.

Paw uses your desktop's Vulkan driver, D-Bus and existing session services.
A working portal backend is needed for the native file picker. The app
does not replace an Apple Silicon Vulkan driver, Wayland compositor or
portal configuration.

Before upload, the workflow verifies both executables are native AArch64
ELFs, validates the desktop shortcut and package file list, installs the
package with Pacman in its disposable Arch userland, checks shared libraries
and runs the installed sidecar's version command. It does not test a GPU,
interactive Wayland/Hyprland, tray, or a live proxy session.
