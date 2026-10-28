#!/usr/bin/env bash
# Writes PKGBUILD and .SRCINFO for both AUR packages into <outdir>/<pkgname>/,
# with checksums taken from the release assets in <dist> (see
# ../release-tarballs.sh). Needs makepkg; must run as a normal user.
#
#   prepare.sh <version> <dist> <outdir> [pkgrel]
set -euo pipefail

[[ $# -eq 3 || $# -eq 4 ]] || { echo "usage: $0 <version> <dist> <outdir> [pkgrel]" >&2; exit 2; }
version="$1" dist="$2" outdir="$3" pkgrel="${4:-1}"
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

declare -A asset=(
  [lucent]="lucent-${version}.tar.gz"
  [lucent-bin]="lucent-${version}-x86_64.tar.gz"
)

for pkg in lucent lucent-bin; do
  file="$dist/${asset[$pkg]}"
  [[ -f "$file" ]] || { echo "missing release asset: $file" >&2; exit 1; }
  sha="$(sha256sum "$file" | cut -d' ' -f1)"
  mkdir -p "$outdir/$pkg"
  bash "$here/generate-pkgbuild.sh" "$pkg" "$version" "$sha" "$pkgrel" > "$outdir/$pkg/PKGBUILD"
  (cd "$outdir/$pkg" && makepkg --printsrcinfo > .SRCINFO)
  echo "$outdir/$pkg: PKGBUILD, .SRCINFO"
done
