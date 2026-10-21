#!/usr/bin/env bash
# Regenerate desktop icons from logo.png → assets/AppIcon.icns + assets/app.ico
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

if ! command -v magick >/dev/null 2>&1; then
  echo "ImageMagick (magick) is required" >&2
  exit 1
fi

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

# Pad non-square logo to a square (black, matching logo background).
magick logo.png -background '#000000' -gravity center \
  -extent '%[fx:max(w,h)]x%[fx:max(w,h)]' "$TMP/icon-square.png"

# macOS .icns (point filter keeps pixel-art sharp)
ICONSET="$TMP/AppIcon.iconset"
mkdir -p "$ICONSET"
for size in 16 32 128 256 512; do
  magick "$TMP/icon-square.png" -filter point -resize "${size}x${size}" \
    "$ICONSET/icon_${size}x${size}.png"
  magick "$TMP/icon-square.png" -filter point -resize "$((size * 2))x$((size * 2))" \
    "$ICONSET/icon_${size}x${size}@2x.png"
done

if ! command -v iconutil >/dev/null 2>&1; then
  echo "iconutil (macOS) is required to build AppIcon.icns" >&2
  exit 1
fi
iconutil -c icns "$ICONSET" -o assets/AppIcon.icns

# Windows .ico (multi-resolution)
magick "$TMP/icon-square.png" -filter point \
  -define icon:auto-resize=256,128,64,48,32,16 assets/app.ico

echo "Wrote assets/AppIcon.icns and assets/app.ico"
ls -lh assets/AppIcon.icns assets/app.ico
