//! Lecture ligne à ligne de la sortie d'un processus (jeu, packwiz-installer,
//! installeur NeoForge), sans jamais échouer sur l'encodage.
//!
//! Sous Windows, Java écrit sa sortie dans la page de code du système (cp1252
//! en français) : « Pokémon » y devient l'octet 0xE9, pas de l'UTF-8.
//! `AsyncBufReadExt::lines` renvoyait alors une erreur ; au lancement, le
//! launcher abandonnait la tâche et tuait le jeu avec (25/09). On lance aussi
//! Java avec `JAVA_UTF8`, mais ce lecteur reste la garantie.

use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};

/// Demande à Java d'écrire sa sortie en UTF-8 (Java 19 et plus).
pub const JAVA_UTF8: [&str; 2] = ["-Dstdout.encoding=UTF-8", "-Dstderr.encoding=UTF-8"];

pub struct Lines<R> {
    reader: BufReader<R>,
    buf: Vec<u8>,
}

impl<R: AsyncRead + Unpin> Lines<R> {
    pub fn new(inner: R) -> Self {
        Self { reader: BufReader::new(inner), buf: Vec::new() }
    }

    /// La ligne suivante, sans son `\r\n` ; un octet invalide devient « � ».
    pub async fn next_line(&mut self) -> std::io::Result<Option<String>> {
        self.buf.clear();
        if self.reader.read_until(b'\n', &mut self.buf).await? == 0 {
            return Ok(None);
        }
        while matches!(self.buf.last(), Some(b'\n' | b'\r')) {
            self.buf.pop();
        }
        Ok(Some(String::from_utf8_lossy(&self.buf).into_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn octets_windows_sans_erreur() {
        // « Pokémon » en cp1252, puis une ligne UTF-8, fins de ligne Windows.
        let data: &[u8] = b"Pok\xe9mon species\r\nd\xc3\xa9j\xc3\xa0\r\nfin";
        let mut lines = Lines::new(data);
        assert_eq!(lines.next_line().await.unwrap().as_deref(), Some("Pok\u{FFFD}mon species"));
        assert_eq!(lines.next_line().await.unwrap().as_deref(), Some("déjà"));
        assert_eq!(lines.next_line().await.unwrap().as_deref(), Some("fin"));
        assert_eq!(lines.next_line().await.unwrap(), None);
    }
}
