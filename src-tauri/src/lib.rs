//! Launcher Turi Craft — cœur (Rust) et commandes appelées par l'interface.

pub mod auth;
pub mod config;
pub mod diag;
pub mod display;
pub mod hardware;
pub mod java;
pub mod launch;
pub mod lines;
pub mod minecraft;
pub mod neoforge;
pub mod net;
pub mod packwiz;
pub mod paths;
pub mod ping;
pub mod presets;
pub mod progress;
pub mod settings;
pub mod skin;
pub mod updates;

use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::async_runtime::JoinHandle;
use tauri::window::{ProgressBarState, ProgressBarStatus};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_opener::OpenerExt;
use tauri_plugin_updater::UpdaterExt;

use paths::Paths;
use progress::{Event, Reporter};
use settings::{Account, Settings};

struct AppState {
    paths: Paths,
    settings: Mutex<Settings>,
    device_code: Mutex<Option<auth::DeviceCode>>,
    /// Le parcours de « Jouer » en cours : l'abandonner arrête la préparation
    /// ou le jeu (les processus sont en kill_on_drop).
    task: Mutex<Option<JoinHandle<()>>>,
    /// Connexion par le navigateur en attente (annulable).
    login: Mutex<Option<tokio::sync::oneshot::Sender<()>>>,
    /// Mise à jour du launcher téléchargée (« Plus tard ») : signature déjà
    /// vérifiée, installée à la fermeture du launcher.
    pending_update: Mutex<Option<(tauri_plugin_updater::Update, Vec<u8>)>>,
}

/// Envoie les événements du cœur à l'interface, et tient la fenêtre au courant :
/// progression dans la barre des tâches, fenêtre réduite ou cachée quand le
/// jeu est au menu, rappelée quand il se ferme.
struct TauriReporter {
    app: AppHandle,
    behavior: String,
}

impl TauriReporter {
    fn window(&self) -> Option<tauri::WebviewWindow> {
        self.app.get_webview_window("main")
    }
    fn taskbar(&self, status: ProgressBarStatus, progress: Option<u64>) {
        if let Some(w) = self.window() {
            let _ = w.set_progress_bar(ProgressBarState { status: Some(status), progress });
        }
    }
}

impl Reporter for TauriReporter {
    fn send(&self, event: Event) {
        match &event {
            Event::Progress { done, total } if *total > 0 => {
                self.taskbar(ProgressBarStatus::Normal, Some(done * 100 / total))
            }
            Event::Milestone { index, count, .. } => {
                self.taskbar(ProgressBarStatus::Normal, Some(((index + 1) * 100 / count) as u64))
            }
            Event::GameReady { .. } => {
                self.taskbar(ProgressBarStatus::None, None);
                if let Some(w) = self.window() {
                    match self.behavior.as_str() {
                        "garder" => {}
                        "fermer" => {
                            let _ = w.hide();
                        }
                        _ => {
                            let _ = w.minimize();
                        }
                    }
                }
            }
            Event::GameExited { code, .. } => {
                self.taskbar(ProgressBarStatus::None, None);
                // « Fermer » : on quitte avec le jeu, sauf s'il a planté —
                // le joueur doit voir le rapport.
                if self.behavior == "fermer" && *code == Some(0) {
                    self.app.exit(0);
                    return;
                }
                if let Some(w) = self.window() {
                    let _ = w.show();
                    let _ = w.unminimize();
                    let _ = w.set_focus();
                }
            }
            _ => {}
        }
        let _ = self.app.emit("launcher", event);
    }
}

type CmdResult<T> = Result<T, String>;
fn err(e: anyhow::Error) -> String {
    format!("{e:#}")
}

#[derive(Serialize)]
struct Overview {
    settings: Settings,
    hardware: hardware::Hardware,
    pack_version: Option<String>,
    /// Mode hors ligne des essais (TURICRAFT_OFFLINE_NAME), sinon absent.
    offline_name: Option<String>,
    data_dir: String,
    disk_free_gb: Option<f64>,
    launcher_version: &'static str,
}

/// Machine mesurée À CÔTÉ : une commande synchrone tourne sur le fil de la
/// fenêtre, et la détection (PowerShell sous Windows, lent juste après
/// l'installation) la figeait au démarrage (25/09).
async fn hardware_off_ui() -> hardware::Hardware {
    tauri::async_runtime::spawn_blocking(hardware::detect).await.unwrap_or_default()
}

#[tauri::command]
async fn overview(state: State<'_, Arc<AppState>>) -> CmdResult<Overview> {
    let hardware = hardware_off_ui().await;
    Ok(Overview {
        settings: state.settings.lock().unwrap().clone(),
        hardware,
        pack_version: packwiz::installed_pack_version(&state.paths),
        offline_name: settings::offline_name(),
        data_dir: state.paths.root.display().to_string(),
        disk_free_gb: hardware::disk_free_gb(&state.paths.root),
        launcher_version: config::LAUNCHER_VERSION,
    })
}

#[tauri::command]
fn save_settings(state: State<'_, Arc<AppState>>, settings: Settings) -> CmdResult<()> {
    let mut s = state.settings.lock().unwrap();
    s.take_user_choices(settings);
    s.save(&state.paths).map_err(err)
}

#[derive(Serialize)]
struct PresetsView {
    file: presets::PresetsFile,
    detected: String,
    resolved: presets::Resolved,
    memory_cap_gb: f64,
    /// Mémoire conseillée pour cette machine (mode Simple).
    memory_auto_gb: f64,
    /// Choix du joueur effacés quand il choisit un préréglage.
    preset_owned: presets::PresetOwned,
    /// Réglages à jour, réglages faits en jeu compris : l'écran repart d'eux.
    settings: Settings,
}

#[tauri::command]
async fn presets_view(state: State<'_, Arc<AppState>>) -> CmdResult<PresetsView> {
    let file = presets::fetch(&settings::pack_url()).await.map_err(err)?;
    let hw = hardware_off_ui().await;
    import_game_changes(&state, &file, &hw);
    let settings = state.settings.lock().unwrap().clone();
    Ok(PresetsView {
        detected: file.detect(&hw),
        resolved: presets::resolve(&file, &hw, &settings),
        memory_cap_gb: file.memory_cap_gb(&hw),
        memory_auto_gb: file.memory_auto_gb(&hw),
        preset_owned: file.preset_owned(),
        settings,
        file,
    })
}

#[tauri::command]
async fn server_status() -> ping::ServerStatus {
    ping::status(config::SERVER_HOST, config::SERVER_PORT).await
}

#[tauri::command]
async fn check_updates(state: State<'_, Arc<AppState>>) -> CmdResult<updates::Updates> {
    Ok(updates::check(&state.paths, &settings::pack_url()).await)
}

/// Nouvelle version du launcher ? (tauri-plugin-updater : latest.json sur le
/// homelab, signature vérifiée avec la clé publique de tauri.conf.json.)
#[derive(Serialize)]
struct LauncherUpdate {
    version: String,
    notes: String,
}

#[tauri::command]
async fn launcher_update_check(app: AppHandle) -> CmdResult<Option<LauncherUpdate>> {
    let update = app.updater().map_err(|e| e.to_string())?.check().await.map_err(|e| e.to_string())?;
    Ok(update.map(|u| LauncherUpdate { version: u.version.clone(), notes: u.body.clone().unwrap_or_default() }))
}

/// Taille de la fenêtre au tout premier lancement : environ un tiers de la
/// surface de l'écran principal, en 16:9 (1478 × 831 en 1440p, 1108 × 624 en
/// 1080p), jamais sous le minimum. Ensuite, tauri-plugin-window-state rend la
/// taille et la position laissées par le joueur. La fenêtre est créée cachée
/// (tauri.conf.json) : on ne la voit pas changer de taille.
fn first_window_size(app: &AppHandle) {
    let Some(w) = app.get_webview_window("main") else { return };
    let saved = app
        .path()
        .app_config_dir()
        .map(|d| d.join(tauri_plugin_window_state::DEFAULT_FILENAME).exists())
        .unwrap_or(false);
    if !saved {
        if let Some(m) = w.current_monitor().ok().flatten().or_else(|| w.primary_monitor().ok().flatten()) {
            let (sw, sh) = (m.size().width as f64, m.size().height as f64);
            let (w_px, h_px) = first_size(sw, sh, m.scale_factor());
            let _ = w.set_size(tauri::PhysicalSize::new(w_px, h_px));
            // Centrée à la main : center() lit la taille d'AVANT (le
            // redimensionnement est asynchrone sous X11) et se décale.
            let origin = m.position();
            let _ = w.set_position(tauri::PhysicalPosition::new(
                origin.x + ((sw - w_px as f64) / 2.0) as i32,
                origin.y + ((sh - h_px as f64) / 2.0) as i32,
            ));
        }
    }
    let _ = w.show();
}

/// Écran (pixels physiques) → fenêtre 16:9 d'un tiers de sa surface, bornée :
/// au moins 820 × 560 (points), au plus 90 % de l'écran.
fn first_size(screen_w: f64, screen_h: f64, scale: f64) -> (u32, u32) {
    let h = (screen_w * screen_h / 3.0 * 9.0 / 16.0).sqrt();
    let w = h * 16.0 / 9.0;
    let w = w.max(820.0 * scale).min(screen_w * 0.9);
    let h = h.max(560.0 * scale).min(screen_h * 0.9);
    (w.round() as u32, h.round() as u32)
}

#[cfg(test)]
mod tests_fenetre {
    use super::first_size;

    #[test]
    fn un_tiers_de_l_ecran_en_16_9() {
        assert_eq!(first_size(2560.0, 1440.0, 1.0), (1478, 831));
        assert_eq!(first_size(1920.0, 1080.0, 1.0), (1109, 624));
        // 4K à 200 % : même taille apparente qu'en 1080p.
        assert_eq!(first_size(3840.0, 2160.0, 2.0), (2217, 1247));
        // Petit écran : jamais sous le minimum de la fenêtre.
        assert_eq!(first_size(1366.0, 768.0, 1.0), (820, 560));
    }
}

/// « Plus tard » : télécharge maintenant (signature vérifiée), installe à la
/// fermeture du launcher — la réouverture suivante est à jour.
#[tauri::command]
async fn launcher_update_later(app: AppHandle, state: State<'_, Arc<AppState>>) -> CmdResult<()> {
    if state.pending_update.lock().unwrap().is_some() {
        return Ok(());
    }
    let update = app
        .updater()
        .map_err(|e| e.to_string())?
        .check()
        .await
        .map_err(|e| e.to_string())?
        .ok_or("aucune mise à jour")?;
    let bytes = update.download(|_, _| {}, || {}).await.map_err(|e| format!("téléchargement impossible : {e}"))?;
    *state.pending_update.lock().unwrap() = Some((update, bytes));
    Ok(())
}

/// Installe la mise à jour mise de côté, s'il y en a une. Sous Windows,
/// l'installeur prend la main et le processus s'arrête ici.
fn install_pending(app: &AppHandle) {
    let Some(state) = app.try_state::<Arc<AppState>>() else { return };
    let Some((update, bytes)) = state.pending_update.lock().unwrap().take() else { return };
    if let Err(e) = update.install(bytes) {
        eprintln!("mise à jour du launcher non installée : {e}");
    }
}

/// Télécharge, vérifie la signature, installe, puis redémarre le launcher.
/// Progression : événement « launcher-update » { done, total }.
#[tauri::command]
async fn launcher_update_install(app: AppHandle, state: State<'_, Arc<AppState>>) -> CmdResult<()> {
    // Déjà téléchargée par « Plus tard » : on l'installe tout de suite.
    let pending = state.pending_update.lock().unwrap().take();
    if let Some((update, bytes)) = pending {
        update.install(bytes).map_err(|e| format!("mise à jour impossible : {e}"))?;
        app.restart();
    }
    let update = app
        .updater()
        .map_err(|e| e.to_string())?
        .check()
        .await
        .map_err(|e| e.to_string())?
        .ok_or("aucune mise à jour")?;
    download_install_restart(&app, update).await
}

/// Réparer le launcher : réinstalle la dernière version publiée, MÊME si
/// c'est celle-ci (fichiers du launcher abîmés, raccourcis perdus, version
/// x64 sur un PC ARM). Même chemin qu'une mise à jour : signature vérifiée,
/// installeur, redémarrage. Les réglages et le jeu ne sont pas touchés.
#[tauri::command]
async fn launcher_reinstall(app: AppHandle) -> CmdResult<()> {
    let update = app
        .updater_builder()
        .version_comparator(|_, _| true)
        .build()
        .map_err(|e| e.to_string())?
        .check()
        .await
        .map_err(|e| format!("serveur du launcher injoignable : {e}"))?
        .ok_or("aucune version publiée")?;
    download_install_restart(&app, update).await
}

/// Progression : événement « launcher-update » { done, total }.
async fn download_install_restart(app: &AppHandle, update: tauri_plugin_updater::Update) -> CmdResult<()> {
    let mut done: u64 = 0;
    let progress = app.clone();
    update
        .download_and_install(
            move |chunk, total| {
                done += chunk as u64;
                let _ = progress.emit("launcher-update", serde_json::json!({ "done": done, "total": total.unwrap_or(0) }));
            },
            || {},
        )
        .await
        .map_err(|e| format!("installation impossible : {e}"))?;
    app.restart();
}

#[tauri::command]
async fn news() -> Vec<updates::NewsItem> {
    updates::news(&settings::pack_url()).await
}

/// Connexion par défaut : la page Microsoft dans le navigateur, retour
/// automatique au launcher. En cas d'échec, l'interface propose le code.
#[tauri::command]
async fn login_browser(app: AppHandle, state: State<'_, Arc<AppState>>) -> CmdResult<Account> {
    let client_id = settings::azure_client_id().ok_or("connexion Microsoft non configurée")?;
    let login = auth::start_browser_login(&client_id).await.map_err(err)?;
    app.opener().open_url(login.url.clone(), None::<&str>).map_err(|e| e.to_string())?;
    let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel();
    // Une nouvelle tentative annule la précédente.
    if let Some(old) = state.login.lock().unwrap().replace(cancel_tx) {
        let _ = old.send(());
    }
    let session = tokio::select! {
        r = auth::finish_browser_login(&client_id, login) => r.map_err(err)?,
        _ = cancel_rx => return Err("annulé".into()),
    };
    state.login.lock().unwrap().take();
    let account = Account { name: session.name, uuid: session.uuid };
    let mut s = state.settings.lock().unwrap();
    s.account = Some(account.clone());
    s.save(&state.paths).map_err(err)?;
    Ok(account)
}

#[tauri::command]
fn login_cancel(state: State<'_, Arc<AppState>>) {
    if let Some(tx) = state.login.lock().unwrap().take() {
        let _ = tx.send(());
    }
}

#[tauri::command]
async fn login_start(state: State<'_, Arc<AppState>>) -> CmdResult<auth::DeviceCode> {
    let client_id = settings::azure_client_id().ok_or("connexion Microsoft non configurée")?;
    let code = auth::start_device_code(&client_id).await.map_err(err)?;
    *state.device_code.lock().unwrap() = Some(code.clone());
    Ok(code)
}

#[tauri::command]
async fn login_finish(state: State<'_, Arc<AppState>>) -> CmdResult<Account> {
    let client_id = settings::azure_client_id().ok_or("connexion Microsoft non configurée")?;
    let code = state.device_code.lock().unwrap().clone().ok_or("aucune connexion en cours")?;
    let session = auth::finish_device_code(&client_id, &code).await.map_err(err)?;
    let account = Account { name: session.name, uuid: session.uuid };
    let mut s = state.settings.lock().unwrap();
    s.account = Some(account.clone());
    s.save(&state.paths).map_err(err)?;
    Ok(account)
}

/// Le skin du compte connecté, en data URL (l'interface découpe la tête).
#[tauri::command]
async fn skin(state: State<'_, Arc<AppState>>) -> CmdResult<String> {
    let uuid = state.settings.lock().unwrap().account.as_ref().map(|a| a.uuid.clone()).ok_or("pas de compte")?;
    skin::skin_data_url(&state.paths, &uuid).await.map_err(err)
}

#[tauri::command]
fn logout(state: State<'_, Arc<AppState>>) -> CmdResult<()> {
    auth::logout();
    let mut s = state.settings.lock().unwrap();
    s.account = None;
    s.save(&state.paths).map_err(err)
}

/// Réglages changés en jeu → réglages du launcher (launch::import_game_changes).
fn import_game_changes(state: &AppState, file: &presets::PresetsFile, hw: &hardware::Hardware) -> Vec<String> {
    let mut s = state.settings.lock().unwrap();
    let changed = launch::import_game_changes(&state.paths, file, hw, &mut s);
    if !changed.is_empty() {
        let _ = s.save(&state.paths);
    }
    changed
}

/// Même chose avant « Jouer » ou « Réparer » : l'écran Qualité n'a peut-être
/// pas été ouvert depuis la dernière partie.
async fn import_before_launch(state: &AppState, r: &dyn Reporter) {
    let Ok(file) = presets::fetch(&settings::pack_url()).await else { return };
    let changed = import_game_changes(state, &file, &hardware::detect());
    if !changed.is_empty() {
        r.log(&format!("réglages faits en jeu repris : {}", changed.join(", ")));
    }
}

/// Réparer, tout de suite : la préparation de « Jouer » (Java, Minecraft,
/// NeoForge, pack, réglages) sans lancer le jeu, chaque fichier relu et
/// comparé à son empreinte. Même file que « Jouer » : jamais les deux à la
/// fois. Rend la main tout de suite, la suite arrive par événements.
#[tauri::command]
fn repair(app: AppHandle, state: State<'_, Arc<AppState>>) -> CmdResult<()> {
    let mut task = state.task.lock().unwrap();
    if task.as_ref().is_some_and(|t| !t.inner().is_finished()) {
        return Err("déjà en cours".into());
    }
    forget_install_state(&state.paths).map_err(|e| e.to_string())?;
    let st = state.inner().clone();
    *task = Some(tauri::async_runtime::spawn(async move {
        let reporter = TauriReporter { app: app.clone(), behavior: "garder".into() };
        net::set_deep_verify(true);
        let result = repair_inner(&st, &reporter).await;
        net::set_deep_verify(false);
        reporter.taskbar(ProgressBarStatus::None, None);
        match result {
            Ok(()) => reporter.send(Event::Repaired),
            Err(e) => {
                reporter.taskbar(ProgressBarStatus::Error, None);
                let _ = app.emit("launcher-error", format!("{e:#}"));
            }
        }
    }));
    Ok(())
}

async fn repair_inner(state: &AppState, r: &dyn Reporter) -> anyhow::Result<()> {
    check_network(&state.paths).await?;
    import_before_launch(state, r).await;
    let settings = state.settings.lock().unwrap().clone();
    let prepared = launch::prepare(&state.paths, &settings, r).await?;
    let mut s = state.settings.lock().unwrap();
    s.applied = Some(prepared.applied.clone());
    (s.once_applied, s.once_files_applied) = prepared.once_applied;
    s.choices_applied = prepared.resolved.choices.clone();
    s.save(&state.paths)
}

/// Vide les empreintes de packwiz.json (packwiz revérifie alors chaque
/// fichier du pack — la manœuvre manuelle de CLAUDE.md) et fait repasser
/// l'installeur NeoForge.
fn forget_install_state(paths: &Paths) -> anyhow::Result<()> {
    forget_pack_hashes(paths)?;
    // Et NeoForge : son installeur repasse (il revérifie ses fichiers).
    if let Ok(dirs) = std::fs::read_dir(paths.versions()) {
        for d in dirs.flatten() {
            let id = d.file_name().to_string_lossy().into_owned();
            let _ = std::fs::remove_file(neoforge::done_marker(paths, &id));
        }
    }
    Ok(())
}

/// packwiz revérifiera chaque fichier du pack au prochain lancement, et
/// remettra ceux qui manquent.
fn forget_pack_hashes(paths: &Paths) -> anyhow::Result<()> {
    let p = paths.instance().join("packwiz.json");
    if let Ok(text) = std::fs::read_to_string(&p) {
        let mut v: serde_json::Value = serde_json::from_str(&text)?;
        if let Some(o) = v.as_object_mut() {
            o.remove("packFileHash");
            o.remove("indexFileHash");
        }
        std::fs::write(&p, serde_json::to_string_pretty(&v)?)?;
    }
    Ok(())
}

/// « Tout remettre à zéro » (Options), sauf le compte : réglages du
/// launcher et réglages du jeu. Au lancement suivant, packwiz remet les
/// fichiers de config du pack, Default Options les touches par défaut, et le
/// launcher son préréglage (Auto) et ses réglages « une fois » (langue,
/// paquets de ressources). Mondes, captures, schémas, cartes et points de
/// passage ne sont pas touchés.
#[tauri::command]
async fn reset_settings(state: State<'_, Arc<AppState>>) -> CmdResult<Settings> {
    {
        let task = state.task.lock().unwrap();
        if task.as_ref().is_some_and(|t| !t.inner().is_finished()) {
            return Err("Ferme d’abord le jeu (ou attends la fin de la préparation).".into());
        }
    }
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || reset_game_settings(&paths))
        .await
        .map_err(|e| e.to_string())?
        .map_err(err)?;
    let mut s = state.settings.lock().unwrap();
    *s = s.reset_keeping_account();
    s.save(&state.paths).map_err(err)?;
    Ok(s.clone())
}

/// Options du jeu (`options.txt` : touches, graphismes, langue) et configs des
/// mods (`config/`), puis packwiz revérifie tout.
fn reset_game_settings(paths: &Paths) -> anyhow::Result<()> {
    let game = paths.instance();
    let absent = |e: &std::io::Error| e.kind() == std::io::ErrorKind::NotFound;
    if let Err(e) = std::fs::remove_file(game.join("options.txt")) {
        if !absent(&e) {
            return Err(e.into());
        }
    }
    if let Err(e) = std::fs::remove_dir_all(game.join("config")) {
        if !absent(&e) {
            return Err(e.into());
        }
    }
    forget_pack_hashes(paths)
}

#[cfg(test)]
mod tests_remise_a_zero {
    use super::{reset_game_settings, Paths};

    #[test]
    fn options_et_configs_effacees_mondes_gardes() {
        let root = std::env::temp_dir().join(format!("turicraft-raz-{}", std::process::id()));
        let game = root.join("instance");
        std::fs::create_dir_all(game.join("config/xaero")).unwrap();
        std::fs::create_dir_all(game.join("saves/Monde")).unwrap();
        std::fs::write(game.join("options.txt"), "key_key.jump:key.keyboard.space\n").unwrap();
        std::fs::write(game.join("config/xaero/minimap.cfg"), "x").unwrap();
        std::fs::write(game.join("saves/Monde/level.dat"), "x").unwrap();
        std::fs::write(game.join("packwiz.json"), r#"{"packFileHash":"a","indexFileHash":"b","cachedFiles":{}}"#).unwrap();

        reset_game_settings(&Paths::new(root.clone())).unwrap();

        assert!(!game.join("options.txt").exists());
        assert!(!game.join("config").exists());
        assert!(game.join("saves/Monde/level.dat").exists());
        let pw = std::fs::read_to_string(game.join("packwiz.json")).unwrap();
        assert!(!pw.contains("packFileHash") && pw.contains("cachedFiles"));
        // Deuxième fois, rien à effacer : pas d'erreur.
        reset_game_settings(&Paths::new(root.clone())).unwrap();
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[tauri::command]
fn open_folder(app: AppHandle, state: State<'_, Arc<AppState>>, which: String) -> CmdResult<()> {
    let game = state.paths.instance();
    let path = match which.as_str() {
        "logs" => game.join("logs"),
        "crash" => game.join("crash-reports"),
        "screenshots" => game.join("screenshots"),
        _ => game,
    };
    std::fs::create_dir_all(&path).ok();
    app.opener().open_path(path.display().to_string(), None::<&str>).map_err(|e| e.to_string())
}

#[tauri::command]
fn open_url(app: AppHandle, url: String) -> CmdResult<()> {
    if !url.starts_with("https://") {
        return Err("adresse refusée".into());
    }
    app.opener().open_url(url, None::<&str>).map_err(|e| e.to_string())
}

/// Tout le parcours de « Jouer ». Rend la main tout de suite : la suite
/// arrive par événements.
#[tauri::command]
fn play(app: AppHandle, state: State<'_, Arc<AppState>>) -> CmdResult<()> {
    let mut task = state.task.lock().unwrap();
    if task.as_ref().is_some_and(|t| !t.inner().is_finished()) {
        return Err("déjà en cours".into());
    }
    let st = state.inner().clone();
    let behavior = st.settings.lock().unwrap().launcher_behavior.clone();
    let screen = game_screen(&app);
    *task = Some(tauri::async_runtime::spawn(async move {
        let reporter = TauriReporter { app: app.clone(), behavior };
        if let Err(e) = play_inner(&st, &reporter, screen).await {
            reporter.taskbar(ProgressBarStatus::Error, None);
            let _ = app.emit("launcher-error", format!("{e:#}"));
        }
    }));
    Ok(())
}

/// Annule la préparation, ou arrête le jeu s'il tourne.
#[tauri::command]
fn stop(app: AppHandle, state: State<'_, Arc<AppState>>) {
    if let Some(t) = state.task.lock().unwrap().take() {
        t.abort();
    }
    // Réparation interrompue : la tâche n'est pas allée jusqu'à le remettre.
    net::set_deep_verify(false);
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.set_progress_bar(ProgressBarState { status: Some(ProgressBarStatus::None), progress: None });
    }
    let _ = app.emit("launcher-stopped", ());
}

/// L'écran principal (celui où Minecraft ouvre sa fenêtre), dans les
/// coordonnées d'écran du jeu : des pixels sous Windows et Linux (GLFW y
/// compte en pixels réels), des points sous macOS.
fn game_screen(app: &AppHandle) -> Option<(u32, u32)> {
    let m = app.primary_monitor().ok().flatten()?;
    let (w, h) = (m.size().width as f64, m.size().height as f64);
    let scale = if cfg!(target_os = "macos") { m.scale_factor() } else { 1.0 };
    Some(((w / scale).round() as u32, (h / scale).round() as u32))
}

async fn play_inner(state: &AppState, r: &dyn Reporter, screen: Option<(u32, u32)>) -> anyhow::Result<()> {
    check_network(&state.paths).await?;
    import_before_launch(state, r).await;
    let settings = state.settings.lock().unwrap().clone();
    // Compte d'abord : inutile de tout préparer pour une session expirée.
    r.stage("account", "Compte");
    let session = match (settings::offline_name(), settings::azure_client_id()) {
        (Some(name), _) => auth::offline_session(&name),
        (None, Some(client_id)) => auth::refresh(&client_id).await?,
        (None, None) => anyhow::bail!("connexion Microsoft non configurée"),
    };
    // Première installation : ~3 Go. Mieux vaut le dire avant qu'après.
    if packwiz::installed_pack_version(&state.paths).is_none() {
        if let Some(free) = hardware::disk_free_gb(&state.paths.root) {
            if free < 4.0 {
                anyhow::bail!("il faut environ 4 Go d'espace libre pour installer le jeu ({free:.1} Go disponibles)");
            }
        }
    }
    let mut prepared = launch::prepare(&state.paths, &settings, r).await?;
    {
        let mut s = state.settings.lock().unwrap();
        s.applied = Some(prepared.applied.clone());
        (s.once_applied, s.once_files_applied) = prepared.once_applied;
        s.choices_applied = prepared.resolved.choices.clone();
        s.save(&state.paths)?;
    }
    // Fenêtre de chargement : un tiers de l'écran, ou la taille laissée par
    // le joueur (launch::game_window_size).
    prepared.window_size = screen.map(|sc| launch::game_window_size(&state.paths.instance(), sc));
    let outcome = launch::launch(&state.paths, &settings, &session, &prepared, r).await?;
    if !outcome.milestones_ms.is_empty() {
        let mut s = state.settings.lock().unwrap();
        s.last_milestones_ms = outcome.milestones_ms;
        s.save(&state.paths)?;
    }
    Ok(())
}

/// Tout ce qui suit passe par le réseau (compte, pack.toml, catalogues,
/// synchro) : sans lui, un message clair plutôt qu'une erreur de reqwest.
async fn check_network(paths: &Paths) -> anyhow::Result<()> {
    let pack_url = settings::pack_url();
    let installed = packwiz::installed_pack_version(paths).is_some();
    match net::reachability(&pack_url).await {
        net::Reach::Ok => {}
        net::Reach::Offline if !installed => anyhow::bail!(
            "Pas de connexion Internet.\nElle est nécessaire pour installer le jeu (environ 2 Go à télécharger). \
             Vérifie ta connexion, puis relance."
        ),
        net::Reach::Offline => anyhow::bail!(
            "Pas de connexion Internet.\nElle est nécessaire pour vérifier ton compte et les mises à jour du pack \
             avant de jouer. Vérifie ta connexion, puis relance."
        ),
        net::Reach::PackDown => {
            let host = reqwest::Url::parse(&pack_url).ok().and_then(|u| u.host_str().map(String::from));
            anyhow::bail!(
                "Le serveur du pack ({}) ne répond pas, alors qu'Internet fonctionne.\n\
                 Réessaie dans quelques minutes ; si ça dure, préviens un admin.",
                host.as_deref().unwrap_or(&pack_url)
            )
        }
    }
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let paths = Paths::default_location();
    let settings = Settings::load(&paths);
    let state = Arc::new(AppState {
        paths,
        settings: Mutex::new(settings),
        device_code: Mutex::new(None),
        task: Mutex::new(None),
        login: Mutex::new(None),
        pending_update: Mutex::new(None),
    });
    tauri::Builder::default()
        // Un second lancement du launcher ramène le premier au lieu d'en
        // ouvrir un autre (deux synchronisations en même temps = conflit).
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.unminimize();
                let _ = w.set_focus();
            }
        }))
        // Taille, position, plein écran… mais pas la visibilité : quitté fenêtre
        // cachée (modes Réduire / Fermer), le launcher se rouvrirait invisible.
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_state_flags(tauri_plugin_window_state::StateFlags::all() - tauri_plugin_window_state::StateFlags::VISIBLE)
                .build(),
        )
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .setup(move |app| {
            // Machine mesurée tout de suite, à côté : l'écran Qualité l'a
            // ensuite sans attendre PowerShell.
            std::thread::spawn(|| {
                hardware::detect();
            });
            app.manage(state);
            first_window_size(app.handle());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            overview,
            save_settings,
            presets_view,
            server_status,
            check_updates,
            launcher_update_check,
            launcher_update_install,
            launcher_reinstall,
            launcher_update_later,
            news,
            login_browser,
            login_cancel,
            login_start,
            login_finish,
            logout,
            skin,
            repair,
            reset_settings,
            open_folder,
            open_url,
            play,
            stop
        ])
        .build(tauri::generate_context!())
        .expect("erreur au démarrage du launcher")
        .run(|app, event| {
            // Mise à jour « Plus tard » : posée en quittant.
            if let tauri::RunEvent::Exit = event {
                install_pending(app);
            }
        });
}
