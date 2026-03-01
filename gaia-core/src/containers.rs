//! Container lifecycle control: start / stop Podman or Docker containers.
//!
//! Gaia-core runs inside a container with the host's Podman (or Docker)
//! socket mounted at `/run/podman/podman.sock` (or `/var/run/docker.sock`).
//! We shell out to the CLI (`podman` or `docker`) so there is no extra Rust
//! dependency, the binary is already installed in the runtime image.
//!
//! On **start** the flow is: pull image → remove stale container → run.
//! This guarantees the latest image is always used and avoids the
//! "no such container" error when the container has never been created.

use std::sync::OnceLock;
use tokio::process::Command;

/// Which container runtime is available on this system.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Runtime {
    Podman,
    Docker,
}

/// Cached runtime detection so we only probe once.
static RUNTIME: OnceLock<Runtime> = OnceLock::new();

/// Detect (and cache) whether `podman` or `docker` is available.
pub async fn runtime() -> Runtime {
    if let Some(r) = RUNTIME.get() {
        return *r;
    }

    // Check the CONTAINER_RUNTIME env-var first (set in compose.yaml).
    if let Ok(val) = std::env::var("CONTAINER_RUNTIME") {
        let rt = match val.to_lowercase().as_str() {
            "docker" => Runtime::Docker,
            _ => Runtime::Podman,
        };
        let _ = RUNTIME.set(rt);
        return rt;
    }

    // Fall back to probing the PATH.
    let has_podman = Command::new("podman")
        .arg("--version")
        .output()
        .await
        .map(|o| o.status.success())
        .unwrap_or(false);

    let rt = if has_podman {
        Runtime::Podman
    } else {
        Runtime::Docker
    };
    let _ = RUNTIME.set(rt);
    rt
}

fn runtime_cmd(rt: Runtime) -> &'static str {
    match rt {
        Runtime::Podman => "podman",
        Runtime::Docker => "docker",
    }
}

// ── Container specification ──────────────────────────────────────────────

/// Everything needed to `run` a managed container from scratch.
struct ContainerSpec {
    image: &'static str,
    env: &'static [&'static str],         // "KEY=VALUE"
    devices: &'static [&'static str],     // "/dev/x:/dev/x"
    volumes: &'static [&'static str],     // "name:/path" or "host:container:opts"
    group_add: &'static [&'static str],   // supplementary groups
    privileged: bool,                     // --privileged
    extra_args: &'static [&'static str],  // additional `podman run` flags
    restart: &'static str,                // restart policy
    // All containers use --network host for mDNS multicast.
}

/// Registry of all managed containers.
///
/// The key is the container name (e.g. `"gaia-radio-web"`).
/// This mirrors the compose.yaml definitions for the `profiles: ["managed"]`
/// services so we can create containers from scratch without compose.
fn spec_for(container_name: &str) -> Option<ContainerSpec> {
    match container_name {
        // ── Gaia Audio ───────────────────────────────────
        "gaia-audio-capture" => Some(ContainerSpec {
            image: "docker.io/fede2/gaia-audio-capture",
            env: &[],
            devices: &["/dev/snd:/dev/snd"],
            volumes: &[
                "gaia-audio-data:/data",
                "/proc/asound:/proc/asound:ro",
            ],
            group_add: &["audio"],
            privileged: false,
            extra_args: &[],
            restart: "unless-stopped",
        }),
        "gaia-audio-processing" => Some(ContainerSpec {
            image: "docker.io/fede2/gaia-audio-processing",
            env: &[],
            devices: &[],
            volumes: &["gaia-audio-data:/data"],
            group_add: &[],
            privileged: false,
            extra_args: &[],
            restart: "unless-stopped",
        }),
        "gaia-audio-web" => Some(ContainerSpec {
            image: "docker.io/fede2/gaia-audio-web",
            env: &["LEPTOS_SITE_ADDR=0.0.0.0:3000"],
            devices: &[],
            volumes: &["gaia-audio-data:/data"],
            group_add: &[],
            privileged: false,
            extra_args: &[],
            restart: "unless-stopped",
        }),
        // ── Gaia Radio ───────────────────────────────────
        "gaia-radio-capture" => Some(ContainerSpec {
            image: "docker.io/fede2/gaia-radio-capture",
            env: &[],
            devices: &["/dev/bus/usb:/dev/bus/usb"],
            volumes: &[],
            group_add: &[],
            privileged: true,
            extra_args: &[],
            restart: "unless-stopped",
        }),
        "gaia-radio-processing" => Some(ContainerSpec {
            image: "docker.io/fede2/gaia-radio-processing",
            env: &[],
            devices: &[],
            volumes: &["readsb-json:/run/readsb"],
            group_add: &[],
            privileged: false,
            extra_args: &[],
            restart: "unless-stopped",
        }),
        "gaia-radio-web" => Some(ContainerSpec {
            image: "docker.io/fede2/gaia-radio-web",
            env: &["WEB_PORT=8080"],
            devices: &[],
            volumes: &[
                "readsb-json:/run/readsb:ro",
                "co2-state:/var/lib/co2tracker",
            ],
            group_add: &[],
            privileged: false,
            extra_args: &[],
            restart: "unless-stopped",
        }),
        // ── GMN / RMS ────────────────────────────────────
        // Camera access in rootless podman requires a host udev rule
        // that sets MODE="0666" on video devices (see setup-host.sh).
        // The actual device path and volume mounts are resolved
        // dynamically in `start()` using `build_gmn_config_args()`.
        "gaia-gmn-config" => Some(ContainerSpec {
            image: "docker.io/fede2/gaia-gmn-config",
            env: &["STREAM_PORT=8181"],
            devices: &[],
            volumes: &[],
            group_add: &[],
            privileged: false,
            extra_args: &[],
            restart: "unless-stopped",
        }),
        // RMS is a single container for GMN capture + processing.
        // It will be split into separate containers later.
        // Video devices are mounted dynamically in start() via
        // mount_video_devices().  The user is "rms" (uid 1000).
        "rms" => Some(ContainerSpec {
            image: "docker.io/fede2/rms",
            env: &[],
            devices: &[],
            volumes: &["rms-data:/home/rms/RMS_data"],
            group_add: &[],
            privileged: false,
            extra_args: &[],
            restart: "unless-stopped",
        }),
        _ => None,
    }
}

/// Derive the container name from a project slug and container kind.
///
/// Convention:  `gaia-{slug}-{kind}`
///
/// Examples
/// --------
/// - `("audio", "capture")` → `"gaia-audio-capture"`
/// - `("radio", "web")`     → `"gaia-radio-web"`
pub fn container_name(slug: &str, kind: &str) -> String {
    // GMN uses a single "rms" container for capture + processing.
    // Once RMS is split, this special case can be removed.
    if slug == "gmn" && (kind == "capture" || kind == "processing") {
        return "rms".into();
    }
    format!("gaia-{slug}-{kind}")
}

// ── Lifecycle operations ─────────────────────────────────────────────────

/// Pull the latest image for a container.  Always called before run.
async fn pull(cmd: &str, image: &str) -> Result<(), String> {
    tracing::info!("Pulling image '{image}'");
    let output = Command::new(cmd)
        .args(["pull", image])
        .output()
        .await
        .map_err(|e| format!("Failed to execute {cmd} pull: {e}"))?;

    if output.status.success() {
        tracing::info!("Image '{image}' pulled");
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // Non-fatal: we can still run with a cached image.
        tracing::warn!("Pull for '{image}' failed (will use cached if available): {stderr}");
        Ok(())
    }
}

/// Remove an existing container (stopped or running).  Errors are ignored
/// because the container may not exist yet.
async fn remove(cmd: &str, name: &str) {
    // Force-remove handles both running and stopped containers.
    let output = Command::new(cmd)
        .args(["rm", "-f", name])
        .output()
        .await;

    match output {
        Ok(o) if o.status.success() => {
            tracing::debug!("Removed old container '{name}'");
        }
        _ => {
            // Likely "no such container", that's fine.
            tracing::debug!("No existing container '{name}' to remove");
        }
    }
}

/// Start a managed container: **pull → remove → run**.
///
/// This always pulls the latest image so that enabling a container from
/// the dashboard also updates it.
pub async fn start(name: &str) -> Result<(), String> {
    let rt = runtime().await;
    let cmd = runtime_cmd(rt);

    let spec = spec_for(name).ok_or_else(|| format!("Unknown container: {name}"))?;

    // 1. Pull the latest image.
    pull(cmd, spec.image).await?;

    // 2. Remove any stale container with the same name.
    remove(cmd, name).await;

    // 3. Build the `run` command; all containers use host networking.
    let mut args: Vec<String> = vec![
        "run".into(),
        "-d".into(),
        "--name".into(),
        name.into(),
        "--network".into(),
        "host".into(),
    ];

    // Restart policy
    args.push("--restart".into());
    args.push(spec.restart.into());

    // Privileged mode (e.g. USB access for SDR dongles)
    if spec.privileged {
        args.push("--privileged".into());
    }

    // Environment variables
    for e in spec.env {
        args.push("-e".into());
        args.push((*e).into());
    }

    // Device mappings
    for d in spec.devices {
        args.push("--device".into());
        args.push((*d).into());
    }

    // Volumes
    for v in spec.volumes {
        args.push("-v".into());
        args.push((*v).into());
    }

    // Supplementary groups
    for g in spec.group_add {
        args.push("--group-add".into());
        args.push((*g).into());
    }

    // Extra arguments (e.g. --userns=keep-id)
    for a in spec.extra_args {
        args.push((*a).into());
    }

    // Dynamic args for containers that need runtime information.
    if name == "gaia-audio-capture" {
        build_audio_capture_args(&mut args).await;
    }
    if name == "gaia-gmn-config" {
        build_gmn_config_args(&mut args).await;
    }
    if name == "rms" {
        build_rms_args(&mut args).await;
    }

    // Image (must be last)
    args.push(spec.image.into());

    tracing::info!("Running container '{name}' via {cmd}");

    let output = Command::new(cmd)
        .args(&args)
        .output()
        .await
        .map_err(|e| format!("Failed to execute {cmd}: {e}"))?;

    if output.status.success() {
        tracing::info!("Container '{name}' started");
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let msg = format!("{cmd} run {name} failed: {stderr}");
        tracing::error!("{msg}");
        Err(msg)
    }
}

/// Build dynamic container arguments for the gaia-audio-capture container.
///
/// Reads the microphone device assigned to project "audio" from the DB
/// and passes it as `REC_CARD` so the capture server uses the correct
/// ALSA device instead of the (broken-in-container) "default" PCM.
async fn build_audio_capture_args(args: &mut Vec<String>) {
    let rec_card = match crate::db::get_all_assignments().await {
        Ok(assignments) => assignments
            .iter()
            .find(|a| a.project == "audio")
            .map(|a| a.device_id.clone()),
        Err(e) => {
            tracing::warn!("Cannot read assignments for audio mic: {e}");
            None
        }
    };

    if let Some(card) = rec_card {
        tracing::info!("gaia-audio-capture: using microphone REC_CARD={card}");
        args.push("-e".into());
        args.push(format!("REC_CARD={card}"));
    } else {
        tracing::warn!(
            "gaia-audio-capture: no microphone assigned to project 'audio' — \
             container will try ALSA default (may fail)"
        );
    }
}

/// Discover and bind-mount every `/dev/video*` node from the host.
///
/// Returns the list of mounted device paths (sorted), or an empty vec
/// if no video devices were found on the host.
async fn mount_video_devices(args: &mut Vec<String>) -> Vec<String> {
    let mut mounted = Vec::new();
    if let Ok(mut entries) = tokio::fs::read_dir("/dev").await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if name_str.starts_with("video") {
                let path = format!("/dev/{name_str}");
                args.push("-v".into());
                args.push(format!("{path}:{path}"));
                mounted.push(path);
            }
        }
    }
    mounted.sort();
    mounted
}

/// Build dynamic container arguments for the gaia-gmn-config container.
///
/// 1. Reads the camera device assigned to GMN from the database
///    (falls back to `/dev/video0`).
/// 2. Bind-mounts every `/dev/video*` node found on the host so the user
///    can switch devices without restarting the container.
/// 3. Sets the `VIDEO_DEVICE` environment variable.
async fn build_gmn_config_args(args: &mut Vec<String>) {
    // Resolve the assigned camera device from the DB.
    let video_device = match crate::db::get_all_assignments().await {
        Ok(assignments) => assignments
            .iter()
            .find(|a| a.project == "gmn")
            .map(|a| a.device_id.clone())
            .unwrap_or_else(|| "/dev/video0".into()),
        Err(_) => "/dev/video0".into(),
    };

    tracing::info!("gaia-gmn-config: using camera device {video_device}");
    args.push("-e".into());
    args.push(format!("VIDEO_DEVICE={video_device}"));

    let mounted = mount_video_devices(args).await;

    if mounted.is_empty() {
        // No video devices found; mount the assigned device path anyway
        // so the error message inside the container makes sense.
        args.push("-v".into());
        args.push(format!("{video_device}:{video_device}"));
        tracing::warn!("No /dev/video* devices found on host");
    } else {
        tracing::info!("gaia-gmn-config: mounted devices {:?}", mounted);
    }
}

/// Build dynamic container arguments for the RMS container.
///
/// RMS needs all `/dev/video*` nodes bind-mounted for USB camera capture.
/// Unlike gaia-gmn-config, RMS reads its camera device from its `.config`
/// file rather than an environment variable.
async fn build_rms_args(args: &mut Vec<String>) {
    let mounted = mount_video_devices(args).await;

    if mounted.is_empty() {
        tracing::warn!("rms: no /dev/video* devices found on host");
    } else {
        tracing::info!("rms: mounted devices {:?}", mounted);
    }
}

/// Stop a container by name.  Returns Ok(()) on success or an error message.
pub async fn stop(name: &str) -> Result<(), String> {
    let rt = runtime().await;
    let cmd = runtime_cmd(rt);

    tracing::info!("Stopping container '{name}' via {cmd}");

    let output = Command::new(cmd)
        .args(["stop", "-t", "10", name])
        .output()
        .await
        .map_err(|e| format!("Failed to execute {cmd}: {e}"))?;

    if output.status.success() {
        tracing::info!("Container '{name}' stopped");
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let msg = format!("{cmd} stop {name} failed: {stderr}");
        tracing::warn!("{msg}");
        Err(msg)
    }
}

/// Check if a container is currently running.
pub async fn is_running(name: &str) -> bool {
    let rt = runtime().await;
    let cmd = runtime_cmd(rt);

    let output = Command::new(cmd)
        .args(["inspect", "--format", "{{.State.Running}}", name])
        .output()
        .await;

    match output {
        Ok(o) if o.status.success() => {
            let stdout = String::from_utf8_lossy(&o.stdout);
            stdout.trim() == "true"
        }
        _ => false,
    }
}

/// Synchronise running containers with the persisted DB state.
///
/// Called once at startup so that containers whose toggle was left "on"
/// in a previous session are started, and those marked "off" are stopped.
pub async fn sync_with_db() {
    let states = match crate::db::all_container_states().await {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("Cannot read container states for sync: {e}");
            return;
        }
    };

    for (slug, kind, enabled) in &states {
        let name = container_name(slug, kind);
        let running = is_running(&name).await;

        match (enabled, running) {
            (true, false) => {
                if let Err(e) = start(&name).await {
                    tracing::warn!("Startup sync: could not start {name}: {e}");
                }
            }
            (false, true) => {
                if let Err(e) = stop(&name).await {
                    tracing::warn!("Startup sync: could not stop {name}: {e}");
                }
            }
            _ => {} // already in the desired state
        }
    }
    tracing::info!("Container sync complete ({} entries)", states.len());
}
