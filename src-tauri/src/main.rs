// Pas de fenêtre de console à côté du launcher sous Windows (en release).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    native_decorations();
    turicraft_lib::run();
}

/// Sous Linux, la fenêtre passe par GTK : sous Wayland, GTK 3 dessine
/// toujours lui-même sa barre de titre, à la GNOME, même sous KDE. Hors de
/// GNOME (et de ses dérivés), on passe par X11 et on demande au gestionnaire
/// de fenêtres de la dessiner : KWin met alors la sienne, celle du système.
/// Rien n'est forcé si le joueur a déjà réglé GDK_BACKEND ou GTK_CSD.
fn native_decorations() {
    #[cfg(target_os = "linux")]
    {
        let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default().to_uppercase();
        let gnome_like = ["GNOME", "UNITY", "PANTHEON", "BUDGIE", "CINNAMON", "MATE", "XFCE"]
            .iter()
            .any(|d| desktop.contains(d));
        if !gnome_like && !desktop.is_empty() {
            if std::env::var_os("GDK_BACKEND").is_none() {
                std::env::set_var("GDK_BACKEND", "x11");
            }
            if std::env::var_os("GTK_CSD").is_none() {
                std::env::set_var("GTK_CSD", "0");
            }
        }
    }
}
