//! NeoForge : on lance son installeur officiel sans interface
//! (`--install-client <dossier>`), exactement comme le ferait un joueur avec
//! le launcher Mojang. Il écrit `versions/neoforge-X/neoforge-X.json`, que
//! `minecraft::resolve_profile` fusionne avec la version vanilla.

use std::path::Path;
use std::process::Stdio;

use anyhow::{bail, Context, Result};
use tokio::io::{AsyncBufReadExt, BufReader};

use crate::minecraft;
use crate::net::{self, Download, Hash};
use crate::paths::Paths;
use crate::progress::Reporter;

pub fn version_id(neoforge: &str) -> String {
    format!("neoforge-{neoforge}")
}

pub async fn ensure_neoforge(paths: &Paths, java: &Path, neoforge: &str, reporter: &dyn Reporter) -> Result<String> {
    reporter.stage("neoforge", &format!("NeoForge {neoforge}"));
    let id = version_id(neoforge);
    let done = done_marker(paths, &id);
    if done.exists() && minecraft::is_installed(paths, &id) {
        reporter.log("déjà installé");
        return Ok(id);
    }

    let client = net::client();
    let installer = paths.tools().join(format!("neoforge-{neoforge}-installer.jar"));
    net::download(
        &client,
        &Download {
            url: format!(
                "https://maven.neoforged.net/releases/net/neoforged/neoforge/{neoforge}/neoforge-{neoforge}-installer.jar"
            ),
            path: installer.clone(),
            hash: Hash::None,
            size: None,
            executable: false,
        },
    )
    .await
    .context("installeur NeoForge")?;

    // L'installeur refuse un dossier sans launcher_profiles.json (il croit
    // n'avoir pas affaire à une installation de Minecraft).
    let mc = paths.minecraft();
    std::fs::create_dir_all(&mc)?;
    let profiles = mc.join("launcher_profiles.json");
    if !profiles.exists() {
        std::fs::write(&profiles, r#"{"profiles":{}}"#)?;
    }

    reporter.log("installation (quelques minutes la première fois)");
    reporter.progress(0, 0);
    // Il écrit son journal dans le dossier courant : on lui donne tools/.
    let mut child = tokio::process::Command::new(java)
        .arg("-jar")
        .arg(&installer)
        .arg("--install-client")
        .arg(&mc)
        .current_dir(paths.tools())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true) // préparation annulée : le processus s'arrête aussi
        .spawn()
        .context("lancement de l'installeur NeoForge")?;

    // stderr lu à part : un tube plein non lu bloquerait l'installeur.
    let stderr = child.stderr.take().unwrap();
    let errors = tokio::spawn(async move {
        let mut lines = BufReader::new(stderr).lines();
        let mut kept = Vec::new();
        while let Ok(Some(l)) = lines.next_line().await {
            kept.push(l);
            if kept.len() > 20 {
                kept.remove(0);
            }
        }
        kept
    });
    let stdout = child.stdout.take().unwrap();
    let mut lines = BufReader::new(stdout).lines();
    let mut tail: Vec<String> = Vec::new();
    while let Some(line) = lines.next_line().await? {
        // Les étapes intéressantes ; le reste va seulement dans la fin gardée.
        if line.starts_with("  Processor") || line.contains("Downloading library") || line.starts_with("Task:") {
            reporter.log(line.trim());
        }
        tail.push(line);
        if tail.len() > 40 {
            tail.remove(0);
        }
    }
    let status = child.wait().await?;
    let stderr_tail = errors.await.unwrap_or_default();
    if !status.success() {
        bail!("l'installeur NeoForge a échoué ({status}) :\n{}\n{}", tail.join("\n"), stderr_tail.join("\n"));
    }
    if !minecraft::version_json_path(paths, &id).exists() {
        bail!("l'installeur NeoForge n'a pas écrit {id}.json");
    }
    std::fs::write(&done, "")?;
    Ok(id)
}

/// Posé quand l'installeur a fini sans erreur. `is_installed` ne suffit pas :
/// l'installeur écrit le JSON et les bibliothèques d'abord, puis fabrique le
/// jeu patché (`neoforge-X-client.jar`), qui n'est PAS dans la liste des
/// bibliothèques. Annulé pendant cette étape, tout semblait installé et le
/// jeu ne démarrait plus. Sans marqueur, on relance l'installeur, qui garde
/// ce qui est déjà bon (empreintes vérifiées).
pub fn done_marker(paths: &Paths, id: &str) -> std::path::PathBuf {
    paths.versions().join(id).join(".turicraft-installe")
}
