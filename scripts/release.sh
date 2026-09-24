#!/usr/bin/env bash
# Publie une version du launcher : version à ses trois endroits, commit, tag
# annoté (ses lignes deviennent les notes affichées aux joueurs), push. La CI
# (.github/workflows/ci.yml) construit, signe et publie une version GitHub ;
# le serveur du pack la recopie, et les launchers installés proposent
# « Mettre à jour ».
#
# Usage : scripts/release.sh 0.2.0 "Connexion par lien, réglages selon l'écran."
set -euo pipefail

VERSION="${1:?version, ex. 0.2.0}"
NOTES="${2:?notes pour les joueurs, entre guillemets}"
[[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || { echo "version X.Y.Z attendue" >&2; exit 1; }

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
[[ -z "$(git status --porcelain)" ]] || { echo "des modifications ne sont pas commitées" >&2; exit 1; }
git rev-parse "v$VERSION" >/dev/null 2>&1 && { echo "v$VERSION existe déjà" >&2; exit 1; }

python3 - "$VERSION" <<'PY'
import json, re, sys
v = sys.argv[1]
p = "src-tauri/tauri.conf.json"
c = json.load(open(p)); c["version"] = v
open(p, "w").write(json.dumps(c, ensure_ascii=False, indent=2) + "\n")
p = "package.json"
c = json.load(open(p)); c["version"] = v
open(p, "w").write(json.dumps(c, ensure_ascii=False, indent="\t") + "\n")
p = "src-tauri/Cargo.toml"
s = open(p).read()
s = re.sub(r'(?m)^version = "[^"]+"', f'version = "{v}"', s, count=1)
open(p, "w").write(s)
PY
(cd src-tauri && cargo update -q -p turicraft-launcher --offline 2>/dev/null || cargo generate-lockfile -q)

git add src-tauri/tauri.conf.json package.json src-tauri/Cargo.toml src-tauri/Cargo.lock
git commit -q -m "version $VERSION"
git tag -a "v$VERSION" -m "$NOTES"
git push -q origin HEAD "v$VERSION"
echo "v$VERSION poussé : la CI construit et publie (Actions → CI)."
