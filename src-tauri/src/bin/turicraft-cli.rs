//! Le launcher sans interface : même code, sortie dans le terminal. Sert aux
//! essais sur une machine sans écran, et au diagnostic chez un joueur.
//!
//! ```text
//! turicraft-cli detect              matériel et préréglage choisi
//! turicraft-cli ping                état du serveur
//! turicraft-cli prepare             Java, Minecraft, NeoForge, pack, préréglage
//! turicraft-cli launch <pseudo>     prepare puis lance le jeu HORS LIGNE
//! turicraft-cli cmdline <pseudo>    écrit la ligne de commande du jeu (cmdline.txt)
//! turicraft-cli login               connexion Microsoft par code, jusqu'au profil
//! turicraft-cli login-web           connexion Microsoft par le navigateur
//! turicraft-cli diag                analyse le dernier crash
//! ```
//!
//! Variables : TURICRAFT_HOME (dossier de données), TURICRAFT_PACK_URL.

use std::time::{Duration, SystemTime};

use anyhow::Result;
use turicraft_lib::{auth, config, diag, hardware, launch, paths::Paths, ping, presets, progress::ConsoleReporter, settings::Settings};

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let paths = Paths::default_location();
    let settings = Settings::load(&paths);
    let r = ConsoleReporter;
    println!("données : {}\npack    : {}", paths.root.display(), turicraft_lib::settings::pack_url());

    match args.first().map(String::as_str) {
        Some("detect") => {
            let hw = hardware::detect();
            println!("{hw:#?}");
            let file = presets::fetch(&turicraft_lib::settings::pack_url()).await?;
            let res = presets::resolve(&file, &hw, &settings);
            println!("préréglage détecté : {}\nrésolu : {res:#?}", file.detect(&hw));
        }
        Some("ping") => println!("{:#?}", ping::status(config::SERVER_HOST, config::SERVER_PORT).await),
        Some("prepare") => {
            launch::prepare(&paths, &settings, &r).await?;
            println!("\nprêt.");
        }
        Some("launch") => {
            let name = args.get(1).cloned().unwrap_or_else(|| "Testeur".into());
            let prepared = launch::prepare(&paths, &settings, &r).await?;
            let session = auth::offline_session(&name);
            let mut s = settings.clone();
            s.join_server = false; // hors ligne : le serveur refuserait
            launch::launch(&paths, &s, &session, &prepared, &r).await?;
        }
        Some("cmdline") => {
            // Ligne de commande du jeu, hors ligne, sans rien lancer. Une
            // valeur par ligne (les chemins peuvent contenir des espaces).
            let name = args.get(1).cloned().unwrap_or_else(|| "Testeur".into());
            let prepared = launch::prepare(&paths, &settings, &r).await?;
            let mut s = settings.clone();
            s.join_server = false;
            let (jvm, game) = launch::command_line(&paths, &s, &auth::offline_session(&name), &prepared)?;
            let out = std::iter::once(prepared.java.display().to_string()).chain(jvm).chain(game);
            std::fs::write(paths.root.join("cmdline.txt"), out.collect::<Vec<_>>().join("\n") + "\n")?;
            println!("écrit : {}", paths.root.join("cmdline.txt").display());
        }
        Some("login") => {
            // Connexion Microsoft complète, jusqu'au profil Minecraft.
            let client_id = turicraft_lib::settings::azure_client_id().ok_or_else(|| anyhow::anyhow!("client ID manquant"))?;
            let code = auth::start_device_code(&client_id).await?;
            println!("\n  Ouvre {} et saisis le code : {}\n", code.verification_uri, code.user_code);
            let session = auth::finish_device_code(&client_id, &code).await?;
            println!("connecté : {} ({})", session.name, session.uuid);
        }
        Some("login-web") => {
            // Connexion par le navigateur, comme le launcher par défaut.
            let client_id = turicraft_lib::settings::azure_client_id().ok_or_else(|| anyhow::anyhow!("client ID manquant"))?;
            let login = auth::start_browser_login(&client_id).await?;
            println!("\n  Ouvre cette adresse et choisis ton compte :\n  {}\n", login.url);
            let session = auth::finish_browser_login(&client_id, login).await?;
            println!("connecté : {} ({})", session.name, session.uuid);
        }
        Some("diag") => {
            let since = SystemTime::now() - Duration::from_secs(7 * 24 * 3600);
            println!("{:#?}", diag::analyze(&paths.instance(), since));
        }
        _ => println!("commandes : detect | ping | prepare | launch <pseudo> | cmdline <pseudo> | login | login-web | diag"),
    }
    Ok(())
}
