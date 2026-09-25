//! Téléchargements. Chaque fichier est écrit dans un `.part` puis renommé une
//! fois son empreinte vérifiée : une coupure réseau ne laisse jamais un fichier
//! tronqué à la place d'un bon.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use futures::{stream, StreamExt};
use sha1::Sha1;
use sha2::{Digest, Sha256};

use crate::progress::Reporter;

pub fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(crate::config::USER_AGENT)
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(300))
        .build()
        .expect("client HTTP")
}

#[derive(Clone, Debug)]
pub enum Hash {
    Sha1(String),
    Sha256(String),
    None,
}

#[derive(Clone, Debug)]
pub struct Download {
    pub url: String,
    pub path: PathBuf,
    pub hash: Hash,
    pub size: Option<u64>,
    pub executable: bool,
}

pub async fn fetch_text(client: &reqwest::Client, url: &str) -> Result<String> {
    let resp = client.get(url).send().await.with_context(|| format!("GET {url}"))?;
    if !resp.status().is_success() {
        bail!("GET {url} : HTTP {}", resp.status());
    }
    let text = resp.text().await?;
    if text.is_empty() {
        // Vu le 24/09 : pack.turi-industries.eu répondait 200 avec un corps
        // vide pour tout. Mieux vaut le dire que d'échouer plus loin.
        bail!("GET {url} : réponse vide (le site du pack est-il bien relié au serveur ?)");
    }
    Ok(text)
}

pub async fn fetch_json<T: serde::de::DeserializeOwned>(client: &reqwest::Client, url: &str) -> Result<T> {
    let text = fetch_text(client, url).await?;
    serde_json::from_str(&text).with_context(|| format!("JSON invalide : {url}"))
}

fn file_hash_matches(path: &Path, hash: &Hash) -> Result<bool> {
    let data = std::fs::read(path)?;
    Ok(match hash {
        Hash::Sha1(expected) => hex::encode(Sha1::digest(&data)).eq_ignore_ascii_case(expected),
        Hash::Sha256(expected) => hex::encode(Sha256::digest(&data)).eq_ignore_ascii_case(expected),
        Hash::None => true,
    })
}

/// Déjà là et bon ? Quand la taille est connue, on se contente d'elle : relire
/// les ~4 000 fichiers d'assets à chaque lancement coûterait plusieurs secondes.
fn already_valid(d: &Download) -> bool {
    let Ok(meta) = std::fs::metadata(&d.path) else { return false };
    match d.size {
        Some(size) => meta.len() == size,
        None => file_hash_matches(&d.path, &d.hash).unwrap_or(false),
    }
}

pub async fn download(client: &reqwest::Client, d: &Download) -> Result<()> {
    if already_valid(d) {
        return Ok(());
    }
    if let Some(parent) = d.path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let part = part_path(&d.path);
    let mut last_err = None;
    for attempt in 0..3 {
        if attempt > 0 {
            tokio::time::sleep(Duration::from_millis(500 * attempt)).await;
        }
        match download_once(client, d, &part).await {
            Ok(()) => {
                tokio::fs::rename(&part, &d.path).await?;
                #[cfg(unix)]
                if d.executable {
                    use std::os::unix::fs::PermissionsExt;
                    std::fs::set_permissions(&d.path, std::fs::Permissions::from_mode(0o755))?;
                }
                return Ok(());
            }
            Err(e) => last_err = Some(e),
        }
    }
    let _ = tokio::fs::remove_file(&part).await;
    Err(last_err.unwrap()).with_context(|| format!("téléchargement de {}", d.url))
}

/// `x.json` → `x.json.part`. Pas `with_extension` : `java.policy` et
/// `java.security` (runtime Java, téléchargés en parallèle) auraient partagé
/// `java.part`, et l'un pouvait recevoir le contenu de l'autre.
fn part_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".part");
    path.with_file_name(name)
}

async fn download_once(client: &reqwest::Client, d: &Download, part: &Path) -> Result<()> {
    let resp = client.get(&d.url).send().await?;
    if !resp.status().is_success() {
        bail!("HTTP {}", resp.status());
    }
    let bytes = resp.bytes().await?;
    tokio::fs::write(part, &bytes).await?;
    if !file_hash_matches(part, &d.hash)? {
        bail!("empreinte incorrecte");
    }
    Ok(())
}

/// Télécharge tout, `concurrency` à la fois ; la progression compte les fichiers.
pub async fn download_all(
    client: &reqwest::Client,
    list: Vec<Download>,
    reporter: &dyn Reporter,
    concurrency: usize,
) -> Result<()> {
    let total = list.len() as u64;
    let done = Arc::new(AtomicU64::new(0));
    reporter.progress(0, total);
    let results: Vec<Result<()>> = stream::iter(list)
        .map(|d| {
            let client = client.clone();
            let done = done.clone();
            async move {
                let r = download(&client, &d).await;
                done.fetch_add(1, Ordering::Relaxed);
                r
            }
        })
        .buffer_unordered(concurrency)
        .inspect(|_| reporter.progress(done.load(Ordering::Relaxed), total))
        .collect()
        .await;
    let errors: Vec<_> = results.into_iter().filter_map(|r| r.err()).collect();
    if let Some(first) = errors.into_iter().next() {
        return Err(first);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn un_part_par_fichier() {
        let dir = Path::new("conf/security");
        assert_ne!(part_path(&dir.join("java.policy")), part_path(&dir.join("java.security")));
        assert_eq!(part_path(&dir.join("java.policy")), dir.join("java.policy.part"));
        assert_eq!(part_path(Path::new("objects/ab/abcdef")), Path::new("objects/ab/abcdef.part"));
    }
}
