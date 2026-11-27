#!/usr/bin/env bash
# Native, disposable Arch Linux ARM build on a GitHub-hosted ARM64 runner.
set -euo pipefail

[[ $(uname -m) == aarch64 ]] || { echo 'An AArch64 host is required' >&2; exit 1; }
: "${RUNNER_TEMP:?}" "${GITHUB_SHA:?}" "${GITHUB_RUN_NUMBER:?}" "${GITHUB_RUN_ATTEMPT:?}"
repo=$(git rev-parse --show-toplevel)
cd "$repo"
[[ $(git rev-parse HEAD) == "$GITHUB_SHA" ]]
[[ -z $(git status --porcelain --untracked-files=all) ]]
core=$(git rev-parse "$GITHUB_SHA:vendor/meow-rs")
[[ $(git -C vendor/meow-rs rev-parse HEAD) == "$core" ]]
[[ -z $(git -C vendor/meow-rs status --porcelain --untracked-files=all) ]]
version=$(python3 -c 'import tomllib; print(tomllib.load(open("Cargo.toml","rb"))["package"]["version"])')
[[ $version =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]
export PAW_PACKAGE_VERSION="$version.r$GITHUB_RUN_NUMBER.$GITHUB_RUN_ATTEMPT.g${GITHUB_SHA:0:12}"
export SOURCE_DATE_EPOCH
SOURCE_DATE_EPOCH=$(git show -s --format=%ct "$GITHUB_SHA")
# shellcheck source=packaging/arch/PKGBUILD
source "$repo/packaging/arch/PKGBUILD"

work=$(mktemp -d "$RUNNER_TEMP/paw-arch.XXXXXXXX")
root="$work/root"
out="$RUNNER_TEMP/paw-arch-artifact"
mkdir -p "$root" "$out" "$work/gnupg"
chmod 700 "$work/gnupg"
fingerprint=68B3537F39A313B3E574D06777193F152BDBE6A6
mirror=https://ca.us.mirror.archlinuxarm.org
image=ArchLinuxARM-aarch64-latest.tar.gz
download() {
  curl --fail --show-error --silent --location --retry 3 \
    --proto '=https' --proto-redir '=https' --output "$2" "$1"
}
download "$mirror/os/$image" "$work/$image"
download "$mirror/os/$image.sig" "$work/$image.sig"
download "https://keyserver.ubuntu.com/pks/lookup?op=get&search=0x$fingerprint" "$work/key.asc"
gpg --homedir "$work/gnupg" --batch --import "$work/key.asc"
gpg --homedir "$work/gnupg" --batch --export "$fingerprint" >"$work/rootfs-keyring.gpg"
test -s "$work/rootfs-keyring.gpg"
gpgv --keyring "$work/rootfs-keyring.gpg" "$work/$image.sig" "$work/$image"
image_sha=$(sha256sum "$work/$image" | cut -d' ' -f1)
sudo tar --numeric-owner --acls --xattrs --xattrs-include="*" -xzf "$work/$image" -C "$root"
rm "$work/$image"

sudo rm -f "$root/etc/resolv.conf"
sudo install -m644 /etc/resolv.conf "$root/etc/resolv.conf"
printf 'Server = %s/$arch/$repo\n' "$mirror" | sudo tee "$root/etc/pacman.d/mirrorlist" >/dev/null
# These mounts only exist in this workflow. Tear them down even on failure.
cleanup_mounts() {
  sudo umount "$root/proc" 2>/dev/null || true
  sudo umount -R "$root/dev" 2>/dev/null || true
}
trap cleanup_mounts EXIT
sudo mount --rbind /dev "$root/dev"
sudo mount --make-rslave "$root/dev"
sudo mount -t proc proc "$root/proc"

archrun() { sudo chroot "$root" "$@"; }
archrun pacman-key --init
archrun pacman-key --populate archlinuxarm
# The runner supplies the kernel; do not generate a boot image in the chroot.
archrun pacman -Syu --needed --noconfirm --ignore linux-aarch64 base-devel \
  "${depends[@]}" "${makedepends[@]}" "${checkdepends[@]}"
archrun find /var/cache/pacman/pkg -maxdepth 1 -type f -delete
archrun useradd --create-home --shell /bin/bash builder
# Archive just Git-tracked data from each verified commit; never copy host
# caches, credentials, build output or the runner's Git metadata into the root.
stage="$work/stage"
mkdir -p "$stage/source/vendor/meow-rs" "$stage/output"
git archive "$GITHUB_SHA" | tar -xf - -C "$stage/source"
git -C vendor/meow-rs archive "$core" | tar -xf - -C "$stage/source/vendor/meow-rs"
cp packaging/arch/{PKGBUILD,meow-paw.desktop} "$stage/"
sudo mv "$stage" "$root/build"
archrun chown -R builder:builder /build
archrun runuser -u builder -- env HOME=/home/builder LC_ALL=C.UTF-8 \
  PAW_PACKAGE_VERSION="$PAW_PACKAGE_VERSION" SOURCE_DATE_EPOCH="$SOURCE_DATE_EPOCH" \
  PKGDEST=/build/output PKGEXT=.pkg.tar.zst \
  bash -c 'cd /build && exec makepkg --noconfirm --clean'

package="$pkgname-$pkgver-$pkgrel-aarch64.pkg.tar.zst"
archrun pacman -Qip "/build/output/$package"
# Reject any unplanned payload (including hooks, configuration, and other
# executables) and all unexpected changes to the app/sidecar layout.
archrun bsdtar -tf "/build/output/$package" | sed 's|^\./||' |
  awk 'substr($0,length($0)) != "/"' | LC_ALL=C sort >"$work/files"
cat >"$work/expected" <<'FILES'
.BUILDINFO
.MTREE
.PKGINFO
usr/bin/meow-paw
usr/lib/meow-paw/meow
usr/lib/meow-paw/meow-paw
usr/share/applications/meow-paw.desktop
usr/share/icons/hicolor/scalable/apps/meow-paw.svg
usr/share/licenses/meow-paw/LICENSE
usr/share/licenses/meow-paw/meow-rs-LICENSE
FILES
diff -u "$work/expected" "$work/files"

# Smoke-install inside the disposable Arch root, never on the host.
archrun pacman -U --noconfirm "/build/output/$package"
archrun pacman -Qkk "$pkgname"
[[ $(archrun readlink -f /usr/bin/meow-paw) == /usr/lib/meow-paw/meow-paw ]]
for binary in meow-paw meow; do
  archrun env LC_ALL=C ldd "/usr/lib/meow-paw/$binary" >"$work/ldd"
  cat "$work/ldd"
  if grep -Fq 'not found' "$work/ldd"; then exit 1; fi
done
archrun runuser -u builder -- /usr/lib/meow-paw/meow --version
sudo install -m644 -o "$(id -u)" -g "$(id -g)" "$root/build/output/$package" "$out/$package"
printf 'paw=%s\ncore=%s\narch_rootfs_sha256=%s\n' "$GITHUB_SHA" "$core" "$image_sha" >"$out/BUILD-INFO.txt"
(cd "$out" && sha256sum "$package" >SHA256SUMS)
