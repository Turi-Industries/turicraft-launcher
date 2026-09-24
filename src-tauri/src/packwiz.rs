//! Synchronisation du pack : packwiz-installer, sans interface, dans le dossier
//! de jeu. Mêmes jars et mêmes options que la commande de pré-lancement de
//! Prism (`scripts/make-prism-instance.sh`).
//!
//! Mods optionnels : packwiz-installer retient le choix de chacun dans
//! `packwiz.json` (`cachedFiles[<fichier .pw.toml>].optionValue`). Le launcher
//! y écrit le choix du préréglage, puis vide `packFileHash`/`indexFileHash` :
//! sans ça, packwiz-installer voit un pack inchangé et ne revérifie rien
//! (vérifié le 24/09 : désactivé → jar supprimé, réactivé → retéléchargé).

use std::collections::HashSet;
use std::path::Path;
use std::process::Stdio;

use anyhow::{bail, Context, Result};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, BufReader};

use crate::paths::Paths;
use crate::progress::Reporter;

// Les jars de tools/ (packwiz-installer, MIT), embarqués dans l'exécutable.
static BOOTSTRAP: &[u8] = include_bytes!("../../tools/packwiz-installer-bootstrap.jar");
static INSTALLER: &[u8] = include_bytes!("../../tools/packwiz-installer.jar");

fn ensure_tools(paths: &Paths) -> Result<()> {
    let dir = paths.tools();
    std::fs::create_dir_all(&dir)?;
    for (name, bytes) in [("packwiz-installer-bootstrap.jar", BOOTSTRAP), ("packwiz-installer.jar", INSTALLER)] {
        let p = dir.join(name);
        let same = std::fs::metadata(&p).map(|m| m.len() == bytes.len() as u64).unwrap_or(false);
        if !same {
            std::fs::write(&p, bytes)?;
        }
    }
    Ok(())
}

/// « (123/749) … » dans la sortie de packwiz-installer.
fn parse_counter(line: &str) -> Option<(u64, u64)> {
    let start = line.find('(')?;
    let end = line[start..].find(')')? + start;
    let (a, b) = line[start + 1..end].split_once('/')?;
    Some((a.trim().parse().ok()?, b.trim().parse().ok()?))
}

pub async fn sync(paths: &Paths, java: &Path, pack_url: &str, reporter: &dyn Reporter) -> Result<()> {
    reporter.stage("pack", "Mods et configuration du pack");
    ensure_tools(paths)?;
    let instance = paths.instance();
    std::fs::create_dir_all(&instance)?;

    let mut child = tokio::process::Command::new(java)
        .arg("-jar")
        .arg(paths.tools().join("packwiz-installer-bootstrap.jar"))
        // Sans ces deux options, le bootstrap interroge l'API GitHub à chaque
        // lancement, sans authentification : HTTP 403 dès le quota atteint.
        .arg("--bootstrap-no-update")
        .arg("--bootstrap-main-jar")
        .arg(paths.tools().join("packwiz-installer.jar"))
        .arg("-g")
        .arg("-s")
        .arg("client")
        .arg(pack_url)
        .current_dir(&instance)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true) // préparation annulée : le processus s'arrête aussi
        .spawn()
        .context("lancement de packwiz-installer")?;

    let stderr = child.stderr.take().unwrap();
    let errors = tokio::spawn(async move {
        let mut lines = BufReader::new(stderr).lines();
        let mut kept = Vec::new();
        while let Ok(Some(l)) = lines.next_line().await {
            kept.push(l);
            if kept.len() > 30 {
                kept.remove(0);
            }
        }
        kept
    });
    let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
    let mut tail = Vec::new();
    while let Some(line) = lines.next_line().await? {
        if let Some((done, total)) = parse_counter(&line) {
            reporter.progress(done, total);
        }
        if line.contains("Failed") || line.contains("Invalid") || line.contains("Error") {
            reporter.log(&line);
        }
        tail.push(line);
        if tail.len() > 30 {
            tail.remove(0);
        }
    }
    let status = child.wait().await?;
    let stderr_tail = errors.await.unwrap_or_default();
    if !status.success() {
        bail!(
            "la synchronisation du pack a échoué ({status}).\nURL : {pack_url}\n{}\n{}",
            tail.join("\n"),
            stderr_tail.join("\n")
        );
    }
    Ok(())
}

/// Écrit le choix des mods optionnels. Rend `true` si quelque chose a changé
/// (il faut alors resynchroniser). Un fichier optionnel absent de
/// `packwiz.json` (pas encore téléchargé) sera traité au passage suivant.
pub fn set_optional(paths: &Paths, all_optional: &HashSet<String>, enabled: &HashSet<String>) -> Result<bool> {
    let p = paths.instance().join("packwiz.json");
    let Ok(text) = std::fs::read_to_string(&p) else { return Ok(false) };
    let mut json: Value = serde_json::from_str(&text).context("packwiz.json illisible")?;
    let mut changed = false;
    if let Some(files) = json.get_mut("cachedFiles").and_then(|v| v.as_object_mut()) {
        for path in all_optional {
            if let Some(entry) = files.get_mut(path).and_then(|v| v.as_object_mut()) {
                let want = enabled.contains(path);
                if entry.get("optionValue").and_then(|v| v.as_bool()) != Some(want) {
                    entry.insert("optionValue".into(), Value::Bool(want));
                    changed = true;
                }
            }
        }
    }
    if changed {
        if let Some(o) = json.as_object_mut() {
            o.remove("packFileHash");
            o.remove("indexFileHash");
        }
        std::fs::write(&p, serde_json::to_string_pretty(&json)?)?;
    }
    Ok(changed)
}

/// Versions de Minecraft et NeoForge demandées par le pack (`[versions]`).
pub async fn pack_versions(pack_url: &str) -> Result<(String, String)> {
    let text = crate::net::fetch_text(&crate::net::client(), pack_url).await.context("lecture de pack.toml")?;
    let v: toml::Value = toml::from_str(&text).context("pack.toml invalide")?;
    let versions = v.get("versions").context("pack.toml sans [versions]")?;
    let get = |k: &str| versions.get(k).and_then(|x| x.as_str()).map(String::from);
    Ok((
        get("minecraft").context("version de Minecraft absente de pack.toml")?,
        get("neoforge").context("version de NeoForge absente de pack.toml")?,
    ))
}

/// Version du pack affichée par le launcher (`version` de pack.toml).
pub fn installed_pack_version(paths: &Paths) -> Option<String> {
    let text = std::fs::read_to_string(paths.instance().join("config/turicraft/version.json")).ok()?;
    let v: Value = serde_json::from_str(&text).ok()?;
    v.get("version")?.as_str().map(String::from)
}
