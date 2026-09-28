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
//! (macOS), `%APPDATA%` (Windows) — ou `%LOCALAPPDATA%` quand `%APPDATA%` est
//! redirigé sur un partage réseau (voir `is_network`).

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

/// Chemin réseau Windows (`\\serveur\partage\…`) : `%APPDATA%` redirigé par
/// un profil itinérant (PC d'école, d'entreprise). L'installeur NeoForge y
/// meurt (« URI has an authority component » : `new File(URL de son jar)`
/// refuse `file://serveur/…`), et 2 Go de jeu n'ont rien à faire sur le
/// réseau. `%LOCALAPPDATA%`, lui, reste local par conception.
fn is_network(p: &Path) -> bool {
    let s = p.to_string_lossy().replace('/', "\\");
    s.starts_with(r"\\?\UNC\") || (s.starts_with(r"\\") && !s.starts_with(r"\\?\") && !s.starts_with(r"\\.\"))
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
        let mut base = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
        if is_network(&base) {
            if let Some(local) = dirs::data_local_dir().filter(|p| !is_network(p)) {
                base = local;
            }
        }
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
    use super::{is_network, safe_join};
    use std::path::Path;

    #[test]
    fn appdata_sur_le_reseau() {
        assert!(is_network(Path::new(r"\\srv-eleves\profils$\joueur\AppData\Roaming")));
        assert!(is_network(Path::new(r"\\?\UNC\srv\partage\AppData")));
        assert!(!is_network(Path::new(r"C:\Users\joueur\AppData\Roaming")));
        assert!(!is_network(Path::new(r"\\?\C:\Users\joueur")));
        assert!(!is_network(Path::new("/home/joueur/.local/share")));
    }

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
