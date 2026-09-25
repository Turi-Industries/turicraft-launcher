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
    let applied = {
        use sha2::{Digest, Sha256};
        let pack_files = packwiz::pack_hashes(paths, &file.managed_files());
        let both = serde_json::json!({ "file": &file, "resolved": &resolved, "pack_files": pack_files });
        hex::encode(Sha256::digest(both.to_string().as_bytes()))
    };
    let first_run = !paths.instance().join("options.txt").exists();
    if first_run || settings.applied.as_deref() != Some(applied.as_str()) {
        presets::apply(paths, &file, &resolved, r)?;
    } else {
        r.log("réglages inchangés : ceux faits en jeu sont gardés");
    }
    let once_applied = presets::apply_once(paths, &file, (settings.once_applied, settings.once_files_applied), r)?;
    early_window_off(&paths.instance())?;
    windowed_until_loaded(&paths.instance())?;
    Ok(Prepared { java, profile, resolved, applied, once_applied })
}

/// Le jeu démarre TOUJOURS en fenêtré ; le plein écran vient après le
/// chargement (kubejs/client_scripts/40_plein_ecran.js). Le choix du joueur
/// est lu dans options.txt — ce que le préréglage vient d'y écrire, ou ce
/// que le joueur a réglé en jeu (F11) à la partie précédente — puis passé au
/// script par config/turicraft/launch.json.
fn windowed_until_loaded(game: &Path) -> Result<()> {
    let options = game.join("options.txt");
    let text = std::fs::read_to_string(&options).unwrap_or_default();
    let wants_fullscreen = text.lines().any(|l| l.trim() == "fullscreen:true");
    let mut lines: Vec<String> = text.lines().filter(|l| !l.starts_with("fullscreen:")).map(String::from).collect();
    lines.push("fullscreen:false".into());
    std::fs::write(&options, lines.join("\n") + "\n")?;
    let dir = game.join("config/turicraft");
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join("launch.json"), serde_json::json!({ "fullscreen": wants_fullscreen }).to_string())?;
    Ok(())
}

/// Plus de fenêtre de chargement NeoForge : le launcher montre la progression
/// (décision 0007 § 5). Posé par le launcher, pas par le pack : sous Prism,
/// les joueurs n'auraient aucun retour pendant une à deux minutes.
/// Vérifié le 24/09 hors bac à sable : menu atteint, Ixeris compris.
fn early_window_off(game: &Path) -> Result<()> {
    let p = game.join("config/fml.toml");
    std::fs::create_dir_all(p.parent().unwrap())?;
    let text = std::fs::read_to_string(&p).unwrap_or_default();
    let mut doc: toml_edit::DocumentMut = text.parse().unwrap_or_default();
    doc["earlyWindowControl"] = toml_edit::value(false);
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
mod tests {
    #[test]
    fn fenetre_puis_plein_ecran() {
        let dir = std::env::temp_dir().join(format!("turicraft-fen-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("options.txt"), "guiScale:3\nfullscreen:true\nlang:fr_fr\n").unwrap();
        super::windowed_until_loaded(&dir).unwrap();
        let opts = std::fs::read_to_string(dir.join("options.txt")).unwrap();
        assert!(opts.contains("fullscreen:false") && !opts.contains("fullscreen:true"));
        assert!(opts.contains("guiScale:3") && opts.contains("lang:fr_fr"));
        let launch = std::fs::read_to_string(dir.join("config/turicraft/launch.json")).unwrap();
        assert_eq!(launch, r#"{"fullscreen":true}"#);
        std::fs::remove_dir_all(&dir).ok();
    }
}
