#!/usr/bin/env bash
set -euo pipefail

ARCH="${1:-amd64}"
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEFAULT_VER="$(grep -m1 '^version = ' "${ROOT_DIR}/Cargo.toml" | sed -E 's/version = "([^"]+)"/\1/')"
VERSION="${2:-${DEFAULT_VER:-0.1.4}}"
DIST_DIR="${ROOT_DIR}/dist"
STAGE_DIR="${DIST_DIR}/stage-linux-${ARCH}"

mkdir -p "${DIST_DIR}"
rm -rf "${STAGE_DIR}"

GUI_BIN="${ROOT_DIR}/target/release/student-journal"
MCP_BIN="${ROOT_DIR}/target/release/student-journal-mcp"

if [[ ! -f "${GUI_BIN}" || ! -f "${MCP_BIN}" ]]; then
  echo "Release binaries missing. Run: cargo build --release --workspace" >&2
  exit 1
fi

# 1. Portable tar.gz
PKG_NAME="Oxidium-${VERSION}-linux-${ARCH}"
mkdir -p "${STAGE_DIR}/${PKG_NAME}"
cp "${GUI_BIN}" "${MCP_BIN}" "${STAGE_DIR}/${PKG_NAME}/"
cp "${ROOT_DIR}/LICENSE.txt" "${ROOT_DIR}/THIRD_PARTY_NOTICES.md" "${STAGE_DIR}/${PKG_NAME}/"
cp "${ROOT_DIR}/docs/mcp/QUICKSTART.md" "${STAGE_DIR}/${PKG_NAME}/MCP-QUICKSTART.md"
cp "${ROOT_DIR}/CHANGELOG.md" "${STAGE_DIR}/${PKG_NAME}/RELEASE-NOTES.md"
cp "${ROOT_DIR}/scripts/linux/oxidium.desktop" "${STAGE_DIR}/${PKG_NAME}/"
cp "${ROOT_DIR}/assets/icon/web/icon-512.png" "${STAGE_DIR}/${PKG_NAME}/oxidium.png"

tar -C "${STAGE_DIR}" -czf "${DIST_DIR}/${PKG_NAME}.tar.gz" "${PKG_NAME}"
echo "Created ${DIST_DIR}/${PKG_NAME}.tar.gz"

# 2. Debian package (.deb) if dpkg-deb is available
if command -v dpkg-deb >/dev/null 2>&1; then
  DEB_ROOT="${STAGE_DIR}/deb"
  mkdir -p "${DEB_ROOT}/DEBIAN" \
           "${DEB_ROOT}/usr/bin" \
           "${DEB_ROOT}/usr/share/applications" \
           "${DEB_ROOT}/usr/share/icons/hicolor/512x512/apps" \
           "${DEB_ROOT}/usr/share/doc/oxidium"

  install -m 0755 "${GUI_BIN}" "${DEB_ROOT}/usr/bin/student-journal"
  install -m 0755 "${MCP_BIN}" "${DEB_ROOT}/usr/bin/student-journal-mcp"
  install -m 0644 "${ROOT_DIR}/scripts/linux/oxidium.desktop" "${DEB_ROOT}/usr/share/applications/oxidium.desktop"
  install -m 0644 "${ROOT_DIR}/assets/icon/web/icon-512.png" "${DEB_ROOT}/usr/share/icons/hicolor/512x512/apps/oxidium.png"
  install -m 0644 "${ROOT_DIR}/LICENSE.txt" "${DEB_ROOT}/usr/share/doc/oxidium/copyright"
  install -m 0644 "${ROOT_DIR}/THIRD_PARTY_NOTICES.md" "${DEB_ROOT}/usr/share/doc/oxidium/THIRD_PARTY_NOTICES.md"
  install -m 0644 "${ROOT_DIR}/docs/mcp/QUICKSTART.md" "${DEB_ROOT}/usr/share/doc/oxidium/MCP-QUICKSTART.md"

  cat > "${DEB_ROOT}/DEBIAN/control" <<EOF
Package: oxidium
Version: ${VERSION}
Section: education
Priority: optional
Architecture: ${ARCH}
Maintainer: Oxidium Contributors <https://github.com/cryals/Oxidium>
Depends: libc6, libfontconfig1, libfreetype6, libxkbcommon0, libssl3t64 | libssl3
Description: Oxidium — Personal Student Journal with built-in MCP server
 Local-first, offline-first personal academic journal, analytics dashboard,
 Kanban task organizer, and Model Context Protocol (MCP) server built with
 Rust, Slint Material Design 3 Expressive, and SQLite.
EOF

  dpkg-deb --build --root-owner-group "${DEB_ROOT}" "${DIST_DIR}/oxidium_${VERSION}_${ARCH}.deb"
  echo "Created ${DIST_DIR}/oxidium_${VERSION}_${ARCH}.deb"
fi

# 3. SHA-256 Checksums
(cd "${DIST_DIR}" && sha256sum Oxidium-*.tar.gz oxidium_*.deb 2>/dev/null > SHA256SUMS-linux.txt || true)
rm -rf "${STAGE_DIR}"
