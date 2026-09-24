#!/usr/bin/env bash
# Retire de l'AppImage les libwayland embarquées, puis la reconstruit.
#
# L'AppImage est construite sur Ubuntu 22.04 et embarque ses libwayland-*.
# Sur une distribution récente (Arch, CachyOS, Fedora…), le libEGL de Mesa
# du SYSTÈME les charge à la place des siennes, plus récentes : WebKit
# s'arrête sur « Could not create default EGL display: EGL_BAD_PARAMETER »,
# et la fenêtre reste grise. Toute distribution de bureau fournit libwayland :
# on laisse celle du système.
#
# Usage : scripts/fix-appimage.sh <fichier.AppImage>
# Avec TAURI_SIGNING_PRIVATE_KEY, réécrit aussi le .sig de la mise à jour.
set -euo pipefail

APPIMAGE="$(realpath "${1:?chemin de l’AppImage}")"
TOOL_VERSION=1.9.1
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
export APPIMAGE_EXTRACT_AND_RUN=1   # pas de FUSE sur les machines de CI

cd "$WORK"
curl -fsSL -o appimagetool "https://github.com/AppImage/appimagetool/releases/download/$TOOL_VERSION/appimagetool-x86_64.AppImage"
chmod +x appimagetool
"$APPIMAGE" --appimage-extract >/dev/null

removed=$(find squashfs-root -name 'libwayland-*.so*' -print -delete)
if [[ -z "$removed" ]]; then
  echo "aucune libwayland embarquée : rien à faire"
  exit 0
fi
echo "retirées :"; echo "$removed" | sed 's|^squashfs-root/|  |'

ARCH=x86_64 ./appimagetool --no-appstream squashfs-root out.AppImage >/dev/null
mv out.AppImage "$APPIMAGE"
chmod +x "$APPIMAGE"

if [[ -n "${TAURI_SIGNING_PRIVATE_KEY:-}" ]]; then
  cd - >/dev/null
  bunx tauri signer sign "$APPIMAGE" >/dev/null
  echo "signature réécrite : $(basename "$APPIMAGE").sig"
fi
