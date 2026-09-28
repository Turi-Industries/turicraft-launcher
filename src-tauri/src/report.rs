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
//!
//! Rapport à la demande (« Envoyer un rapport », écran Journal) : une
//! déconnexion, un gel ou un bug en jeu ne ferment pas le jeu en erreur, et
//! rien ne part tout seul. Même contenu sans crash-report, avec ce que le
//! joueur a choisi comme raison. Jamais deux fois le même journal :
//! l'empreinte de `latest.log` est gardée après l'envoi (`dernier-rapport.json`,
//! et celle du dernier crash), et le serveur rend le numéro d'un rapport
//! identique déjà reçu.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

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
    /// Empreinte de `latest.log` au moment du rapport (`log_fingerprint`).
    #[serde(default)]
    pub log_fingerprint: Option<String>,
}

fn saved(paths: &Paths) -> PathBuf {
    paths.root.join("dernier-crash.json")
}

fn latest_log(paths: &Paths) -> PathBuf {
    paths.instance().join("logs/latest.log")
}

/// Empreinte de `latest.log` tel qu'il est sur le disque. Tant qu'elle ne
/// change pas, c'est le même journal : inutile de le renvoyer.
pub fn log_fingerprint(paths: &Paths) -> Option<String> {
    let bytes = std::fs::read(latest_log(paths)).ok()?;
    Some(hex::encode(Sha256::digest(&bytes)))
}

/// Raisons proposées au joueur (choix fermé) : identifiant → libellé.
pub const REASONS: [(&str, &str); 4] = [
    ("deconnexion", "Déconnecté du serveur"),
    ("fige", "Jeu figé ou très lent"),
    ("bug", "Bug en jeu"),
    ("autre", "Autre problème"),
];

/// Qui envoie : le compte du launcher, et le jeton de la dernière session
/// s'il est en mémoire (à masquer s'il traîne dans un journal).
pub struct Identity<'a> {
    pub name: &'a str,
    pub uuid: &'a str,
    pub access_token: Option<&'a str>,
}

/// Ce qui est commun aux deux rapports : qui, quelle machine, quels réglages.
fn base_meta(paths: &Paths, settings: &Settings, who: &Identity) -> serde_json::Value {
    serde_json::json!({
        "time": SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0),
        "player": { "name": who.name, "uuid": who.uuid },
        "launcher": crate::config::LAUNCHER_VERSION,
        "pack": crate::packwiz::installed_pack_version(paths),
        "os": {
            "family": std::env::consts::OS,
            "arch": std::env::consts::ARCH,
            "version": sysinfo::System::long_os_version(),
        },
        "hardware": crate::hardware::detect(),
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
    })
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
    let who = Identity { name: &session.name, uuid: &session.uuid, access_token: Some(&session.access_token) };
    let scrub = Scrub::new(who.access_token);
    let mut files = BTreeMap::new();
    if let Some(path) = &crash.report {
        let name = if crash.native { "hs_err.log" } else { "crash-report.txt" };
        if let Some(text) = read_capped(Path::new(path), MAX_REPORT) {
            let text = if crash.native { without_environment(&text) } else { text };
            files.insert(name.to_string(), scrub.apply(&text));
        }
    }
    if let Some(text) = read_capped(&latest_log(paths), MAX_LOG) {
        files.insert("latest.log".to_string(), scrub.apply(&text));
    }
    let crash_json = serde_json::to_value(crash).unwrap_or_default();
    let mut meta = base_meta(paths, settings, &who);
    meta["kind"] = "crash".into();
    meta["exit_code"] = code.into();
    meta["elapsed_s"] = elapsed.as_secs().into();
    // Jusqu'où le démarrage est allé : 7/7 = le jeu était au menu.
    meta["milestones"] = format!("{milestones_reached}/{milestones}").into();
    meta["crash"] = scrub.apply_json(crash_json);
    Report { meta, files, sent_id: None, log_fingerprint: log_fingerprint(paths) }
}

/// Rapport à la demande : `latest.log` du jeu (en cours ou dernier lancé),
/// la machine, les réglages, la raison choisie par le joueur.
pub fn collect_manual(paths: &Paths, settings: &Settings, who: &Identity, reason: &str, game_running: bool) -> Result<Report> {
    let label = REASONS.iter().find(|(id, _)| *id == reason).map(|(_, l)| *l).context("raison inconnue")?;
    let scrub = Scrub::new(who.access_token);
    let text = read_capped(&latest_log(paths), MAX_LOG).context("aucun journal du jeu : lance le jeu une fois")?;
    let mut files = BTreeMap::new();
    files.insert("latest.log".to_string(), scrub.apply(&text));
    let mut meta = base_meta(paths, settings, who);
    meta["kind"] = "manual".into();
    meta["reason"] = label.into();
    meta["game_running"] = game_running.into();
    Ok(Report { meta, files, sent_id: None, log_fingerprint: log_fingerprint(paths) })
}

/// Dernier rapport à la demande envoyé : son journal et son numéro.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
struct Sent {
    fingerprint: String,
    id: String,
}

fn sent_path(paths: &Paths) -> PathBuf {
    paths.root.join("dernier-rapport.json")
}

/// Numéro du rapport déjà envoyé avec le `latest.log` actuel (à la demande,
/// ou avec le dernier crash), s'il y en a un.
pub fn already_sent(paths: &Paths) -> Option<String> {
    let fp = log_fingerprint(paths)?;
    let manual: Option<Sent> = std::fs::read(sent_path(paths)).ok().and_then(|b| serde_json::from_slice(&b).ok());
    if let Some(s) = manual.filter(|s| s.fingerprint == fp) {
        return Some(s.id);
    }
    let crash = Report::load(paths)?;
    if crash.log_fingerprint.as_deref() == Some(fp.as_str()) {
        return crash.sent_id;
    }
    None
}

/// Envoie un rapport à la demande et garde l'empreinte de son journal.
pub async fn send_manual(paths: &Paths, pack_url: &str, report: &Report) -> Result<String> {
    let id = send(pack_url, report).await?;
    if let Some(fingerprint) = &report.log_fingerprint {
        let sent = Sent { fingerprint: fingerprint.clone(), id: id.clone() };
        std::fs::create_dir_all(&paths.root)?;
        std::fs::write(sent_path(paths), serde_json::to_vec(&sent)?)?;
    }
    Ok(id)
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
    fn new(access_token: Option<&str>) -> Self {
        let mut pairs = Vec::new();
        if let Some(token) = access_token.filter(|t| t.len() >= 8) {
            pairs.push((token.to_string(), "***".to_string()));
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
        assert_eq!(rep.meta["kind"], "crash");
        assert!(rep.log_fingerprint.is_some());
        rep.save(&paths).unwrap();
        let back = Report::load(&paths).unwrap();
        assert_eq!(back.files, rep.files);
        assert!(back.sent_id.is_none());
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn rapport_a_la_demande_jamais_deux_fois() {
        let dir = std::env::temp_dir().join(format!("turicraft-rapport-manuel-{}", std::process::id()));
        let paths = Paths::new(dir.clone());
        let who = Identity { name: "Joueur", uuid: "u", access_token: Some("eyJsecretjeton") };
        // Pas encore de journal : rien à envoyer.
        assert!(collect_manual(&paths, &Settings::default(), &who, "fige", true).is_err());
        std::fs::create_dir_all(paths.instance().join("logs")).unwrap();
        std::fs::write(paths.instance().join("logs/latest.log"), "Timed out eyJsecretjeton\n").unwrap();
        assert!(collect_manual(&paths, &Settings::default(), &who, "inventee", true).is_err());
        let rep = collect_manual(&paths, &Settings::default(), &who, "deconnexion", true).unwrap();
        assert_eq!(rep.meta["kind"], "manual");
        assert_eq!(rep.meta["reason"], "Déconnecté du serveur");
        assert!(!rep.files["latest.log"].contains("eyJsecretjeton"));
        assert!(already_sent(&paths).is_none());
        // Envoi noté (sans réseau : ce que fait send_manual après la réponse).
        let sent = Sent { fingerprint: rep.log_fingerprint.clone().unwrap(), id: "ABC123".into() };
        std::fs::write(sent_path(&paths), serde_json::to_vec(&sent).unwrap()).unwrap();
        assert_eq!(already_sent(&paths).as_deref(), Some("ABC123"));
        // Le journal change (le jeu a continué) : nouveau rapport possible.
        std::fs::write(paths.instance().join("logs/latest.log"), "Timed out\nencore\n").unwrap();
        assert!(already_sent(&paths).is_none());
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn journal_deja_envoye_avec_le_crash() {
        let dir = std::env::temp_dir().join(format!("turicraft-rapport-crash-{}", std::process::id()));
        let paths = Paths::new(dir.clone());
        std::fs::create_dir_all(paths.instance().join("logs")).unwrap();
        std::fs::write(paths.instance().join("logs/latest.log"), "crash\n").unwrap();
        let rep = Report { meta: serde_json::json!({}), files: BTreeMap::new(), sent_id: Some("C0FFEE".into()), log_fingerprint: log_fingerprint(&paths) };
        rep.save(&paths).unwrap();
        assert_eq!(already_sent(&paths).as_deref(), Some("C0FFEE"));
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn jeton_et_dossier_personnel_masques() {
        let session = Session { name: "Jean".into(), uuid: "u".into(), access_token: "eyJsecretjeton".into(), xuid: String::new() };
        let scrub = Scrub::new(Some(&session.access_token));
        let home = dirs::home_dir().unwrap().display().to_string();
        let out = scrub.apply(&format!("--accessToken eyJsecretjeton dans {home}/turicraft/instance"));
        assert!(!out.contains("eyJsecretjeton") && !out.contains(&home), "{out}");
        assert!(out.contains("~/turicraft/instance"));
        let j = scrub.apply_json(serde_json::json!({ "report": format!("{home}/crash.txt"), "n": 3 }));
        assert_eq!(j["report"], "~/crash.txt");
        assert_eq!(j["n"], 3);
    }
}
