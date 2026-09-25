//! Minecraft vanilla (jar, bibliothèques, assets) et fichiers de version :
//! lecture, fusion avec une version fille (`inheritsFrom`, c'est ainsi que
//! NeoForge se greffe), règles par système, et ligne de commande.
//!
//! Format de référence : les fichiers de piston-meta.mojang.com, tels que le
//! launcher officiel les lit.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use serde::Deserialize;
use serde_json::Value;

use crate::net::{self, Download, Hash};
use crate::paths::Paths;
use crate::progress::Reporter;

const MANIFEST_URL: &str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
const RESOURCES_URL: &str = "https://resources.download.minecraft.net";

// ─── Fichier de version ─────────────────────────────────────────────────────

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct VersionJson {
    pub id: String,
    pub inherits_from: Option<String>,
    pub main_class: Option<String>,
    #[serde(default)]
    pub libraries: Vec<Library>,
    pub arguments: Option<Arguments>,
    pub asset_index: Option<AssetIndexRef>,
    pub assets: Option<String>,
    pub downloads: Option<VersionDownloads>,
    pub logging: Option<Logging>,
    #[serde(rename = "type")]
    pub kind: Option<String>,
}

#[derive(Deserialize, Clone, Debug, Default)]
pub struct Arguments {
    #[serde(default)]
    pub game: Vec<Value>,
    #[serde(default)]
    pub jvm: Vec<Value>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct AssetIndexRef {
    pub id: String,
    pub url: String,
    pub sha1: String,
    pub size: u64,
}

#[derive(Deserialize, Clone, Debug)]
pub struct VersionDownloads {
    pub client: Artifact,
}

#[derive(Deserialize, Clone, Debug)]
pub struct Artifact {
    pub path: Option<String>,
    pub sha1: String,
    pub size: u64,
    pub url: String,
}

#[derive(Deserialize, Clone, Debug)]
pub struct Library {
    pub name: String,
    pub downloads: Option<LibraryDownloads>,
    /// Bibliothèques Maven sans bloc `downloads` (anciens formats).
    pub url: Option<String>,
    pub rules: Option<Vec<Rule>>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct LibraryDownloads {
    pub artifact: Option<Artifact>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct Rule {
    pub action: String,
    pub os: Option<OsRule>,
    pub features: Option<HashMap<String, bool>>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct OsRule {
    pub name: Option<String>,
    pub arch: Option<String>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct Logging {
    pub client: Option<LoggingClient>,
}
#[derive(Deserialize, Clone, Debug)]
pub struct LoggingClient {
    pub argument: String,
    pub file: LoggingFile,
}
#[derive(Deserialize, Clone, Debug)]
pub struct LoggingFile {
    pub id: String,
    pub sha1: String,
    pub size: u64,
    pub url: String,
}

/// Nom de l'OS tel que l'écrivent les règles de Mojang.
fn os_name() -> &'static str {
    match std::env::consts::OS {
        "macos" => "osx",
        "windows" => "windows",
        _ => "linux",
    }
}

/// Une règle ne s'applique que si tout ce qu'elle précise correspond. Les
/// `features` (démo, résolution imposée, quickPlay…) sont toutes fausses ici.
pub fn rules_allow(rules: &Option<Vec<Rule>>) -> bool {
    let Some(rules) = rules else { return true };
    let mut allowed = false;
    for r in rules {
        let mut matches = true;
        if let Some(os) = &r.os {
            if let Some(name) = &os.name {
                matches &= name == os_name();
            }
            if let Some(arch) = &os.arch {
                matches &= arch == "x86" && std::env::consts::ARCH == "x86";
            }
        }
        if let Some(features) = &r.features {
            matches &= features.values().all(|v| !*v);
        }
        if matches {
            allowed = r.action == "allow";
        }
    }
    allowed
}

/// `groupe:artefact:version[:classificateur][@ext]` → chemin Maven.
pub fn maven_path(name: &str) -> Result<String> {
    let (coords, ext) = match name.split_once('@') {
        Some((c, e)) => (c, e),
        None => (name, "jar"),
    };
    let parts: Vec<&str> = coords.split(':').collect();
    if parts.len() < 3 {
        return Err(anyhow!("coordonnée Maven invalide : {name}"));
    }
    let (group, artifact, version) = (parts[0].replace('.', "/"), parts[1], parts[2]);
    let file = match parts.get(3) {
        Some(classifier) => format!("{artifact}-{version}-{classifier}.{ext}"),
        None => format!("{artifact}-{version}.{ext}"),
    };
    Ok(format!("{group}/{artifact}/{version}/{file}"))
}

impl Library {
    pub fn path(&self) -> Result<String> {
        if let Some(p) = self.downloads.as_ref().and_then(|d| d.artifact.as_ref()).and_then(|a| a.path.clone()) {
            return Ok(p);
        }
        maven_path(&self.name)
    }

    fn download(&self, libraries: &Path) -> Result<Option<Download>> {
        if let Some(a) = self.downloads.as_ref().and_then(|d| d.artifact.as_ref()) {
            if a.url.is_empty() {
                // Produit localement par l'installeur NeoForge (client patché…).
                return Ok(None);
            }
            return Ok(Some(Download {
                url: a.url.clone(),
                path: crate::paths::safe_join(libraries, self.path()?)?,
                hash: Hash::Sha1(a.sha1.clone()),
                size: Some(a.size),
                executable: false,
            }));
        }
        if let Some(base) = &self.url {
            let rel = maven_path(&self.name)?;
            return Ok(Some(Download {
                url: format!("{}/{rel}", base.trim_end_matches('/')),
                path: crate::paths::safe_join(libraries, rel)?,
                hash: Hash::None,
                size: None,
                executable: false,
            }));
        }
        Ok(None)
    }
}

// ─── Lecture et fusion ──────────────────────────────────────────────────────

pub fn version_json_path(paths: &Paths, id: &str) -> PathBuf {
    paths.versions().join(id).join(format!("{id}.json"))
}

pub fn read_version(paths: &Paths, id: &str) -> Result<VersionJson> {
    let p = version_json_path(paths, id);
    let text = std::fs::read_to_string(&p).with_context(|| format!("lecture de {}", p.display()))?;
    serde_json::from_str(&text).with_context(|| format!("fichier de version invalide : {}", p.display()))
}

/// Version prête à lancer : la fille (NeoForge) complétée par sa mère.
#[derive(Clone, Debug)]
pub struct Profile {
    pub id: String,
    /// Version vanilla dont on prend le jar.
    pub jar_version: String,
    pub main_class: String,
    pub libraries: Vec<Library>,
    pub game_args: Vec<Value>,
    pub jvm_args: Vec<Value>,
    pub asset_index: AssetIndexRef,
    pub logging: Option<LoggingClient>,
    pub kind: String,
}

pub fn resolve_profile(paths: &Paths, id: &str) -> Result<Profile> {
    let child = read_version(paths, id)?;
    let (parent, jar_version) = match &child.inherits_from {
        Some(p) => (Some(read_version(paths, p)?), p.clone()),
        None => (None, child.id.clone()),
    };
    let base = parent.clone().unwrap_or_default();

    // Bibliothèques : celles de la fille d'abord ; une bibliothèque de la mère
    // déjà fournie par la fille (même groupe:artefact:classificateur) est écartée.
    let key = |l: &Library| {
        let p: Vec<&str> = l.name.split(':').collect();
        format!("{}:{}:{}", p.first().unwrap_or(&""), p.get(1).unwrap_or(&""), p.get(3).unwrap_or(&""))
    };
    let mut seen = HashSet::new();
    let mut libraries = Vec::new();
    for l in child.libraries.iter().chain(base.libraries.iter()) {
        if seen.insert(key(l)) {
            libraries.push(l.clone());
        }
    }

    let args_of = |v: &VersionJson| v.arguments.clone().unwrap_or_default();
    let (ca, pa) = (args_of(&child), args_of(&base));

    Ok(Profile {
        id: child.id.clone(),
        jar_version,
        main_class: child.main_class.clone().or(base.main_class.clone()).ok_or_else(|| anyhow!("mainClass absente"))?,
        libraries,
        game_args: pa.game.into_iter().chain(ca.game).collect(),
        jvm_args: pa.jvm.into_iter().chain(ca.jvm).collect(),
        asset_index: child
            .asset_index
            .clone()
            .or(base.asset_index.clone())
            .ok_or_else(|| anyhow!("assetIndex absent"))?,
        // Pas la config de journalisation vanilla pour une version fille :
        // elle remplacerait celle de NeoForge (plus de debug.log, sortie en
        // XML). Prism fait de même. On ne prend donc que celle de la fille.
        logging: child.logging.as_ref().and_then(|l| l.client.clone()),
        kind: child.kind.clone().or(base.kind.clone()).unwrap_or_else(|| "release".into()),
    })
}

// ─── Installation de la version vanilla ─────────────────────────────────────

#[derive(Deserialize)]
struct VersionManifest {
    versions: Vec<ManifestVersion>,
}
#[derive(Deserialize)]
struct ManifestVersion {
    id: String,
    url: String,
    sha1: String,
}

#[derive(Deserialize)]
struct AssetIndex {
    objects: HashMap<String, AssetObject>,
}
#[derive(Deserialize)]
struct AssetObject {
    hash: String,
    size: u64,
}

pub async fn ensure_vanilla(paths: &Paths, version: &str, reporter: &dyn Reporter) -> Result<()> {
    reporter.stage("minecraft", &format!("Minecraft {version}"));
    let client = net::client();

    let manifest: VersionManifest = net::fetch_json(&client, MANIFEST_URL).await.context("liste des versions de Mojang")?;
    let entry = manifest
        .versions
        .iter()
        .find(|v| v.id == version)
        .ok_or_else(|| anyhow!("version {version} inconnue de Mojang"))?;
    net::download(
        &client,
        &Download {
            url: entry.url.clone(),
            path: version_json_path(paths, version),
            hash: Hash::Sha1(entry.sha1.clone()),
            size: None,
            executable: false,
        },
    )
    .await?;
    let v = read_version(paths, version)?;

    let mut list = Vec::new();
    if let Some(d) = &v.downloads {
        list.push(Download {
            url: d.client.url.clone(),
            path: paths.versions().join(version).join(format!("{version}.jar")),
            hash: Hash::Sha1(d.client.sha1.clone()),
            size: Some(d.client.size),
            executable: false,
        });
    }
    for lib in v.libraries.iter().filter(|l| rules_allow(&l.rules)) {
        if let Some(d) = lib.download(&paths.libraries())? {
            list.push(d);
        }
    }
    if let Some(l) = v.logging.as_ref().and_then(|l| l.client.as_ref()) {
        list.push(Download {
            url: l.file.url.clone(),
            path: crate::paths::safe_join(&paths.assets().join("log_configs"), &l.file.id)?,
            hash: Hash::Sha1(l.file.sha1.clone()),
            size: Some(l.file.size),
            executable: false,
        });
    }
    reporter.log("jeu et bibliothèques");
    net::download_all(&client, list, reporter, 16).await?;

    let idx = v.asset_index.as_ref().ok_or_else(|| anyhow!("assetIndex absent"))?;
    let idx_path = paths.assets().join("indexes").join(format!("{}.json", idx.id));
    net::download(
        &client,
        &Download {
            url: idx.url.clone(),
            path: idx_path.clone(),
            hash: Hash::Sha1(idx.sha1.clone()),
            size: Some(idx.size),
            executable: false,
        },
    )
    .await?;
    let index: AssetIndex = serde_json::from_str(&std::fs::read_to_string(&idx_path)?)?;
    let mut seen = HashSet::new();
    let assets: Vec<Download> = index
        .objects
        .values()
        .filter(|o| seen.insert(o.hash.clone()))
        .map(|o| {
            let prefix = &o.hash[..2];
            Download {
                url: format!("{RESOURCES_URL}/{prefix}/{}", o.hash),
                path: paths.assets().join("objects").join(prefix).join(&o.hash),
                hash: Hash::Sha1(o.hash.clone()),
                size: Some(o.size),
                executable: false,
            }
        })
        .collect();
    reporter.log(&format!("{} sons et textures", assets.len()));
    net::download_all(&client, assets, reporter, 32).await?;
    Ok(())
}

/// Bibliothèques d'une version fille (NeoForge) encore absentes.
pub async fn ensure_libraries(paths: &Paths, profile: &Profile, reporter: &dyn Reporter) -> Result<()> {
    let mut list = Vec::new();
    for lib in profile.libraries.iter().filter(|l| rules_allow(&l.rules)) {
        if let Some(d) = lib.download(&paths.libraries())? {
            list.push(d);
        }
    }
    net::download_all(&net::client(), list, reporter, 16).await
}

// ─── Ligne de commande ──────────────────────────────────────────────────────

/// Aplati une liste d'arguments du fichier de version (chaînes, ou objets
/// `{rules, value}`), règles appliquées.
fn flatten_args(args: &[Value]) -> Vec<String> {
    let mut out = Vec::new();
    for a in args {
        match a {
            Value::String(s) => out.push(s.clone()),
            Value::Object(o) => {
                let rules: Option<Vec<Rule>> = o.get("rules").and_then(|r| serde_json::from_value(r.clone()).ok());
                if !rules_allow(&rules) {
                    continue;
                }
                match o.get("value") {
                    Some(Value::String(s)) => out.push(s.clone()),
                    Some(Value::Array(v)) => out.extend(v.iter().filter_map(|x| x.as_str().map(String::from))),
                    _ => {}
                }
            }
            _ => {}
        }
    }
    out
}

pub struct LaunchVars {
    pub player_name: String,
    pub uuid: String,
    pub access_token: String,
    pub xuid: String,
    pub user_type: String,
    pub game_dir: PathBuf,
    pub memory_mb: u64,
    pub extra_jvm: Vec<String>,
}

/// Classpath dans l'ordre des bibliothèques, puis le jar du jeu. Le jar est
/// recopié sous le nom de la version lancée (`neoforge-X/neoforge-X.jar`) :
/// NeoForge l'écarte par ce nom (`-DignoreList=…,${version_name}.jar`).
pub fn classpath(paths: &Paths, profile: &Profile) -> Result<Vec<PathBuf>> {
    let mut cp = Vec::new();
    let mut seen = HashSet::new();
    for lib in profile.libraries.iter().filter(|l| rules_allow(&l.rules)) {
        let p = paths.libraries().join(lib.path()?);
        if seen.insert(p.clone()) {
            cp.push(p);
        }
    }
    let vanilla_jar = paths.versions().join(&profile.jar_version).join(format!("{}.jar", profile.jar_version));
    let jar = paths.versions().join(&profile.id).join(format!("{}.jar", profile.id));
    if profile.id != profile.jar_version && !jar.exists() {
        std::fs::create_dir_all(jar.parent().unwrap())?;
        std::fs::copy(&vanilla_jar, &jar)?;
    }
    cp.push(jar);
    Ok(cp)
}

pub fn command_line(paths: &Paths, profile: &Profile, vars: &LaunchVars) -> Result<(Vec<String>, Vec<String>)> {
    let sep = if cfg!(windows) { ";" } else { ":" };
    let cp: Vec<String> = classpath(paths, profile)?.iter().map(|p| p.display().to_string()).collect();
    let natives = paths.natives();
    std::fs::create_dir_all(&natives)?;

    let subst: HashMap<&str, String> = HashMap::from([
        ("auth_player_name", vars.player_name.clone()),
        ("version_name", profile.id.clone()),
        ("game_directory", vars.game_dir.display().to_string()),
        ("assets_root", paths.assets().display().to_string()),
        ("assets_index_name", profile.asset_index.id.clone()),
        ("auth_uuid", vars.uuid.clone()),
        ("auth_access_token", vars.access_token.clone()),
        ("clientid", String::new()),
        ("auth_xuid", vars.xuid.clone()),
        ("user_type", vars.user_type.clone()),
        ("version_type", profile.kind.clone()),
        ("natives_directory", natives.display().to_string()),
        ("launcher_name", crate::config::LAUNCHER_NAME.into()),
        ("launcher_version", crate::config::LAUNCHER_VERSION.into()),
        ("classpath", cp.join(sep)),
        ("classpath_separator", sep.into()),
        ("library_directory", paths.libraries().display().to_string()),
        ("user_properties", "{}".into()),
    ]);
    let fill = |s: &str| {
        let mut out = s.to_string();
        for (k, v) in &subst {
            out = out.replace(&format!("${{{k}}}"), v);
        }
        out
    };

    let mut jvm = vec![format!("-Xmx{}m", vars.memory_mb), "-Xms1024m".to_string()];
    jvm.extend(vars.extra_jvm.iter().cloned());
    jvm.extend(crate::lines::JAVA_UTF8.map(String::from));
    jvm.extend(flatten_args(&profile.jvm_args).iter().map(|a| fill(a)));
    if let Some(l) = &profile.logging {
        let cfg = paths.assets().join("log_configs").join(&l.file.id);
        jvm.push(l.argument.replace("${path}", &cfg.display().to_string()));
    }
    jvm.push(profile.main_class.clone());
    let game: Vec<String> = flatten_args(&profile.game_args).iter().map(|a| fill(a)).collect();
    Ok((jvm, game))
}

/// Le fichier de version existe et toutes ses bibliothèques sont là.
pub fn is_installed(paths: &Paths, id: &str) -> bool {
    let Ok(profile) = resolve_profile(paths, id) else { return false };
    profile
        .libraries
        .iter()
        .filter(|l| rules_allow(&l.rules))
        .all(|l| l.path().map(|p| paths.libraries().join(p).exists()).unwrap_or(false))
}
