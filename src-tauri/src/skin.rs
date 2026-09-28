//! Le vrai skin du joueur, pour afficher sa tête dans le launcher. Pris chez
//! Mojang (profil → texture), gardé en cache pour les lancements hors ligne.
//! Un service tiers (mc-heads.net) affichait Steve à la place.

use anyhow::{anyhow, Context, Result};
use base64::Engine;

use crate::paths::Paths;

/// Un skin de moins de 6 h est repris tel quel : la tête s'affiche dès
/// l'ouverture, sans deux requêtes chez Mojang (qui limite ce service).
const FRESH: std::time::Duration = std::time::Duration::from_secs(6 * 3600);

/// Le skin en `data:image/png;base64,…` : l'interface le découpe elle-même
/// (tête 8×8 à (8,8), chapeau à (40,8)), sans autoriser d'autre site. Pour
/// le compte du joueur comme pour ceux du classement du Snake.
pub async fn skin_data_url(paths: &Paths, uuid: &str) -> Result<String> {
    // L'UUID vient aussi du classement (serveur du pack) : il finit dans un
    // chemin et une adresse, donc 32 chiffres hexadécimaux et rien d'autre.
    let uuid = uuid.replace('-', "").to_ascii_lowercase();
    if uuid.len() != 32 || !uuid.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(anyhow!("UUID invalide"));
    }
    let dir = paths.root.join("skins");
    let cache = dir.join(format!("{uuid}.png"));
    // Ancien emplacement (launcher 0.5 et avant), à la racine des données.
    let old = paths.root.join(format!("skin-{uuid}.png"));
    if old.exists() && !cache.exists() {
        let _ = std::fs::create_dir_all(&dir).and_then(|_| std::fs::rename(&old, &cache));
    }
    let age = std::fs::metadata(&cache).and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok());
    let bytes = match age {
        Some(a) if a < FRESH => std::fs::read(&cache)?,
        _ => match fetch(&uuid).await {
            Ok(b) => {
                let _ = std::fs::create_dir_all(&dir).and_then(|_| std::fs::write(&cache, &b));
                b
            }
            Err(e) => std::fs::read(&cache).map_err(|_| e)?,
        },
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
    // Seulement les textures de Mojang : l'adresse vient du profil.
    if !url.starts_with("https://textures.minecraft.net/") {
        return Err(anyhow!("adresse de skin inattendue"));
    }
    let resp = client.get(&url).send().await?.error_for_status()?;
    Ok(resp.bytes().await?.to_vec())
}
