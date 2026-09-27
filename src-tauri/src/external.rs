//! Ouvrir un lien ou un dossier avec le programme du bureau.
//!
//! Sous Linux, l'AppImage embarque son propre `xdg-open` (1.1.3, celui
//! d'Ubuntu 22.04), placé en tête de `PATH`, et ses bibliothèques
//! (`LD_LIBRARY_PATH`, variables GTK…). Ce `xdg-open` ne connaît pas KDE
//! Plasma 6 (`KDE_SESSION_VERSION=6`) : il ne lance rien et répond « réussi ».
//! Plus aucun lien ni dossier ne s'ouvrait chez Jean (27/09). On appelle donc
//! le `xdg-open` du système, dans l'environnement du bureau : sans rien de ce
//! que l'AppImage y a ajouté.
//!
//! Adresses hors web (`discord://`…) : `gio open`, AppImage ou pas. Sous KDE,
//! `xdg-open` passe par `kde-open`, dont le filtre d'adresses prend
//! `discord:` pour un nom de serveur et envoie `http://discord//-/channels/…`
//! au navigateur, alors que l'association `discord://` → Vesktop était
//! juste (27/09). `gio open` lit les associations sans filtre.

use std::ffi::OsString;

/// Rien à faire ici (hors Linux, ou adresse web ou dossier hors AppImage) :
/// `None`, le plugin opener s'en charge.
pub fn system_open(target: &str) -> Option<std::io::Result<()>> {
    #[cfg(target_os = "linux")]
    {
        let appdir = std::env::var("APPDIR").ok();
        let custom = custom_scheme(target);
        if appdir.is_none() && !custom {
            return None;
        }
        let env = match &appdir {
            Some(a) => desktop_env(std::env::vars_os(), a),
            None => std::env::vars_os().collect(),
        };
        let path = env.iter().find(|(k, _)| k == "PATH").map(|(_, v)| v.clone()).unwrap_or_default();
        let find = |name: &str| std::env::split_paths(&path).map(|d| d.join(name)).find(|p| p.is_file());
        let (program, args): (_, Vec<&str>) = match find("gio") {
            Some(gio) if custom => (gio, vec!["open", target]),
            _ => (find("xdg-open")?, vec![target]),
        };
        let spawned = std::process::Command::new(program)
            .args(args)
            .env_clear()
            .envs(env)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
        Some(spawned.map(|mut child| {
            // Attendu à part : pas de processus zombie, pas d'attente ici.
            std::thread::spawn(move || child.wait());
        }))
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = target;
        None
    }
}

/// Adresse d'une application (`discord://…`), pas du web ni un dossier.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn custom_scheme(target: &str) -> bool {
    match target.split_once("://") {
        Some((scheme, _)) => {
            !scheme.is_empty()
                && scheme.chars().all(|c| c.is_ascii_alphanumeric() || "+-.".contains(c))
                && !["http", "https", "file"].contains(&scheme.to_ascii_lowercase().as_str())
        }
        None => false,
    }
}

/// Environnement à donner aux programmes lancés par le launcher (le jeu) :
/// celui du bureau, sans l'AppImage. Hors AppImage : `None`, rien à changer.
/// Sans ça, le jeu hérite de l'`xdg-open` et des bibliothèques de l'AppImage :
/// les liens cliqués en jeu ne s'ouvrent pas non plus sous Plasma 6.
pub fn desktop_env_for_children() -> Option<Vec<(OsString, OsString)>> {
    if !cfg!(target_os = "linux") {
        return None;
    }
    let appdir = std::env::var("APPDIR").ok()?;
    Some(desktop_env(std::env::vars_os(), &appdir))
}

/// Posées par l'AppImage (AppRun, crochet GTK de linuxdeploy) et sans
/// chemin à trier : retirées d'office.
const APPIMAGE_ONLY: &[&str] = &["APPDIR", "APPIMAGE", "ARGV0", "OWD", "GTK_THEME", "GDK_BACKEND", "GTK_CSD"];

/// L'environnement du launcher, moins l'AppImage : variables à elle
/// retirées, et dans les listes de chemins (`PATH`, `XDG_DATA_DIRS`,
/// `LD_LIBRARY_PATH`…), les entrées qui pointent dans une AppImage montée ou
/// extraite. Une variable qui n'avait que de telles entrées disparaît.
/// (Après un redémarrage du launcher, l'environnement porte DEUX montages :
/// l'ancien et le nouveau.)
fn desktop_env(vars: impl Iterator<Item = (OsString, OsString)>, appdir: &str) -> Vec<(OsString, OsString)> {
    let appdir = appdir.trim_end_matches('/');
    let inside = |p: &str| {
        (!appdir.is_empty() && p.starts_with(appdir)) || p.contains("/.mount_") || p.contains("/appimage_extracted_")
    };
    vars.filter(|(k, _)| !APPIMAGE_ONLY.iter().any(|a| k == a))
        .filter_map(|(k, v)| {
            let Some(s) = v.to_str() else { return Some((k, v)) };
            if !s.contains('/') {
                return Some((k, v));
            }
            let kept: Vec<&str> = s.split(':').filter(|p| !inside(p)).collect();
            if kept.iter().all(|p| p.is_empty()) {
                return None;
            }
            Some((k, OsString::from(kept.join(":"))))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(pairs: &[(&str, &str)]) -> Vec<(OsString, OsString)> {
        pairs.iter().map(|(k, v)| (OsString::from(k), OsString::from(v))).collect()
    }

    #[test]
    fn adresses_d_application() {
        assert!(custom_scheme("discord://-/channels/1/2/3"));
        assert!(!custom_scheme("https://discord.com/channels/1/2/3"));
        assert!(!custom_scheme("http://localhost:1234/"));
        assert!(!custom_scheme("/home/j/.local/share/turicraft/instance/screenshots"));
        assert!(!custom_scheme("C:\\Users\\j\\x"));
    }

    #[test]
    fn environnement_du_bureau_sans_l_appimage() {
        let a = "/tmp/.mount_Turi-CEpaCkE";
        let vars = env(&[
            ("PATH", "/tmp/.mount_Turi-CEpaCkE/usr/bin/:/tmp/.mount_Turi-CipeLEa/usr/bin/:/usr/local/bin:/usr/bin"),
            ("LD_LIBRARY_PATH", "/tmp/.mount_Turi-CEpaCkE/usr/lib/:/tmp/.mount_Turi-CipeLEa/usr/lib/"),
            ("XDG_DATA_DIRS", "/tmp/.mount_Turi-CEpaCkE/usr/share/:/usr/share:/var/lib/flatpak/exports/share"),
            ("GTK_PATH", "/tmp/.mount_Turi-CEpaCkE//usr/lib/gtk-3.0"),
            ("APPDIR", a),
            ("APPIMAGE", "/home/j/Applications/Turi-Craft.AppImage"),
            ("GDK_BACKEND", "x11"),
            ("KDE_SESSION_VERSION", "6"),
            ("HOME", "/home/j"),
            ("DISPLAY", ":0"),
        ]);
        let out: std::collections::HashMap<String, String> = desktop_env(vars.into_iter(), a)
            .into_iter()
            .map(|(k, v)| (k.into_string().unwrap(), v.into_string().unwrap()))
            .collect();
        assert_eq!(out["PATH"], "/usr/local/bin:/usr/bin");
        assert_eq!(out["XDG_DATA_DIRS"], "/usr/share:/var/lib/flatpak/exports/share");
        for gone in ["LD_LIBRARY_PATH", "GTK_PATH", "APPDIR", "APPIMAGE", "GDK_BACKEND"] {
            assert!(!out.contains_key(gone), "{gone} devrait être retirée");
        }
        assert_eq!((out["KDE_SESSION_VERSION"].as_str(), out["HOME"].as_str(), out["DISPLAY"].as_str()), ("6", "/home/j", ":0"));
    }
}
