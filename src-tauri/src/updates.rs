//! Mises à jour (L5) : version du pack installée contre celle en ligne.
//! Celle du launcher passe par tauri-plugin-updater (lib.rs,
//! launcher_update_check) : latest.json sur le homelab, signé.

use serde::{Deserialize, Serialize};

use crate::paths::Paths;

#[derive(Serialize, Clone, Debug)]
pub struct Updates {
    pub pack_installed: Option<String>,
    pub pack_online: Option<String>,
    pub launcher_current: String,
}

fn base_url(pack_url: &str) -> Option<&str> {
    pack_url.rsplit_once('/').map(|(b, _)| b)
}

pub async fn check(paths: &Paths, pack_url: &str) -> Updates {
    let client = crate::net::client();
    let pack_online = async {
        let text = crate::net::fetch_text(&client, pack_url).await.ok()?;
        let v: toml::Value = toml::from_str(&text).ok()?;
        v.get("version")?.as_str().map(String::from)
    }
    .await;
    Updates {
        pack_installed: crate::packwiz::installed_pack_version(paths),
        pack_online,
        launcher_current: crate::config::LAUNCHER_VERSION.to_string(),
    }
}

// ─── Nouveautés ─────────────────────────────────────────────────────────────

/// Une entrée de `turicraft/news.json` (servi avec le pack, hors index).
#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct NewsItem {
    pub date: String,
    pub title: String,
    #[serde(default)]
    pub body: String,
    /// Lien vers le message d'origine (nouveautés venues de Discord).
    #[serde(default)]
    pub url: String,
}

pub async fn news(pack_url: &str) -> Vec<NewsItem> {
    let Some(base) = base_url(pack_url) else { return Vec::new() };
    crate::net::fetch_json(&crate::net::client(), &format!("{base}/turicraft/news.json")).await.unwrap_or_default()
}
