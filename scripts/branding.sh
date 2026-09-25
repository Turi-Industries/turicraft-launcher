#!/usr/bin/env bash
# Pose le logo Turi Craft à la place de l'icône provisoire (le « T » jaune).
#
# Le logo n'est pas dans ce dépôt : il n'est pas sous GPL (voir README,
# « Licence »). La CI le télécharge depuis TURI_LOGO_URL (variable du dépôt) ;
# sans elle, un fork construit avec le « T » et rien ne casse.
#
# Usage : scripts/branding.sh [chemin-ou-url]    (défaut : $TURI_LOGO_URL)
# Écrit src-tauri/icons/ (toutes les tailles) et src/lib/assets/branding/
# (logo de la barre latérale, ignoré par git). En local, revenir au « T » :
#   git checkout src-tauri/icons && rm -rf src/lib/assets/branding
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
echo "branding : logo posé depuis $SRC"
