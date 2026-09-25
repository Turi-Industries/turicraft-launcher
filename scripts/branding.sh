#!/usr/bin/env bash
# Pose le logo Turi Craft à la place de l'icône provisoire (le « T » jaune).
#
# Le logo n'est pas dans ce dépôt : il n'est pas sous GPL (voir README,
# « Licence »). La CI le télécharge depuis TURI_LOGO_URL (variable du dépôt) ;
# sans elle, un fork construit avec le « T » et rien ne casse.
#
# Usage : scripts/branding.sh [chemin-ou-url]    (défaut : $TURI_LOGO_URL)
# Écrit src-tauri/icons/ (toutes les tailles), src/lib/assets/branding/
# (logo de la barre latérale), src-tauri/branding/ (images des installeurs)
# et src-tauri/tauri.branding.conf.json — ces trois-là ignorés par git.
# En local, revenir au « T » :
#   git checkout src-tauri/icons && rm -rf src/lib/assets/branding src-tauri/branding src-tauri/tauri.branding.conf.json
set -euo pipefail

SRC="${1:-${TURI_LOGO_URL:-}}"
if [[ -z "$SRC" ]]; then
	echo "branding : pas de logo (TURI_LOGO_URL vide), icône provisoire gardée"
	exit 0
fi

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

if [[ "$SRC" == http://* || "$SRC" == https://* ]]; then
	# --retry-all-errors : le 25/09, un runner macOS n'a pas résolu le nom
	# pendant une douzaine de secondes (curl 6), les autres si.
	curl -fsSL --retry 6 --retry-delay 10 --retry-all-errors -o "$TMP/logo.png" "$SRC"
else
	cp "$SRC" "$TMP/logo.png"
fi

# Un PNG commence par \x89PNG : une page d'erreur HTML servie en 200 ne doit
# pas devenir l'icône.
[[ "$(head -c 4 "$TMP/logo.png" | tail -c 3)" == "PNG" ]] || { echo "branding : $SRC n'est pas un PNG" >&2; exit 1; }

if ! out="$(bun tauri icon "$TMP/logo.png" -o "$TMP/icons" 2>&1)"; then
	echo "$out" >&2
	exit 1
fi
# Seules les tailles de bureau : pas de version Android ni iOS.
rm -rf "$TMP/icons/android" "$TMP/icons/ios"
cp "$TMP"/icons/* src-tauri/icons/

mkdir -p src/lib/assets/branding
cp "$TMP/icons/128x128@2x.png" src/lib/assets/branding/logo.png

# Images des installeurs (Windows : accueil, en-tête ; macOS : fenêtre du
# .dmg), et la config qui les déclare — à passer à `tauri build --config`.
# Pillow : $PYTHON (CI : actions/setup-python), sinon celui du système.
PY="${PYTHON:-$(command -v python || command -v python3)}"
"$PY" -m pip install --quiet pillow 2>/dev/null \
	|| "$PY" -m pip install --quiet --user --break-system-packages pillow
"$PY" scripts/installer-images.py "$TMP/logo.png"
cat > src-tauri/tauri.branding.conf.json <<'JSON'
{
  "bundle": {
    "windows": {
      "nsis": {
        "sidebarImage": "branding/sidebar.bmp",
        "headerImage": "branding/header.bmp",
        "uninstallerHeaderImage": "branding/header.bmp"
      }
    },
    "macOS": {
      "dmg": {
        "background": "branding/dmg.png",
        "windowSize": { "width": 660, "height": 400 },
        "appPosition": { "x": 180, "y": 200 },
        "applicationFolderPosition": { "x": 480, "y": 200 }
      }
    }
  }
}
JSON
# En CI : la construction la prend en plus de tauri.conf.json.
if [[ -n "${GITHUB_ENV:-}" ]]; then
	echo "TURI_BRANDING_ARGS=--config src-tauri/tauri.branding.conf.json" >> "$GITHUB_ENV"
fi
echo "branding : logo posé depuis $SRC"
