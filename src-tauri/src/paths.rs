//! Où le launcher range tout. Un seul dossier racine par joueur :
//!
//! ```text
//! <données>/turicraft/
//!   settings.json      réglages du launcher (aucun secret)
//!   runtime/           Java de Mojang
//!   minecraft/         versions, libraries, assets — partagés, comme ~/.minecraft
//!   instance/          le dossier de jeu (mods, config, saves…)
//!   tools/             packwiz-installer
//! ```
//!
//! `<données>` : `~/.local/share` (Linux), `~/Library/Application Support`
//! (macOS), `%APPDATA%` (Windows).

use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct Paths {
    pub root: PathBuf,
}

impl Paths {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// Dossier par défaut, ou `TURICRAFT_HOME` s'il est défini (essais).
    pub fn default_location() -> Self {
        if let Some(p) = std::env::var_os("TURICRAFT_HOME") {
            return Self::new(PathBuf::from(p));
        }
        let base = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
        Self::new(base.join("turicraft"))
    }

    pub fn settings(&self) -> PathBuf {
        self.root.join("settings.json")
    }
    pub fn runtime(&self) -> PathBuf {
        self.root.join("runtime")
    }
    pub fn minecraft(&self) -> PathBuf {
        self.root.join("minecraft")
    }
    pub fn versions(&self) -> PathBuf {
        self.minecraft().join("versions")
    }
    pub fn libraries(&self) -> PathBuf {
        self.minecraft().join("libraries")
    }
    pub fn assets(&self) -> PathBuf {
        self.minecraft().join("assets")
    }
    pub fn instance(&self) -> PathBuf {
        self.root.join("instance")
    }
    pub fn natives(&self) -> PathBuf {
        self.instance().join("natives")
    }
    pub fn tools(&self) -> PathBuf {
        self.root.join("tools")
    }
}
