//! Constantes du pack. Ce qui change d'une version du pack à l'autre (versions
//! de Minecraft et NeoForge) est relu dans `pack.toml` au lancement ; ce qui
//! est ici ne bouge qu'avec le launcher.

/// URL du pack. Pas réglable dans le launcher : `TURICRAFT_PACK_URL` pour les essais.
pub const DEFAULT_PACK_URL: &str = "https://pack.turi-industries.eu/pack.toml";

/// Serveur rejoint par « Jouer » et interrogé pour le nombre de joueurs.
pub const SERVER_HOST: &str = "play.turi-industries.eu";
pub const SERVER_PORT: u16 = 25565;

/// Runtime Java de Mojang pour Minecraft 1.21.x (Java 21).
pub const JAVA_COMPONENT: &str = "java-runtime-delta";

/// Client ID de l'App Registration Azure de Turi Industries (approuvée par
/// Mojang). Ce n'est pas un secret : identifiant public d'une application
/// cliente. `TURICRAFT_AZURE_CLIENT_ID` à la compilation
/// le remplace — pour un autre pack, il faut sa propre App Registration.
pub const AZURE_CLIENT_ID: Option<&str> = match option_env!("TURICRAFT_AZURE_CLIENT_ID") {
    Some(id) => Some(id),
    None => Some("165d4854-4593-4834-ac11-c2389c3ae22c"),
};

pub const LAUNCHER_NAME: &str = "turicraft-launcher";
pub const LAUNCHER_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const USER_AGENT: &str = concat!("TuriCraftLauncher/", env!("CARGO_PKG_VERSION"));
