//! Classement du Snake : lu et envoyé au service `/snake/` du serveur du pack.
//!
//! Le pseudo est prouvé comme sur un serveur Minecraft : le serveur donne un
//! défi, le launcher l'annonce à Mojang avec le jeton du joueur
//! (`session/minecraft/join`), puis le serveur demande à Mojang si ce pseudo
//! l'a bien annoncé. Le jeton ne part que chez Mojang, jamais au serveur du
//! pack.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::auth::Session;

const JOIN: &str = "https://sessionserver.mojang.com/session/minecraft/join";

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Entry {
    pub name: String,
    pub uuid: String,
    pub score: u32,
    pub date: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Top {
    pub top: Vec<Entry>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Submitted {
    pub top: Vec<Entry>,
    pub rank: u32,
    pub best: u32,
}

/// Mojang refuse la session (jeton expiré) : une session fraîche peut passer.
#[derive(Debug)]
pub struct SessionRefused(pub u16);

impl std::fmt::Display for SessionRefused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Mojang refuse la vérification du compte (HTTP {})", self.0)
    }
}

impl std::error::Error for SessionRefused {}

/// `https://…/pack.toml` → `https://…/snake`.
fn base(pack_url: &str) -> String {
    let root = pack_url.rsplit_once('/').map_or(pack_url, |(b, _)| b);
    format!("{root}/snake")
}

/// Message d'erreur du service (« {"error": …} »), sinon le code HTTP.
async fn check(resp: reqwest::Response) -> Result<reqwest::Response> {
    if resp.status().is_success() {
        return Ok(resp);
    }
    let code = resp.status();
    let msg = resp.json::<serde_json::Value>().await.ok().and_then(|v| v["error"].as_str().map(String::from));
    bail!("{}", msg.unwrap_or_else(|| format!("HTTP {code}")))
}

pub async fn top(pack_url: &str) -> Result<Vec<Entry>> {
    let resp = crate::net::client().get(format!("{}/top", base(pack_url))).send().await.context("classement injoignable")?;
    Ok(check(resp).await?.json::<Top>().await?.top)
}

pub async fn submit(pack_url: &str, session: &Session, score: u32) -> Result<Submitted> {
    let http = crate::net::client();
    let base = base(pack_url);
    #[derive(Deserialize)]
    struct Challenge {
        server_id: String,
    }
    let ch: Challenge =
        check(http.get(format!("{base}/challenge")).send().await.context("classement injoignable")?).await?.json().await?;
    // Même annonce que le jeu quand il rejoint un serveur.
    let join = http
        .post(JOIN)
        .json(&serde_json::json!({
            "accessToken": session.access_token,
            "selectedProfile": session.uuid.replace('-', ""),
            "serverId": ch.server_id,
        }))
        .send()
        .await
        .context("Mojang injoignable")?;
    if !join.status().is_success() {
        return Err(SessionRefused(join.status().as_u16()).into());
    }
    let resp = http
        .post(format!("{base}/score"))
        .json(&serde_json::json!({ "name": session.name, "server_id": ch.server_id, "score": score }))
        .send()
        .await
        .context("classement injoignable")?;
    Ok(check(resp).await?.json().await?)
}

#[cfg(test)]
mod tests {
    #[test]
    fn adresse_du_service() {
        assert_eq!(super::base("https://pack.turi-industries.eu/pack.toml"), "https://pack.turi-industries.eu/snake");
    }
}
