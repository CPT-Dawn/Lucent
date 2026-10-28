#!/usr/bin/env bash
# Checks the release packaging scripts through what they produce:
# the PKGBUILDs (read back with `makepkg --printsrcinfo`) and the tarballs.
# Needs makepkg, so run it on Arch as a normal user: bash packaging/test.sh
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

failures=0
fail() {
  echo "FAIL: $*" >&2
  failures=$((failures + 1))
}

# expect_line FILE LINE: FILE contains LINE exactly (tabs stripped).
expect_line() {
  if ! sed 's/^\t//' "$1" | grep -qxF -- "$2"; then
    fail "$(basename "$(dirname "$1")")/$(basename "$1") lacks: $2"
  fi
}
reject_line() {
  if sed 's/^\t//' "$1" | grep -qxF -- "$2"; then
    fail "$(basename "$(dirname "$1")")/$(basename "$1") should not have: $2"
  fi
}

srcinfo_for() {
  local pkg="$1" sha="$2"
  mkdir -p "$work/$pkg"
  bash "$here/aur/generate-pkgbuild.sh" "$pkg" 9.8.7 "$sha" > "$work/$pkg/PKGBUILD"
  (cd "$work/$pkg" && makepkg --printsrcinfo > .SRCINFO)
}

src_sha=1111111111111111111111111111111111111111111111111111111111111111
bin_sha=2222222222222222222222222222222222222222222222222222222222222222

# --- lucent: built from the source tarball --------------------------------
srcinfo_for lucent "$src_sha"
s="$work/lucent/.SRCINFO"
expect_line "$s" "pkgbase = lucent"
expect_line "$s" "pkgname = lucent"
expect_line "$s" "pkgver = 9.8.7"
expect_line "$s" "pkgrel = 1"
expect_line "$s" "pkgdesc = Fast, beautiful notification daemon for Wayland with history, Do Not Disturb and themes"
expect_line "$s" "url = https://github.com/CPT-Dawn/Lucent"
expect_line "$s" "license = MIT"
expect_line "$s" "arch = x86_64"
expect_line "$s" "arch = aarch64"
expect_line "$s" "makedepends = cargo"
expect_line "$s" "depends = gtk4"
expect_line "$s" "depends = gtk4-layer-shell"
expect_line "$s" "conflicts = lucent-bin"
expect_line "$s" "source = lucent-9.8.7.tar.gz::https://github.com/CPT-Dawn/Lucent/releases/download/v9.8.7/lucent-9.8.7.tar.gz"
expect_line "$s" "sha256sums = $src_sha"
grep -qxF "# Maintainer: CPT-Dawn <dev@cptdawn.dev>" "$work/lucent/PKGBUILD" ||
  fail "lucent/PKGBUILD lacks the maintainer line"

# --- lucent-bin: the prebuilt x86_64 tarball --------------------------------
srcinfo_for lucent-bin "$bin_sha"
s="$work/lucent-bin/.SRCINFO"
expect_line "$s" "pkgbase = lucent-bin"
expect_line "$s" "pkgname = lucent-bin"
expect_line "$s" "pkgver = 9.8.7"
expect_line "$s" "pkgdesc = Fast, beautiful notification daemon for Wayland with history, Do Not Disturb and themes"
expect_line "$s" "arch = x86_64"
reject_line "$s" "arch = aarch64"
reject_line "$s" "makedepends = cargo"
expect_line "$s" "depends = gtk4"
expect_line "$s" "depends = gtk4-layer-shell"
expect_line "$s" "provides = lucent=9.8.7"
expect_line "$s" "conflicts = lucent"
expect_line "$s" "source = lucent-9.8.7-x86_64.tar.gz::https://github.com/CPT-Dawn/Lucent/releases/download/v9.8.7/lucent-9.8.7-x86_64.tar.gz"
expect_line "$s" "sha256sums = $bin_sha"
grep -qxF "# Maintainer: CPT-Dawn <dev@cptdawn.dev>" "$work/lucent-bin/PKGBUILD" ||
  fail "lucent-bin/PKGBUILD lacks the maintainer line"

# --- the generator refuses bad input ----------------------------------------
for args in "lucent-git 1.0.0 $src_sha" "lucent v1.0.0 $src_sha" "lucent 1.0.0 nothex" "lucent 1.0.0"; do
  # shellcheck disable=SC2086
  if bash "$here/aur/generate-pkgbuild.sh" $args > /dev/null 2>&1; then
    fail "generator accepted: $args"
  fi
done

# --- release tarballs ---------------------------------------------------------
repo="$(cd "$here/.." && pwd)"
printf '#!/bin/sh\n' > "$work/fake-lucent"
bash "$here/release-tarballs.sh" 9.8.7 "$work/fake-lucent" "$work/dist" > /dev/null

bin_list="$(tar -tzf "$work/dist/lucent-9.8.7-x86_64.tar.gz" | LC_ALL=C sort)"
expected_bin="lucent-9.8.7-x86_64/
lucent-9.8.7-x86_64/LICENSE
lucent-9.8.7-x86_64/README.md
lucent-9.8.7-x86_64/lucent
lucent-9.8.7-x86_64/org.freedesktop.Notifications.service"
[[ "$bin_list" == "$expected_bin" ]] ||
  fail "binary tarball contents:"$'\n'"$bin_list"
tar -xzf "$work/dist/lucent-9.8.7-x86_64.tar.gz" -C "$work" lucent-9.8.7-x86_64/lucent
[[ -x "$work/lucent-9.8.7-x86_64/lucent" ]] || fail "binary in tarball is not executable"

src_list="$(tar -tzf "$work/dist/lucent-9.8.7.tar.gz")"
for f in Cargo.toml Cargo.lock LICENSE README.md org.freedesktop.Notifications.service src/main.rs; do
  grep -qxF "lucent-9.8.7/$f" <<< "$src_list" || fail "source tarball lacks $f"
done
if grep -q '^lucent-9.8.7/\(target\|\.deps\|\.cargo\)/' <<< "$src_list"; then
  fail "source tarball contains build or machine-local files"
fi

(cd "$work/dist" && sha256sum -c --quiet SHA256SUMS) || fail "SHA256SUMS does not verify"

if ((failures)); then
  echo "$failures check(s) failed" >&2
  exit 1
fi
echo "packaging: all checks passed"
