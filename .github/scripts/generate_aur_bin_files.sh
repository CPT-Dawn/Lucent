#!/usr/bin/env bash
# Writes the PKGBUILD for lucent-bin, which installs the prebuilt release
# tarballs from package_release.sh. .SRCINFO is left to
# `makepkg --printsrcinfo` so it can never drift from the PKGBUILD.
#
# Checksums come from the published assets themselves, so the package pins
# exactly what users download. ASSET_DIR points at local tarballs instead
# (e.g. to test the PKGBUILD before tagging).
set -euo pipefail

# RELEASE_TAG wins: on a manual run the runner forces GITHUB_REF_NAME to the
# branch the workflow was dispatched from, whatever the step's env says.
release_tag="${RELEASE_TAG:-${GITHUB_REF_NAME:-}}"
if [[ -z "$release_tag" ]]; then
  echo "RELEASE_TAG or GITHUB_REF_NAME is required (example: v1.0.0)." >&2
  exit 1
fi

if [[ "$release_tag" != v* ]]; then
  echo "Release tag must start with 'v' (received: $release_tag)." >&2
  exit 1
fi

pkgver="${release_tag#v}"
pkgrel="${PKGREL:-1}"
maintainer="${AUR_MAINTAINER:-CPT-Dawn <ping@cptdawn.dev>}"
repo="${GITHUB_REPOSITORY:-CPT-Dawn/Lucent}"
server_url="${GITHUB_SERVER_URL:-https://github.com}"
arches=(x86_64 aarch64)

declare -A sums
for arch in "${arches[@]}"; do
  asset="lucent-${pkgver}-${arch}.tar.gz"
  if [[ -n "${ASSET_DIR:-}" ]]; then
    sums[$arch]="$(sha256sum "$ASSET_DIR/$asset" | awk '{print $1}')"
  else
    tmp="$(mktemp)"
    curl -fsSL --retry 3 \
      "${server_url}/${repo}/releases/download/${release_tag}/${asset}" -o "$tmp"
    sums[$arch]="$(sha256sum "$tmp" | awk '{print $1}')"
    rm -f "$tmp"
  fi
done

# depends mirrors the binary's NEEDED libraries, same as the source package.
cat > PKGBUILD <<EOF
# Maintainer: ${maintainer}
pkgname=lucent-bin
_pkgname=lucent
pkgver=${pkgver}
pkgrel=${pkgrel}
pkgdesc="The lightest notification daemon for Wayland (prebuilt binary)"
arch=('x86_64' 'aarch64')
url="${server_url}/${repo}"
license=('MIT')
depends=('cairo' 'pango' 'glib2' 'dbus' 'libgcc' 'glibc')
provides=("\${_pkgname}=\${pkgver}")
conflicts=("\${_pkgname}")
# Upstream already strips the binary: ship it byte-for-byte, with no empty
# debug package.
options=('!strip' '!debug')
source_x86_64=("\${url}/releases/download/v\${pkgver}/\${_pkgname}-\${pkgver}-x86_64.tar.gz")
source_aarch64=("\${url}/releases/download/v\${pkgver}/\${_pkgname}-\${pkgver}-aarch64.tar.gz")
sha256sums_x86_64=('${sums[x86_64]}')
sha256sums_aarch64=('${sums[aarch64]}')

package() {
  cd "\${_pkgname}-\${pkgver}-\${CARCH}"

  install -Dm755 lucent "\${pkgdir}/usr/bin/lucent"
  install -Dm644 dev.cptdawn.Lucent.service \\
    "\${pkgdir}/usr/share/dbus-1/services/dev.cptdawn.Lucent.service"

  install -Dm644 LICENSE "\${pkgdir}/usr/share/licenses/\${pkgname}/LICENSE"
  install -Dm644 README.md "\${pkgdir}/usr/share/doc/\${_pkgname}/README.md"
  install -Dm644 config.toml "\${pkgdir}/usr/share/doc/\${_pkgname}/config.toml"
}
EOF
