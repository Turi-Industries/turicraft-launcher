#!/usr/bin/env bash
# Captures de l'interface en mode aperçu (faux cœur, voir src/lib/preview.ts).
# Prérequis : `bun run dev` lancé (port 1420), WebKitGTK 4.1 et python-gobject.
#
# Usage : scripts/captures.sh <dossier de sortie> [largeur hauteur]
# Chaque état listé plus bas donne une image <nom>.png.
set -euo pipefail

OUT="${1:?dossier de sortie}"
W="${2:-960}"
H="${3:-640}"
mkdir -p "$OUT"

# Moteur du launcher sous Linux (WebKitGTK), dans une fenêtre hors écran.
# Chrome ne suffit pas : ce qui est juste dans Chrome peut ne pas l'être
# dans le launcher (skill launcher-ux).
HERE="$(cd "$(dirname "$0")" && pwd)"
export GDK_BACKEND=x11 WEBKIT_DISABLE_COMPOSITING_MODE=1

# nom|requête
STATES=(
  "jouer|vue=jouer"
  "jouer-sans-compte|vue=jouer&compte=0"
  "jouer-lien|vue=jouer&compte=0&etat=lien"
  "jouer-code|vue=jouer&compte=0&etat=code"
  "jouer-preparation|vue=jouer&etat=prep"
  "jouer-lancement|vue=jouer&etat=lancement"
  "jouer-en-jeu|vue=jouer&etat=jeu"
  "jouer-crash|vue=jouer&etat=crash"
  "jouer-pilote|vue=jouer&etat=pilote"
  "jouer-integree|vue=jouer&etat=integree"
  "jouer-mods|vue=jouer&etat=mods"
  "jouer-hors-ligne|vue=jouer&etat=hors-ligne"
  "jouer-pack-hs|vue=jouer&etat=pack-hs"
  "jouer-serveur-hs|vue=jouer&serveur=0&maj=1"
  "qualite|vue=qualite"
  "qualite-option-changee|vue=qualite&perso=1"
  "qualite-avance|vue=qualite&preset=personnalise"
  "options|vue=options"
  "options-mise-a-jour|vue=options&maj=1"
  "options-reparation|vue=options&etat=reparation"
  "options-repare|vue=options&etat=repare"
  "options-raz|vue=options&etat=raz"
  "options-raz-fait|vue=options&etat=raz-fait"
  "mise-a-jour-fenetre|vue=jouer&maj=1&popup=1"
  "journal|vue=journal"
  "journal-lignes|vue=journal&journal=1"
  "jouer-dossiers|vue=jouer&dossiers=1"
  "jouer-3d-35|vue=jouer&angle=35"
  "jouer-3d-80|vue=jouer&angle=80"
  "jouer-3d-160|vue=jouer&angle=160"
  "jouer-snake-classement-hs|vue=jouer&etat=lancement&classement=0"
)

for s in "${STATES[@]}"; do
  name="${s%%|*}"
  query="${s#*|}"
  python3 "$HERE/capture-webkit.py" "http://localhost:1420/?$query" "$OUT/$name.png" "$W" "$H" >/dev/null 2>&1
  echo "$OUT/$name.png"
done
