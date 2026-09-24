//! Compte Microsoft → Minecraft (L1).
//!
//! Chaîne : OAuth Microsoft (flux « code d'appareil ») → Xbox Live → XSTS →
//! Minecraft Services, puis profil (qui prouve que le compte possède le jeu).
//! Le flux par code d'appareil évite un serveur web local : le launcher
//! affiche un code, le joueur le saisit sur microsoft.com/link.
//!
//! Exige une App Registration Azure approuvée par Mojang, en « Mobile and
//! desktop applications » avec « Allow public client flows » (voir README).
//! Le jeton de renouvellement va dans le trousseau du système, jamais sur disque.

use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;

const AUTHORITY: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0";
const SCOPE: &str = "XboxLive.signin offline_access";
const KEYRING_SERVICE: &str = "turicraft-launcher";
const KEYRING_USER: &str = "microsoft-refresh-token";

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct DeviceCode {
    pub user_code: String,
    pub device_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    pub interval: u64,
    pub message: Option<String>,
}

#[derive(Deserialize)]
struct MsToken {
    access_token: String,
    refresh_token: Option<String>,
}

#[derive(Deserialize)]
struct MsError {
    error: String,
    error_description: Option<String>,
}

/// Ce qu'il faut pour lancer le jeu. `access_token` ne sort jamais du launcher
/// que vers la ligne de commande du jeu.
#[derive(Clone)]
pub struct Session {
    pub name: String,
    pub uuid: String,
    pub access_token: String,
    pub xuid: String,
}

fn http() -> reqwest::Client {
    crate::net::client()
}

pub async fn start_device_code(client_id: &str) -> Result<DeviceCode> {
    let resp = http()
        .post(format!("{AUTHORITY}/devicecode"))
        .form(&[("client_id", client_id), ("scope", SCOPE)])
        .send()
        .await?;
    if !resp.status().is_success() {
        bail!("Microsoft refuse la demande de code ({}) : {}", resp.status(), resp.text().await.unwrap_or_default());
    }
    Ok(resp.json().await?)
}

/// Attend que le joueur ait saisi le code, puis ouvre la session.
pub async fn finish_device_code(client_id: &str, code: &DeviceCode) -> Result<Session> {
    let deadline = Instant::now() + Duration::from_secs(code.expires_in);
    let mut interval = code.interval.max(1);
    loop {
        if Instant::now() > deadline {
            bail!("le code a expiré, recommence la connexion");
        }
        tokio::time::sleep(Duration::from_secs(interval)).await;
        let resp = http()
            .post(format!("{AUTHORITY}/token"))
            .form(&[
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ("client_id", client_id),
                ("device_code", code.device_code.as_str()),
            ])
            .send()
            .await?;
        if resp.status().is_success() {
            let tok: MsToken = resp.json().await?;
            return session_from_ms(tok).await;
        }
        let err: MsError = resp.json().await.context("réponse de Microsoft illisible")?;
        match err.error.as_str() {
            "authorization_pending" => continue,
            "slow_down" => interval += 5,
            "authorization_declined" => bail!("connexion refusée sur la page de Microsoft"),
            "expired_token" => bail!("le code a expiré, recommence la connexion"),
            _ => bail!("Microsoft : {} {}", err.error, err.error_description.unwrap_or_default()),
        }
    }
}

// ─── Connexion par le navigateur (par défaut) ───────────────────────────────
//
// Flux « code d'autorisation » avec PKCE et redirection locale : le joueur
// choisit son compte sur la page Microsoft, qui renvoie le navigateur vers un
// petit serveur du launcher (http://localhost:<port>, le temps de la
// connexion). Exige « http://localhost » dans les Redirect URIs de l'App
// Registration (plateforme Mobile and desktop). Sinon : le code d'appareil.

/// Page de connexion prête à ouvrir, et l'écoute qui attend le retour.
pub struct BrowserLogin {
    pub url: String,
    listener: tokio::net::TcpListener,
    verifier: String,
    state: String,
    redirect: String,
}

fn random_b64(bytes: usize) -> String {
    use base64::Engine;
    let mut buf = Vec::with_capacity(bytes);
    while buf.len() < bytes {
        buf.extend_from_slice(uuid::Uuid::new_v4().as_bytes());
    }
    buf.truncate(bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(buf)
}

fn url_encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

fn url_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < bytes.len() => {
                if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                    out.push(v);
                    i += 2;
                }
            }
            b => out.push(b),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub async fn start_browser_login(client_id: &str) -> Result<BrowserLogin> {
    use base64::Engine;
    use sha2::{Digest, Sha256};
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await?;
    let redirect = format!("http://localhost:{}", listener.local_addr()?.port());
    let verifier = random_b64(48);
    let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let state = random_b64(16);
    let url = format!(
        "{AUTHORITY}/authorize?client_id={}&response_type=code&redirect_uri={}&response_mode=query&scope={}&code_challenge={challenge}&code_challenge_method=S256&state={state}&prompt=select_account",
        url_encode(client_id),
        url_encode(&redirect),
        url_encode(SCOPE),
    );
    Ok(BrowserLogin { url, listener, verifier, state, redirect })
}

/// La page affichée dans le navigateur au retour.
fn page(ok: bool, message: &str) -> String {
    let (title, color) = if ok { ("Connecté à Turi Craft", "#f5c518") } else { ("Connexion impossible", "#ff6b6b") };
    let body = format!(
        "<!doctype html><meta charset=utf-8><title>{title}</title>\
         <body style=\"margin:0;height:100vh;display:grid;place-items:center;background:#121316;color:#f1f1f1;font:16px system-ui,sans-serif\">\
         <div style=\"text-align:center;border-top:3px solid {color};padding:28px 36px;background:#000\">\
         <h1 style=\"margin:0 0 8px;font-size:22px;color:{color}\">{title}</h1><p style=\"margin:0;color:#a3a6ad\">{message}</p></div></body>"
    );
    format!("HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len())
}

/// Attend le retour de Microsoft (5 minutes au plus), puis ouvre la session.
pub async fn finish_browser_login(client_id: &str, login: BrowserLogin) -> Result<Session> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let wait = async {
        loop {
            let (mut sock, _) = login.listener.accept().await?;
            let mut buf = vec![0u8; 8192];
            let n = sock.read(&mut buf).await.unwrap_or(0);
            let req = String::from_utf8_lossy(&buf[..n]).to_string();
            // « GET /?code=…&state=… HTTP/1.1 » ; le reste (favicon…) est ignoré.
            let Some(query) = req.lines().next().and_then(|l| l.split_whitespace().nth(1)).and_then(|p| p.split_once('?')).map(|(_, q)| q.to_string()) else {
                let _ = sock.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n").await;
                continue;
            };
            let params: std::collections::HashMap<String, String> = query
                .split('&')
                .filter_map(|kv| kv.split_once('='))
                .map(|(k, v)| (k.to_string(), url_decode(v)))
                .collect();
            if let Some(err) = params.get("error") {
                let desc = params.get("error_description").cloned().unwrap_or_default();
                let _ = sock.write_all(page(false, "Reviens au launcher.").as_bytes()).await;
                bail!("Microsoft : {err} {desc}");
            }
            if params.get("state") != Some(&login.state) {
                let _ = sock.write_all(page(false, "Réponse inattendue : reviens au launcher.").as_bytes()).await;
                bail!("réponse de connexion inattendue (state)");
            }
            let Some(code) = params.get("code").cloned() else { continue };
            let _ = sock.write_all(page(true, "Tu peux fermer cet onglet et revenir au launcher.").as_bytes()).await;
            return Ok::<String, anyhow::Error>(code);
        }
    };
    let code = tokio::time::timeout(Duration::from_secs(300), wait)
        .await
        .map_err(|_| anyhow!("connexion abandonnée (5 minutes sans réponse)"))??;
    let resp = http()
        .post(format!("{AUTHORITY}/token"))
        .form(&[
            ("grant_type", "authorization_code"),
            ("client_id", client_id),
            ("code", code.as_str()),
            ("redirect_uri", login.redirect.as_str()),
            ("code_verifier", login.verifier.as_str()),
            ("scope", SCOPE),
        ])
        .send()
        .await?;
    if !resp.status().is_success() {
        let e: MsError = resp.json().await.context("réponse de Microsoft illisible")?;
        bail!("Microsoft : {} {}", e.error, e.error_description.unwrap_or_default());
    }
    session_from_ms(resp.json().await?).await
}

/// Session à partir du jeton gardé dans le trousseau (lancements suivants).
pub async fn refresh(client_id: &str) -> Result<Session> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)?;
    let refresh_token = entry.get_password().map_err(|_| anyhow!("aucun compte enregistré : connecte-toi"))?;
    let resp = http()
        .post(format!("{AUTHORITY}/token"))
        .form(&[
            ("grant_type", "refresh_token"),
            ("client_id", client_id),
            ("refresh_token", refresh_token.as_str()),
            ("scope", SCOPE),
        ])
        .send()
        .await?;
    if !resp.status().is_success() {
        let _ = entry.delete_credential();
        bail!("la session a expiré, reconnecte-toi");
    }
    session_from_ms(resp.json().await?).await
}

pub fn logout() {
    if let Ok(entry) = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER) {
        let _ = entry.delete_credential();
    }
}

async fn session_from_ms(ms: MsToken) -> Result<Session> {
    if let Some(rt) = &ms.refresh_token {
        keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)?
            .set_password(rt)
            .context("impossible d'enregistrer le compte dans le trousseau du système")?;
    }
    let c = http();

    // Xbox Live
    let xbl: serde_json::Value = c
        .post("https://user.auth.xboxlive.com/user/authenticate")
        .json(&json!({
            "Properties": { "AuthMethod": "RPS", "SiteName": "user.auth.xboxlive.com", "RpsTicket": format!("d={}", ms.access_token) },
            "RelyingParty": "http://auth.xboxlive.com",
            "TokenType": "JWT"
        }))
        .send()
        .await?
        .error_for_status()
        .context("Xbox Live")?
        .json()
        .await?;
    let xbl_token = xbl["Token"].as_str().ok_or_else(|| anyhow!("Xbox Live : pas de jeton"))?;
    let uhs = xbl["DisplayClaims"]["xui"][0]["uhs"].as_str().ok_or_else(|| anyhow!("Xbox Live : pas d'uhs"))?;

    // XSTS
    let resp = c
        .post("https://xsts.auth.xboxlive.com/xsts/authorize")
        .json(&json!({
            "Properties": { "SandboxId": "RETAIL", "UserTokens": [xbl_token] },
            "RelyingParty": "rp://api.minecraftservices.com/",
            "TokenType": "JWT"
        }))
        .send()
        .await?;
    if resp.status().as_u16() == 401 {
        let e: serde_json::Value = resp.json().await.unwrap_or_default();
        bail!(match e["XErr"].as_u64() {
            Some(2148916233) => "ce compte Microsoft n'a pas de profil Xbox : crée-le sur xbox.com, puis réessaie".to_string(),
            Some(2148916235) => "Xbox Live n'est pas disponible dans ton pays".to_string(),
            Some(2148916236) | Some(2148916237) => "vérification d'âge demandée par Xbox : connecte-toi une fois sur xbox.com".to_string(),
            Some(2148916238) => "compte enfant : il doit être ajouté à une famille Microsoft par un adulte".to_string(),
            other => format!("Xbox refuse la connexion (XErr {other:?})"),
        });
    }
    let xsts: serde_json::Value = resp.error_for_status().context("XSTS")?.json().await?;
    let xsts_token = xsts["Token"].as_str().ok_or_else(|| anyhow!("XSTS : pas de jeton"))?;
    let xuid = xsts["DisplayClaims"]["xui"][0]["xid"].as_str().unwrap_or_default().to_string();

    // Minecraft
    let mc: serde_json::Value = c
        .post("https://api.minecraftservices.com/authentication/login_with_xbox")
        .json(&json!({ "identityToken": format!("XBL3.0 x={uhs};{xsts_token}") }))
        .send()
        .await?
        .error_for_status()
        .context("Minecraft Services (le client ID est-il approuvé par Mojang ?)")?
        .json()
        .await?;
    let access_token = mc["access_token"].as_str().ok_or_else(|| anyhow!("Minecraft : pas de jeton"))?.to_string();

    let resp = c
        .get("https://api.minecraftservices.com/minecraft/profile")
        .bearer_auth(&access_token)
        .send()
        .await?;
    if resp.status().as_u16() == 404 {
        bail!("ce compte ne possède pas Minecraft Java Edition");
    }
    let profile: serde_json::Value = resp.error_for_status()?.json().await?;
    Ok(Session {
        name: profile["name"].as_str().unwrap_or_default().to_string(),
        uuid: profile["id"].as_str().unwrap_or_default().to_string(),
        access_token,
        xuid,
    })
}

/// Mode hors ligne (essais) : UUID dérivé du pseudo, comme le fait le serveur
/// vanilla pour « OfflinePlayer:<pseudo> ».
pub fn offline_session(name: &str) -> Session {
    // Java UUID.nameUUIDFromBytes : MD5 SANS espace de noms, puis version 3.
    use md5::{Digest, Md5};
    let mut h: [u8; 16] = Md5::digest(format!("OfflinePlayer:{name}").as_bytes()).into();
    h[6] = (h[6] & 0x0f) | 0x30;
    h[8] = (h[8] & 0x3f) | 0x80;
    let uuid = uuid::Uuid::from_bytes(h);
    Session { name: name.to_string(), uuid: uuid.simple().to_string(), access_token: "0".into(), xuid: "0".into() }
}

#[cfg(test)]
mod tests {
    #[test]
    fn encodage_url() {
        assert_eq!(super::url_encode("XboxLive.signin offline_access"), "XboxLive.signin%20offline_access");
        assert_eq!(super::url_decode("a%20b%3Dc+d"), "a b=c d");
    }

    #[test]
    fn uuid_hors_ligne_comme_java() {
        // UUID.nameUUIDFromBytes("OfflinePlayer:Notch".getBytes(UTF_8))
        assert_eq!(super::offline_session("Notch").uuid, "b50ad385829d3141a2167e7d7539ba7f");
    }
}
