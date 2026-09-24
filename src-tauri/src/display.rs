//! L'écran PRINCIPAL : sa fréquence, et s'il fait du VRR (FreeSync, G-Sync,
//! Adaptive-Sync). Sert à limiter les images/s à ce que l'écran affiche, et à
//! couper la synchro verticale quand le VRR la rend inutile.
//!
//! Attention au second écran : sur un PC de test, l'écran principal est à
//! 144 Hz, le second à 165 Hz — prendre le plus rapide serait faux.
//!
//! « Compatible » vient de l'EDID de l'écran (ce qu'il annonce). « Actif »
//! ne se sait que là où le système le dit : KDE (politique VRR), et sous
//! Windows on suppose le pilote réglé par défaut (AMD FreeSync et G-Sync
//! Compatible s'activent seuls). macOS : jamais — sa synchronisation est
//! imposée en fenêtre, ProMotion est géré par le système.

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Display {
    /// Fréquence de l'écran principal, en Hz.
    pub refresh_hz: Option<u32>,
    /// Hauteur de l'écran principal, en pixels réels (taille de l'interface).
    pub height_px: Option<u32>,
    /// L'écran annonce FreeSync / G-Sync / Adaptive-Sync dans son EDID.
    pub vrr_capable: bool,
    /// Le système a le VRR activé pour cet écran (quand on peut le savoir).
    pub vrr_active: bool,
    /// D'où viennent ces informations (affiché au survol de « auto »).
    pub source: String,
}

// ─── EDID ───────────────────────────────────────────────────────────────────

/// OUI des blocs « vendor specific » d'une extension CTA-861, en octets.
const OUI_AMD_FREESYNC: [u8; 3] = [0x1A, 0x00, 0x00];
const OUI_NVIDIA_GSYNC: [u8; 3] = [0x4B, 0x04, 0x00];

/// L'écran annonce-t-il le VRR ? Blocs FreeSync (AMD) ou G-Sync (NVIDIA)
/// d'une extension CTA-861, ou bloc Adaptive-Sync d'une extension DisplayID.
pub fn edid_vrr_capable(edid: &[u8]) -> bool {
    edid.chunks(128).skip(1).any(|ext| match ext.first() {
        // CTA-861 : blocs de données de l'octet 4 à l'octet `ext[2]`
        Some(0x02) if ext.len() >= 4 => {
            let end = (ext[2] as usize).min(ext.len());
            let mut i = 4;
            while i < end {
                let (tag, len) = (ext[i] >> 5, (ext[i] & 0x1F) as usize);
                if tag == 3 && len >= 3 && i + 4 <= ext.len() {
                    let oui = [ext[i + 1], ext[i + 2], ext[i + 3]];
                    if oui == OUI_AMD_FREESYNC || oui == OUI_NVIDIA_GSYNC {
                        return true;
                    }
                }
                if len == 0 && tag == 0 {
                    break;
                }
                i += 1 + len;
            }
            false
        }
        // DisplayID : blocs (étiquette, révision, longueur) à partir de
        // l'octet 5 ; « Adaptive-Sync » a l'étiquette 0x2B.
        Some(0x70) if ext.len() >= 5 => {
            let end = (5 + ext[2] as usize).min(ext.len());
            let mut i = 5;
            while i + 3 <= end {
                if ext[i] == 0x2B {
                    return true;
                }
                if ext[i] == 0 && ext[i + 2] == 0 {
                    break;
                }
                i += 3 + ext[i + 2] as usize;
            }
            false
        }
        _ => false,
    })
}

// ─── Par système ────────────────────────────────────────────────────────────

pub fn detect() -> Display {
    platform().unwrap_or_default()
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

#[cfg(target_os = "linux")]
fn platform() -> Option<Display> {
    // EDID des écrans branchés, par nom de sortie (« DP-2 »…).
    let edid_of = |output: &str| -> Option<Vec<u8>> {
        std::fs::read_dir("/sys/class/drm").ok()?.flatten().find_map(|e| {
            let n = e.file_name().to_string_lossy().to_string();
            (n.split_once('-').map(|(_, o)| o) == Some(output))
                .then(|| std::fs::read(e.path().join("edid")).ok())
                .flatten()
                .filter(|b| b.len() >= 128)
        })
    };

    // KDE : l'écran de priorité 1 est le principal ; vrrPolicy 0 = jamais,
    // 1 = toujours, 2 = automatique (plein écran).
    if let Some(json) = run("kscreen-doctor", &["-j"]) {
        let v: serde_json::Value = serde_json::from_str(&json).ok()?;
        let outputs = v["outputs"].as_array()?;
        let primary = outputs
            .iter()
            .filter(|o| o["enabled"].as_bool() == Some(true))
            .min_by_key(|o| o["priority"].as_u64().unwrap_or(u64::MAX))?;
        let current = primary["currentModeId"].as_str().unwrap_or_default();
        let mode = primary["modes"].as_array()?.iter().find(|m| m["id"].as_str() == Some(current));
        let refresh = mode.and_then(|m| m["refreshRate"].as_f64()).map(|r| r.round() as u32);
        let height = mode.and_then(|m| m["size"]["height"].as_u64()).map(|h| h as u32);
        let name = primary["name"].as_str().unwrap_or_default();
        let capable = edid_of(name).is_some_and(|e| edid_vrr_capable(&e));
        let policy = primary["vrrPolicy"].as_u64().unwrap_or(0);
        return Some(Display {
            refresh_hz: refresh,
            height_px: height,
            vrr_capable: capable,
            vrr_active: capable && policy != 0,
            source: format!("KDE, écran {name}"),
        });
    }

    // Ailleurs : xrandr, écran marqué « primary », mode courant « * ». Le VRR
    // peut y être compatible, mais on ne sait pas s'il est activé.
    let out = run("xrandr", &["--current"])?;
    let mut primary: Option<String> = None;
    let mut refresh = None;
    let mut height = None;
    let mut in_primary = false;
    for line in out.lines() {
        if !line.starts_with(' ') {
            in_primary = line.contains(" connected primary");
            if in_primary {
                primary = line.split_whitespace().next().map(String::from);
            }
        } else if in_primary && refresh.is_none() && line.contains('*') {
            height = line
                .split_whitespace()
                .next()
                .and_then(|m| m.split_once('x'))
                .and_then(|(_, h)| h.trim_end_matches(|c: char| !c.is_ascii_digit()).parse().ok());
            refresh = line
                .split_whitespace()
                .find(|t| t.contains('*'))
                .and_then(|t| t.trim_end_matches(['*', '+']).parse::<f64>().ok())
                .map(|r| r.round() as u32);
        }
    }
    let name = primary?;
    Some(Display {
        refresh_hz: refresh,
        height_px: height,
        vrr_capable: edid_of(&name).is_some_and(|e| edid_vrr_capable(&e)),
        vrr_active: false,
        source: format!("xrandr, écran {name}"),
    })
}

#[cfg(windows)]
fn platform() -> Option<Display> {
    // Fréquence : celle de l'écran principal, vue par la carte graphique.
    // EDID : ceux des écrans actifs (WmiMonitorID → registre). Faute de
    // savoir lequel est le principal, « compatible » si TOUS le sont.
    let script = r#"
$vc = Get-CimInstance Win32_VideoController | Where-Object CurrentRefreshRate | Select-Object -First 1
$hz = "$($vc.CurrentRefreshRate);$($vc.CurrentVerticalResolution)"
$ids = Get-CimInstance -Namespace root\wmi -ClassName WmiMonitorID -ErrorAction SilentlyContinue | ForEach-Object { $_.InstanceName -replace '_0$','' }
$edids = foreach ($id in $ids) {
  $p = "HKLM:\SYSTEM\CurrentControlSet\Enum\$id\Device Parameters"
  $e = (Get-ItemProperty -Path $p -Name EDID -ErrorAction SilentlyContinue).EDID
  if ($e) { [Convert]::ToBase64String($e) }
}
"$hz|" + ($edids -join ',')
"#;
    let out = run("powershell", &["-NoProfile", "-NonInteractive", "-Command", script])?;
    let (hz_h, edids) = out.trim().split_once('|')?;
    let (hz, h) = hz_h.split_once(';').unwrap_or((hz_h, ""));
    use base64::Engine;
    let edids: Vec<Vec<u8>> = edids
        .split(',')
        .filter(|s| !s.is_empty())
        .filter_map(|s| base64::engine::general_purpose::STANDARD.decode(s).ok())
        .collect();
    let capable = !edids.is_empty() && edids.iter().all(|e| edid_vrr_capable(e));
    Some(Display {
        refresh_hz: hz.trim().parse().ok(),
        height_px: h.trim().parse().ok(),
        vrr_capable: capable,
        // AMD FreeSync et G-Sync Compatible s'activent seuls dans les pilotes.
        vrr_active: capable,
        source: "Windows".into(),
    })
}

#[cfg(target_os = "macos")]
fn platform() -> Option<Display> {
    // « _spdisplays_resolution » : « 2560 x 1440 @ 144.00Hz » ; l'écran
    // principal porte « spdisplays_main ». ProMotion sans fréquence : 120.
    let out = run("system_profiler", &["SPDisplaysDataType", "-json"])?;
    let v: serde_json::Value = serde_json::from_str(&out).ok()?;
    let displays: Vec<&serde_json::Value> = v["SPDisplaysDataType"]
        .as_array()?
        .iter()
        .flat_map(|g| g["spdisplays_ndrvs"].as_array().into_iter().flatten())
        .collect();
    let main = displays
        .iter()
        .find(|d| d["spdisplays_main"].as_str() == Some("spdisplays_yes"))
        .or(displays.first())?;
    let res = main["_spdisplays_resolution"].as_str().unwrap_or_default();
    let refresh = res
        .split('@')
        .nth(1)
        .and_then(|s| s.trim().trim_end_matches("Hz").trim().parse::<f64>().ok())
        .map(|r| r.round() as u32)
        .or_else(|| main.to_string().contains("ProMotion").then_some(120));
    // Pixels réels (« _spdisplays_pixels » : « 3024 x 1964 ») : Minecraft
    // compte en pixels réels, même sur un écran Retina.
    let height = main["_spdisplays_pixels"]
        .as_str()
        .or(Some(res))
        .and_then(|p| p.split('x').nth(1))
        .and_then(|h| h.split_whitespace().next())
        .and_then(|h| h.parse().ok());
    Some(Display { refresh_hz: refresh, height_px: height, vrr_capable: false, vrr_active: false, source: "macOS".into() })
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
fn platform() -> Option<Display> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// EDID minimal : bloc de base + extension CTA contenant un bloc
    /// « vendor specific » avec l'OUI donné.
    fn edid_with_vsdb(oui: [u8; 3]) -> Vec<u8> {
        let mut e = vec![0u8; 256];
        e[126] = 1;
        let ext = &mut e[128..];
        ext[0] = 0x02;
        ext[1] = 0x03;
        ext[2] = 4 + 1 + 5; // un bloc de 5 octets de données
        ext[4] = (3 << 5) | 5;
        ext[5..8].copy_from_slice(&oui);
        e
    }

    #[test]
    fn vrr_dans_l_edid() {
        assert!(edid_vrr_capable(&edid_with_vsdb(OUI_AMD_FREESYNC)));
        assert!(edid_vrr_capable(&edid_with_vsdb(OUI_NVIDIA_GSYNC)));
        // HDMI (OUI 0x000C03) : rien à voir avec le VRR
        assert!(!edid_vrr_capable(&edid_with_vsdb([0x03, 0x0C, 0x00])));
        assert!(!edid_vrr_capable(&[0u8; 128]));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn les_vrais_ecrans_de_cette_machine() {
        // Informatif : n'échoue pas sur une machine sans écran (CI).
        let d = detect();
        println!("{d:?}");
    }
}
