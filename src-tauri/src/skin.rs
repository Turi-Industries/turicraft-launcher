//! Le vrai skin du joueur, pour afficher sa tête dans le launcher. Pris chez
//! Mojang (profil → texture), gardé en cache pour les lancements hors ligne.
//! Un service tiers (mc-heads.net) affichait Steve à la place.

use anyhow::{anyhow, Context, Result};
use base64::Engine;

use crate::paths::Paths;

/// Le skin en `data:image/png;base64,…` : l'interface le découpe elle-même
/// (tête 8×8 à (8,8), chapeau à (40,8)), sans autoriser d'autre site.
pub async fn skin_data_url(paths: &Paths, uuid: &str) -> Result<String> {
    let cache = paths.root.join(format!("skin-{uuid}.png"));
    let bytes = match fetch(uuid).await {
        Ok(b) => {
            let _ = std::fs::write(&cache, &b);
            b
        }
        Err(e) => std::fs::read(&cache).map_err(|_| e)?,
    };
    Ok(format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes)))
}

async fn fetch(uuid: &str) -> Result<Vec<u8>> {
    let client = crate::net::client();
    let profile: serde_json::Value =
        crate::net::fetch_json(&client, &format!("https://sessionserver.mojang.com/session/minecraft/profile/{uuid}"))
            .await
            .context("profil Mojang")?;
    let encoded = profile["properties"]
        .as_array()
        .and_then(|p| p.iter().find(|x| x["name"] == "textures"))
        .and_then(|x| x["value"].as_str())
        .ok_or_else(|| anyhow!("profil sans textures"))?;
    let textures: serde_json::Value =
        serde_json::from_slice(&base64::engine::general_purpose::STANDARD.decode(encoded)?)?;
    let url = textures["textures"]["SKIN"]["url"].as_str().ok_or_else(|| anyhow!("pas de skin"))?;
    // Mojang donne une adresse http:// ; le même fichier est servi en https.
    let url = url.replacen("http://", "https://", 1);
    let resp = client.get(&url).send().await?.error_for_status()?;
    Ok(resp.bytes().await?.to_vec())
}
