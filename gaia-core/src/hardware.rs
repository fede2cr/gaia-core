//! Detect local hardware devices: SDR dongles, audio capture cards, and cameras.
//!
//! Each detector shells out to a well-known system tool (rtl_test, arecord, v4l2)
//! and parses the output.  All functions are async-safe via `tokio::process`.

use serde::{Deserialize, Serialize};
use std::process::Stdio;
use tokio::process::Command;

// ── Shared device model ──────────────────────────────────────────────────

/// The kind of hardware device.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum DeviceKind {
    Sdr,
    Microphone,
    Camera,
    Gpu,
}

/// A detected hardware device on the local host.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HwDevice {
    pub kind: DeviceKind,
    /// Short identifier (e.g. "hw:1,0", "/dev/video0", "rtlsdr:0").
    pub id: String,
    /// Human-readable label.
    pub label: String,
    /// Which Gaia project can use this device.
    pub suggested_project: String,
}

/// Acceleration backend detected on the host.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum AccelBackend {
    /// AMD ROCm (MIGraphX / HIP) — `/dev/kfd` + `/dev/dri` present.
    Rocm,
}

/// A detected GPU with optional acceleration info.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GpuInfo {
    /// Render node path, e.g. "/dev/dri/renderD128".
    pub render_node: String,
    /// Card node path, e.g. "/dev/dri/card0".
    pub card_node: Option<String>,
    /// Human-readable name from the driver, e.g. "AMD Radeon RX 7900 XTX".
    pub label: String,
    /// Detected acceleration backend.
    pub backend: AccelBackend,
}

// ── SDR dongles ──────────────────────────────────────────────────────────

/// Detect RTL-SDR dongles via `rtl_test -t` (exits quickly).
pub async fn detect_sdr() -> Vec<HwDevice> {
    let output = Command::new("rtl_test")
        .arg("-t")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await;

    let Ok(output) = output else {
        tracing::debug!("rtl_test not found or failed to run");
        return vec![];
    };

    // rtl_test writes device info to stderr
    let stderr = String::from_utf8_lossy(&output.stderr);
    let mut devices = Vec::new();

    // Lines like: "  0:  Realtek, RTL2838UHIDIR, SN: 00000001"
    for line in stderr.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("Found ") {
            // "Found 1 device(s):" header line, skip it
            if rest.contains("device") {
                continue;
            }
        }
        if trimmed.starts_with(|c: char| c.is_ascii_digit()) && trimmed.contains(':') {
            let parts: Vec<&str> = trimmed.splitn(2, ':').collect();
            if parts.len() == 2 {
                let idx = parts[0].trim();
                let label = parts[1].trim().to_string();
                devices.push(HwDevice {
                    kind: DeviceKind::Sdr,
                    id: format!("rtlsdr:{idx}"),
                    label: if label.is_empty() {
                        format!("RTL-SDR #{idx}")
                    } else {
                        label
                    },
                    suggested_project: "radio".into(),
                });
            }
        }
    }

    devices
}

// ── Audio capture devices ────────────────────────────────────────────────

/// Detect audio capture devices.
///
/// Primary method: parse `/proc/asound/pcm` + `/proc/asound/cards` (works
/// without any external tool).  Falls back to shelling out to `arecord -l`
/// when `/proc/asound` is unavailable.
pub async fn detect_microphones() -> Vec<HwDevice> {
    // Try the /proc/asound approach first (always available on Linux,
    // including inside containers when /proc is mounted).
    let devs = detect_microphones_proc().await;
    if !devs.is_empty() {
        return devs;
    }
    tracing::debug!("/proc/asound yielded no capture devices, trying arecord");
    detect_microphones_arecord().await
}

/// Read `/proc/asound/pcm` for lines containing "capture" and cross-reference
/// `/proc/asound/cards` for human-readable labels.
async fn detect_microphones_proc() -> Vec<HwDevice> {
    // ── Parse /proc/asound/pcm ──────────────────────────────────────
    // Format: "CC-DD: StreamName : StreamName : [playback N] [: capture N]"
    let pcm = match tokio::fs::read_to_string("/proc/asound/pcm").await {
        Ok(s) => s,
        Err(_) => return vec![],
    };

    // ── Parse /proc/asound/cards for card labels ────────────────────
    // Format: " N [ID             ]: Driver - LongName\n                      Description"
    let cards_text = tokio::fs::read_to_string("/proc/asound/cards")
        .await
        .unwrap_or_default();
    let card_labels = parse_card_labels(&cards_text);

    let mut devices = Vec::new();

    for line in pcm.lines() {
        // Only capture-capable devices
        if !line.contains("capture") {
            continue;
        }

        let trimmed = line.trim();
        // "01-00: ALC897 Analog : ALC897 Analog : playback 1 : capture 1"
        let Some((id_part, rest)) = trimmed.split_once(':') else {
            continue;
        };
        let Some((card_str, dev_str)) = id_part.trim().split_once('-') else {
            continue;
        };
        let Ok(card) = card_str.parse::<u32>() else { continue };
        let Ok(dev) = dev_str.parse::<u32>() else { continue };

        // The stream name is the first colon-separated field after the id.
        let stream_name = rest
            .split(':')
            .next()
            .map(|s| s.trim().to_string())
            .unwrap_or_default();

        // Prefer the human-readable card label from /proc/asound/cards;
        // fall back to the PCM stream name.
        let (device_id, label) = match card_labels.get(&card) {
            Some(info) => (
                // Use the ALSA card name so the ID works inside containers
                // where the numeric card index may differ from the host.
                format!("hw:CARD={},DEV={dev}", info.alsa_name),
                info.long_name.clone(),
            ),
            None => (
                format!("hw:{card},{dev}"),
                if stream_name.is_empty() {
                    format!("Card {card} Device {dev}")
                } else {
                    stream_name
                },
            ),
        };

        devices.push(HwDevice {
            kind: DeviceKind::Microphone,
            id: device_id,
            label,
            suggested_project: "audio".into(),
        });
    }

    devices
}

/// Info extracted from `/proc/asound/cards` for one sound card.
#[derive(Clone, Debug)]
struct CardInfo {
    /// Short ALSA identifier (e.g. "iCE", "Generic_1") -- used in
    /// `hw:CARD=<name>,DEV=<n>` which is stable across namespaces.
    alsa_name: String,
    /// Human-readable long name (e.g. "Blue Snowball iCE").
    long_name: String,
}

/// Parse `/proc/asound/cards` into a map of card-number → info.
fn parse_card_labels(text: &str) -> std::collections::HashMap<u32, CardInfo> {
    let mut map = std::collections::HashMap::new();
    for line in text.lines() {
        let trimmed = line.trim();
        // Lines starting with a digit are card header lines:
        // " 1 [Generic_1      ]: HDA-Intel - HD-Audio Generic"
        if !trimmed.starts_with(|c: char| c.is_ascii_digit()) {
            continue;
        }
        let card_num = trimmed
            .split_whitespace()
            .next()
            .and_then(|s| s.parse::<u32>().ok());
        // The ALSA short name is inside the first [ ... ] bracket.
        let alsa_name = trimmed
            .find('[')
            .and_then(|start| {
                trimmed[start + 1..]
                    .find(']')
                    .map(|end| trimmed[start + 1..start + 1 + end].trim().to_string())
            });
        // The long name comes after " - "
        let long_name = trimmed
            .find(" - ")
            .map(|pos| trimmed[pos + 3..].trim().to_string());
        if let (Some(num), Some(aname), Some(lname)) = (card_num, alsa_name, long_name) {
            map.insert(num, CardInfo { alsa_name: aname, long_name: lname });
        }
    }
    map
}

/// Fallback: detect ALSA capture devices via `arecord -l`.
async fn detect_microphones_arecord() -> Vec<HwDevice> {
    let output = Command::new("arecord")
        .arg("-l")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await;

    let Ok(output) = output else {
        tracing::debug!("arecord not found or failed to run");
        return vec![];
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut devices = Vec::new();

    // Lines like: "card 1: Device [USB Audio Device], device 0: USB Audio [USB Audio]"
    // also: "card 4: iCE [Blue Snowball iCE], device 0: USB Audio [USB Audio]"
    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("card ") {
            let card_num = trimmed
                .strip_prefix("card ")
                .and_then(|s| s.split(':').next())
                .and_then(|s| s.trim().parse::<u32>().ok());
            let dev_num = trimmed
                .find("device ")
                .and_then(|pos| {
                    trimmed[pos..]
                        .strip_prefix("device ")
                        .and_then(|s| s.split(':').next())
                        .and_then(|s| s.trim().parse::<u32>().ok())
                });

            // Extract the ALSA card name from first [...] after "card N:"
            // e.g. "card 4: iCE [Blue Snowball iCE]" → card_name = "iCE"
            let card_name = trimmed
                .find(':')
                .and_then(|colon_pos| {
                    let after_colon = &trimmed[colon_pos + 1..];
                    after_colon.trim().split_whitespace().next().map(|s| s.to_string())
                });

            if let (Some(card), Some(dev)) = (card_num, dev_num) {
                let label = trimmed
                    .find('[')
                    .and_then(|start| {
                        trimmed[start + 1..]
                            .find(']')
                            .map(|end| trimmed[start + 1..start + 1 + end].to_string())
                    })
                    .unwrap_or_else(|| format!("Card {card} Device {dev}"));

                // Use card name for stable ID across namespaces;
                // fall back to numeric index if we can't parse the name.
                let device_id = match &card_name {
                    Some(name) if !name.is_empty() => format!("hw:CARD={name},DEV={dev}"),
                    _ => format!("hw:{card},{dev}"),
                };

                devices.push(HwDevice {
                    kind: DeviceKind::Microphone,
                    id: device_id,
                    label,
                    suggested_project: "audio".into(),
                });
            }
        }
    }

    devices
}

// -- Video capture devices ────────────────────────────────────────────────

/// Detect V4L2 video capture devices by scanning `/dev/video*` and
/// reading the device name from `/sys/class/video4linux/*/name`.
///
/// Filters out metadata-only nodes by checking the `index` sysfs attribute.
/// USB cameras often create multiple /dev/video* nodes (e.g. video0 for
/// capture, video1 for metadata).  Only index 0 nodes are actual capture
/// devices.
pub async fn detect_cameras() -> Vec<HwDevice> {
    let mut devices = Vec::new();

    let entries = match tokio::fs::read_dir("/sys/class/video4linux").await {
        Ok(e) => e,
        Err(_) => return devices,
    };

    let mut entries = entries;
    while let Ok(Some(entry)) = entries.next_entry().await {
        let fname = entry.file_name();
        let dev_name = fname.to_string_lossy().to_string();
        if !dev_name.starts_with("video") {
            continue;
        }

        // Filter out metadata-only nodes.
        // The sysfs `index` file contains the sub-device index.  Index 0
        // means the primary video capture node; higher indices are
        // typically metadata or output nodes.
        let index_path = entry.path().join("index");
        if let Ok(idx) = tokio::fs::read_to_string(&index_path).await {
            if idx.trim() != "0" {
                continue;
            }
        }

        let name_path = entry.path().join("name");
        let label = tokio::fs::read_to_string(&name_path)
            .await
            .unwrap_or_else(|_| dev_name.clone())
            .trim()
            .to_string();

        let dev_path = format!("/dev/{dev_name}");

        devices.push(HwDevice {
            kind: DeviceKind::Camera,
            id: dev_path,
            label,
            suggested_project: "light".into(),
        });
    }

    // Sort by device path for stable ordering
    devices.sort_by(|a, b| a.id.cmp(&b.id));
    devices
}

// ── GPU / accelerator detection ──────────────────────────────────────────

/// Detect AMD GPUs with ROCm support.
///
/// Checks for the presence of `/dev/kfd` (the ROCm kernel-mode driver)
/// and enumerates render nodes under `/dev/dri/renderD*`.  Each render
/// node that exposes an `amdgpu` driver via sysfs is reported.
///
/// The human-readable GPU name is read from the DRM subsystem or from
/// `rocm-smi` output when available.
pub async fn detect_gpus() -> Vec<GpuInfo> {
    tracing::debug!("GPU detection: starting scan");

    // Check for the ROCm / KFD kernel module.
    //
    // We check multiple indicators because gaia-core runs inside a
    // container that may not have /dev/kfd mapped but can still see
    // the host sysfs when /sys is bind-mounted read-only.
    //
    // 1. /dev/kfd            – direct device node (on host or if passed through)
    // 2. /sys/class/kfd      – KFD class in sysfs (visible with /sys mount)
    // 3. /sys/module/amdgpu  – amdgpu kernel module loaded
    let dev_kfd = tokio::fs::try_exists("/dev/kfd").await.unwrap_or(false);
    let sys_kfd = tokio::fs::try_exists("/sys/class/kfd").await.unwrap_or(false);
    let sys_amdgpu = tokio::fs::try_exists("/sys/module/amdgpu").await.unwrap_or(false);
    tracing::debug!(
        "GPU detection: /dev/kfd={dev_kfd} /sys/class/kfd={sys_kfd} /sys/module/amdgpu={sys_amdgpu}"
    );

    let has_rocm = dev_kfd || sys_kfd || sys_amdgpu;
    if !has_rocm {
        tracing::info!("No ROCm indicators found (/dev/kfd, /sys/class/kfd, /sys/module/amdgpu)");
        return vec![];
    }

    let mut gpus = Vec::new();

    // Scan /sys/class/drm/ for render nodes backed by the amdgpu driver.
    let mut entries = match tokio::fs::read_dir("/sys/class/drm").await {
        Ok(e) => e,
        Err(e) => {
            tracing::warn!("GPU detection: cannot read /sys/class/drm: {e}");
            return gpus;
        }
    };

    while let Ok(Some(entry)) = entries.next_entry().await {
        let name = entry.file_name();
        let name_str = name.to_string_lossy().to_string();

        // We want renderD* nodes (e.g. renderD128).
        if !name_str.starts_with("renderD") {
            continue;
        }

        // Verify this is an amdgpu device by reading the driver symlink.
        let driver_link = entry.path().join("device/driver");
        let driver_target = tokio::fs::read_link(&driver_link).await;
        let is_amdgpu = match &driver_target {
            Ok(target) => target
                .file_name()
                .map(|f| f.to_string_lossy().contains("amdgpu"))
                .unwrap_or(false),
            Err(_) => false,
        };
        tracing::debug!(
            "GPU detection: {name_str} driver={} amdgpu={is_amdgpu}",
            driver_target.as_ref().map(|t| t.display().to_string()).unwrap_or_else(|e| format!("err({e})"))
        );
        if !is_amdgpu {
            continue;
        }

        let render_node = format!("/dev/dri/{name_str}");

        // Try to find the corresponding card node.
        let render_num: Option<u32> = name_str
            .strip_prefix("renderD")
            .and_then(|s| s.parse().ok());
        let card_node = render_num.map(|n| format!("/dev/dri/card{}", n - 128));

        // Read the GPU product name from sysfs.
        let label = read_gpu_label(&entry.path()).await;

        tracing::debug!(
            "GPU detection: found AMD GPU render_node={render_node} card={card_node:?} label={label:?}"
        );
        gpus.push(GpuInfo {
            render_node,
            card_node,
            label,
            backend: AccelBackend::Rocm,
        });
    }

    if gpus.is_empty() {
        tracing::info!("/dev/kfd present but no amdgpu render nodes found in /sys/class/drm");
    } else {
        tracing::info!(
            "Detected {} AMD GPU(s) with ROCm support: {:?}",
            gpus.len(),
            gpus.iter().map(|g| &g.label).collect::<Vec<_>>()
        );
    }

    gpus
}

/// Try to read a human-readable label for a GPU from sysfs.
///
/// Checks `device/product_name` first (set by some amdgpu drivers),
/// then falls back to reading the PCI vendor/device ID and formatting it.
async fn read_gpu_label(drm_path: &std::path::Path) -> String {
    // Method 1: product_name (available on many amdgpu devices).
    let product_path = drm_path.join("device/product_name");
    if let Ok(name) = tokio::fs::read_to_string(&product_path).await {
        let name = name.trim().to_string();
        if !name.is_empty() {
            return name;
        }
    }

    // Method 2: Build label from PCI vendor:device IDs.
    let vendor_path = drm_path.join("device/vendor");
    let device_path = drm_path.join("device/device");
    let vendor = tokio::fs::read_to_string(&vendor_path)
        .await
        .unwrap_or_default()
        .trim()
        .to_string();
    let device = tokio::fs::read_to_string(&device_path)
        .await
        .unwrap_or_default()
        .trim()
        .to_string();

    if !vendor.is_empty() && !device.is_empty() {
        return format!("AMD GPU [{vendor}:{device}]");
    }

    "AMD GPU (unknown model)".into()
}

/// Convert detected GPUs into generic `HwDevice` items for the
/// dashboard hardware list.
fn gpus_to_hw_devices(gpus: &[GpuInfo]) -> Vec<HwDevice> {
    gpus.iter()
        .map(|g| HwDevice {
            kind: DeviceKind::Gpu,
            id: g.render_node.clone(),
            label: g.label.clone(),
            suggested_project: "processing".into(),
        })
        .collect()
}

// ── Detect all ───────────────────────────────────────────────────────────

/// Run all hardware detectors in parallel and return a combined list.
pub async fn detect_all() -> Vec<HwDevice> {
    let (sdrs, mics, cams, gpus) =
        tokio::join!(detect_sdr(), detect_microphones(), detect_cameras(), detect_gpus());
    let mut all = Vec::with_capacity(sdrs.len() + mics.len() + cams.len() + gpus.len());
    all.extend(sdrs);
    all.extend(mics);
    all.extend(cams);
    all.extend(gpus_to_hw_devices(&gpus));
    all
}
