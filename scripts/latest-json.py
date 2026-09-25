#!/usr/bin/env python3
"""Écrit latest.json, le fichier que lit tauri-plugin-updater, à partir des
installeurs construits par la CI et de leurs signatures (.sig).

Usage : latest-json.py <dossier des fichiers> <version> <notes> > latest.json

Les adresses pointent vers le serveur du pack (LAUNCHER_BASE_URL), qui
recopie chaque version publiée sur GitHub.
"""
import datetime
import json
import os
import sys
import urllib.parse

BASE = os.environ.get("LAUNCHER_BASE_URL", "https://pack.turi-industries.eu/launcher/")

# plateforme de l'updater → fin du nom du fichier de mise à jour
# Un seul installeur Windows (x64 + ARM64) et une seule app Mac (universelle) :
# les deux plateformes de chacun pointent vers le même fichier. Un launcher x64
# installé sur un PC ARM redevient ainsi natif à sa prochaine mise à jour.
PLATFORMS = {
    "linux-x86_64": "_amd64.AppImage",
    "windows-x86_64": "_windows-setup.exe",
    "windows-aarch64": "_windows-setup.exe",
    "darwin-aarch64": "_universal.app.tar.gz",
    "darwin-x86_64": "_universal.app.tar.gz",
}


def main() -> None:
    folder, version, notes = sys.argv[1], sys.argv[2], sys.argv[3]
    files = os.listdir(folder)
    platforms = {}
    for platform, suffix in PLATFORMS.items():
        match = [f for f in files if f.endswith(suffix)]
        if not match:
            print(f"absent : {platform} (*{suffix})", file=sys.stderr)
            continue
        name = match[0]
        sig = os.path.join(folder, name + ".sig")
        if not os.path.exists(sig):
            sys.exit(f"signature manquante : {name}.sig (TAURI_SIGNING_PRIVATE_KEY ?)")
        platforms[platform] = {
            "signature": open(sig).read().strip(),
            "url": BASE + urllib.parse.quote(name),
        }
    if not platforms:
        sys.exit("aucun fichier de mise à jour trouvé")
    json.dump(
        {
            "version": version,
            "notes": notes,
            "pub_date": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
            "platforms": platforms,
        },
        sys.stdout,
        ensure_ascii=False,
        indent=2,
    )


if __name__ == "__main__":
    main()
