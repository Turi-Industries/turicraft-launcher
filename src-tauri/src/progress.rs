//! Ce que le cœur raconte à l'interface (événements Tauri) ou au terminal
//! (turicraft-cli). Le cœur ne sait pas lequel des deux l'écoute.

use serde::Serialize;

#[derive(Serialize, Clone, Debug)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Event {
    /// Une nouvelle étape commence (« java », « minecraft », « pack »…).
    Stage { id: String, label: String },
    /// Avancement de l'étape en cours. `total = 0` : durée inconnue.
    Progress { done: u64, total: u64 },
    /// Une ligne pour le journal du launcher.
    Log { line: String },
    /// Démarrage du jeu : jalon atteint, et estimation d'après le lancement
    /// précédent (`expected_ms` = durée totale attendue, 0 si inconnue).
    Milestone { index: usize, count: usize, label: String, elapsed_ms: u64, expected_ms: u64 },
    /// Le jeu est au menu.
    GameReady { elapsed_ms: u64 },
    /// Le pilote graphique ralentit tout le jeu (diag::slow_gl_driver).
    GpuWarning { renderer: String, advice: String },
    /// Le jeu s'est arrêté.
    GameExited { code: Option<i32>, crash: Option<crate::diag::CrashSummary> },
}

pub trait Reporter: Send + Sync {
    fn send(&self, event: Event);

    fn stage(&self, id: &str, label: &str) {
        self.send(Event::Stage { id: id.into(), label: label.into() });
    }
    fn progress(&self, done: u64, total: u64) {
        self.send(Event::Progress { done, total });
    }
    fn log(&self, line: &str) {
        self.send(Event::Log { line: line.to_string() });
    }
}

/// Pour turicraft-cli : tout sur la sortie standard, la progression en place.
pub struct ConsoleReporter;

impl Reporter for ConsoleReporter {
    fn send(&self, event: Event) {
        match event {
            Event::Stage { label, .. } => println!("\n== {label}"),
            Event::Progress { done, total } if total > 0 => {
                if done == total || done % 50 == 0 {
                    println!("   {done}/{total}");
                }
            }
            Event::Progress { .. } => {}
            Event::Log { line } => println!("   {line}"),
            Event::Milestone { index, count, label, elapsed_ms, .. } => {
                println!("   [{}/{count}] {label} ({:.1} s)", index + 1, elapsed_ms as f64 / 1000.0)
            }
            Event::GpuWarning { renderer, advice } => println!("   ATTENTION ({renderer}) : {advice}"),
            Event::GameReady { elapsed_ms } => println!("   jeu au menu en {:.1} s", elapsed_ms as f64 / 1000.0),
            Event::GameExited { code, crash } => {
                println!("   jeu arrêté (code {code:?})");
                if let Some(c) = crash {
                    println!("   crash : {} — {}", c.description, c.cause);
                }
            }
        }
    }
}
