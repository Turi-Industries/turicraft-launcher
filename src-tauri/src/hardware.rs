//! Ce qui choisit le préréglage : RAM totale, fils du processeur, carte
//! graphique (dédiée ou intégrée, mémoire vidéo). Au mieux de ce que chaque
//! système expose sans droits administrateur ; en cas de doute, on suppose
//! une carte intégrée — un préréglage trop bas se corrige d'un clic, un
//! préréglage trop haut fait planter.

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Hardware {
    pub ram_gb: f64,
    pub cpu_threads: usize,
    pub gpu_name: String,
    pub gpu_dedicated: bool,
    pub vram_gb: f64,
    /// Puce Apple : processeur et carte graphique partagent la même mémoire.
    #[serde(default)]
    pub shared_memory: bool,
    /// PC Windows à processeur ARM (Snapdragon X) : carte Adreno intégrée,
    /// pilote OpenGL moins rodé — préréglages revus à la baisse (presets.toml).
    #[serde(default)]
    pub windows_arm: bool,
    /// L'écran principal : fréquence, VRR.
    #[serde(default)]
    pub display: crate::display::Display,
}

/// Mesurée une fois par lancement du launcher : sous Windows, la carte et
/// l'écran passent par PowerShell (une à deux secondes). Refaite à chaque
/// clic de l'écran Qualité, elle faisait arriver les réponses dans le
/// désordre — un préréglage choisi s'affichait avec les valeurs du précédent.
pub fn detect() -> Hardware {
    static CACHE: std::sync::OnceLock<Hardware> = std::sync::OnceLock::new();
    CACHE.get_or_init(detect_now).clone()
}

fn detect_now() -> Hardware {
    let mut sys = sysinfo::System::new();
    sys.refresh_memory();
    let ram_gb = sys.total_memory() as f64 / 1024f64.powi(3);
    let cpu_threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
    // Écran et carte graphique en même temps : deux PowerShell sous Windows.
    let display = std::thread::spawn(crate::display::detect);
    let (gpu_name, gpu_dedicated, vram_gb) = gpu();
    let windows_arm = cfg!(windows) && native_arm64();
    // Snapdragon comme puce Apple : la carte graphique prend sur la RAM.
    let shared_memory = (cfg!(target_os = "macos") && std::env::consts::ARCH == "aarch64") || windows_arm;
    let display = display.join().unwrap_or_default();
    Hardware { ram_gb, cpu_threads, gpu_name, gpu_dedicated, vram_gb, shared_memory, windows_arm, display }
}

/// Le processeur est-il ARM64, même si le launcher est la version x64 qui
/// tourne en émulation (Prism) ? `consts::ARCH` dit pour quoi le launcher a
/// été compilé, pas sur quoi il tourne : il faut demander à Windows.
#[cfg(windows)]
pub fn native_arm64() -> bool {
    #[link(name = "kernel32")]
    extern "system" {
        fn GetCurrentProcess() -> isize;
        fn IsWow64Process2(process: isize, process_machine: *mut u16, native_machine: *mut u16) -> i32;
    }
    const IMAGE_FILE_MACHINE_ARM64: u16 = 0xAA64;
    let (mut process, mut native) = (0u16, 0u16);
    // SAFETY : pseudo-handle du processus courant, deux u16 à remplir.
    let ok = unsafe { IsWow64Process2(GetCurrentProcess(), &mut process, &mut native) } != 0;
    if ok { native == IMAGE_FILE_MACHINE_ARM64 } else { std::env::consts::ARCH == "aarch64" }
}

#[cfg(not(windows))]
pub fn native_arm64() -> bool {
    std::env::consts::ARCH == "aarch64"
}

fn run(cmd: &str, args: &[&str]) -> Option<String> {
    let mut c = std::process::Command::new(cmd);
    c.args(args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let out = c.output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).to_string())
}

/// Carte NVIDIA : nvidia-smi donne nom et mémoire, quel que soit le système.
fn nvidia() -> Option<(String, bool, f64)> {
    let out = run("nvidia-smi", &["--query-gpu=name,memory.total", "--format=csv,noheader,nounits"])?;
    let line = out.lines().next()?;
    let (name, mib) = line.rsplit_once(',')?;
    Some((name.trim().to_string(), true, mib.trim().parse::<f64>().ok()? / 1024.0))
}

#[cfg(target_os = "linux")]
fn gpu() -> (String, bool, f64) {
    if let Some(g) = nvidia() {
        return g;
    }
    // AMD et Intel : /sys/class/drm/cardN/device. AMD expose sa VRAM ; une
    // puce AMD intégrée (APU) n'en réserve que quelques centaines de Mo.
    let mut best = ("carte graphique inconnue".to_string(), false, 0.0);
    if let Ok(entries) = std::fs::read_dir("/sys/class/drm") {
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if !name.starts_with("card") || name.contains('-') {
                continue;
            }
            let dev = e.path().join("device");
            let vendor = std::fs::read_to_string(dev.join("vendor")).unwrap_or_default();
            let vram = std::fs::read_to_string(dev.join("mem_info_vram_total"))
                .ok()
                .and_then(|s| s.trim().parse::<f64>().ok())
                .map(|b| b / 1024f64.powi(3))
                .unwrap_or(0.0);
            let (label, dedicated) = match vendor.trim() {
                "0x1002" => ("AMD", vram >= 2.0),
                "0x8086" => ("Intel", false),
                "0x10de" => ("NVIDIA", true),
                _ => continue,
            };
            if dedicated && !best.1 || vram > best.2 || best.0.starts_with("carte") {
                best = (label.to_string(), dedicated, vram);
            }
        }
    }
    best
}

#[cfg(windows)]
fn gpu() -> (String, bool, f64) {
    if let Some(g) = nvidia() {
        return g;
    }
    // Win32_VideoController.AdapterRAM plafonne à 4 Go (entier 32 bits) : la
    // vraie valeur est dans le registre, qwMemorySize.
    let script = r#"Get-ItemProperty 'HKLM:\SYSTEM\ControlSet001\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}\0*' -ErrorAction SilentlyContinue | ForEach-Object { "$($_.DriverDesc)|$($_.'HardwareInformation.qwMemorySize')" }"#;
    let mut best = ("carte graphique inconnue".to_string(), false, 0.0);
    if let Some(out) = run("powershell", &["-NoProfile", "-NonInteractive", "-Command", script]) {
        for line in out.lines() {
            let Some((name, mem)) = line.split_once('|') else { continue };
            let vram = mem.trim().parse::<f64>().unwrap_or(0.0) / 1024f64.powi(3);
            let lname = name.to_lowercase();
            // Adreno (Snapdragon X) : intégrée, mais le registre lui prête
            // parfois plusieurs Go — elle passait pour une carte dédiée.
            let integrated = (lname.contains("intel") && !lname.contains("arc"))
                || lname.contains("qualcomm")
                || lname.contains("adreno")
                || lname.contains("basic display");
            let dedicated = !integrated && vram >= 2.0;
            if dedicated && !best.1 || vram > best.2 {
                best = (name.trim().to_string(), dedicated, vram);
            }
        }
    }
    best
}

#[cfg(target_os = "macos")]
fn gpu() -> (String, bool, f64) {
    // Puce Apple : mémoire partagée, comptée comme « intégrée » (presets.toml).
    if std::env::consts::ARCH == "aarch64" {
        return ("Apple Silicon".into(), false, 0.0);
    }
    let Some(out) = run("system_profiler", &["SPDisplaysDataType", "-json"]) else {
        return ("carte graphique inconnue".into(), false, 0.0);
    };
    let v: serde_json::Value = serde_json::from_str(&out).unwrap_or_default();
    let mut best = ("carte graphique inconnue".to_string(), false, 0.0);
    for d in v["SPDisplaysDataType"].as_array().into_iter().flatten() {
        let name = d["sppci_model"].as_str().unwrap_or("?").to_string();
        // « spdisplays_vram » : dédiée ; « spdisplays_vram_shared » : intégrée.
        if let Some(s) = d["spdisplays_vram"].as_str() {
            let gb = s.split_whitespace().next().and_then(|n| n.parse::<f64>().ok()).unwrap_or(0.0);
            let gb = if s.contains("MB") { gb / 1024.0 } else { gb };
            if gb > best.2 {
                best = (name, true, gb);
            }
        } else if best.0.starts_with("carte") {
            best = (name, false, 0.0);
        }
    }
    best
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
fn gpu() -> (String, bool, f64) {
    ("carte graphique inconnue".into(), false, 0.0)
}

/// Intel hybride (12e génération et plus, Linux) : les cœurs rapides (P).
/// `None` sur tout autre processeur, ou si les cœurs lents (E) sont coupés.
/// Le noyau y promène le fil de rendu du jeu sur les cœurs lents (3,8 GHz
/// contre 4,9 sur un i7-12700K) : un cœur saturé qui ralentit d'un coup.
#[cfg(target_os = "linux")]
pub fn performance_cores() -> Option<Vec<usize>> {
    let read = |f: &str| std::fs::read_to_string(format!("/sys/devices/{f}/cpus")).ok().map(|s| parse_cpu_list(&s));
    let (p, e) = (read("cpu_core")?, read("cpu_atom")?);
    (!p.is_empty() && !e.is_empty()).then_some(p)
}

/// Pose l'affinité avant `exec` : chaque fil de Java en hérite, fil de
/// rendu compris (posée après coup, seul le fil principal la prendrait).
/// Un échec est sans conséquence : le jeu part sur tous les cœurs.
#[cfg(target_os = "linux")]
pub fn pin_to_cores(cmd: &mut tokio::process::Command, cores: Vec<usize>) {
    // SAFETY : entre fork et exec, seulement des appels sans allocation.
    unsafe {
        cmd.pre_exec(move || {
            let mut set: libc::cpu_set_t = std::mem::zeroed();
            for &c in cores.iter().filter(|&&c| c < libc::CPU_SETSIZE as usize) {
                libc::CPU_SET(c, &mut set);
            }
            libc::sched_setaffinity(0, std::mem::size_of::<libc::cpu_set_t>(), &set);
            Ok(())
        });
    }
}

/// Liste de processeurs du noyau : « 0-15 », « 0-7,16,18-19 ».
pub fn parse_cpu_list(s: &str) -> Vec<usize> {
    s.trim()
        .split(',')
        .filter_map(|part| match part.split_once('-') {
            Some((a, b)) => Some(a.trim().parse().ok()?..=b.trim().parse().ok()?),
            None => part.trim().parse().ok().map(|n| n..=n),
        })
        .flatten()
        .collect()
}

/// Espace libre (Go) sur le disque qui porte `path` : celui dont le point de
/// montage est le plus long préfixe du chemin.
pub fn disk_free_gb(path: &std::path::Path) -> Option<f64> {
    let disks = sysinfo::Disks::new_with_refreshed_list();
    let target = path.ancestors().find(|p| p.exists())?.canonicalize().ok()?;
    disks
        .list()
        .iter()
        .filter(|d| target.starts_with(d.mount_point()))
        .max_by_key(|d| d.mount_point().as_os_str().len())
        .map(|d| d.available_space() as f64 / 1024f64.powi(3))
}

#[cfg(test)]
mod tests {
    use super::parse_cpu_list;

    #[test]
    fn liste_de_processeurs() {
        assert_eq!(parse_cpu_list("0-15\n"), (0..=15).collect::<Vec<_>>());
        assert_eq!(parse_cpu_list("0-3,8,10-11"), vec![0, 1, 2, 3, 8, 10, 11]);
        assert!(parse_cpu_list("\n").is_empty());
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn affinite_heritee_par_le_processus_lance() {
        let mut cmd = tokio::process::Command::new("grep");
        cmd.args(["Cpus_allowed_list", "/proc/self/status"]);
        super::pin_to_cores(&mut cmd, vec![0]);
        let out = String::from_utf8(cmd.output().await.unwrap().stdout).unwrap();
        assert_eq!(out.split_whitespace().next_back(), Some("0"));
    }
}
