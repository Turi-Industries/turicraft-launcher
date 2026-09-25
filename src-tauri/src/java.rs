//! Java 21 : le runtime de Mojang (`java-runtime-delta`), celui du launcher
//! officiel. Aucune installation système demandée au joueur.

use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::{anyhow, Context, Result};
use serde::Deserialize;

use crate::net::{self, Download, Hash};
use crate::paths::Paths;
use crate::progress::Reporter;

const RUNTIMES_URL: &str =
    "https://launchermeta.mojang.com/v1/products/java-runtime/2ec0cc96c44e5a76b9c8b7c39df7210883d12871/all.json";

#[derive(Deserialize)]
struct RuntimeEntry {
    manifest: ManifestRef,
    version: RuntimeVersion,
}
#[derive(Deserialize)]
struct ManifestRef {
    url: String,
}
#[derive(Deserialize)]
struct RuntimeVersion {
    name: String,
}

#[derive(Deserialize)]
struct Manifest {
    files: HashMap<String, ManifestFile>,
}
#[derive(Deserialize)]
struct ManifestFile {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    executable: bool,
    downloads: Option<FileDownloads>,
    target: Option<String>,
}
#[derive(Deserialize)]
struct FileDownloads {
    raw: RawDownload,
}
#[derive(Deserialize)]
struct RawDownload {
    sha1: String,
    size: u64,
    url: String,
}

/// Clé de plateforme dans le catalogue de Mojang. Sous Windows, celle du
/// PROCESSEUR : un launcher x64 sur Snapdragon ferait sinon tourner Java — et
/// tout le jeu — en émulation x64. Minecraft livre ses natives LWJGL arm64.
fn platform() -> Result<&'static str> {
    let arch = if cfg!(windows) && crate::hardware::native_arm64() { "aarch64" } else { std::env::consts::ARCH };
    Ok(match (std::env::consts::OS, arch) {
        ("linux", "x86_64") => "linux",
        ("linux", "x86") => "linux-i386",
        ("macos", "aarch64") => "mac-os-arm64",
        ("macos", _) => "mac-os",
        ("windows", "x86_64") => "windows-x64",
        ("windows", "aarch64") => "windows-arm64",
        ("windows", "x86") => "windows-x86",
        (os, arch) => return Err(anyhow!("plateforme non prise en charge : {os}/{arch}")),
    })
}

/// Chemin de l'exécutable java à l'intérieur d'un runtime installé.
/// Sous Windows, `javaw` : pas de fenêtre de console à côté du jeu.
fn java_binary(root: &std::path::Path) -> PathBuf {
    if cfg!(target_os = "macos") {
        root.join("jre.bundle/Contents/Home/bin/java")
    } else if cfg!(windows) {
        root.join("bin").join("javaw.exe")
    } else {
        root.join("bin").join("java")
    }
}

/// Installe (ou vérifie) le runtime et rend le chemin de `java`.
pub async fn ensure_java(paths: &Paths, component: &str, reporter: &dyn Reporter) -> Result<PathBuf> {
    reporter.stage("java", "Java");
    let client = net::client();
    // Java arm64 sous un launcher x64 : dossier à part, pour ne pas mêler
    // ses fichiers à ceux d'un runtime x64 déjà installé.
    let root = match platform()? {
        "windows-arm64" if std::env::consts::ARCH != "aarch64" => paths.runtime().join(format!("{component}-arm64")),
        _ => paths.runtime().join(component),
    };
    let java = java_binary(&root);

    let all: HashMap<String, HashMap<String, Vec<RuntimeEntry>>> = net::fetch_json(&client, RUNTIMES_URL)
        .await
        .context("catalogue des runtimes Java de Mojang")?;
    let entry = all
        .get(platform()?)
        .and_then(|p| p.get(component))
        .and_then(|v| v.first())
        .ok_or_else(|| anyhow!("runtime {component} indisponible pour {}", platform().unwrap_or("?")))?;
    reporter.log(&format!("Java {} ({component})", entry.version.name));

    let manifest: Manifest = net::fetch_json(&client, &entry.manifest.url).await?;
    let mut downloads = Vec::new();
    let mut links = Vec::new();
    for (rel, f) in &manifest.files {
        let path = root.join(rel);
        match f.kind.as_str() {
            "directory" => std::fs::create_dir_all(&path)?,
            "file" => {
                if let Some(d) = &f.downloads {
                    downloads.push(Download {
                        url: d.raw.url.clone(),
                        path,
                        hash: Hash::Sha1(d.raw.sha1.clone()),
                        size: Some(d.raw.size),
                        executable: f.executable,
                    });
                }
            }
            "link" => {
                if let Some(t) = &f.target {
                    links.push((path, t.clone()));
                }
            }
            _ => {}
        }
    }
    net::download_all(&client, downloads, reporter, 16).await?;

    #[cfg(unix)]
    for (path, target) in links {
        if std::fs::symlink_metadata(&path).is_err() {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::os::unix::fs::symlink(&target, &path)?;
        }
    }
    #[cfg(not(unix))]
    let _ = links;

    if !java.exists() {
        return Err(anyhow!("java introuvable après installation : {}", java.display()));
    }
    Ok(java)
}
