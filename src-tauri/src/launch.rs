//! Préparer puis lancer le jeu (L2 → L4).
//!
//! Préparation, à chaque clic sur « Jouer » (rapide quand tout est déjà là) :
//! versions lues dans pack.toml → Java → Minecraft → NeoForge → pack
//! (packwiz) → mods optionnels du préréglage → configs du préréglage.
//!
//! Lancement : la sortie du jeu est lue ligne à ligne ; des jalons connus
//! deviennent une progression, estimée d'après la durée du lancement précédent
//! sur la même machine.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Instant, SystemTime};

use anyhow::{Context, Result};

use crate::auth::Session;
use crate::minecraft::{self, LaunchVars, Profile};
use crate::paths::Paths;
use crate::presets::{self, Resolved};
use crate::progress::{Event, Reporter};
use crate::settings::Settings;
use crate::{config, hardware, java, neoforge, packwiz};

pub struct Prepared {
    pub java: PathBuf,
    pub profile: Profile,
    pub resolved: Resolved,
    /// Empreinte des réglages appliqués : à enregistrer dans
    /// `Settings::applied` une fois le lancement parti.
    pub applied: String,
    /// Versions des réglages « une fois » posées (`Settings::once_applied`,
    /// `Settings::once_files_applied`).
    pub once_applied: (u32, u32),
    /// Taille de la fenêtre du jeu pendant le chargement (`--width`,
    /// `--height`), en coordonnées d'écran du jeu. Posée par l'appelant, qui
    /// connaît l'écran (`game_window_size`) ; `None` : taille de Minecraft.
    pub window_size: Option<(u32, u32)>,
}

pub async fn prepare(paths: &Paths, settings: &Settings, r: &dyn Reporter) -> Result<Prepared> {
    let pack_url = crate::settings::pack_url();
    r.stage("versions", "Lecture du pack");
    let (mc, neo) = packwiz::pack_versions(&pack_url).await?;
    r.log(&format!("Minecraft {mc}, NeoForge {neo}"));

    let java = java::ensure_java(paths, config::JAVA_COMPONENT, r).await?;
    minecraft::ensure_vanilla(paths, &mc, r).await?;
    let id = neoforge::ensure_neoforge(paths, &java, &neo, r).await?;
    let profile = minecraft::resolve_profile(paths, &id)?;
    minecraft::ensure_libraries(paths, &profile, r).await?;

    packwiz::sync(paths, &java, &pack_url, r).await?;

    r.stage("presets", "Préréglage");
    let file = presets::fetch(&pack_url).await?;
    let hw = hardware::detect();
    let resolved = presets::resolve(&file, &hw, settings);
    let changed = packwiz::set_optional(paths, &file.all_optional(), &file.enabled_optional(&resolved.groups))?;
    if changed {
        r.log("mods optionnels modifiés : nouvelle synchronisation");
        packwiz::sync(paths, &java, &pack_url, r).await?;
    }
    // Les réglages ne sont réécrits que s'ils ont changé côté launcher (choix
    // du joueur, ou presets.toml mis à jour) : sinon, ce que le joueur a
    // réglé EN JEU (distance, plein écran…) serait écrasé à chaque lancement.
    // Ou si une mise à jour du pack a remplacé un fichier qu'ils touchent
    // (DistantHorizons.toml…) : le fichier neuf n'a plus les réglages.
    let applied = applied_hash(paths, &file, &resolved);
    let first_run = !paths.instance().join("options.txt").exists();
    if first_run || settings.applied.as_deref() != Some(applied.as_str()) {
        presets::apply(paths, &file, &resolved, r)?;
    } else {
        r.log("réglages inchangés : ceux faits en jeu sont gardés");
    }
    let once_applied = presets::apply_once(paths, &file, (settings.once_applied, settings.once_files_applied), r)?;
    early_window(&paths.instance(), None)?;
    windowed_until_loaded(&paths.instance(), wants_fullscreen(&file, &resolved))?;
    Ok(Prepared { java, profile, resolved, applied, once_applied, window_size: None })
}

/// Empreinte des réglages à écrire : fichier de préréglages, choix résolus,
/// et versions des fichiers de config du pack qu'ils touchent.
pub fn applied_hash(paths: &Paths, file: &presets::PresetsFile, resolved: &presets::Resolved) -> String {
    use sha2::{Digest, Sha256};
    let pack_files = packwiz::pack_hashes(paths, &file.managed_files());
    let both = serde_json::json!({ "file": file, "resolved": resolved, "pack_files": pack_files });
    hex::encode(Sha256::digest(both.to_string().as_bytes()))
}

/// Réglages changés EN JEU (distance, images/s, synchro, shaders…) : repris
/// comme choix du joueur, pour que le launcher affiche ce que le jeu a et ne
/// l'écrase pas au lancement suivant. Seulement si RIEN n'a changé côté
/// launcher depuis le dernier lancement (sinon, le launcher a le dernier
/// mot, comme avant). Rend les réglages repris.
///
/// Exception, les choix (pack de shaders) : chacun est comparé à sa valeur
/// du dernier lancement, pas à l'empreinte de tous les réglages. Une mise à
/// jour du pack change l'empreinte ; elle ne doit pas remplacer le pack de
/// shaders que le joueur a choisi en jeu.
pub fn import_game_changes(paths: &Paths, file: &presets::PresetsFile, hw: &crate::hardware::Hardware, s: &mut Settings) -> Vec<String> {
    let resolved = presets::resolve(file, hw, s);
    let unchanged = s.applied.as_deref().is_some_and(|stored| applied_hash(paths, file, &resolved) == stored);
    let (sliders, toggles, choices) = presets::game_values(&paths.instance(), file);
    let mut changed = Vec::new();
    for (id, v) in choices {
        // Changé dans le launcher depuis la dernière partie : il l'emporte.
        let touched_in_launcher = s.choices_applied.get(&id).is_some_and(|a| Some(a) != resolved.choices.get(&id));
        if touched_in_launcher || resolved.choices.get(&id) == Some(&v) {
            continue;
        }
        // Sans lancement noté (launcher tout juste à jour), seule une valeur
        // inconnue du launcher est reprise : ailleurs, la valeur conseillée
        // s'applique (Unbound à la place de Reimagined, 26/09).
        if !unchanged && !s.choices_applied.contains_key(&id) && file.choices[&id].values.iter().any(|x| x.id == v && !x.other) {
            continue;
        }
        s.choices.insert(id.clone(), v);
        changed.push(id);
    }
    if unchanged {
        for (id, v) in toggles {
            if resolved.toggles.get(&id) != Some(&v) {
                s.toggles.insert(id.clone(), v);
                changed.push(id);
            }
        }
        for (id, v) in sliders {
            // Curseur grisé (vue lointaine coupée) : sa valeur ne compte pas.
            let active = file.sliders[&id].requires.as_ref().map_or(true, |t| s.toggles.get(t).copied().or(resolved.toggles.get(t).copied()).unwrap_or(false));
            if active && resolved.sliders.get(&id) != Some(&v) {
                s.sliders.insert(id.clone(), v);
                changed.push(id);
            }
        }
    }
    if !changed.is_empty() {
        let now = presets::resolve(file, hw, s);
        s.choices_applied.extend(now.choices.clone());
        // Le jeu a déjà ces valeurs : rien à réécrire au prochain lancement.
        if unchanged {
            s.applied = Some(applied_hash(paths, file, &now));
        }
    }
    changed
}

/// Préférence « Hautes performances » de Windows (Paramètres → Affichage →
/// Graphiques) pour le Java du jeu : sans elle, un portable à deux cartes
/// lançait le jeu sur l'intégrée (25/09). Vaut pour NVIDIA et AMD. Un choix
/// déjà fait pour ce programme (par le joueur) est laissé tel quel.
#[cfg(windows)]
fn prefer_dedicated_gpu(java: &Path, r: &dyn Reporter) {
    use std::os::windows::process::CommandExt;
    const KEY: &str = r"HKCU\Software\Microsoft\DirectX\UserGpuPreferences";
    let exe = java.display().to_string();
    let reg = |args: &[&str]| {
        std::process::Command::new("reg")
            .args(args)
            .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    };
    if reg(&["query", KEY, "/v", &exe]) {
        return;
    }
    if reg(&["add", KEY, "/v", &exe, "/t", "REG_SZ", "/d", "GpuPreference=2;", "/f"]) {
        r.log("carte graphique : le jeu utilisera la plus puissante (réglage Windows posé)");
    }
}

/// Plein écran après le chargement : l'option du launcher qui règle
/// `fullscreen` (« Plein écran » dans presets.toml). Elle suit aussi la façon
/// dont le joueur a quitté le jeu : `import_game_changes` la relit dans
/// config/turicraft/window.json (presets::game_values). `None` : le pack n'a
/// pas cette option, options.txt décide.
fn wants_fullscreen(file: &presets::PresetsFile, resolved: &Resolved) -> Option<bool> {
    let (id, _) = file.toggles.iter().find(|(_, t)| t.options.contains_key("fullscreen"))?;
    resolved.toggles.get(id).copied()
}

/// Le jeu démarre TOUJOURS en fenêtré (d'un tiers de l'écran, ou à la taille
/// laissée par le joueur : `game_window_size`) ; le plein écran vient après le
/// chargement (kubejs/client_scripts/40_plein_ecran.js), passé au script par
/// config/turicraft/launch.json.
fn windowed_until_loaded(game: &Path, wants: Option<bool>) -> Result<()> {
    let options = game.join("options.txt");
    let text = std::fs::read_to_string(&options).unwrap_or_default();
    let wants_fullscreen = wants.unwrap_or_else(|| text.lines().any(|l| l.trim() == "fullscreen:true"));
    let mut lines: Vec<String> = text.lines().filter(|l| !l.starts_with("fullscreen:")).map(String::from).collect();
    lines.push("fullscreen:false".into());
    std::fs::write(&options, lines.join("\n") + "\n")?;
    let dir = game.join("config/turicraft");
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join("launch.json"), serde_json::json!({ "fullscreen": wants_fullscreen }).to_string())?;
    Ok(())
}

/// Taille de la fenêtre du jeu pendant le chargement, en coordonnées d'écran
/// du jeu (`screen` : l'écran principal, dans ces mêmes coordonnées). La
/// taille laissée par le joueur à la dernière partie, en fenêtré
/// (config/turicraft/window.json, écrit par le script KubeJS), si elle tient
/// dans l'écran ; sinon un tiers de sa surface en 16:9, comme le launcher.
pub fn game_window_size(game: &Path, screen: (u32, u32)) -> (u32, u32) {
    let (sw, sh) = screen;
    let saved = std::fs::read_to_string(game.join("config/turicraft/window.json"))
        .ok()
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
        // KubeJS écrit ses nombres en décimal (1600.0) : lus en f64.
        .and_then(|v| Some((v.get("width")?.as_f64()?.round() as u32, v.get("height")?.as_f64()?.round() as u32)));
    if let Some((w, h)) = saved.filter(|&(w, h)| (640..=sw).contains(&w) && (360..=sh).contains(&h)) {
        return (w, h);
    }
    let (sw, sh) = (sw as f64, sh as f64);
    let h = (sw * sh / 3.0 * 9.0 / 16.0).sqrt();
    let w = h * 16.0 / 9.0;
    // Jamais sous la taille de Minecraft (854 × 480), jamais plus que l'écran.
    (w.max(854.0).min(sw).round() as u32, h.max(480.0).min(sh).round() as u32)
}

/// Fenêtre de chargement NeoForge, habillée par Drippy Early Loading Module
/// (fond noir, logo Mojang, barre jaune, logo Turi Industries en bas à
/// gauche) : sans elle, la fenêtre du jeu restait noire ~20 s pendant la
/// construction des mods (Jean, 26/09 ; revient sur la décision 0007 § 5).
/// Pas sur macOS : le module y dessine en OpenGL ancien, refusé par le profil
/// Core de macOS (plantage au démarrage) ; son correctif exige Drippy 3.1.2.
/// Sa taille : celle de la fenêtre du jeu (`--width/--height`), que le module
/// ne lit pas — il lit `earlyWindowWidth/Height`.
const EARLY_WINDOW: bool = !cfg!(target_os = "macos");

fn early_window(game: &Path, size: Option<(u32, u32)>) -> Result<()> {
    let p = game.join("config/fml.toml");
    std::fs::create_dir_all(p.parent().unwrap())?;
    let text = std::fs::read_to_string(&p).unwrap_or_default();
    let mut doc: toml_edit::DocumentMut = text.parse().unwrap_or_default();
    doc["earlyWindowControl"] = toml_edit::value(EARLY_WINDOW);
    if let Some((w, h)) = size {
        doc["earlyWindowWidth"] = toml_edit::value(w as i64);
        doc["earlyWindowHeight"] = toml_edit::value(h as i64);
    }
    std::fs::write(&p, doc.to_string())?;
    Ok(())
}

/// Jalons du démarrage, relevés dans un vrai latest.log du pack (24/09 :
/// 74 s au total sur un PC de test). Le premier de chaque seulement.
const MILESTONES: &[(&str, &str)] = &[
    ("ModLauncher running", "Démarrage de Java"),
    ("Launching target", "Mods trouvés"),
    ("Setting user:", "Fenêtre du jeu"),
    ("[modloading-worker-", "Construction des mods"),
    ("Reloading ResourceManager", "Chargement des ressources"),
    ("textures/atlas/blocks.png-atlas", "Textures"),
    ("resource reload: FINISHED", "Menu"),
];

/// Ligne de commande complète : arguments JVM (jusqu'à la classe principale
/// comprise) et arguments du jeu.
pub fn command_line(
    paths: &Paths,
    settings: &Settings,
    session: &Session,
    prepared: &Prepared,
) -> Result<(Vec<String>, Vec<String>)> {
    let vars = LaunchVars {
        player_name: session.name.clone(),
        uuid: session.uuid.clone(),
        access_token: session.access_token.clone(),
        xuid: session.xuid.clone(),
        user_type: if session.access_token == "0" { "legacy".into() } else { "msa".into() },
        game_dir: paths.instance(),
        memory_mb: (prepared.resolved.memory_gb * 1024.0).round() as u64,
        // ZGC sur un grand tas, G1 sur un petit (presets.toml, [jvm]).
        extra_jvm: prepared.resolved.jvm_flags.clone(),
    };
    let (jvm, mut game_args) = minecraft::command_line(paths, &prepared.profile, &vars)?;
    if let Some((w, h)) = prepared.window_size {
        game_args.extend(["--width".into(), w.to_string(), "--height".into(), h.to_string()]);
    }
    if settings.join_server {
        game_args.push("--quickPlayMultiplayer".into());
        game_args.push(format!("{}:{}", config::SERVER_HOST, config::SERVER_PORT));
    }
    Ok((jvm, game_args))
}

pub struct Outcome {
    pub code: Option<i32>,
    pub milestones_ms: Vec<u64>,
}

pub async fn launch(
    paths: &Paths,
    settings: &Settings,
    session: &Session,
    prepared: &Prepared,
    r: &dyn Reporter,
) -> Result<Outcome> {
    r.stage("launch", "Lancement du jeu");
    let game = paths.instance();
    early_window(&game, prepared.window_size)?;
    let (jvm, game_args) = command_line(paths, settings, session, prepared)?;

    let started_at = SystemTime::now();
    let start = Instant::now();
    let mut cmd = tokio::process::Command::new(&prepared.java);
    cmd.args(&jvm)
        .args(&game_args)
        .current_dir(&game)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // « Arrêter » dans le launcher abandonne la tâche : le jeu part avec.
        .kill_on_drop(true);
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    // Portable à deux cartes : le jeu sur la grosse, pas sur l'intégrée.
    let hw = hardware::detect();
    #[cfg(windows)]
    if hw.gpu_dedicated {
        // Reconnue par le pilote NVIDIA (Optimus) : celle du Minecraft Launcher.
        cmd.env("SHIM_MCCOMPAT", "0x800000001");
        prefer_dedicated_gpu(&prepared.java, r);
    }
    let mut child = cmd.spawn().context("lancement de Java")?;

    // stderr : seulement gardé pour le journal du launcher.
    let stderr = child.stderr.take().unwrap();
    tokio::spawn(async move {
        let mut lines = crate::lines::Lines::new(stderr);
        while let Ok(Some(_)) = lines.next_line().await {}
    });

    let expected = settings.last_milestones_ms.last().copied().unwrap_or(0);
    let mut reached: Vec<u64> = Vec::new();
    let mut lines = crate::lines::Lines::new(child.stdout.take().unwrap());
    while let Some(line) = lines.next_line().await? {
        if let Some((renderer, advice)) = crate::diag::slow_gl_driver(&line) {
            r.log(&format!("pilote graphique lent : {renderer}"));
            r.send(Event::GpuWarning { title: "Pilote graphique à mettre à jour".into(), renderer, advice: advice.into() });
        } else if let Some(renderer) = crate::diag::integrated_instead_of_dedicated(&line, &hw) {
            r.log(&format!("jeu lancé sur la carte intégrée : {renderer}"));
            r.send(Event::GpuWarning {
                title: "Le jeu tourne sur la carte graphique intégrée".into(),
                renderer,
                advice: format!(
                    "Ta machine a une carte plus puissante ({}). Dans Windows : Paramètres → Système → Affichage → \
                     Graphiques → javaw.exe (dans le dossier turicraft) → Hautes performances. Puis relance le jeu.",
                    hw.gpu_name
                ),
            });
        }
        let next = reached.len();
        if next < MILESTONES.len() && line.contains(MILESTONES[next].0) {
            let elapsed = start.elapsed().as_millis() as u64;
            reached.push(elapsed);
            r.send(Event::Milestone {
                index: next,
                count: MILESTONES.len(),
                label: MILESTONES[next].1.into(),
                elapsed_ms: elapsed,
                expected_ms: expected,
            });
            if next == MILESTONES.len() - 1 {
                r.send(Event::GameReady { elapsed_ms: elapsed });
            }
        }
    }
    let status = child.wait().await?;
    let code = status.code();
    let crash = if status.success() { None } else { crate::diag::analyze(&game, started_at) };
    r.send(Event::GameExited { code, crash });
    Ok(Outcome { code, milestones_ms: if reached.len() == MILESTONES.len() { reached } else { Vec::new() } })
}

#[cfg(test)]
mod tests_reglages_en_jeu {
    use super::*;

    /// La fenêtre de chargement prend la taille de la fenêtre du jeu ; les
    /// autres réglages de fml.toml restent.
    #[test]
    fn fenetre_de_chargement() {
        let dir = std::env::temp_dir().join(format!("turicraft-fml-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("config")).unwrap();
        std::fs::write(dir.join("config/fml.toml"), "earlyWindowControl = false\nmaxThreads = 1\nearlyWindowWidth = 854\n").unwrap();
        early_window(&dir, Some((1109, 624))).unwrap();
        let t: toml::Table = std::fs::read_to_string(dir.join("config/fml.toml")).unwrap().parse().unwrap();
        assert_eq!(t["earlyWindowControl"].as_bool(), Some(EARLY_WINDOW));
        assert_eq!((t["earlyWindowWidth"].as_integer(), t["earlyWindowHeight"].as_integer()), (Some(1109), Some(624)));
        assert_eq!(t["maxThreads"].as_integer(), Some(1));
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Distance et synchro changées EN JEU : reprises par le launcher, qui ne
    /// réécrit plus rien ; mais si le joueur change un réglage dans le
    /// launcher entre-temps, c'est le launcher qui gagne.
    #[test]
    fn reglages_changes_en_jeu_repris() {
        let file = presets::parse(include_str!("../fixtures/presets.toml")).unwrap();
        let hw = crate::hardware::Hardware { ram_gb: 32.0, cpu_threads: 20, gpu_dedicated: true, vram_gb: 16.0, ..Default::default() };
        let dir = std::env::temp_dir().join(format!("turicraft-enjeu-{}", std::process::id()));
        let paths = Paths::new(dir.clone());
        let mut s = Settings { preset: "haut".into(), ..Default::default() };
        let r = presets::resolve(&file, &hw, &s);
        presets::apply(&paths, &file, &r, &crate::progress::ConsoleReporter).unwrap();
        s.applied = Some(applied_hash(&paths, &file, &r));
        assert!(import_game_changes(&paths, &file, &hw, &mut s).is_empty(), "rien changé en jeu");

        // En jeu : distance 12, synchro verticale coupée.
        let opts = paths.instance().join("options.txt");
        let text = std::fs::read_to_string(&opts).unwrap().replace("renderDistance:16", "renderDistance:12").replace("enableVsync:true", "enableVsync:false");
        std::fs::write(&opts, text).unwrap();
        let mut changed = import_game_changes(&paths, &file, &hw, &mut s);
        changed.sort();
        assert_eq!(changed, vec!["distance".to_string(), "synchro_verticale".to_string()]);
        assert_eq!((s.sliders["distance"], s.toggles["synchro_verticale"]), (12, false));
        // Le jeu a déjà ces valeurs : au lancement, rien à réécrire.
        assert_eq!(s.applied.as_deref(), Some(applied_hash(&paths, &file, &presets::resolve(&file, &hw, &s)).as_str()));

        // Le joueur choisit Moyen dans le launcher, puis rechange en jeu : le
        // launcher a le dernier mot (il réécrira au lancement).
        s.preset = "moyen".into();
        let text = std::fs::read_to_string(&opts).unwrap().replace("renderDistance:12", "renderDistance:20");
        std::fs::write(&opts, text).unwrap();
        assert!(import_game_changes(&paths, &file, &hw, &mut s).is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Pack de shaders ajouté par le joueur et choisi en jeu : repris
    /// (« Autre ») et jamais écrasé, même quand presets.toml ou le pack
    /// changent. Reimagined d'avant le choix (launcher 0.1.12) : remplacé par
    /// Unbound, la valeur conseillée (26/09).
    #[test]
    fn pack_de_shaders_du_joueur_garde() {
        let file = presets::parse(include_str!("../fixtures/presets.toml")).unwrap();
        let hw = crate::hardware::Hardware { ram_gb: 32.0, cpu_threads: 20, gpu_dedicated: true, vram_gb: 16.0, ..Default::default() };
        let dir = std::env::temp_dir().join(format!("turicraft-shaders-{}", std::process::id()));
        let paths = Paths::new(dir.clone());
        let iris = paths.instance().join("config/iris.properties");
        let pack = || std::fs::read_to_string(&iris).unwrap().lines().find_map(|l| l.strip_prefix("shaderPack=").map(String::from)).unwrap();
        let apply = |s: &mut Settings| {
            let r = presets::resolve(&file, &hw, s);
            presets::apply(&paths, &file, &r, &crate::progress::ConsoleReporter).unwrap();
            s.applied = Some(applied_hash(&paths, &file, &r));
            s.choices_applied = r.choices.clone();
        };
        let mut s = Settings { preset: "haut".into(), ..Default::default() };
        // Installation neuve : rien à reprendre, Unbound.
        assert!(import_game_changes(&paths, &file, &hw, &mut s).is_empty());
        std::fs::create_dir_all(iris.parent().unwrap()).unwrap();
        std::fs::write(&iris, "enableShaders=false\nshaderPack=\n").unwrap();
        assert!(import_game_changes(&paths, &file, &hw, &mut s).is_empty());

        // Venu du launcher 0.1.12 : Reimagined, aucun choix noté.
        std::fs::write(&iris, "enableShaders=true\nshaderPack=ComplementaryReimagined_r5.9.3 + EuphoriaPatches_1.10.5\n").unwrap();
        s.applied = Some("empreinte du launcher 0.1.12".into());
        assert!(import_game_changes(&paths, &file, &hw, &mut s).is_empty());
        apply(&mut s);
        assert_eq!(pack(), "ComplementaryUnbound_r5.9.3 + EuphoriaPatches_1.10.5");

        // En jeu : un pack ajouté par le joueur. Le pack du serveur a changé
        // entre-temps (autre empreinte) : repris quand même.
        let text = std::fs::read_to_string(&iris).unwrap().replace(&pack(), "BSL_v10.0.zip");
        std::fs::write(&iris, text).unwrap();
        s.applied = Some("mise à jour du pack".into());
        assert_eq!(import_game_changes(&paths, &file, &hw, &mut s), vec!["shader_pack".to_string()]);
        assert_eq!(s.choices["shader_pack"], "autre");
        apply(&mut s);
        assert_eq!(pack(), "BSL_v10.0.zip");
        // Au lancement suivant, toujours rien à reprendre ni à écraser.
        assert!(import_game_changes(&paths, &file, &hw, &mut s).is_empty());

        // Reimagined choisi en jeu : repris tel quel.
        let text = std::fs::read_to_string(&iris).unwrap().replace(&pack(), "ComplementaryReimagined_r5.9.3 + EuphoriaPatches_1.10.5");
        std::fs::write(&iris, text).unwrap();
        assert_eq!(import_game_changes(&paths, &file, &hw, &mut s), vec!["shader_pack".to_string()]);
        assert_eq!(s.choices["shader_pack"], "reimagined");

        // Le joueur choisit Unbound dans le launcher : il l'emporte.
        s.choices.insert("shader_pack".into(), "unbound".into());
        assert!(import_game_changes(&paths, &file, &hw, &mut s).is_empty());
        apply(&mut s);
        assert_eq!(pack(), "ComplementaryUnbound_r5.9.3 + EuphoriaPatches_1.10.5");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn plein_ecran_comme_laisse_en_jeu() {
        let file = presets::parse(include_str!("../fixtures/presets.toml")).unwrap();
        let hw = crate::hardware::Hardware { ram_gb: 32.0, cpu_threads: 20, gpu_dedicated: true, vram_gb: 16.0, ..Default::default() };
        let dir = std::env::temp_dir().join(format!("turicraft-pleinecran-{}", std::process::id()));
        let paths = Paths::new(dir.clone());
        let mut s = Settings { preset: "haut".into(), ..Default::default() };
        let r = presets::resolve(&file, &hw, &s);
        assert_eq!(r.toggles.get("plein_ecran"), Some(&true), "plein écran par défaut");
        presets::apply(&paths, &file, &r, &crate::progress::ConsoleReporter).unwrap();
        s.applied = Some(applied_hash(&paths, &file, &r));
        // Le lancement force fullscreen:false dans options.txt : ce n'est PAS
        // un choix du joueur, rien à reprendre.
        windowed_until_loaded(&paths.instance(), wants_fullscreen(&file, &r)).unwrap();
        assert!(import_game_changes(&paths, &file, &hw, &mut s).is_empty());
        // Le joueur quitte en fenêtré (F11) : la partie suivante le reste.
        let w = paths.instance().join("config/turicraft/window.json");
        std::fs::write(&w, r#"{"fullscreen":false,"width":1600,"height":900}"#).unwrap();
        assert_eq!(import_game_changes(&paths, &file, &hw, &mut s), vec!["plein_ecran".to_string()]);
        let r = presets::resolve(&file, &hw, &s);
        assert_eq!(wants_fullscreen(&file, &r), Some(false));
        // Puis quitte en plein écran : de nouveau plein écran.
        std::fs::write(&w, r#"{"fullscreen":true,"width":1600,"height":900}"#).unwrap();
        assert_eq!(import_game_changes(&paths, &file, &hw, &mut s), vec!["plein_ecran".to_string()]);
        assert_eq!(wants_fullscreen(&file, &presets::resolve(&file, &hw, &s)), Some(true));
        std::fs::remove_dir_all(&dir).ok();
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn fenetre_puis_plein_ecran() {
        let dir = std::env::temp_dir().join(format!("turicraft-fen-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("options.txt"), "guiScale:3\nfullscreen:true\nlang:fr_fr\n").unwrap();
        super::windowed_until_loaded(&dir, None).unwrap();
        let opts = std::fs::read_to_string(dir.join("options.txt")).unwrap();
        assert!(opts.contains("fullscreen:false") && !opts.contains("fullscreen:true"));
        assert!(opts.contains("guiScale:3") && opts.contains("lang:fr_fr"));
        let launch = std::fs::read_to_string(dir.join("config/turicraft/launch.json")).unwrap();
        assert_eq!(launch, r#"{"fullscreen":true}"#);
        // L'option du launcher l'emporte sur options.txt (forcé à false au
        // lancement précédent) : elle suit la façon dont le joueur a quitté.
        super::windowed_until_loaded(&dir, Some(true)).unwrap();
        let launch = std::fs::read_to_string(dir.join("config/turicraft/launch.json")).unwrap();
        assert_eq!(launch, r#"{"fullscreen":true}"#);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn fenetre_de_chargement() {
        let dir = std::env::temp_dir().join(format!("turicraft-taille-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("config/turicraft")).unwrap();
        // Rien d'enregistré : un tiers de l'écran, en 16:9.
        assert_eq!(super::game_window_size(&dir, (2560, 1440)), (1478, 831));
        assert_eq!(super::game_window_size(&dir, (1920, 1080)), (1109, 624));
        // Petit écran : jamais sous la taille de Minecraft.
        assert_eq!(super::game_window_size(&dir, (1366, 768)), (854, 480));
        // La taille laissée par le joueur, si elle tient dans l'écran.
        std::fs::write(dir.join("config/turicraft/window.json"), r#"{"fullscreen":false,"width":1600,"height":900}"#).unwrap();
        assert_eq!(super::game_window_size(&dir, (2560, 1440)), (1600, 900));
        std::fs::write(dir.join("config/turicraft/window.json"), r#"{"fullscreen":true,"width":1600.0,"height":900.0}"#).unwrap();
        assert_eq!(super::game_window_size(&dir, (2560, 1440)), (1600, 900), "nombres écrits par KubeJS");
        // Plus grande que l'écran (écran changé depuis) : un tiers à nouveau.
        assert_eq!(super::game_window_size(&dir, (1366, 768)), (854, 480));
        std::fs::remove_dir_all(&dir).ok();
    }
}
