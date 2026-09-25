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

use std::path::{Component, Path, PathBuf};

/// `base/rel`, pour un chemin relatif venu du réseau (manifeste Java de
/// Mojang, bibliothèques, presets.toml) : refusé s'il est absolu ou remonte
/// (`..`) — un fichier mal formé ou trafiqué ne doit rien écrire hors de
/// `base`.
pub fn safe_join(base: &Path, rel: impl AsRef<Path>) -> anyhow::Result<PathBuf> {
    let rel = rel.as_ref();
    if rel.as_os_str().is_empty() || !rel.components().all(|c| matches!(c, Component::Normal(_) | Component::CurDir)) {
        anyhow::bail!("chemin refusé (hors du dossier prévu) : {}", rel.display());
    }
    Ok(base.join(rel))
}

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

#[cfg(test)]
mod tests {
    use super::safe_join;
    use std::path::Path;

    #[test]
    fn chemins_venus_du_reseau() {
        let base = Path::new("/jeu");
        assert_eq!(safe_join(base, "config/a.toml").unwrap(), Path::new("/jeu/config/a.toml"));
        assert!(safe_join(base, "../settings.json").is_err());
        assert!(safe_join(base, "config/../../x").is_err());
        assert!(safe_join(base, "/etc/passwd").is_err());
        assert!(safe_join(base, "").is_err());
    }
}
