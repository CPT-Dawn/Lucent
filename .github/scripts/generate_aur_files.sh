#!/usr/bin/env bash
set -euo pipefail

if [[ -z "${GITHUB_REF_NAME:-}" ]]; then
  echo "GITHUB_REF_NAME is required (example: v1.0.0)." >&2
  exit 1
fi

release_tag="${GITHUB_REF_NAME}"
if [[ "$release_tag" != v* ]]; then
  echo "Release tag must start with 'v' (received: $release_tag)." >&2
  exit 1
fi

pkgname="lucent"
_reponame="Lucent"
pkgver="${release_tag#v}"
pkgrel="${PKGREL:-1}"
maintainer="${AUR_MAINTAINER:-CPT-Dawn <dev@cptdawn.dev>}"
repo="${GITHUB_REPOSITORY:-CPT-Dawn/Lucent}"
server_url="${GITHUB_SERVER_URL:-https://github.com}"

tarball_url="${server_url}/${repo}/archive/refs/tags/${release_tag}.tar.gz"

# SOURCE_TARBALL lets a local run checksum a tarball already on disk instead
# of downloading the release (e.g. to test the PKGBUILD before tagging).
if [[ -n "${SOURCE_TARBALL:-}" ]]; then
  sha256="$(sha256sum "$SOURCE_TARBALL" | awk '{print $1}')"
else
  tmp_tarball="$(mktemp)"
  trap 'rm -f "$tmp_tarball"' EXIT
  curl -fsSL "$tarball_url" -o "$tmp_tarball"
  sha256="$(sha256sum "$tmp_tarball" | awk '{print $1}')"
fi

pkgdesc="The lightest notification daemon for Wayland"

# depends mirrors the binary's NEEDED libraries. No wayland: wayland-client is
# pure Rust here and never links libwayland-client. No libxkbcommon: sctk is
# built without its keyboard feature.

cat > PKGBUILD <<EOF
# Maintainer: ${maintainer}
pkgname=${pkgname}
_reponame=${_reponame}
pkgver=${pkgver}
pkgrel=${pkgrel}
pkgdesc="${pkgdesc}"
arch=('x86_64' 'aarch64')
url="${server_url}/${repo}"
license=('MIT')
depends=('cairo' 'pango' 'glib2' 'dbus' 'libgcc' 'glibc')
makedepends=('cargo')
provides=("lucent")
source=("${tarball_url}")
sha256sums=('${sha256}')

build() {
  cd "\${_reponame}-\${pkgver}"
  export CARGO_TARGET_DIR="target"
  # Packed relocations: about 6% off the binary and no startup relocation
  # pass. Arch's glibc is new enough; generic source builds leave it out.
  export RUSTFLAGS="\${RUSTFLAGS:-} -C link-arg=-Wl,-z,pack-relative-relocs"
  cargo build --release --locked
}

package() {
  cd "\${_reponame}-\${pkgver}"

  install -Dm755 "target/release/lucent" "\${pkgdir}/usr/bin/lucent"

  install -Dm644 \\
    "dev.cptdawn.Lucent.service" \\
    "\${pkgdir}/usr/share/dbus-1/services/dev.cptdawn.Lucent.service"

  install -Dm644 LICENSE "\${pkgdir}/usr/share/licenses/\${pkgname}/LICENSE"
  install -Dm644 README.md "\${pkgdir}/usr/share/doc/\${pkgname}/README.md"
  install -Dm644 default_config.toml "\${pkgdir}/usr/share/doc/\${pkgname}/config.toml"
}
EOF

cat > .SRCINFO <<EOF
pkgbase = ${pkgname}
	pkgdesc = ${pkgdesc}
	pkgver = ${pkgver}
	pkgrel = ${pkgrel}
	url = ${server_url}/${repo}
	arch = x86_64
	arch = aarch64
	license = MIT
	makedepends = cargo
	depends = cairo
	depends = pango
	depends = glib2
	depends = dbus
	depends = libgcc
	depends = glibc
	provides = lucent
	source = ${tarball_url}
	sha256sums = ${sha256}

pkgname = ${pkgname}
EOF