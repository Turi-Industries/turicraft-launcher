//! Liens Discord des nouveautés (« Lire sur Discord ») : ouverts dans
//! l'application Discord quand une application prend les adresses
//! `discord://` (Discord, PTB, Canary, Vesktop…), sinon dans le navigateur.

use std::path::PathBuf;

/// `https://discord.com/channels/G/C/M` → `discord://-/channels/G/C/M`, que
/// l'application ouvre directement sur le message. Autre adresse : `None`.
pub fn app_link(url: &str) -> Option<String> {
    let rest = ["https://discord.com/", "https://discordapp.com/", "https://ptb.discord.com/", "https://canary.discord.com/"]
        .iter()
        .find_map(|p| url.strip_prefix(p))?;
    rest.starts_with("channels/").then(|| format!("discord://-/{rest}"))
}

/// Une application prend-elle les adresses `discord://` ? Relu à chaque
/// clic : le joueur peut l'installer launcher ouvert. Lent sous Windows
/// (`reg`) : à appeler hors du fil de la fenêtre.
pub fn app_installed() -> bool {
    #[cfg(target_os = "linux")]
    {
        desktop_handles_discord(&data_dirs())
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // Clé posée par l'installeur de Discord (HKCU) ou pour la machine :
        // HKCR réunit les deux.
        std::process::Command::new("reg")
            .args(["query", r"HKCR\discord", "/v", "URL Protocol"])
            .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
            .output()
            .is_ok_and(|o| o.status.success())
    }
    #[cfg(target_os = "macos")]
    {
        let home = dirs::home_dir().unwrap_or_default();
        ["Discord", "Discord PTB", "Discord Canary", "Vesktop"].iter().any(|n| {
            let app = format!("{n}.app");
            std::path::Path::new("/Applications").join(&app).exists() || home.join("Applications").join(&app).exists()
        })
    }
    #[cfg(not(any(target_os = "linux", windows, target_os = "macos")))]
    {
        false
    }
}

/// Dossiers où Linux cherche les applications (`<dossier>/applications`) :
/// variables XDG, plus Flatpak et Snap, absents de XDG_DATA_DIRS quand le
/// launcher tourne en AppImage.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn data_dirs() -> Vec<PathBuf> {
    let home = dirs::home_dir().unwrap_or_default();
    let mut dirs = vec![std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).unwrap_or_else(|| home.join(".local/share"))];
    let system = std::env::var("XDG_DATA_DIRS").unwrap_or_default();
    dirs.extend(system.split(':').filter(|d| !d.is_empty()).map(PathBuf::from));
    dirs.extend(
        ["/usr/local/share", "/usr/share", "/var/lib/flatpak/exports/share", "/var/lib/snapd/desktop"].map(PathBuf::from),
    );
    dirs.push(home.join(".local/share/flatpak/exports/share"));
    dirs
}

/// Un `.desktop` déclare-t-il `x-scheme-handler/discord` dans `MimeType=` ?
/// (Et pas `discord-<id>`, que Discord pose pour lancer des jeux.)
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn desktop_handles_discord(dirs: &[PathBuf]) -> bool {
    dirs.iter().filter_map(|d| std::fs::read_dir(d.join("applications")).ok()).flatten().flatten().any(|e| {
        let path = e.path();
        path.extension().is_some_and(|x| x == "desktop")
            && std::fs::read_to_string(&path).is_ok_and(|t| {
                t.lines()
                    .filter_map(|l| l.strip_prefix("MimeType="))
                    .any(|m| m.split(';').any(|s| s.trim() == "x-scheme-handler/discord"))
            })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lien_vers_l_application() {
        assert_eq!(app_link("https://discord.com/channels/1/2/3").as_deref(), Some("discord://-/channels/1/2/3"));
        assert_eq!(app_link("https://discordapp.com/channels/1/2").as_deref(), Some("discord://-/channels/1/2"));
        assert_eq!(app_link("https://discord.com/invite/abc"), None);
        assert_eq!(app_link("https://turi-industries.eu/channels/1"), None);
    }

    #[test]
    fn application_qui_prend_les_liens_discord() {
        let dir = std::env::temp_dir().join(format!("turicraft-discord-{}", std::process::id()));
        let apps = dir.join("applications");
        std::fs::create_dir_all(&apps).unwrap();
        // Raccourci de jeu posé par Discord : ne compte pas.
        std::fs::write(apps.join("discord-5698.desktop"), "[Desktop Entry]\nMimeType=x-scheme-handler/discord-5698;\n").unwrap();
        assert!(!desktop_handles_discord(std::slice::from_ref(&dir)));
        std::fs::write(apps.join("vesktop.desktop"), "[Desktop Entry]\nName=Vesktop\nMimeType=x-scheme-handler/discord\n").unwrap();
        assert!(desktop_handles_discord(&[dir.clone()]));
        std::fs::remove_dir_all(dir).ok();
    }
}
