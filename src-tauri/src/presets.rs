//! Préréglages de qualité : `turicraft/presets.toml`, servi avec le pack
//! et relu à chaque lancement (modifier un préréglage ne demande pas de
//! recompiler le launcher). Exemple complet : `fixtures/presets.toml`.
//!
//! Appliqué APRÈS la synchronisation packwiz, qui réécrirait sinon les
//! fichiers de config livrés par le pack.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::Path;

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};

use crate::hardware::Hardware;
use crate::paths::Paths;
use crate::progress::Reporter;

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct PresetsFile {
    pub schema: u32,
    pub detection: toml::Table,
    pub memory: MemoryCfg,
    #[serde(default)]
    pub optional_groups: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pub toggles: BTreeMap<String, Toggle>,
    #[serde(default)]
    pub sliders: BTreeMap<String, Slider>,
    /// Nom et description de chaque groupe de mods optionnels (section Mods).
    #[serde(default)]
    pub group_info: BTreeMap<String, GroupInfo>,
    /// Ajustements selon la machine, par-dessus le préréglage.
    #[serde(default)]
    pub adapt: Vec<Adapt>,
    /// Réglages posés UNE fois par installation (langue…) : ensuite, le
    /// joueur décide. Monter `version` les repose une fois chez tout le monde.
    pub once: Option<Once>,
    #[serde(default)]
    pub jvm: Option<JvmCfg>,
    pub presets: BTreeMap<String, Preset>,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct Once {
    pub version: u32,
    #[serde(default)]
    pub options: BTreeMap<String, String>,
    /// Fichiers de config, posés eux aussi une seule fois (accueil de Voice
    /// Chat…) : le joueur reste libre de les changer ensuite. Leur propre
    /// version : les launchers 0.1.x ignorent `files` mais comptent `version`
    /// — la monter pour des fichiers les leur aurait fait « consommer ».
    #[serde(default)]
    pub files_version: u32,
    #[serde(default)]
    pub files: BTreeMap<String, FileEdit>,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct GroupInfo {
    pub label: String,
    #[serde(default)]
    pub description: String,
}

/// Règle d'ajustement automatique : si la machine remplit `when` (et que le
/// préréglage est dans `presets`, s'il est donné), ses valeurs deviennent les
/// valeurs conseillées. Le choix du joueur passe toujours par-dessus.
#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct Adapt {
    pub note: String,
    #[serde(default)]
    pub when: Cond,
    pub presets: Option<Vec<String>>,
    #[serde(default)]
    pub toggles: BTreeMap<String, bool>,
    /// Nombre, ou « ecran » / « ecran-3 » : la fréquence de l'écran
    /// principal (moins 3). Ignoré si la fréquence est inconnue.
    #[serde(default)]
    pub sliders: BTreeMap<String, toml::Value>,
    #[serde(default)]
    pub files: BTreeMap<String, FileEdit>,
}

/// Valeur d'un curseur dans une règle : nombre, « interface » (taille de
/// l'interface selon la hauteur de l'écran), ou « ecran[-n] » (fréquence).
fn adapt_value(v: &toml::Value, hw: &Hardware) -> Option<i64> {
    match v {
        toml::Value::Integer(i) => Some(*i),
        // Le mode automatique de Minecraft prend la plus grande échelle qui
        // tient (4 en 1080p, 6 en 1440p) : trop grand. Hauteur
        // / 480 : 2 en 1080p, 3 en 1440p, 5 en 4K.
        toml::Value::String(s) if s == "interface" => {
            let h = hw.display.height_px? as f64;
            Some(((h / 480.0).round() as i64).clamp(2, 6))
        }
        toml::Value::String(s) => {
            let hz = hw.display.refresh_hz? as i64;
            let minus = s.strip_prefix("ecran").map(|r| r.trim().trim_start_matches('-').parse::<i64>().unwrap_or(0))?;
            // Minecraft limite à 250 i/s ; au-delà, « illimité » (260).
            let v = hz - minus;
            Some(if v >= 250 { 260 } else { v })
        }
        _ => None,
    }
}

#[derive(Deserialize, Serialize, Clone, Debug, Default)]
pub struct Cond {
    pub min_ram_gb: Option<f64>,
    pub max_ram_gb: Option<f64>,
    pub min_cpu_threads: Option<usize>,
    pub max_cpu_threads: Option<usize>,
    pub min_vram_gb: Option<f64>,
    pub max_vram_gb: Option<f64>,
    /// « dedicated » ou « integrated »
    pub gpu: Option<String>,
    /// L'écran principal fait du VRR (FreeSync, G-Sync), activé.
    pub vrr: Option<bool>,
    /// La fréquence de l'écran principal est connue.
    pub refresh_known: Option<bool>,
    /// La hauteur de l'écran principal est connue.
    pub height_known: Option<bool>,
    /// PC Windows à processeur ARM (Snapdragon X).
    pub windows_arm: Option<bool>,
}

impl Cond {
    fn holds(&self, hw: &Hardware) -> bool {
        // Mêmes tolérances que la détection : 1 Go de RAM, 0,25 Go de VRAM.
        self.min_ram_gb.map_or(true, |m| hw.ram_gb >= m - 1.0)
            && self.max_ram_gb.map_or(true, |m| hw.ram_gb < m - 0.5)
            && self.min_cpu_threads.map_or(true, |m| hw.cpu_threads >= m)
            && self.max_cpu_threads.map_or(true, |m| hw.cpu_threads <= m)
            && self.min_vram_gb.map_or(true, |m| hw.vram_gb >= m - 0.25)
            && self.max_vram_gb.map_or(true, |m| hw.vram_gb < m)
            && match self.gpu.as_deref() {
                Some("dedicated") => hw.gpu_dedicated,
                Some("integrated") => !hw.gpu_dedicated,
                _ => true,
            }
            && self.vrr.map_or(true, |v| hw.display.vrr_active == v)
            && self.refresh_known.map_or(true, |v| hw.display.refresh_hz.is_some() == v)
            && self.height_known.map_or(true, |v| hw.display.height_px.is_some() == v)
            && self.windows_arm.map_or(true, |v| hw.windows_arm == v)
    }
}

/// Réglage chiffré du mode Simple (distance d'affichage…). Il écrit soit une
/// ligne d'options.txt (`option`), soit une clé d'un fichier (`file`, `key`).
#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct Slider {
    pub label: String,
    #[serde(default)]
    pub description: String,
    pub min: i64,
    pub max: i64,
    pub step: i64,
    #[serde(default)]
    pub unit: String,
    /// Valeur quand ni le préréglage ni une règle n'en donne (sinon `min`).
    pub default: Option<i64>,
    pub option: Option<String>,
    pub file: Option<String>,
    pub format: Option<String>,
    pub key: Option<String>,
    /// Grisé et ignoré si cette option du jeu est coupée.
    pub requires: Option<String>,
}

impl Slider {
    /// La valeur que le préréglage écrit pour ce réglage.
    fn preset_value(&self, p: &Preset) -> Option<i64> {
        if let Some(o) = &self.option {
            return p.options.get(o)?.trim_matches('"').parse().ok();
        }
        let set = &p.files.get(self.file.as_ref()?)?.set;
        set.get(self.key.as_ref()?)?.as_integer()
    }
}

/// Mémoire du jeu, selon la MACHINE et non la qualité graphique : peu de RAM
/// ne veut pas dire une petite carte graphique.
#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct MemoryCfg {
    /// Launchers 0.1.x seulement (RAM − réserve) ; ignoré dès que `steps` existe.
    #[serde(default)]
    pub reserve_system_gb: u64,
    /// RAM totale → mémoire du jeu : le palier le plus haut atteint.
    #[serde(default)]
    pub steps: Vec<MemoryStep>,
    /// Retiré sur une puce Apple : la carte graphique prend sur la même mémoire.
    #[serde(default)]
    pub shared_gpu_gb: f64,
    /// Plafond du réglage manuel : RAM − `min_free_gb` (− `shared_gpu_gb`).
    #[serde(default = "default_min_free")]
    pub min_free_gb: f64,
}

fn default_min_free() -> f64 {
    2.0
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct MemoryStep {
    pub ram: f64,
    pub game: f64,
}

/// Ramasse-miettes. ZGC n'a presque pas de pauses, mais il lui faut de la
/// marge : sur un petit tas, il bloque les allocations le temps de nettoyer
/// (« allocation stall ») — de grosses saccades. G1 en dessous.
#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct JvmCfg {
    pub zgc_min_memory_gb: f64,
    pub zgc_min_cpu_threads: usize,
    pub zgc: Vec<String>,
    pub g1: Vec<String>,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct Preset {
    pub label: String,
    pub description: String,
    /// Launchers 0.1.x seulement : la mémoire suit désormais la machine.
    #[serde(default)]
    pub memory_gb: Option<u64>,
    #[serde(default)]
    pub groups: Vec<String>,
    #[serde(default)]
    pub resource_packs: Vec<String>,
    /// Valeur des interrupteurs pour ce préréglage (shaders d'Ultra…).
    #[serde(default)]
    pub toggles: BTreeMap<String, bool>,
    #[serde(default)]
    pub options: BTreeMap<String, String>,
    #[serde(default)]
    pub files: BTreeMap<String, FileEdit>,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct FileEdit {
    pub format: String,
    pub set: toml::Table,
    /// Ne rien faire si le fichier n'existe pas encore : un mod le crée au
    /// premier démarrage (defaultoptions…), le créer avant lui l'en empêcherait.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub if_exists: bool,
}

/// Option du jeu (mode Simple) : ce qu'elle écrit quand elle est activée, et
/// dans `off` quand elle ne l'est pas.
#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct Toggle {
    /// Section de l'écran : « graphismes », « mods » ou « interface ».
    #[serde(default)]
    pub category: String,
    pub label: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub default: bool,
    #[serde(default)]
    pub files: BTreeMap<String, FileEdit>,
    /// Lignes d'options.txt quand l'option est activée.
    #[serde(default)]
    pub options: BTreeMap<String, String>,
    /// Groupes de mods optionnels ajoutés (activée) ou retirés (désactivée).
    #[serde(default)]
    pub groups: Vec<String>,
    pub off: Option<ToggleOff>,
}

#[derive(Deserialize, Serialize, Clone, Debug, Default)]
pub struct ToggleOff {
    #[serde(default)]
    pub files: BTreeMap<String, FileEdit>,
    #[serde(default)]
    pub options: BTreeMap<String, String>,
}

/// Ce que le joueur a choisi, résolu : un préréglage de base, les groupes de
/// mods optionnels, les interrupteurs, la mémoire.
#[derive(Serialize, Clone, Debug)]
pub struct Resolved {
    pub preset: String,
    pub groups: Vec<String>,
    pub toggles: BTreeMap<String, bool>,
    pub sliders: BTreeMap<String, i64>,
    pub memory_gb: f64,
    /// Ramasse-miettes retenu (« ZGC », « G1 ») et ses options Java.
    pub gc: String,
    pub jvm_flags: Vec<String>,
    /// Ce que les ajustements automatiques ont changé : id → raison.
    pub adapted: BTreeMap<String, String>,
    /// Fichiers écrits par les ajustements automatiques.
    pub adapt_files: BTreeMap<String, FileEdit>,
}

pub async fn fetch(pack_url: &str) -> Result<PresetsFile> {
    let url = pack_url
        .rsplit_once('/')
        .map(|(base, _)| format!("{base}/turicraft/presets.toml"))
        .ok_or_else(|| anyhow!("URL du pack invalide : {pack_url}"))?;
    let text = crate::net::fetch_text(&crate::net::client(), &url).await.context("lecture de presets.toml")?;
    parse(&text)
}

pub fn parse(text: &str) -> Result<PresetsFile> {
    let f: PresetsFile = toml::from_str(text).context("presets.toml invalide")?;
    if f.schema != 1 {
        return Err(anyhow!("presets.toml : schéma {} inconnu de ce launcher (mets-le à jour)", f.schema));
    }
    Ok(f)
}

impl PresetsFile {
    /// Premier préréglage de `detection.order` dont la machine remplit toutes
    /// les conditions ; à défaut, le dernier.
    pub fn detect(&self, hw: &Hardware) -> String {
        let order: Vec<String> = self
            .detection
            .get("order")
            .and_then(|o| o.as_array())
            .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
            .unwrap_or_default();
        for id in &order {
            let Some(rule) = self.detection.get(id).and_then(|r| r.as_table()) else { return id.clone() };
            let num = |k: &str| rule.get(k).and_then(|v| v.as_float().or(v.as_integer().map(|i| i as f64)));
            // Tolérance d'1 Go : la mémoire réservée à une puce graphique intégrée
            // est retirée du total annoncé (16 Go → ~15,3).
            let ok_ram = num("min_ram_gb").map_or(true, |m| hw.ram_gb >= m - 1.0);
            let ok_cpu = num("min_cpu_threads").map_or(true, |m| hw.cpu_threads as f64 >= m);
            let ok_gpu = match rule.get("gpu").and_then(|v| v.as_str()) {
                Some("dedicated") => hw.gpu_dedicated,
                _ => true,
            };
            let ok_vram = num("min_vram_gb").map_or(true, |m| m <= 0.0 || hw.vram_gb >= m - 0.25);
            let ok_arm = rule.get("windows_arm").and_then(|v| v.as_bool()).map_or(true, |v| hw.windows_arm == v);
            if ok_ram && ok_cpu && ok_gpu && ok_vram && ok_arm {
                return id.clone();
            }
        }
        order.last().cloned().unwrap_or_else(|| "faible".into())
    }

    /// RAM annoncée, arrondie : un « 8 Go » annonce ~7,6 Go utilisables, un
    /// « 16 Go » avec carte intégrée ~15,3.
    fn ram_rounded(hw: &Hardware) -> f64 {
        hw.ram_gb.round()
    }

    fn shared_gpu(&self, hw: &Hardware) -> f64 {
        if hw.shared_memory { self.memory.shared_gpu_gb } else { 0.0 }
    }

    /// Mémoire conseillée pour le jeu sur cette machine (par pas de 0,5 Go).
    pub fn memory_auto_gb(&self, hw: &Hardware) -> f64 {
        // Même tolérance que la détection : 1 Go (24 Go annoncés 23,2 ; 8 Go, 7,6).
        let Some(step) = self.memory.steps.iter().filter(|s| hw.ram_gb >= s.ram - 1.0).last() else {
            return self.memory_cap_gb(hw); // ancien fichier : RAM − réserve
        };
        half((step.game - self.shared_gpu(hw)).min(self.memory_cap_gb(hw)))
    }

    /// Plafond du réglage manuel : ce qu'il faut laisser au système.
    pub fn memory_cap_gb(&self, hw: &Hardware) -> f64 {
        let ram = Self::ram_rounded(hw);
        let cap = if self.memory.steps.is_empty() {
            ram - self.memory.reserve_system_gb as f64
        } else {
            ram - self.memory.min_free_gb - self.shared_gpu(hw)
        };
        half(cap.max(2.0))
    }

    /// Ramasse-miettes et options Java pour cette mémoire et ce processeur.
    pub fn jvm_for(&self, memory_gb: f64, hw: &Hardware) -> (String, Vec<String>) {
        match &self.jvm {
            Some(j) if memory_gb >= j.zgc_min_memory_gb && hw.cpu_threads >= j.zgc_min_cpu_threads => ("ZGC".into(), j.zgc.clone()),
            Some(j) => ("G1".into(), j.g1.clone()),
            None => ("ZGC".into(), vec!["-XX:+UseZGC".into(), "-XX:+ZGenerational".into()]),
        }
    }

    /// Fichiers de config que le launcher réécrit (préréglages, options,
    /// ajustements, curseurs). S'ils viennent du pack, une mise à jour du pack
    /// peut les remplacer : il faut alors réappliquer (voir launch.rs).
    pub fn managed_files(&self) -> BTreeSet<String> {
        let mut out: BTreeSet<String> = self.presets.values().flat_map(|p| p.files.keys().cloned()).collect();
        for t in self.toggles.values() {
            out.extend(t.files.keys().cloned());
            out.extend(t.off.iter().flat_map(|o| o.files.keys().cloned()));
        }
        out.extend(self.adapt.iter().flat_map(|a| a.files.keys().cloned()));
        out.extend(self.sliders.values().filter_map(|s| s.file.clone()));
        out
    }

    pub fn all_optional(&self) -> HashSet<String> {
        self.optional_groups.values().flatten().cloned().collect()
    }

    pub fn enabled_optional(&self, groups: &[String]) -> HashSet<String> {
        groups.iter().filter_map(|g| self.optional_groups.get(g)).flatten().cloned().collect()
    }

    /// Paquets de ressources de tous les préréglages : ceux que le launcher gère.
    fn managed_packs(&self) -> HashSet<String> {
        self.presets.values().flat_map(|p| p.resource_packs.iter().cloned()).collect()
    }
}

// ─── Application ────────────────────────────────────────────────────────────

/// Pose une fois les réglages de `[once]` si la version a monté depuis la
/// dernière fois (`done` : celle déjà posée). Rend la version à retenir.
/// Rend (version des options, version des fichiers) désormais posées.
pub fn apply_once(paths: &Paths, file: &PresetsFile, done: (u32, u32), reporter: &dyn Reporter) -> Result<(u32, u32)> {
    let Some(once) = &file.once else { return Ok(done) };
    if once.files_version > done.1 {
        for (rel, edit) in &once.files {
            edit_file(&paths.instance().join(rel), edit).with_context(|| format!("réglage de première fois : {rel}"))?;
        }
    }
    let files_done = done.1.max(once.files_version);
    if once.version <= done.0 {
        return Ok((done.0, files_done));
    }
    let p = paths.instance().join("options.txt");
    let text = std::fs::read_to_string(&p).unwrap_or_default();
    let mut lines: Vec<String> = text.lines().map(String::from).collect();
    for (k, v) in &once.options {
        let prefix = format!("{k}:");
        lines.retain(|l| !l.starts_with(&prefix));
        lines.push(format!("{prefix}{v}"));
    }
    std::fs::create_dir_all(p.parent().unwrap())?;
    std::fs::write(&p, lines.join("\n") + "\n")?;
    reporter.log("réglages de première fois posés (langue…)");
    Ok((once.version, files_done))
}

pub fn apply(paths: &Paths, file: &PresetsFile, r: &Resolved, reporter: &dyn Reporter) -> Result<()> {
    let preset = file.presets.get(&r.preset).ok_or_else(|| anyhow!("préréglage inconnu : {}", r.preset))?;
    let game = paths.instance();

    // Paquets de ressources : ceux du préréglage, et seulement s'ils sont là
    // (Fresh Animations n'est téléchargé que si son groupe est coché).
    let wanted: Vec<String> = file
        .managed_packs()
        .into_iter()
        .filter(|p| preset.resource_packs.contains(p) || r.groups.iter().any(|g| g == "animations"))
        .filter(|p| p.strip_prefix("file/").map_or(true, |f| game.join("resourcepacks").join(f).exists()))
        .collect();
    // Options du préréglage, puis celles des options du jeu, puis les
    // curseurs : le dernier a le dernier mot.
    let mut options = preset.options.clone();
    for (id, t) in &file.toggles {
        let on = r.toggles.get(id).copied().unwrap_or(t.default);
        let extra = if on { Some(&t.options) } else { t.off.as_ref().map(|o| &o.options) };
        options.extend(extra.into_iter().flatten().map(|(k, v)| (k.clone(), v.clone())));
    }
    let active = |sl: &Slider| sl.requires.as_ref().map_or(true, |t| r.toggles.get(t).copied().unwrap_or(false));
    for (id, sl) in &file.sliders {
        if let (Some(o), Some(v), true) = (&sl.option, r.sliders.get(id), active(sl)) {
            options.insert(o.clone(), v.to_string());
        }
    }
    set_options(&game.join("options.txt"), &options, &file.managed_packs(), &wanted)?;

    for (rel, edit) in &preset.files {
        edit_file(&game.join(rel), edit).with_context(|| format!("préréglage {} : {rel}", r.preset))?;
    }
    for (id, t) in &file.toggles {
        let on = r.toggles.get(id).copied().unwrap_or(t.default);
        let files = if on { Some(&t.files) } else { t.off.as_ref().map(|o| &o.files) };
        for (rel, edit) in files.into_iter().flatten() {
            edit_file(&game.join(rel), edit).with_context(|| format!("option {id} : {rel}"))?;
        }
    }
    for (rel, edit) in &r.adapt_files {
        edit_file(&game.join(rel), edit).with_context(|| format!("ajustement automatique : {rel}"))?;
    }
    for (id, sl) in &file.sliders {
        if let (Some(rel), Some(key), Some(v), true) = (&sl.file, &sl.key, r.sliders.get(id), active(sl)) {
            let mut set = toml::Table::new();
            set.insert(key.clone(), toml::Value::Integer(*v));
            let edit = FileEdit { format: sl.format.clone().unwrap_or_else(|| "toml".into()), set, if_exists: false };
            edit_file(&game.join(rel), &edit).with_context(|| format!("réglage {id} : {rel}"))?;
        }
    }
    reporter.log(&format!("préréglage appliqué : {}", preset.label));
    Ok(())
}

/// `options.txt` : `clé:valeur` par ligne. Créé s'il n'existe pas encore
/// (premier lancement) — Minecraft complète les clés absentes.
fn set_options(
    path: &Path,
    values: &BTreeMap<String, String>,
    managed_packs: &HashSet<String>,
    wanted_packs: &[String],
) -> Result<()> {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let mut lines: Vec<String> = text.lines().map(String::from).collect();
    let mut set = |key: &str, value: String| {
        let prefix = format!("{key}:");
        match lines.iter_mut().find(|l| l.starts_with(&prefix)) {
            Some(l) => *l = format!("{prefix}{value}"),
            None => lines.push(format!("{prefix}{value}")),
        }
    };
    for (k, v) in values {
        set(k, v.clone());
    }

    // resourcePacks:["vanilla","mod_resources",…] — on garde ceux que le
    // launcher ne gère pas, dans leur ordre, puis on ajoute les siens.
    let current: Vec<String> = text
        .lines()
        .find_map(|l| l.strip_prefix("resourcePacks:"))
        .and_then(|v| serde_json::from_str(v).ok())
        .unwrap_or_else(|| vec!["vanilla".into(), "mod_resources".into()]);
    let mut packs: Vec<String> = current.into_iter().filter(|p| !managed_packs.contains(p)).collect();
    packs.extend(wanted_packs.iter().cloned());
    set("resourcePacks", serde_json::to_string(&packs)?);

    std::fs::create_dir_all(path.parent().unwrap())?;
    std::fs::write(path, lines.join("\n") + "\n")?;
    Ok(())
}

fn edit_file(path: &Path, edit: &FileEdit) -> Result<()> {
    if edit.if_exists && !path.exists() {
        return Ok(());
    }
    std::fs::create_dir_all(path.parent().unwrap())?;
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let out = match edit.format.as_str() {
        "json" => edit_json(&text, &edit.set)?,
        "toml" => edit_toml(&text, &edit.set)?,
        "properties" => edit_properties(&text, &edit.set),
        f => return Err(anyhow!("format inconnu : {f}")),
    };
    std::fs::write(path, out)?;
    Ok(())
}

fn toml_to_json(v: &toml::Value) -> serde_json::Value {
    serde_json::to_value(v).unwrap_or(serde_json::Value::Null)
}

/// Clés pointées : `"quality.weather_quality"` = objet `quality`, champ `weather_quality`.
fn edit_json(text: &str, set: &toml::Table) -> Result<String> {
    let mut root: serde_json::Value = if text.trim().is_empty() {
        serde_json::json!({})
    } else {
        serde_json::from_str(text).context("JSON existant illisible")?
    };
    for (key, value) in set {
        // Clé qui contient elle-même des points (Chat Tools :
        // « general.RestoreMessages.Enabled ») : telle quelle si elle existe.
        if let Some(obj) = root.as_object_mut().filter(|o| o.contains_key(key)) {
            obj.insert(key.clone(), toml_to_json(value));
            continue;
        }
        let mut cur = &mut root;
        let parts: Vec<&str> = key.split('.').collect();
        for part in &parts[..parts.len() - 1] {
            let obj = cur.as_object_mut().ok_or_else(|| anyhow!("{key} : pas un objet"))?;
            cur = obj.entry(part.to_string()).or_insert_with(|| serde_json::json!({}));
        }
        cur.as_object_mut()
            .ok_or_else(|| anyhow!("{key} : pas un objet"))?
            .insert(parts[parts.len() - 1].to_string(), toml_to_json(value));
    }
    Ok(serde_json::to_string_pretty(&root)? + "\n")
}

/// Mise en forme et commentaires du fichier conservés (toml_edit).
fn edit_toml(text: &str, set: &toml::Table) -> Result<String> {
    let mut doc: toml_edit::DocumentMut = text.parse().context("TOML existant illisible")?;
    for (key, value) in set {
        let parts: Vec<&str> = key.split('.').collect();
        let mut table = doc.as_table_mut();
        for part in &parts[..parts.len() - 1] {
            if !table.contains_key(part) {
                table.insert(part, toml_edit::Item::Table(toml_edit::Table::new()));
            }
            table = table[part].as_table_mut().ok_or_else(|| anyhow!("{key} : pas une table"))?;
        }
        let v: toml_edit::Value = match value {
            toml::Value::String(s) => s.as_str().into(),
            toml::Value::Integer(i) => (*i).into(),
            toml::Value::Float(f) => (*f).into(),
            toml::Value::Boolean(b) => (*b).into(),
            other => return Err(anyhow!("{key} : type non pris en charge ({other})")),
        };
        let last = parts[parts.len() - 1];
        match table.get_mut(last).and_then(|i| i.as_value_mut()) {
            // garde la décoration (commentaire en fin de ligne)
            Some(existing) => {
                let decor = existing.decor().clone();
                *existing = v;
                *existing.decor_mut() = decor;
            }
            None => {
                table.insert(last, toml_edit::value(v));
            }
        }
    }
    Ok(doc.to_string())
}

fn edit_properties(text: &str, set: &toml::Table) -> String {
    let mut lines: Vec<String> = text.lines().map(String::from).collect();
    for (key, value) in set {
        let v = match value {
            toml::Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        let found = lines.iter_mut().find(|l| {
            let t = l.trim_start();
            !t.starts_with('#') && t.split(['=', ':']).next().map(str::trim) == Some(key.as_str())
        });
        match found {
            Some(l) => *l = format!("{key}={v}"),
            None => lines.push(format!("{key}={v}")),
        }
    }
    lines.join("\n") + "\n"
}

/// Résout le choix du joueur (réglages) en préréglage concret.
pub fn resolve(file: &PresetsFile, hw: &Hardware, s: &crate::settings::Settings) -> Resolved {
    let detected = file.detect(hw);
    let base = match s.preset.as_str() {
        "auto" | "personnalise" | "" => {
            if s.preset == "personnalise" {
                s.custom_base.clone().unwrap_or_else(|| detected.clone())
            } else {
                detected.clone()
            }
        }
        other if file.presets.contains_key(other) => other.to_string(),
        _ => detected.clone(),
    };
    let preset = &file.presets[&base];
    let mut groups = preset.groups.clone();

    // Valeurs conseillées : option par défaut → préréglage → ajustements
    // automatiques selon la machine. Puis le choix du joueur par-dessus.
    let mut advised_toggles: BTreeMap<String, bool> = file
        .toggles
        .iter()
        .map(|(id, t)| (id.clone(), preset.toggles.get(id).copied().unwrap_or(t.default)))
        .collect();
    let mut advised_sliders: BTreeMap<String, i64> = file
        .sliders
        .iter()
        .map(|(id, sl)| (id.clone(), sl.preset_value(preset).or(sl.default).unwrap_or(sl.min)))
        .collect();
    let mut adapted = BTreeMap::new();
    let mut adapt_files = BTreeMap::new();
    for rule in &file.adapt {
        let preset_ok = rule.presets.as_ref().map_or(true, |p| p.contains(&base));
        if !preset_ok || !rule.when.holds(hw) {
            continue;
        }
        for (id, v) in &rule.toggles {
            advised_toggles.insert(id.clone(), *v);
            adapted.insert(id.clone(), rule.note.clone());
        }
        for (id, v) in &rule.sliders {
            if let Some(v) = adapt_value(v, hw) {
                advised_sliders.insert(id.clone(), v);
                adapted.insert(id.clone(), rule.note.clone());
            }
        }
        adapt_files.extend(rule.files.iter().map(|(k, v)| (k.clone(), v.clone())));
    }

    let toggles: BTreeMap<String, bool> = advised_toggles
        .iter()
        .map(|(id, v)| (id.clone(), s.toggles.get(id).copied().unwrap_or(*v)))
        .collect();
    for (id, t) in &file.toggles {
        for g in &t.groups {
            groups.retain(|x| x != g);
            if toggles[id] {
                groups.push(g.clone());
            }
        }
    }
    // Mods choisis un par un dans la section Mods.
    for (g, on) in &s.mods {
        groups.retain(|x| x != g);
        if *on {
            groups.push(g.clone());
        }
    }
    let sliders: BTreeMap<String, i64> = file
        .sliders
        .iter()
        .map(|(id, sl)| {
            let v = s.sliders.get(id).copied().unwrap_or(advised_sliders[id]);
            (id.clone(), v.clamp(sl.min, sl.max))
        })
        .collect();
    let auto = file.memory_auto_gb(hw);
    let wanted = if s.preset == "personnalise" { s.custom_memory_gb.unwrap_or(auto) } else { auto };
    let memory_gb = half(wanted.min(file.memory_cap_gb(hw)).max(2.0));
    let (gc, jvm_flags) = file.jvm_for(memory_gb, hw);
    let _ = preset;
    // Ce que le joueur a changé lui-même n'est plus « ajusté ».
    adapted.retain(|id, _| !s.toggles.contains_key(id) && !s.sliders.contains_key(id));
    Resolved { preset: base, groups, toggles, sliders, memory_gb, gc, jvm_flags, adapted, adapt_files }
}

/// Arrondi au demi-Go inférieur.
fn half(gb: f64) -> f64 {
    (gb * 2.0).floor() / 2.0
}

#[cfg(test)]
mod tests {
    use super::*;

    // Copie du presets.toml du pack. Le dépôt du pack remplace ce fichier par
    // sa version avant de lancer ces tests : un préréglage mal formé casse là.
    const REAL: &str = include_str!("../fixtures/presets.toml");

    #[test]
    fn le_vrai_fichier_se_lit() {
        let f = parse(REAL).unwrap();
        assert_eq!(f.presets.len(), 3);
        assert!(f.toggles["shaders"].default);
        assert_eq!(f.presets["faible"].toggles.get("shaders"), Some(&false));
        assert!(f.all_optional().contains("mods/lambdynamiclights.pw.toml"));
    }

    #[test]
    fn detection() {
        let f = parse(REAL).unwrap();
        let hw = |ram, cpu, dedicated, vram| Hardware {
            ram_gb: ram,
            cpu_threads: cpu,
            gpu_dedicated: dedicated,
            vram_gb: vram,
            gpu_name: String::new(),
            shared_memory: false, windows_arm: false,
            display: Default::default(),
        };
        assert_eq!(f.detect(&hw(32.0, 16, true, 8.0)), "haut");
        assert_eq!(f.detect(&hw(16.0, 8, false, 0.0)), "moyen");
        // Peu de RAM ne veut pas dire des graphismes moches : la carte et le
        // processeur choisissent (8 Go + bonne carte → Haut).
        assert_eq!(f.detect(&hw(7.6, 8, true, 8.0)), "haut");
        assert_eq!(f.detect(&hw(8.0, 8, false, 0.0)), "moyen");
        // Faible : processeur modeste, ou vraiment trop peu de mémoire.
        assert_eq!(f.detect(&hw(16.0, 4, true, 4.0)), "faible");
        assert_eq!(f.detect(&hw(4.0, 8, false, 0.0)), "faible");
    }

    /// La mémoire suit la machine ; ZGC seulement s'il a de la marge.
    #[test]
    fn memoire_et_ramasse_miettes() {
        let f = parse(REAL).unwrap();
        let hw = |ram: f64, cpu: usize, mac: bool| Hardware {
            ram_gb: ram,
            cpu_threads: cpu,
            gpu_dedicated: !mac,
            vram_gb: if mac { 0.0 } else { 8.0 },
            gpu_name: String::new(),
            shared_memory: mac, windows_arm: false,
            display: Default::default(),
        };
        let s = crate::settings::Settings::default();
        let r = |h: &Hardware| {
            let r = resolve(&f, h, &s);
            (r.memory_gb, r.gc)
        };
        assert_eq!(r(&hw(7.6, 8, false)), (6.0, "G1".to_string()));   // PC 8 Go
        assert_eq!(r(&hw(8.0, 8, true)), (5.5, "G1".to_string()));    // Mac 8 Go : 0,5 de moins (24/09)
        assert_eq!(r(&hw(16.0, 10, true)), (7.5, "G1".to_string()));  // Mac 16 Go
        assert_eq!(r(&hw(15.3, 12, false)), (8.0, "ZGC".to_string())); // PC 16 Go
        assert_eq!(r(&hw(32.0, 8, false)), (12.0, "ZGC".to_string()));
        assert_eq!(r(&hw(23.2, 20, false)), (10.0, "ZGC".to_string())); // 24 Go annoncés 23,2
        assert_eq!(r(&hw(15.3, 4, false)), (8.0, "G1".to_string()));  // 4 fils : ZGC manquerait de cœurs
        // Plafond du réglage manuel : ce qu'il faut au système.
        assert_eq!(f.memory_cap_gb(&hw(7.6, 8, false)), 6.0);
        assert_eq!(f.memory_cap_gb(&hw(8.0, 8, true)), 5.5);
        // 8 Go + bonne carte : Haut, mais distance ramenée à 10 (mémoire), et
        // vue lointaine à 64 ; les shaders et le reste de l'image restent.
        let r8 = resolve(&f, &hw(7.6, 8, false), &s);
        assert_eq!((r8.preset.as_str(), r8.sliders["distance"], r8.sliders["distance_lointaine"]), ("haut", 10, 64));
        assert!(r8.toggles["shaders"]);
        // Réglage manuel au-delà du plafond : ramené au plafond.
        let mut s2 = crate::settings::Settings::default();
        s2.preset = "personnalise".into();
        s2.custom_memory_gb = Some(12.0);
        assert_eq!(resolve(&f, &hw(8.0, 8, true), &s2).memory_gb, 5.5);
    }

    #[test]
    fn options_du_jeu() {
        let f = parse(REAL).unwrap();
        let hw = Hardware { ram_gb: 32.0, cpu_threads: 16, gpu_dedicated: true, vram_gb: 12.0, gpu_name: String::new(), shared_memory: false, windows_arm: false, display: Default::default() };
        let mut s = crate::settings::Settings::default();
        // Haut : shaders et objets physiques activés par défaut
        let r = resolve(&f, &hw, &s);
        assert_eq!(r.preset, "haut");
        assert!(r.toggles["shaders"] && r.groups.contains(&"objets_physiques".to_string()));
        // Corps en première personne : activé à partir de Moyen
        assert!(r.groups.contains(&"premiere_personne".to_string()));
        // Le joueur coupe les objets physiques et le corps
        s.toggles.insert("objets_physiques".into(), false);
        s.toggles.insert("premiere_personne".into(), false);
        let r = resolve(&f, &hw, &s);
        assert!(!r.groups.contains(&"objets_physiques".to_string()));
        assert!(!r.groups.contains(&"premiere_personne".to_string()));
        // Faible : shaders coupés, corps en première personne aussi
        s.preset = "faible".into();
        s.toggles.clear();
        let r = resolve(&f, &hw, &s);
        assert!(!r.toggles["shaders"] && !r.toggles["premiere_personne"]);
        // Curseurs : valeur du préréglage, puis celle du joueur, bornée
        assert_eq!(r.sliders["distance"], 8);
        s.preset = "haut".into();
        // 12 Go de VRAM : ajustement « grosse carte graphique » (192 au lieu de 128)
        assert_eq!(resolve(&f, &hw, &s).sliders["distance_lointaine"], 192);
        s.sliders.insert("distance".into(), 99);
        assert_eq!(resolve(&f, &hw, &s).sliders["distance"], 32);
    }

    #[test]
    fn ajustements_automatiques() {
        let f = parse(REAL).unwrap();
        let s = crate::settings::Settings::default();
        // Portable à carte intégrée, 4 cœurs, 8 Go : Faible, sans shaders ni
        // son 3D, vue lointaine réduite
        let hw = Hardware { ram_gb: 7.6, cpu_threads: 4, gpu_dedicated: false, vram_gb: 0.0, gpu_name: String::new(), shared_memory: false, windows_arm: false, display: Default::default() };
        let r = resolve(&f, &hw, &s);
        assert_eq!(r.preset, "faible");
        assert!(!r.toggles["shaders"] && !r.toggles["son_3d"]);
        assert_eq!(r.sliders["distance_lointaine"], 64);
        assert!(r.adapted.contains_key("son_3d"));
        // Grosse machine : distance 16, vue lointaine 192, 4 fils pour DH
        let hw = Hardware { ram_gb: 32.0, cpu_threads: 20, gpu_dedicated: true, vram_gb: 20.0, gpu_name: String::new(), shared_memory: false, windows_arm: false, display: Default::default() };
        let r = resolve(&f, &hw, &s);
        assert_eq!((r.sliders["distance"], r.sliders["distance_lointaine"]), (16, 192));
        assert!(r.adapt_files.contains_key("config/DistantHorizons.toml"));
        // Le choix du joueur l'emporte, et n'est plus marqué « ajusté »
        let mut s = s;
        s.sliders.insert("distance".into(), 10);
        let r = resolve(&f, &hw, &s);
        assert_eq!(r.sliders["distance"], 10);
        assert!(!r.adapted.contains_key("distance"));
        // Un mod coupé à la main
        s.mods.insert("animations".into(), false);
        assert!(!resolve(&f, &hw, &s).groups.contains(&"animations".to_string()));
    }

    /// Snapdragon X1 (12 cœurs, 16 Go, Adreno) : Faible, et pas davantage si
    /// le joueur choisit Moyen — ça saccadait en Moyen (25/09).
    #[test]
    fn snapdragon_windows_arm() {
        let f = parse(REAL).unwrap();
        let hw = Hardware { ram_gb: 15.6, cpu_threads: 12, gpu_dedicated: false, vram_gb: 0.0, gpu_name: "Qualcomm(R) Adreno(TM) X1-85 GPU".into(), shared_memory: true, windows_arm: true, display: Default::default() };
        let mut s = crate::settings::Settings::default();
        let r = resolve(&f, &hw, &s);
        assert_eq!(r.preset, "faible");
        assert_eq!((r.memory_gb, r.gc.as_str()), (7.5, "G1"));   // mémoire partagée : 0,5 de moins
        // Sans ARM, la même machine serait en Moyen.
        assert_eq!(f.detect(&Hardware { windows_arm: false, ..hw.clone() }), "moyen");
        s.preset = "moyen".into();
        let r = resolve(&f, &hw, &s);
        assert_eq!(r.sliders["distance"], 8);
        assert!(!r.toggles["shaders"] && !r.toggles["vue_lointaine"] && !r.toggles["son_3d"]);
        assert!(r.adapted["distance"].contains("Snapdragon"));
    }

    #[test]
    fn edition_json_toml_properties() {
        let mut set = toml::Table::new();
        set.insert("quality.weather_quality".into(), "FAST".into());
        let out = edit_json(r#"{"quality":{"leaves_quality":"FANCY"}}"#, &set).unwrap();
        assert!(out.contains(r#""weather_quality": "FAST""#) && out.contains("leaves_quality"));

        let mut set = toml::Table::new();
        set.insert("client.quickEnableRendering".into(), false.into());
        let out = edit_toml("[client]\n\t#commentaire\n\tquickEnableRendering = true\n", &set).unwrap();
        assert!(out.contains("quickEnableRendering = false") && out.contains("#commentaire"));

        let mut set = toml::Table::new();
        set.insert("enableShaders".into(), "true".into());
        assert_eq!(edit_properties("#x\nenableShaders=false\n", &set), "#x\nenableShaders=true\n");
    }

    #[test]
    fn ecran_principal() {
        let f = parse(REAL).unwrap();
        let s = crate::settings::Settings::default();
        let screen = |hz: Option<u32>, vrr: bool| Hardware {
            ram_gb: 32.0,
            cpu_threads: 20,
            gpu_dedicated: true,
            vram_gb: 20.0,
            gpu_name: String::new(),
            shared_memory: false, windows_arm: false,
            display: crate::display::Display { refresh_hz: hz, height_px: Some(1440), vrr_capable: vrr, vrr_active: vrr, source: String::new() },
        };
        // Écran 144 Hz, FreeSync actif → synchro coupée, 141 i/s
        let r = resolve(&f, &screen(Some(144), true), &s);
        assert!(!r.toggles["synchro_verticale"]);
        assert_eq!(r.sliders["images"], 141);
        // 360 Hz sans VRR : Minecraft ne limite pas au-delà de 250 → illimité
        let r = resolve(&f, &screen(Some(360), false), &s);
        assert!(r.toggles["synchro_verticale"]);
        assert_eq!(r.sliders["images"], 260);
        // Mac (jamais de VRR), 120 Hz : synchro gardée, 120 i/s
        let r = resolve(&f, &screen(Some(120), false), &s);
        assert!(r.toggles["synchro_verticale"] && r.sliders["images"] == 120);
        // Taille de l'interface : 3 en 1440p (hauteur / 480)
        assert_eq!(resolve(&f, &screen(Some(144), true), &s).sliders["interface"], 3);
        // Fréquence inconnue : illimité, quel que soit le préréglage
        let mut s = s;
        for p in ["faible", "moyen", "haut"] {
            s.preset = p.into();
            assert_eq!(resolve(&f, &screen(None, false), &s).sliders["images"], 260, "{p}");
            assert_eq!(resolve(&f, &screen(Some(144), true), &s).sliders["images"], 141, "{p}");
        }
    }

    #[test]
    fn cles_entre_guillemets() {
        // Format réel de travelerstitles-neoforge-1_21.toml (24/09).
        let text = "\t[\"Traveler's Titles\".\"Biome Titles\"]\n\t\t\"Enable Biome Titles\" = true\n\t\t\"Text Display Time\" = 50\n";
        let mut set = toml::Table::new();
        set.insert("Traveler's Titles.Biome Titles.Enable Biome Titles".into(), false.into());
        let out = edit_toml(text, &set).unwrap();
        assert!(out.contains("\"Enable Biome Titles\" = false"), "{out}");
        assert!(out.contains("\"Text Display Time\" = 50"));
    }

    #[test]
    fn options_et_paquets() {
        let dir = std::env::temp_dir().join(format!("turicraft-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("options.txt");
        std::fs::write(&p, "renderDistance:12\nresourcePacks:[\"vanilla\",\"file/FreshAnimations_v1.10.4.zip\",\"file/perso.zip\"]\n").unwrap();
        let managed: HashSet<String> = ["file/FreshAnimations_v1.10.4.zip".to_string()].into();
        let vals = BTreeMap::from([("renderDistance".to_string(), "6".to_string())]);
        set_options(&p, &vals, &managed, &[]).unwrap();
        let out = std::fs::read_to_string(&p).unwrap();
        assert!(out.contains("renderDistance:6"));
        assert!(out.contains(r#"resourcePacks:["vanilla","file/perso.zip"]"#));
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Distant Horizons se coupe VRAIMENT : `rendererMode`, relu par DH au
    /// démarrage. `quickEnableRendering`, écrit avant, n'était qu'un raccourci
    /// de son écran de réglages : DH restait actif partout (Mac, 24/09).
    #[test]
    fn vue_lointaine_ecrit_la_vraie_option_de_dh() {
        let f = parse(REAL).unwrap();
        let dir = std::env::temp_dir().join(format!("turicraft-dh-{}", std::process::id()));
        let paths = crate::paths::Paths::new(dir.clone());
        let mode = |hw: &Hardware| {
            let r = resolve(&f, hw, &crate::settings::Settings::default());
            apply(&paths, &f, &r, &crate::progress::ConsoleReporter).unwrap();
            let t: toml::Table = std::fs::read_to_string(paths.instance().join("config/DistantHorizons.toml")).unwrap().parse().unwrap();
            t["client"]["advanced"]["debugging"]["rendererMode"].as_str().unwrap().to_string()
        };
        // Mac à puce Apple : mémoire partagée → carte « intégrée » → DH coupé.
        let mac = Hardware { ram_gb: 16.0, cpu_threads: 10, gpu_dedicated: false, vram_gb: 0.0, gpu_name: "Apple Silicon".into(), shared_memory: true, windows_arm: false, display: Default::default() };
        assert_eq!(mode(&mac), "DISABLED");
        // Grosse carte dédiée : DH actif.
        let pc = Hardware { ram_gb: 32.0, cpu_threads: 16, gpu_dedicated: true, vram_gb: 20.0, gpu_name: String::new(), shared_memory: false, windows_arm: false, display: Default::default() };
        assert_eq!(mode(&pc), "DEFAULT");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Réglages « une fois » dans des fichiers : posés à la première
    /// installation seulement ; `if_exists` ne crée pas le fichier ; une clé
    /// JSON qui contient des points reste telle quelle.
    #[test]
    fn reglages_une_fois_dans_des_fichiers() {
        let f = parse(REAL).unwrap();
        let dir = std::env::temp_dir().join(format!("turicraft-once-{}", std::process::id()));
        let paths = crate::paths::Paths::new(dir.clone());
        let game = paths.instance();
        std::fs::create_dir_all(game.join("config")).unwrap();
        std::fs::write(game.join("config/chat_tools.json"), r#"{"general.RestoreMessages.SplitLineEnabled": true, "general.RestoreMessages.Enabled": true}"#).unwrap();
        let once = f.once.as_ref().unwrap();
        // Installation passée par un launcher 0.1.x : options faites, fichiers jamais.
        let v = apply_once(&paths, &f, (once.version, 0), &crate::progress::ConsoleReporter).unwrap();
        assert_eq!(v, (once.version, once.files_version));
        let voice = std::fs::read_to_string(game.join("config/voicechat/voicechat-client.properties")).unwrap();
        assert!(voice.contains("onboarding_finished=true"));
        let chat: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(game.join("config/chat_tools.json")).unwrap()).unwrap();
        assert_eq!(chat["general.RestoreMessages.SplitLineEnabled"], false);
        assert_eq!(chat["general.RestoreMessages.Enabled"], true);
        assert!(chat.get("general").is_none());
        // Déjà posé : le joueur change d'avis, on n'y touche plus.
        std::fs::write(game.join("config/voicechat/voicechat-client.properties"), "onboarding_finished=false\n").unwrap();
        apply_once(&paths, &f, v, &crate::progress::ConsoleReporter).unwrap();
        assert!(std::fs::read_to_string(game.join("config/voicechat/voicechat-client.properties")).unwrap().contains("=false"));
        // Sans fichier Chat Tools (première installation) : pas créé.
        std::fs::remove_file(game.join("config/chat_tools.json")).unwrap();
        apply_once(&paths, &f, (0, 0), &crate::progress::ConsoleReporter).unwrap();
        assert!(!game.join("config/chat_tools.json").exists());
        std::fs::remove_dir_all(&dir).ok();
    }
}
