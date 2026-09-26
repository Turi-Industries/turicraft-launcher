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
    pub custom_memory_gb: Option<f64>,
    /// Options du jeu (shaders, vue lointaine…) changées par le joueur ;
    /// absentes = valeur conseillée par le préréglage.
    pub toggles: BTreeMap<String, bool>,
    /// Mods optionnels activés ou coupés un par un (section Mods).
    pub mods: BTreeMap<String, bool>,
    /// Curseurs (distances, images par seconde) changés par le joueur.
    pub sliders: BTreeMap<String, i64>,
    /// Choix (pack de shaders…) faits par le joueur, ou repris du jeu.
    pub choices: BTreeMap<String, String>,
    /// Valeur de chaque choix au dernier lancement : si le jeu a changé
    /// depuis sans que le joueur touche au launcher, c'est le jeu qui a
    /// raison (launch::import_game_changes).
    pub choices_applied: BTreeMap<String, String>,
    /// Empreinte des réglages appliqués au dernier lancement. Tant qu'elle ne
    /// change pas, le launcher ne réécrit rien : ce que le joueur a réglé EN
    /// JEU (distance, plein écran…) est gardé.
    pub applied: Option<String>,
    /// Version des réglages « une fois » (`[once]`) déjà posée.
    pub once_applied: u32,
    /// Et celle des fichiers de `[once]` (`files_version`).
    #[serde(default)]
    pub once_files_applied: u32,
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
            choices: BTreeMap::new(),
            choices_applied: BTreeMap::new(),
            mods: BTreeMap::new(),
            applied: None,
            once_applied: 0,
            once_files_applied: 0,
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

    /// Ce que l'écran a le droit de changer : les choix du joueur. Le reste
    /// (réglages appliqués, réglages « une fois », compte, durées) est tenu
    /// par le cœur : l'écran en garde une copie qui peut dater — la renvoyer
    /// telle quelle effaçait ce que le dernier lancement avait noté (réglages
    /// faits en jeu écrasés, langue remise en français — 25/09).
    pub fn take_user_choices(&mut self, from: Settings) {
        self.preset = from.preset;
        self.custom_base = from.custom_base;
        self.custom_groups = from.custom_groups;
        self.custom_memory_gb = from.custom_memory_gb;
        self.toggles = from.toggles;
        self.mods = from.mods;
        self.sliders = from.sliders;
        self.choices = from.choices;
        self.join_server = from.join_server;
        self.launcher_behavior = from.launcher_behavior;
    }

    /// « Tout remettre à zéro » (Options) : comme à l'installation, sauf le
    /// compte et la durée des lancements sur cette machine (elle estime le
    /// temps restant, ce n'est pas un réglage).
    pub fn reset_keeping_account(&self) -> Self {
        Self { account: self.account.clone(), last_milestones_ms: self.last_milestones_ms.clone(), ..Default::default() }
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

#[cfg(test)]
mod tests {
    use super::*;

    /// L'écran renvoie une copie ancienne : ce que le cœur a noté depuis reste.
    #[test]
    fn l_ecran_ne_change_que_les_choix_du_joueur() {
        let mut core = Settings { applied: Some("neuf".into()), once_applied: 1, last_milestones_ms: vec![1, 2], account: Some(Account { name: "J".into(), uuid: "u".into() }), ..Default::default() };
        let ui = Settings { preset: "moyen".into(), join_server: true, applied: None, once_applied: 0, ..Default::default() };
        core.take_user_choices(ui);
        assert_eq!((core.preset.as_str(), core.join_server), ("moyen", true));
        assert_eq!((core.applied.as_deref(), core.once_applied, core.last_milestones_ms.len()), (Some("neuf"), 1, 2));
        assert!(core.account.is_some());
    }

    /// Remise à zéro : tout revient au défaut, sauf le compte et les durées.
    #[test]
    fn remise_a_zero_garde_le_compte() {
        let mut toggles = BTreeMap::new();
        toggles.insert("shaders".into(), false);
        let s = Settings {
            preset: "faible".into(),
            toggles,
            applied: Some("x".into()),
            once_applied: 2,
            once_files_applied: 1,
            join_server: true,
            launcher_behavior: "fermer".into(),
            account: Some(Account { name: "J".into(), uuid: "u".into() }),
            last_milestones_ms: vec![1, 2],
            ..Default::default()
        };
        let r = s.reset_keeping_account();
        assert_eq!(r.preset, "auto");
        assert!(r.toggles.is_empty() && r.applied.is_none() && !r.join_server);
        assert_eq!((r.once_applied, r.once_files_applied, r.launcher_behavior.as_str()), (0, 0, "reduire"));
        assert_eq!(r.account.map(|a| a.name), Some("J".into()));
        assert_eq!(r.last_milestones_ms, vec![1, 2]);
    }
}
