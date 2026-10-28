#!/usr/bin/env bash
# Builds the release assets into <outdir>:
#   lucent-<version>.tar.gz          source (git archive of HEAD), for `lucent`
#   lucent-<version>-x86_64.tar.gz   prebuilt binary + D-Bus activation file,
#                                    licence and README, for `lucent-bin`
#   SHA256SUMS
#
#   release-tarballs.sh <version> <path/to/lucent-binary> <outdir>
set -euo pipefail

[[ $# -eq 3 ]] || { echo "usage: $0 <version> <binary> <outdir>" >&2; exit 2; }
version="$1" binary="$2" outdir="$3"

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
mkdir -p "$outdir"
outdir="$(cd "$outdir" && pwd)"

src="lucent-${version}.tar.gz"
git -C "$repo" archive --format=tar.gz --prefix="lucent-${version}/" -o "$outdir/$src" HEAD

bin="lucent-${version}-x86_64"
stage="$(mktemp -d)"
trap 'rm -rf "$stage"' EXIT
mkdir "$stage/$bin"
install -m755 "$binary" "$stage/$bin/lucent"
install -m644 -t "$stage/$bin" \
  "$repo/org.freedesktop.Notifications.service" "$repo/LICENSE" "$repo/README.md"
tar -C "$stage" --sort=name --owner=0 --group=0 --numeric-owner \
  -czf "$outdir/$bin.tar.gz" "$bin"

(cd "$outdir" && sha256sum "$src" "$bin.tar.gz" > SHA256SUMS)
cat "$outdir/SHA256SUMS"
