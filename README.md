# Launcher Turi Craft

Le launcher du modpack **Turi Craft V2** (Minecraft 1.21.1, NeoForge). Un bouton « Jouer » : il installe Java, Minecraft,
NeoForge et les mods du pack, choisit les réglages selon la machine, connecte
le compte Microsoft et lance le jeu.

Tauri 2 (Rust) + SvelteKit (Svelte 5) + bun. Windows (x64 et ARM64), macOS
(Apple et Intel), Linux (AppImage, .deb).

## Ce qu'il fait

- **Installation complète** : Java de Mojang, jeu vanilla, NeoForge, puis le
  pack par [packwiz-installer](https://github.com/packwiz/packwiz-installer)
  — seuls les fichiers modifiés sont retéléchargés.
- **Compte Microsoft** : connexion par le navigateur (retour automatique),
  code d'appareil en secours. Le jeton de renouvellement va dans le trousseau
  du système.
- **Qualité selon la machine** : mémoire, processeur, carte graphique et
  écran principal (fréquence, FreeSync / G-Sync) choisissent un préréglage,
  des mods optionnels et les options du jeu. Ce que le joueur change en jeu
  est conservé. Préréglages et règles sont décrits dans `turicraft/presets.toml`,
  servi avec le pack : les modifier ne demande pas de nouvelle version du
  launcher. Exemple complet : [`src-tauri/fixtures/presets.toml`](src-tauri/fixtures/presets.toml).
- **Au lancement** : jalons et temps restant, jeu en fenêtré pendant le
  chargement puis en plein écran, bouton Arrêter, diagnostic en cas de crash.
- **Accueil** : état du serveur (joueurs, latence), nouveautés du pack, tête
  du skin du joueur.
- **Mises à jour** du pack et du launcher lui-même (signées, `tauri-plugin-updater`).

## Développer

```bash
bun install
bun tauri dev                      # l'application, rechargée à chaque modification
bun tauri build                    # installeurs dans src-tauri/target/release/bundle/
cd src-tauri && cargo test --lib   # tests du cœur
```

Linux : `libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev patchelf libdbus-1-dev`.

Sans interface, `turicraft-cli` passe par le même code :

```bash
cd src-tauri
export TURICRAFT_HOME=$PWD/target/tchome
cargo run --features cli --bin turicraft-cli -- detect      # matériel, préréglage choisi
cargo run --features cli --bin turicraft-cli -- prepare     # Java, Minecraft, NeoForge, pack, préréglage
cargo run --features cli --bin turicraft-cli -- launch Moi  # et lance le jeu HORS LIGNE
```

`TURICRAFT_PACK_URL` pointe vers un autre `pack.toml` (un serveur de pack
local, par exemple).

L'interface se regarde aussi hors de Tauri : `bun run dev`, puis
`http://localhost:1420/?vue=jouer` — hors de Tauri, `src/lib/preview.ts` remplace le cœur
par des données de démonstration. `scripts/captures.sh` photographie chaque
écran avec WebKitGTK, le moteur du launcher sous Linux.

| Fichier | Rôle |
|---|---|
| `src-tauri/src/lib.rs` | commandes appelées par l'interface |
| `launch.rs` | le parcours de « Jouer » : préparation, lancement, jalons |
| `java.rs`, `minecraft.rs`, `neoforge.rs` | Java de Mojang, jeu vanilla, NeoForge |
| `packwiz.rs`, `presets.rs`, `hardware.rs`, `display.rs` | synchro du pack, préréglages, détection |
| `auth.rs`, `skin.rs` | compte Microsoft, tête du joueur |
| `diag.rs`, `ping.rs`, `updates.rs` | crash, état du serveur, mises à jour |
| `src/lib/views/` | les écrans |

## Pour un autre pack

Tout ce qui est propre à Turi Craft est dans `src-tauri/src/config.rs`
(adresse du pack, serveur, client ID Azure) et `src-tauri/tauri.conf.json`
(identifiant, clé publique et adresse des mises à jour). Il faut :

- une **App Registration Azure** à soi, en « Mobile and desktop applications »,
  avec « Allow public client flows » et la redirection `http://localhost`,
  puis la faire approuver par Mojang pour l'API Minecraft
  ([formulaire](https://aka.ms/mce-reviewappid)) ;
- une **paire de clés de mise à jour** (`bun tauri signer generate`) : la
  publique dans `tauri.conf.json`, la privée en secret GitHub
  `TAURI_SIGNING_PRIVATE_KEY`.

## Publier une version

```bash
scripts/release.sh 0.2.0 "Ce qui change, pour les joueurs."
```

Le tag `v0.2.0` déclenche la CI : un installeur par système — Windows (x64
et ARM64 dans le même fichier, `src-tauri/windows/universel.nsh`), macOS
(universel Intel + Apple), Linux (AppImage, .deb) —, signés pour la mise à
jour, et `latest.json`, dans une version GitHub.

## Licence

[GPL-3.0-or-later](LICENSE). Les jars de `tools/` ont leur propre licence
(MIT, voir [`tools/README.md`](tools/README.md)).

Le nom et le logo **Turi Craft** ne sont pas couverts par la GPL : tous
droits réservés, Turi Industries. Le logo n'est pas dans ce dépôt : la CI le
pose au moment de construire (`scripts/branding.sh`). Une version modifiée
garde l'icône provisoire et doit porter un autre nom.

Le modpack lui-même (mods, configurations) n'est pas dans ce dépôt : le
launcher le télécharge au lancement.

Projet non officiel, sans lien avec Mojang ni Microsoft. Minecraft est une
marque de Mojang AB.
