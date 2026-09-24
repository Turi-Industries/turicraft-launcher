//! Après un arrêt anormal du jeu : ce qu'on peut en dire au joueur (et à l'équipe du serveur)
//! sans lui demander d'ouvrir des fichiers. Même logique que
//! scripts/diag-client.sh : le crash-report nomme le SYMPTÔME, la première
//! vraie erreur de latest.log nomme souvent le COUPABLE.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::Serialize;

#[derive(Serialize, Clone, Debug)]
pub struct CrashSummary {
    /// « Rendering screen », « Exception while adding particle »…
    pub description: String,
    /// Première ligne de l'exception.
    pub cause: String,
    pub report: Option<String>,
    /// Première ERROR/FATAL non bénigne de latest.log.
    pub first_error: Option<String>,
    /// FML en « broken mod state » : les erreurs suivantes sont des cascades.
    pub cascade: bool,
    pub native: bool,
}

/// Lignes loggées en ERROR mais sans valeur diagnostique (diag-client.sh).
const BENIGN: &[&str] = &[
    "glfwInit took",
    "Cowardly refusing to send event",
    "Negative index in crash report handler",
    "Error loading class:",
    "Unsupported installed optional dependencies",
    "Access transformer file META-INF/accesstransformer.cfg provided by mod",
    "Unsupported Uniform Type",
];

fn newest(dir: &Path, prefix: &str, suffix: &str, since: SystemTime) -> Option<PathBuf> {
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter(|e| {
            let n = e.file_name().to_string_lossy().to_string();
            n.starts_with(prefix) && n.ends_with(suffix)
        })
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .filter(|(t, _)| *t >= since)
        .max_by_key(|(t, _)| *t)
        .map(|(_, p)| p)
}

pub fn analyze(game_dir: &Path, since: SystemTime) -> Option<CrashSummary> {
    let log = std::fs::read_to_string(game_dir.join("logs/latest.log")).unwrap_or_default();
    let first_error = log
        .lines()
        .find(|l| (l.contains("/ERROR]") || l.contains("/FATAL]")) && !BENIGN.iter().any(|b| l.contains(b)))
        .map(|l| l.chars().take(300).collect());
    let cascade = log.contains("broken mod state");

    // Crash natif de la JVM : ni crash-report, ni fin de latest.log.
    if let Some(h) = newest(game_dir, "hs_err_pid", ".log", since) {
        let text = std::fs::read_to_string(&h).unwrap_or_default();
        let frame = text
            .lines()
            .skip_while(|l| !l.contains("Problematic frame"))
            .nth(1)
            .unwrap_or("")
            .trim_start_matches('#')
            .trim()
            .to_string();
        return Some(CrashSummary {
            description: "Plantage de Java (code natif)".into(),
            cause: frame,
            report: Some(h.display().to_string()),
            first_error,
            cascade,
            native: true,
        });
    }

    let report = newest(&game_dir.join("crash-reports"), "crash-", ".txt", since)?;
    let text = std::fs::read_to_string(&report).unwrap_or_default();
    let mut lines = text.lines();
    let description = lines
        .by_ref()
        .find_map(|l| l.strip_prefix("Description: "))
        .unwrap_or("?")
        .to_string();
    let cause = lines.find(|l| !l.trim().is_empty()).unwrap_or("").chars().take(300).collect();
    Some(CrashSummary {
        description,
        cause,
        report: Some(report.display().to_string()),
        first_error,
        cascade,
        native: false,
    })
}
