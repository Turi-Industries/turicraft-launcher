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
    /// L'écran principal : fréquence, VRR.
    #[serde(default)]
    pub display: crate::display::Display,
}

pub fn detect() -> Hardware {
    let mut sys = sysinfo::System::new();
    sys.refresh_memory();
    let ram_gb = sys.total_memory() as f64 / 1024f64.powi(3);
    let cpu_threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
    let (gpu_name, gpu_dedicated, vram_gb) = gpu();
    let shared_memory = cfg!(target_os = "macos") && std::env::consts::ARCH == "aarch64";
    Hardware { ram_gb, cpu_threads, gpu_name, gpu_dedicated, vram_gb, shared_memory, display: crate::display::detect() }
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
            let integrated = (lname.contains("intel") && !lname.contains("arc")) || lname.contains("basic display");
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
