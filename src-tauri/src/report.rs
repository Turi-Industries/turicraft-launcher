//! Rapport de crash envoyé à l'équipe du serveur : le service `/crash/` du
//! serveur du pack le range pour qu'on puisse l'analyser sans demander au
//! joueur d'aller chercher des fichiers.
//!
//! Contenu : le crash-report (ou le hs_err d'un crash natif), `latest.log`
//! (début et fin s'il est trop gros), la machine, les réglages du launcher,
//! le pseudo. Nettoyé avant l'envoi : dossier personnel remplacé par `~`,
//! jeton de session masqué, variables d'environnement du hs_err retirées.
//!
//! Le rapport est d'abord enregistré (`dernier-crash.json`) : le joueur qui
//! a coupé l'envoi automatique, ou dont l'envoi a échoué, peut l'envoyer
//! ensuite ; au lancement suivant, les journaux auront changé.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::auth::Session;
use crate::diag::CrashSummary;
use crate::paths::Paths;
use crate::settings::Settings;

/// `latest.log` au-delà : son début (le chargement des mods, la première
/// erreur) et sa fin (le crash).
const MAX_LOG: usize = 6 * 1024 * 1024;
const LOG_HEAD: usize = 1024 * 1024;
/// Crash-report, hs_err : quelques centaines de Ko d'ordinaire.
const MAX_REPORT: usize = 2 * 1024 * 1024;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Report {
    /// Qui, quelle machine, quelles versions, quel crash.
    pub meta: serde_json::Value,
    /// Nom → contenu (texte).
    pub files: BTreeMap<String, String>,
    /// Numéro rendu par le serveur, une fois envoyé.
    #[serde(default)]
    pub sent_id: Option<String>,
}

fn saved(paths: &Paths) -> PathBuf {
    paths.root.join("dernier-crash.json")
}

#[allow(clippy::too_many_arguments)]
pub fn collect(
    paths: &Paths,
    settings: &Settings,
    session: &Session,
    crash: &CrashSummary,
    code: Option<i32>,
    elapsed: Duration,
    milestones_reached: usize,
    milestones: usize,
) -> Report {
    let scrub = Scrub::new(session);
    let mut files = BTreeMap::new();
    if let Some(path) = &crash.report {
        let name = if crash.native { "hs_err.log" } else { "crash-report.txt" };
        if let Some(text) = read_capped(Path::new(path), MAX_REPORT) {
            let text = if crash.native { without_environment(&text) } else { text };
            files.insert(name.to_string(), scrub.apply(&text));
        }
    }
    if let Some(text) = read_capped(&paths.instance().join("logs/latest.log"), MAX_LOG) {
        files.insert("latest.log".to_string(), scrub.apply(&text));
    }
    let hw = crate::hardware::detect();
    let crash_json = serde_json::to_value(crash).unwrap_or_default();
    let meta = serde_json::json!({
        "time": SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0),
        "player": { "name": session.name, "uuid": session.uuid },
        "launcher": crate::config::LAUNCHER_VERSION,
        "pack": crate::packwiz::installed_pack_version(paths),
        "os": {
            "family": std::env::consts::OS,
            "arch": std::env::consts::ARCH,
            "version": sysinfo::System::long_os_version(),
        },
        "hardware": hw,
        "settings": {
            "preset": settings.preset,
            "custom_base": settings.custom_base,
            "custom_memory_gb": settings.custom_memory_gb,
            "toggles": settings.toggles,
            "sliders": settings.sliders,
            "mods": settings.mods,
            "choices": settings.choices,
            "join_server": settings.join_server,
        },
        "exit_code": code,
        "elapsed_s": elapsed.as_secs(),
        // Jusqu'où le démarrage est allé : 7/7 = le jeu était au menu.
        "milestones": format!("{milestones_reached}/{milestones}"),
        "crash": scrub.apply_json(crash_json),
    });
    Report { meta, files, sent_id: None }
}

impl Report {
    pub fn save(&self, paths: &Paths) -> Result<()> {
        std::fs::create_dir_all(&paths.root)?;
        let tmp = saved(paths).with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec(self)?)?;
        std::fs::rename(&tmp, saved(paths))?;
        Ok(())
    }

    pub fn load(paths: &Paths) -> Option<Self> {
        serde_json::from_slice(&std::fs::read(saved(paths)).ok()?).ok()
    }
}

/// `https://…/pack.toml` → `https://…/crash`.
fn base(pack_url: &str) -> String {
    let root = pack_url.rsplit_once('/').map_or(pack_url, |(b, _)| b);
    format!("{root}/crash")
}

/// Envoie le rapport, rend son numéro. Compressé : un `latest.log` de 6 Mo
/// en fait moins d'un.
pub async fn send(pack_url: &str, report: &Report) -> Result<String> {
    let body = serde_json::to_vec(&serde_json::json!({ "meta": report.meta, "files": report.files }))?;
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    gz.write_all(&body)?;
    let gz = gz.finish()?;
    let resp = crate::net::client()
        .post(format!("{}/report", base(pack_url)))
        .header("Content-Type", "application/json")
        .header("Content-Encoding", "gzip")
        .timeout(Duration::from_secs(60))
        .body(gz)
        .send()
        .await
        .context("serveur des rapports injoignable")?;
    let status = resp.status();
    let v: serde_json::Value = resp.json().await.unwrap_or_default();
    if !status.is_success() {
        bail!("{}", v["error"].as_str().map(String::from).unwrap_or_else(|| format!("HTTP {status}")));
    }
    v["id"].as_str().map(String::from).context("réponse du serveur sans numéro")
}

/// Envoie le rapport enregistré et note son numéro. Déjà envoyé : rend le
/// même numéro sans renvoyer.
pub async fn send_saved(paths: &Paths, pack_url: &str) -> Result<String> {
    let mut report = Report::load(paths).context("aucun rapport de crash enregistré")?;
    if let Some(id) = &report.sent_id {
        return Ok(id.clone());
    }
    let id = send(pack_url, &report).await?;
    report.sent_id = Some(id.clone());
    report.save(paths)?;
    Ok(id)
}

/// Lit un fichier texte ; au-delà de `max` octets, son début et sa fin.
fn read_capped(path: &Path, max: usize) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    Some(cap(&bytes, max))
}

fn cap(bytes: &[u8], max: usize) -> String {
    if bytes.len() <= max {
        return String::from_utf8_lossy(bytes).into_owned();
    }
    let head = LOG_HEAD.min(max / 2);
    let tail = max - head;
    format!(
        "{}\n\n[… {} octets coupés par le launcher …]\n\n{}",
        String::from_utf8_lossy(&bytes[..head]),
        bytes.len() - head - tail,
        String::from_utf8_lossy(&bytes[bytes.len() - tail..])
    )
}

/// Le hs_err de la JVM recopie toutes les variables d'environnement (nom
/// d'utilisateur, chemins, parfois des jetons) : section retirée.
fn without_environment(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut skipping = false;
    for line in text.lines() {
        if line.starts_with("Environment Variables:") {
            skipping = true;
            out.push_str("Environment Variables: [retirées par le launcher]\n");
            continue;
        }
        if skipping {
            if line.trim().is_empty() {
                skipping = false;
            } else {
                continue;
            }
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

/// Ce qui ne doit pas quitter le PC du joueur.
struct Scrub {
    /// (à chercher, remplacement), les plus longs d'abord.
    pairs: Vec<(String, String)>,
}

impl Scrub {
    fn new(session: &Session) -> Self {
        let mut pairs = Vec::new();
        if session.access_token.len() >= 8 {
            pairs.push((session.access_token.clone(), "***".to_string()));
        }
        if let Some(home) = dirs::home_dir() {
            let h = home.display().to_string();
            // Java écrit les chemins Windows avec \ ou /, et parfois doublés.
            for variant in [h.clone(), h.replace('\\', "/"), h.replace('\\', "\\\\")] {
                if variant.len() >= 3 && !pairs.iter().any(|(p, _)| *p == variant) {
                    pairs.push((variant, "~".to_string()));
                }
            }
        }
        pairs.sort_by_key(|(p, _)| std::cmp::Reverse(p.len()));
        Self { pairs }
    }

    fn apply(&self, text: &str) -> String {
        let mut out = text.to_string();
        for (from, to) in &self.pairs {
            if out.contains(from.as_str()) {
                out = out.replace(from.as_str(), to);
            }
        }
        out
    }

    fn apply_json(&self, v: serde_json::Value) -> serde_json::Value {
        match v {
            serde_json::Value::String(s) => serde_json::Value::String(self.apply(&s)),
            serde_json::Value::Array(a) => a.into_iter().map(|x| self.apply_json(x)).collect(),
            serde_json::Value::Object(o) => o.into_iter().map(|(k, x)| (k, self.apply_json(x))).collect(),
            other => other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adresse_du_service() {
        assert_eq!(base("https://pack.turi-industries.eu/pack.toml"), "https://pack.turi-industries.eu/crash");
    }

    #[test]
    fn journal_trop_gros_coupe_au_milieu() {
        let bytes: Vec<u8> = (0..100u8).collect::<Vec<_>>().repeat(10); // 1000 octets
        let text = cap(&bytes, 400);
        assert!(text.contains("[… 600 octets coupés par le launcher …]"));
        assert_eq!(cap(b"court", 400), "court");
    }

    #[test]
    fn variables_d_environnement_retirees() {
        let hs = "Problematic frame:\n# C  [libc.so]\n\nEnvironment Variables:\nUSERNAME=jean\nPATH=C:\\x\n\nSignal Handlers:\n";
        let out = without_environment(hs);
        assert!(!out.contains("USERNAME"), "{out}");
        assert!(out.contains("Problematic frame") && out.contains("Signal Handlers:"));
    }

    #[test]
    fn rapport_rassemble_et_enregistre() {
        let dir = std::env::temp_dir().join(format!("turicraft-rapport-{}", std::process::id()));
        let paths = Paths::new(dir.clone());
        let game = paths.instance();
        std::fs::create_dir_all(game.join("crash-reports")).unwrap();
        std::fs::create_dir_all(game.join("logs")).unwrap();
        let report_path = game.join("crash-reports/crash-2026-09-27_12.00.00-client.txt");
        std::fs::write(&report_path, "Description: Rendering screen\n\njava.lang.NullPointerException\n").unwrap();
        std::fs::write(game.join("logs/latest.log"), "[Render thread/ERROR] boum --accessToken eyJsecretjeton\n").unwrap();
        let session = Session { name: "Joueur".into(), uuid: "u".into(), access_token: "eyJsecretjeton".into(), xuid: String::new() };
        let crash = CrashSummary {
            description: "Rendering screen".into(),
            cause: "java.lang.NullPointerException".into(),
            report: Some(report_path.display().to_string()),
            first_error: None,
            cascade: false,
            native: false,
        };
        let rep = collect(&paths, &Settings::default(), &session, &crash, Some(1), Duration::from_secs(90), 7, 7);
        assert!(rep.files["crash-report.txt"].contains("Rendering screen"));
        assert!(!rep.files["latest.log"].contains("eyJsecretjeton"));
        assert_eq!(rep.meta["player"]["name"], "Joueur");
        assert_eq!(rep.meta["milestones"], "7/7");
        rep.save(&paths).unwrap();
        let back = Report::load(&paths).unwrap();
        assert_eq!(back.files, rep.files);
        assert!(back.sent_id.is_none());
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn jeton_et_dossier_personnel_masques() {
        let session = Session { name: "Jean".into(), uuid: "u".into(), access_token: "eyJsecretjeton".into(), xuid: String::new() };
        let scrub = Scrub::new(&session);
        let home = dirs::home_dir().unwrap().display().to_string();
        let out = scrub.apply(&format!("--accessToken eyJsecretjeton dans {home}/turicraft/instance"));
        assert!(!out.contains("eyJsecretjeton") && !out.contains(&home), "{out}");
        assert!(out.contains("~/turicraft/instance"));
        let j = scrub.apply_json(serde_json::json!({ "report": format!("{home}/crash.txt"), "n": 3 }));
        assert_eq!(j["report"], "~/crash.txt");
        assert_eq!(j["n"], 3);
    }
}
