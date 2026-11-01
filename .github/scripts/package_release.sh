#!/usr/bin/env bash
# Packs a release build into lucent-<version>-<arch>.tar.gz plus a .sha256,
# the assets lucent-bin installs from.
#
# Usage: package_release.sh <tag> <arch> [binary] [outdir]
#
# The tarball is reproducible: sorted entries, fixed owner, mtimes pinned to
# the tagged commit and gzip without a timestamp. Rebuilding the same binary
# gives the same checksum.
set -euo pipefail

tag="${1:?usage: package_release.sh <tag> <arch> [binary] [outdir]}"
arch="${2:?usage: package_release.sh <tag> <arch> [binary] [outdir]}"
binary="${3:-target/release/lucent}"
outdir="${4:-dist}"

if [[ "$tag" != v* ]]; then
  echo "Release tag must start with 'v' (received: $tag)." >&2
  exit 1
fi
version="${tag#v}"

name="lucent-${version}-${arch}"
epoch="${SOURCE_DATE_EPOCH:-$(git log -1 --format=%ct)}"

stage="$(mktemp -d)"
trap 'rm -rf "$stage"' EXIT

install -Dm755 "$binary" "$stage/$name/lucent"
install -Dm644 dev.cptdawn.Lucent.service "$stage/$name/dev.cptdawn.Lucent.service"
install -Dm644 default_config.toml "$stage/$name/config.toml"
install -Dm644 LICENSE "$stage/$name/LICENSE"
install -Dm644 README.md "$stage/$name/README.md"

mkdir -p "$outdir"
tar --sort=name --mtime="@${epoch}" --owner=0 --group=0 --numeric-owner \
  -C "$stage" -cf - "$name" | gzip -9n > "$outdir/$name.tar.gz"

(cd "$outdir" && sha256sum "$name.tar.gz" > "$name.tar.gz.sha256")
cat "$outdir/$name.tar.gz.sha256"
