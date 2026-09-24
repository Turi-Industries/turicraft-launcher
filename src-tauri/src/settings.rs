//! Réglages du launcher, dans `settings.json`. Aucun secret ici : le jeton de
//! renouvellement Microsoft va dans le trousseau du système (auth.rs).

use std::collections::BTreeMap;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::paths::Paths;

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Settings {
    /// « auto », « faible », « moyen », « haut », « ultra » ou « personnalise »
    /// (mode Avancé).
    pub preset: String,
    /// Personnalisé : préréglage de départ, groupes cochés, mémoire.
    pub custom_base: Option<String>,
    pub custom_groups: Vec<String>,
    pub custom_memory_gb: Option<u64>,
    /// Options du jeu (shaders, vue lointaine…) changées par le joueur ;
    /// absentes = valeur conseillée par le préréglage.
    pub toggles: BTreeMap<String, bool>,
    /// Mods optionnels activés ou coupés un par un (section Mods).
    pub mods: BTreeMap<String, bool>,
    /// Curseurs (distances, images par seconde) changés par le joueur.
    pub sliders: BTreeMap<String, i64>,
    /// Empreinte des réglages appliqués au dernier lancement. Tant qu'elle ne
    /// change pas, le launcher ne réécrit rien : ce que le joueur a réglé EN
    /// JEU (distance, plein écran…) est gardé.
    pub applied: Option<String>,
    /// Version des réglages « une fois » (`[once]`) déjà posée.
    pub once_applied: u32,
    /// Rejoindre directement le serveur au lancement (désactivé par défaut :
    /// le joueur arrive sur le menu du pack).
    pub join_server: bool,
    /// Ce que fait le launcher quand le jeu est au menu : « reduire »,
    /// « garder » ou « fermer » (caché, puis quitté à la fin du jeu).
    pub launcher_behavior: String,
    /// Compte : nom et UUID Minecraft (publics). Le jeton n'est pas ici.
    pub account: Option<Account>,
    /// Durée de chaque jalon au lancement précédent, pour estimer le temps
    /// restant sur CETTE machine.
    pub last_milestones_ms: Vec<u64>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Account {
    pub name: String,
    pub uuid: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            preset: "auto".into(),
            custom_base: None,
            custom_groups: Vec::new(),
            custom_memory_gb: None,
            toggles: BTreeMap::new(),
            sliders: BTreeMap::new(),
            mods: BTreeMap::new(),
            applied: None,
            once_applied: 0,
            join_server: false,
            launcher_behavior: "reduire".into(),
            account: None,
            last_milestones_ms: Vec::new(),
        }
    }
}

impl Settings {
    pub fn load(paths: &Paths) -> Self {
        std::fs::read_to_string(paths.settings())
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, paths: &Paths) -> Result<()> {
        std::fs::create_dir_all(&paths.root)?;
        let tmp = paths.settings().with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(self)?)?;
        std::fs::rename(tmp, paths.settings())?;
        Ok(())
    }
}

// Ce que le joueur ne doit PAS pouvoir modifier (décision du 24/09) : ni dans
// l'interface, ni dans settings.json. Seules des variables d'environnement,
// pour les essais, les remplacent.

/// URL du pack. `TURICRAFT_PACK_URL` : essais contre un serveur de pack local.
pub fn pack_url() -> String {
    std::env::var("TURICRAFT_PACK_URL").unwrap_or_else(|_| crate::config::DEFAULT_PACK_URL.into())
}

/// Mode hors ligne, pour les essais seulement (`TURICRAFT_OFFLINE_NAME`) :
/// pas de compte Microsoft, et le serveur (online-mode) refuse la connexion.
pub fn offline_name() -> Option<String> {
    std::env::var("TURICRAFT_OFFLINE_NAME").ok().filter(|s| !s.trim().is_empty())
}

pub fn azure_client_id() -> Option<String> {
    crate::config::AZURE_CLIENT_ID.map(String::from)
}
