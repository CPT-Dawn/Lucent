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
maintainer="${AUR_MAINTAINER:-CPT-Dawn <dawnsp0456@gmail.com>}"
repo="${GITHUB_REPOSITORY:-CPT-Dawn/Lucent}"
server_url="${GITHUB_SERVER_URL:-https://github.com}"

tarball_url="${server_url}/${repo}/archive/refs/tags/${release_tag}.tar.gz"

tmp_tarball="$(mktemp)"
trap 'rm -f "$tmp_tarball"' EXIT

curl -fsSL "$tarball_url" -o "$tmp_tarball"
sha256="$(sha256sum "$tmp_tarball" | awk '{print $1}')"

cat > PKGBUILD <<EOF
# Maintainer: ${maintainer}
pkgname=${pkgname}
_reponame=${_reponame}
pkgver=${pkgver}
pkgrel=${pkgrel}
pkgdesc="A blazing-fast, D-Bus activated Wayland notification daemon"
arch=('x86_64' 'aarch64')
url="${server_url}/${repo}"
license=('MIT')
depends=('glib2' 'gtk4' 'gtk4-layer-shell' 'dbus')
makedepends=('cargo')
provides=("lucent")
conflicts=("lucent-git")
source=("${tarball_url}")
sha256sums=('${sha256}')

build() {
  cd "\${_reponame}-\${pkgver}"
  export CARGO_TARGET_DIR="target"
  cargo build --release --locked
}

package() {
  cd "\${_reponame}-\${pkgver}"

  install -Dm755 "target/release/lucent" "\${pkgdir}/usr/bin/lucent"
  
  install -Dm644 \\
    "org.freedesktop.Notifications.service" \\
    "\${pkgdir}/usr/share/dbus-1/services/org.freedesktop.Notifications.service"

  install -Dm644 LICENSE "\${pkgdir}/usr/share/licenses/\${pkgname}/LICENSE"
  install -Dm644 README.md "\${pkgdir}/usr/share/doc/\${pkgname}/README.md"
}
EOF

cat > .SRCINFO <<EOF
pkgbase = ${pkgname}
	pkgdesc = A blazing-fast, D-Bus activated Wayland notification daemon
	pkgver = ${pkgver}
	pkgrel = ${pkgrel}
	url = ${server_url}/${repo}
	arch = x86_64
	arch = aarch64
	license = MIT
	makedepends = cargo
	depends = glib2
	depends = gtk4
	depends = gtk4-layer-shell
	depends = dbus
	provides = lucent
	conflicts = lucent-git
	source = ${tarball_url}
	sha256sums = ${sha256}

pkgname = ${pkgname}
EOF