//! Le serveur est-il là, et combien de joueurs ? Protocole « Server List
//! Ping » de Minecraft (le même que la liste des serveurs du jeu).

use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use serde::Serialize;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

#[derive(Serialize, Clone, Debug)]
pub struct ServerStatus {
    pub online: bool,
    pub players: u32,
    pub max_players: u32,
    pub version: String,
    /// Message du serveur (texte brut, sans codes de couleur).
    pub motd: String,
    pub latency_ms: u64,
    pub error: Option<String>,
}

fn write_varint(buf: &mut Vec<u8>, mut v: i32) {
    loop {
        let mut b = (v & 0x7f) as u8;
        v = ((v as u32) >> 7) as i32;
        if v != 0 {
            b |= 0x80;
        }
        buf.push(b);
        if v == 0 {
            break;
        }
    }
}

async fn read_varint(s: &mut TcpStream) -> Result<i32> {
    let mut result = 0i32;
    for i in 0..5 {
        let b = s.read_u8().await?;
        result |= ((b & 0x7f) as i32) << (7 * i);
        if b & 0x80 == 0 {
            return Ok(result);
        }
    }
    Err(anyhow!("varint trop long"))
}

fn packet(id: i32, body: &[u8]) -> Vec<u8> {
    let mut inner = Vec::new();
    write_varint(&mut inner, id);
    inner.extend_from_slice(body);
    let mut out = Vec::new();
    write_varint(&mut out, inner.len() as i32);
    out.extend(inner);
    out
}

async fn query(host: &str, port: u16) -> Result<ServerStatus> {
    // La latence se mesure comme en jeu : aller-retour d'un paquet « ping »
    // sur la connexion déjà ouverte. Chronométrer l'ouverture incluait la
    // résolution DNS, qui ajoutait jusqu'à ~100 ms quand le cache expirait
    // (des bonds à 100 ms sur une connexion stable).
    let mut s = TcpStream::connect((host, port)).await?;
    s.set_nodelay(true)?;

    let mut hs = Vec::new();
    write_varint(&mut hs, -1); // version de protocole : « n'importe laquelle »
    write_varint(&mut hs, host.len() as i32);
    hs.extend_from_slice(host.as_bytes());
    hs.extend_from_slice(&port.to_be_bytes());
    write_varint(&mut hs, 1); // état suivant : statut
    s.write_all(&packet(0, &hs)).await?;
    s.write_all(&packet(0, &[])).await?;

    let _len = read_varint(&mut s).await?;
    let _id = read_varint(&mut s).await?;
    let n = read_varint(&mut s).await? as usize;
    if n > 1 << 20 {
        return Err(anyhow!("réponse trop grande"));
    }
    let mut json = vec![0u8; n];
    s.read_exact(&mut json).await?;
    let v: serde_json::Value = serde_json::from_slice(&json)?;

    // Ping (0x01) avec une valeur, pong (0x01) avec la même.
    let token: i64 = 0x7475_7269; // « turi »
    let start = Instant::now();
    s.write_all(&packet(1, &token.to_be_bytes())).await?;
    let _len = read_varint(&mut s).await?;
    let _id = read_varint(&mut s).await?;
    let _echo = s.read_i64().await?;
    let latency_ms = start.elapsed().as_millis() as u64;

    Ok(ServerStatus {
        online: true,
        players: v["players"]["online"].as_u64().unwrap_or(0) as u32,
        max_players: v["players"]["max"].as_u64().unwrap_or(0) as u32,
        version: v["version"]["name"].as_str().unwrap_or("").to_string(),
        motd: plain_text(&v["description"]),
        latency_ms,
        error: None,
    })
}

/// Le MOTD arrive en texte ou en composant JSON (`text` + `extra`), avec des
/// codes de couleur « §x » : on n'en garde que le texte.
fn plain_text(v: &serde_json::Value) -> String {
    fn walk(v: &serde_json::Value, out: &mut String) {
        match v {
            serde_json::Value::String(s) => out.push_str(s),
            serde_json::Value::Object(o) => {
                if let Some(t) = o.get("text") {
                    walk(t, out);
                }
                if let Some(serde_json::Value::Array(extra)) = o.get("extra") {
                    extra.iter().for_each(|e| walk(e, out));
                }
            }
            serde_json::Value::Array(a) => a.iter().for_each(|e| walk(e, out)),
            _ => {}
        }
    }
    let mut raw = String::new();
    walk(v, &mut raw);
    let mut out = String::new();
    let mut chars = raw.chars();
    while let Some(c) = chars.next() {
        if c == '§' {
            chars.next();
        } else {
            out.push(c);
        }
    }
    out.trim().to_string()
}

pub async fn status(host: &str, port: u16) -> ServerStatus {
    match tokio::time::timeout(Duration::from_secs(5), query(host, port)).await {
        Ok(Ok(s)) => s,
        Ok(Err(e)) => offline(e.to_string()),
        Err(_) => offline("pas de réponse en 5 s".into()),
    }
}

fn offline(error: String) -> ServerStatus {
    ServerStatus {
        online: false,
        players: 0,
        max_players: 0,
        version: String::new(),
        motd: String::new(),
        latency_ms: 0,
        error: Some(error),
    }
}
