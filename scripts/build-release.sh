#!/usr/bin/env bash
# Build release artifacts for the current host OS into dist/:
#
#   macOS  → dist/macos-<arch>/rs-nes.app  +  dist/macos-<arch>/rs-nes.dmg
#   Linux  → dist/linux-<arch>/rs-nes           (portable single binary)
#   Windows→ dist/windows-<arch>/rs-nes.exe     (portable single binary)
#
# Cross-OS artifacts are produced by CI (.github/workflows/release.yml).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
VERSION="${VERSION:-0.1.0}"

cargo build --release --locked 2>/dev/null || cargo build --release

OS="$(uname -s)"
ARCH="$(uname -m)"
case "$OS" in
  Darwin)  PLATFORM="macos" ;;
  Linux)   PLATFORM="linux" ;;
  MINGW*|MSYS*|CYGWIN*) PLATFORM="windows" ;;
  *)       PLATFORM="$(echo "$OS" | tr '[:upper:]' '[:lower:]')" ;;
esac
case "$ARCH" in
  arm64|aarch64) ARCH_TAG="aarch64" ;;
  x86_64|amd64)  ARCH_TAG="x86_64" ;;
  *)             ARCH_TAG="$ARCH" ;;
esac

OUT_DIR="$ROOT/dist/${PLATFORM}-${ARCH_TAG}"
rm -rf "$OUT_DIR"
mkdir -p "$OUT_DIR"

pack_macos_app_dmg() {
  local bin="$ROOT/target/release/rs-nes"
  local app="$OUT_DIR/rs-nes.app"
  local dmg="$OUT_DIR/rs-nes-${VERSION}-macos-${ARCH_TAG}.dmg"
  local staging
  staging="$(mktemp -d)"

  mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
  cp "$bin" "$app/Contents/MacOS/rs-nes"
  chmod +x "$app/Contents/MacOS/rs-nes"
  sed "s/0\\.1\\.0/${VERSION}/g" "$ROOT/scripts/Info.plist" > "$app/Contents/Info.plist"

  # Ad-hoc sign so Gatekeeper is less noisy for local installs.
  if command -v codesign >/dev/null 2>&1; then
    codesign --force --deep --sign - "$app" 2>/dev/null || true
  fi
  if command -v xattr >/dev/null 2>&1; then
    xattr -cr "$app" 2>/dev/null || true
  fi

  mkdir -p "$staging"
  cp -R "$app" "$staging/"
  ln -s /Applications "$staging/Applications"

  rm -f "$dmg"
  hdiutil create \
    -volname "rs-nes" \
    -srcfolder "$staging" \
    -ov -format UDZO \
    "$dmg"
  rm -rf "$staging"

  echo "macOS app: $app"
  echo "macOS DMG: $dmg"
  ls -lh "$app/Contents/MacOS/rs-nes" "$dmg"
}

pack_portable() {
  if [[ "$PLATFORM" == "windows" ]]; then
    cp "$ROOT/target/release/rs-nes.exe" "$OUT_DIR/rs-nes.exe"
    echo "Portable: $OUT_DIR/rs-nes.exe"
    ls -lh "$OUT_DIR/rs-nes.exe"
  else
    cp "$ROOT/target/release/rs-nes" "$OUT_DIR/rs-nes"
    chmod +x "$OUT_DIR/rs-nes"
    echo "Portable: $OUT_DIR/rs-nes"
    ls -lh "$OUT_DIR/rs-nes"
  fi
}

case "$PLATFORM" in
  macos) pack_macos_app_dmg ;;
  *)     pack_portable ;;
esac
