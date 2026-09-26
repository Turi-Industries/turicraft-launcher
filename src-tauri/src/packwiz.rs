//! Synchronisation du pack : packwiz-installer, sans interface, dans le dossier
//! de jeu. Mêmes jars et mêmes options que la commande de pré-lancement de
//! Prism (`scripts/make-prism-instance.sh`).
//!
//! Mods optionnels : packwiz-installer retient le choix de chacun dans
//! `packwiz.json` (`cachedFiles[<fichier .pw.toml>].optionValue`). Le launcher
//! y écrit le choix du préréglage, puis vide `packFileHash`/`indexFileHash` :
//! sans ça, packwiz-installer voit un pack inchangé et ne revérifie rien
//! (vérifié le 24/09 : désactivé → jar supprimé, réactivé → retéléchargé).

use std::collections::{BTreeMap, HashSet};
use std::path::Path;
use std::process::Stdio;

use anyhow::{bail, Context, Result};
use serde_json::Value;

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
        .args(crate::lines::JAVA_UTF8)
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
        let mut lines = crate::lines::Lines::new(stderr);
        let mut kept = Vec::new();
        while let Ok(Some(l)) = lines.next_line().await {
            kept.push(l);
            if kept.len() > 30 {
                kept.remove(0);
            }
        }
        kept
    });
    let mut lines = crate::lines::Lines::new(child.stdout.take().unwrap());
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
/// Pour chaque fichier demandé, le hash de sa version DANS LE PACK, tel que
/// packwiz-installer l'a noté (`packwiz.json`, `cachedFiles`). Il change quand
/// une mise à jour du pack remplace le fichier — pas quand le launcher ou le
/// joueur le modifie ensuite.
pub fn pack_hashes(paths: &Paths, files: &std::collections::BTreeSet<String>) -> BTreeMap<String, String> {
    let manifest: Value = std::fs::read_to_string(paths.instance().join("packwiz.json"))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default();
    files
        .iter()
        .filter_map(|f| {
            let h = manifest["cachedFiles"][f]["hash"]["value"].as_str()?;
            Some((f.clone(), h.to_string()))
        })
        .collect()
}

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

/// Dossier (dans l'instance) où vont les mods ajoutés à la main.
pub const MODS_SET_ASIDE: &str = "mods-desactives";

/// Met de côté les jars (et zips) du dossier `mods/` que le pack n'a pas
/// installés : un mod ajouté à la main. Le serveur refuse un joueur qui a un
/// mod absent du pack (anti-triche, turicraft-compat) ; autant le dire avant
/// la connexion, et le jeu reste identique pour tout le monde en solo aussi.
///
/// Le pack, c'est `packwiz.json` : `cachedLocation` de chaque fichier
/// installé. Sans manifeste lisible, ou sans aucun mod dedans, on ne touche à
/// rien (mieux vaut laisser un mod de trop que vider le dossier). Les fichiers
/// vont dans `mods-desactives/`, jamais supprimés. Rend leurs noms.
pub fn set_aside_unknown_mods(paths: &Paths) -> Result<Vec<String>> {
    let instance = paths.instance();
    let Ok(text) = std::fs::read_to_string(instance.join("packwiz.json")) else {
        return Ok(Vec::new());
    };
    let manifest: Value = serde_json::from_str(&text).context("packwiz.json illisible")?;
    let known: HashSet<String> = manifest["cachedFiles"]
        .as_object()
        .map(|files| {
            files
                .values()
                .filter_map(|f| f["cachedLocation"].as_str())
                .filter_map(|l| l.strip_prefix("mods/"))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    if known.is_empty() {
        return Ok(Vec::new());
    }
    let Ok(entries) = std::fs::read_dir(instance.join("mods")) else {
        return Ok(Vec::new());
    };
    let mut moved = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let lower = name.to_lowercase();
        let is_mod = lower.ends_with(".jar") || lower.ends_with(".zip");
        if !is_mod || known.contains(&name) || !entry.file_type().is_ok_and(|t| t.is_file()) {
            continue;
        }
        let dir = instance.join(MODS_SET_ASIDE);
        std::fs::create_dir_all(&dir).context("création de mods-desactives")?;
        // Un fichier du même nom déjà mis de côté : on ne l'écrase pas.
        let mut dest = dir.join(&name);
        let mut n = 2;
        while dest.exists() {
            dest = dir.join(format!("{n}-{name}"));
            n += 1;
        }
        std::fs::rename(entry.path(), &dest).with_context(|| format!("déplacement de {name}"))?;
        moved.push(name);
    }
    moved.sort();
    Ok(moved)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_des_fichiers_du_pack() {
        let dir = std::env::temp_dir().join(format!("turicraft-pw-{}", std::process::id()));
        let paths = Paths::new(dir.clone());
        std::fs::create_dir_all(paths.instance()).unwrap();
        std::fs::write(
            paths.instance().join("packwiz.json"),
            r#"{"cachedFiles":{"config/DistantHorizons.toml":{"hash":{"type":"sha256","value":"abc"}}}}"#,
        )
        .unwrap();
        let wanted = ["config/DistantHorizons.toml".to_string(), "config/absent.toml".to_string()].into();
        let h = pack_hashes(&paths, &wanted);
        assert_eq!(h.len(), 1);
        assert_eq!(h["config/DistantHorizons.toml"], "abc");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn mods_ajoutes_mis_de_cote() {
        let dir = std::env::temp_dir().join(format!("turicraft-pw-aside-{}", std::process::id()));
        let paths = Paths::new(dir.clone());
        let mods = paths.instance().join("mods");
        std::fs::create_dir_all(&mods).unwrap();
        // Sans manifeste : rien ne bouge.
        std::fs::write(mods.join("triche.jar"), b"x").unwrap();
        assert!(set_aside_unknown_mods(&paths).unwrap().is_empty());
        assert!(mods.join("triche.jar").exists());

        std::fs::write(
            paths.instance().join("packwiz.json"),
            r#"{"cachedFiles":{"mods/sodium.pw.toml":{"cachedLocation":"mods/sodium.jar"},
                "config/a.toml":{"cachedLocation":"config/a.toml"}}}"#,
        )
        .unwrap();
        std::fs::write(mods.join("sodium.jar"), b"x").unwrap();
        std::fs::write(mods.join("notes.txt"), b"x").unwrap();
        std::fs::create_dir_all(paths.instance().join(MODS_SET_ASIDE)).unwrap();
        std::fs::write(paths.instance().join(MODS_SET_ASIDE).join("triche.jar"), b"ancien").unwrap();

        assert_eq!(set_aside_unknown_mods(&paths).unwrap(), vec!["triche.jar".to_string()]);
        assert!(mods.join("sodium.jar").exists(), "un mod du pack reste");
        assert!(mods.join("notes.txt").exists(), "ce qui n'est pas un mod reste");
        assert!(!mods.join("triche.jar").exists());
        let aside = paths.instance().join(MODS_SET_ASIDE);
        assert_eq!(std::fs::read(aside.join("triche.jar")).unwrap(), b"ancien", "rien n'est écrasé");
        assert!(aside.join("2-triche.jar").exists());
        std::fs::remove_dir_all(&dir).ok();
    }
}
