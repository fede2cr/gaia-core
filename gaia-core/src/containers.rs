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
//!
//! Container specifications (image, volumes, devices, etc.) are loaded from
/// `containers.toml` (next to `compose.yaml`) at startup.  If the file is
/// missing or unreadable the built-in defaults are used.

use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};

use serde::Deserialize;
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

pub fn runtime_cmd(rt: Runtime) -> &'static str {
    match rt {
        Runtime::Podman => "podman",
        Runtime::Docker => "docker",
    }
}

// ── Container specification ──────────────────────────────────────────────

/// Default path for the container configuration file.
/// Lives next to `compose.yaml` in the project root.
const CONFIG_PATH: &str = "containers.toml";

/// Everything needed to `run` a managed container from scratch.
///
/// Loaded from `containers.toml` (or built-in defaults).
#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub struct ContainerSpec {
    pub image: String,
    #[serde(default)]
    env: Vec<String>,
    #[serde(default)]
    devices: Vec<String>,
    #[serde(default)]
    volumes: Vec<String>,
    #[serde(default)]
    group_add: Vec<String>,
    #[serde(default)]
    privileged: bool,
    #[serde(default)]
    extra_args: Vec<String>,
    #[serde(default = "default_restart")]
    restart: String,
}

fn default_restart() -> String {
    "unless-stopped".into()
}

impl Default for ContainerSpec {
    fn default() -> Self {
        Self {
            image: String::new(),
            env: vec![],
            devices: vec![],
            volumes: vec![],
            group_add: vec![],
            privileged: false,
            extra_args: vec![],
            restart: default_restart(),
        }
    }
}

/// The top-level TOML file: a map of container-name → spec.
///
/// ```toml
/// [gaia-audio-capture]
/// image = "docker.io/fede2/gaia-audio-capture"
/// devices = ["/dev/snd:/dev/snd"]
/// volumes = ["gaia-audio-data:/data", "/proc/asound:/proc/asound:ro"]
/// group_add = ["audio"]
/// ```
#[derive(Clone, Debug, Default, Deserialize)]
pub struct ContainerConfig {
    #[serde(flatten)]
    pub containers: HashMap<String, ContainerSpec>,
}

/// Cached config loaded once at startup.
static CONFIG: OnceLock<ContainerConfig> = OnceLock::new();

/// Load the container config file, falling back to built-in defaults.
fn load_config() -> ContainerConfig {
    let path = std::env::var("GAIA_CONTAINERS_CONFIG")
        .unwrap_or_else(|_| CONFIG_PATH.into());

    match std::fs::read_to_string(&path) {
        Ok(text) => match toml::from_str::<ContainerConfig>(&text) {
            Ok(cfg) => {
                tracing::info!(
                    "Loaded container config from {path} ({} containers)",
                    cfg.containers.len()
                );
                cfg
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to parse {path}: {e} -- using built-in defaults"
                );
                builtin_config()
            }
        },
        Err(_) => {
            tracing::info!(
                "No container config at {path} -- using built-in defaults"
            );
            builtin_config()
        }
    }
}

/// Get the (cached) container config.
pub fn config() -> &'static ContainerConfig {
    CONFIG.get_or_init(load_config)
}

/// Look up a container spec by name.
///
/// For audio processing model containers (`gaia-audio-processing-{slug}`),
/// falls back to the base `gaia-audio-processing` spec since all models
/// use the same image and volume layout.
fn spec_for(container_name: &str) -> Option<ContainerSpec> {
    if let Some(spec) = config().containers.get(container_name) {
        return Some(spec.clone());
    }
    // Fallback for model-specific audio processing containers.
    if container_name.starts_with("gaia-audio-processing-") {
        return config().containers.get("gaia-audio-processing").cloned();
    }
    // Fallback for model-specific light processing containers.
    if container_name.starts_with("gaia-light-processing-") {
        return config().containers.get("gaia-light-processing").cloned();
    }
    None
}

/// Hard-coded defaults so gaia-core works out of the box without a config file.
fn builtin_config() -> ContainerConfig {
    let mut m = HashMap::new();

    m.insert("gaia-audio-capture".into(), ContainerSpec {
        image: "docker.io/fede2/gaia-audio-capture".into(),
        devices: vec!["/dev/snd:/dev/snd".into()],
        volumes: vec![
            "gaia-audio-data:/data".into(),
            "/proc/asound:/proc/asound:ro".into(),
        ],
        group_add: vec!["audio".into()],
        ..Default::default()
    });

    m.insert("gaia-audio-processing".into(), ContainerSpec {
        image: "docker.io/fede2/gaia-audio-processing".into(),
        volumes: vec![
            "gaia-audio-data:/data".into(),
            "gaia-audio-models:/models".into(),
        ],
        ..Default::default()
    });

    m.insert("gaia-audio-web".into(), ContainerSpec {
        image: "docker.io/fede2/gaia-audio-web".into(),
        env: vec!["LEPTOS_SITE_ADDR=0.0.0.0:3000".into()],
        volumes: vec!["gaia-audio-data:/data".into()],
        ..Default::default()
    });

    m.insert("gaia-radio-capture".into(), ContainerSpec {
        image: "docker.io/fede2/gaia-radio-capture".into(),
        devices: vec!["/dev/bus/usb:/dev/bus/usb".into()],
        privileged: true,
        ..Default::default()
    });

    m.insert("gaia-radio-processing".into(), ContainerSpec {
        image: "docker.io/fede2/gaia-radio-processing".into(),
        volumes: vec!["readsb-json:/run/readsb".into()],
        ..Default::default()
    });

    m.insert("gaia-radio-web".into(), ContainerSpec {
        image: "docker.io/fede2/gaia-radio-web".into(),
        env: vec!["WEB_PORT=8080".into()],
        volumes: vec![
            "readsb-json:/run/readsb:ro".into(),
            "co2-state:/var/lib/co2tracker".into(),
        ],
        ..Default::default()
    });

    m.insert("gaia-gmn-config".into(), ContainerSpec {
        image: "docker.io/fede2/gaia-gmn-config".into(),
        env: vec!["STREAM_PORT=8181".into()],
        ..Default::default()
    });

    m.insert("gaia-gmn-capture".into(), ContainerSpec {
        image: "docker.io/fede2/gaia-gmn-capture".into(),
        volumes: vec!["gaia-gmn-data:/data".into()],
        ..Default::default()
    });

    m.insert("rms".into(), ContainerSpec {
        image: "docker.io/fede2/rms".into(),
        volumes: vec!["rms-data:/home/rms/RMS_data".into()],
        ..Default::default()
    });

    // ── Gaia Light (camera trap) ──────────────────────────────────

    m.insert("gaia-light-capture".into(), ContainerSpec {
        image: "docker.io/fede2/gaia-light-capture".into(),
        volumes: vec!["gaia-light-data:/data".into()],
        ..Default::default()
    });

    m.insert("gaia-light-processing".into(), ContainerSpec {
        image: "docker.io/fede2/gaia-light-processing".into(),
        volumes: vec![
            "gaia-light-data:/data".into(),
            "gaia-light-models:/models".into(),
        ],
        ..Default::default()
    });

    m.insert("gaia-light-web".into(), ContainerSpec {
        image: "docker.io/fede2/gaia-light-web".into(),
        env: vec!["LEPTOS_SITE_ADDR=0.0.0.0:8190".into()],
        volumes: vec!["gaia-light-data:/data".into()],
        ..Default::default()
    });

    ContainerConfig { containers: m }
}

// ── Container lifecycle status ───────────────────────────────────────────

/// Global container status tracker.
///
/// Tracks the lifecycle phase of each container: `"stopped"`, `"pulling"`,
/// `"starting"`, `"running"`, or `"error: <message>"`.
static STATUSES: OnceLock<RwLock<HashMap<String, String>>> = OnceLock::new();

fn statuses() -> &'static RwLock<HashMap<String, String>> {
    STATUSES.get_or_init(|| RwLock::new(HashMap::new()))
}

/// Set the lifecycle status of a container.
pub fn set_status(name: &str, status: &str) {
    if let Ok(mut map) = statuses().write() {
        map.insert(name.to_string(), status.to_string());
    }
}

/// Get the lifecycle status of a single container.
pub fn get_status(name: &str) -> String {
    statuses()
        .read()
        .ok()
        .and_then(|map| map.get(name).cloned())
        .unwrap_or_else(|| "stopped".into())
}

/// Snapshot of all container statuses (for the UI to poll).
pub fn all_statuses() -> Vec<(String, String)> {
    statuses()
        .read()
        .map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
        .unwrap_or_default()
}

/// Derive the project slug from a container name.
///
/// This is the inverse of [`container_name`]: given `"gaia-audio-capture"`
/// it returns `"audio"`.  Returns an empty string for unrecognised names.
fn project_slug_from_container(name: &str) -> String {
    // Special cases first.
    if name == "rms" {
        return "gmn".into();
    }
    // General pattern: gaia-{slug}-{kind…}
    if let Some(rest) = name.strip_prefix("gaia-") {
        // The slug is the first segment: audio, radio, gmn, light.
        for slug in ["audio", "radio", "gmn", "light"] {
            if rest.starts_with(slug) {
                return slug.into();
            }
        }
    }
    String::new()
}

/// Derive the container name from a project slug and container kind.
///
/// Convention:  `gaia-{slug}-{kind}`
///
/// For audio processing models, the `kind` may be `"processing:{model}"`
/// which maps to `"gaia-audio-processing-{model}"`, except for the
/// default BirdNET model where `"processing"` maps to the legacy name
/// `"gaia-audio-processing"`.
///
/// Examples
/// --------
/// - `("audio", "capture")`           → `"gaia-audio-capture"`
/// - `("audio", "processing")`        → `"gaia-audio-processing"` (BirdNET)
/// - `("audio", "processing:perch")`  → `"gaia-audio-processing-perch"`
/// - `("radio", "web")`               → `"gaia-radio-web"`
pub fn container_name(slug: &str, kind: &str) -> String {
    // Legacy: the monolithic RMS container is still used for "processing"
    // until the processing pipeline is extracted.
    if slug == "gmn" && kind == "processing" {
        return "rms".into();
    }
    // Audio processing model containers: "processing:perch" → "gaia-audio-processing-perch"
    if let Some(model_slug) = kind.strip_prefix("processing:") {
        return format!("gaia-{slug}-processing-{model_slug}");
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
    set_status(name, "pulling");
    pull(cmd, &spec.image).await?;

    // 2. Remove any stale container with the same name.
    set_status(name, "starting");
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
    args.push(spec.restart.clone());

    // Privileged mode (e.g. USB access for SDR dongles)
    if spec.privileged {
        args.push("--privileged".into());
    }

    // Environment variables
    for e in &spec.env {
        args.push("-e".into());
        args.push(e.clone());
    }

    // Debug logging — inject RUST_LOG=debug when the operator has
    // enabled debug mode for this project in the Settings page.
    let project_slug = project_slug_from_container(name);
    if !project_slug.is_empty() && crate::db::is_debug_enabled(&project_slug).await {
        // Only inject if the spec doesn't already set RUST_LOG.
        let has_rust_log = spec.env.iter().any(|e| e.starts_with("RUST_LOG="));
        if !has_rust_log {
            tracing::info!("Injecting RUST_LOG=debug for container '{name}' (project '{project_slug}')");
            args.push("-e".into());
            args.push("RUST_LOG=debug".into());
        }
    }

    // Node name — inject a human-friendly identifier so web UIs and
    // processing servers can display it instead of an IP address.
    if let Ok(Some(node_name)) = crate::db::get_setting("node_name").await {
        if !node_name.is_empty() {
            args.push("-e".into());
            args.push(format!("NODE_NAME={node_name}"));
        }
    }

    // Device mappings
    for d in &spec.devices {
        args.push("--device".into());
        args.push(d.clone());
    }

    // Volumes
    for v in &spec.volumes {
        args.push("-v".into());
        args.push(v.clone());
    }

    // Supplementary groups
    for g in &spec.group_add {
        args.push("--group-add".into());
        args.push(g.clone());
    }

    // Extra arguments (e.g. --userns=keep-id)
    for a in &spec.extra_args {
        args.push(a.clone());
    }

    // Dynamic args for containers that need runtime information.
    if name == "gaia-audio-capture" {
        build_audio_capture_args(&mut args).await;
    }
    if name.starts_with("gaia-audio-processing") {
        // Derive the model slug from the container name.
        let model_slug = if name == "gaia-audio-processing" {
            "birdnet".to_string()
        } else {
            name.strip_prefix("gaia-audio-processing-")
                .unwrap_or("birdnet")
                .to_string()
        };
        build_audio_processing_args(&mut args, &model_slug).await;
    }
    if name == "gaia-gmn-config" {
        build_gmn_config_args(&mut args).await;
    }
    if name == "gaia-gmn-capture" {
        build_gmn_capture_args(&mut args).await;
    }
    if name == "rms" {
        build_rms_args(&mut args).await;
    }
    if name == "gaia-light-capture" {
        build_light_capture_args(&mut args).await;
    }
    if name.starts_with("gaia-light-processing") {
        let model_slug = name.strip_prefix("gaia-light-processing-")
            .unwrap_or("pytorch-wildlife")
            .to_string();
        build_light_processing_args(&mut args, &model_slug).await;
    }
    // Image (must be last)
    args.push(spec.image);

    tracing::info!("Running container '{name}' via {cmd}");

    let output = Command::new(cmd)
        .args(&args)
        .output()
        .await
        .map_err(|e| format!("Failed to execute {cmd}: {e}"))?;

    if output.status.success() {
        tracing::info!("Container '{name}' started");
        set_status(name, "running");

        // For audio processing containers, validate model loading in the
        // background so we can report issues to the operator.
        if name.starts_with("gaia-audio-processing") {
            let cname = name.to_string();
            tokio::spawn(async move {
                validate_audio_processing(&cname).await;
            });
        }

        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let msg = format!("{cmd} run {name} failed: {stderr}");
        tracing::error!("{msg}");
        set_status(name, &format!("error: {msg}"));
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
            "gaia-audio-capture: no microphone assigned to project 'audio' -- \
             container will try ALSA default (may fail)"
        );
    }
}

/// Inject the station latitude/longitude and model information into
/// the processing container so the model can filter species by
/// geographic range and identify which model to run.
///
/// Also passes `PROCESSING_NODE_COUNT` so the container knows how many
/// sibling processing nodes exist (used for recording lifecycle: a
/// recording is only deleted when all nodes have analysed it).
async fn build_audio_processing_args(args: &mut Vec<String>, model_slug: &str) {
    let lat = crate::db::get_setting("latitude").await.ok().flatten();
    let lon = crate::db::get_setting("longitude").await.ok().flatten();

    match (lat, lon) {
        (Some(la), Some(lo)) if !la.is_empty() && !lo.is_empty() => {
            tracing::info!("gaia-audio-processing ({model_slug}): LATITUDE={la}, LONGITUDE={lo}");
            args.push("-e".into());
            args.push(format!("LATITUDE={la}"));
            args.push("-e".into());
            args.push(format!("LONGITUDE={lo}"));
        }
        _ => {
            tracing::warn!(
                "gaia-audio-processing ({model_slug}): no station location configured -- \
                 model will not filter by geographic range"
            );
        }
    }

    // Tell the container which model to run.
    args.push("-e".into());
    args.push(format!("MODEL_SLUGS={model_slug}"));

    // Instance identifier for multi-instance coordination.
    args.push("-e".into());
    args.push(format!("PROCESSING_INSTANCE={model_slug}"));

    // Number of parallel processing threads (default handled by container).
    if let Ok(Some(threads)) = crate::db::get_setting("processing_threads").await {
        if !threads.is_empty() {
            args.push("-e".into());
            args.push(format!("PROCESSING_THREADS={threads}"));
        }
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

/// Build dynamic container arguments for gaia-gmn-capture.
///
/// 1. Reads the camera device assigned to GMN from the database
///    (falls back to `/dev/video0`).
/// 2. Bind-mounts every `/dev/video*` node found on the host.
/// 3. Sets the `VIDEO_DEVICE` and `STATION_ID` environment variables.
async fn build_gmn_capture_args(args: &mut Vec<String>) {
    let (video_device, station_id) = match crate::db::get_all_assignments().await {
        Ok(assignments) => {
            let dev = assignments
                .iter()
                .find(|a| a.project == "gmn")
                .map(|a| a.device_id.clone())
                .unwrap_or_else(|| "/dev/video0".into());
            (dev, "XX0001".to_string())
        }
        Err(_) => ("/dev/video0".into(), "XX0001".to_string()),
    };

    tracing::info!("gaia-gmn-capture: using camera device {video_device}");
    args.push("-e".into());
    args.push(format!("VIDEO_DEVICE={video_device}"));
    args.push("-e".into());
    args.push(format!("STATION_ID={station_id}"));

    let mounted = mount_video_devices(args).await;
    if mounted.is_empty() {
        args.push("-v".into());
        args.push(format!("{video_device}:{video_device}"));
        tracing::warn!("No /dev/video* devices found on host");
    } else {
        tracing::info!("gaia-gmn-capture: mounted devices {:?}", mounted);
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

/// Build dynamic container arguments for gaia-light-capture.
///
/// 1. Reads the camera device assigned to project "light" from the DB
///    (falls back to `/dev/video0`).
/// 2. Bind-mounts all `/dev/video*` nodes from the host.
/// 3. Sets `VIDEO_DEVICE` so the capture server knows which camera to use.
async fn build_light_capture_args(args: &mut Vec<String>) {
    let video_device = match crate::db::get_all_assignments().await {
        Ok(assignments) => assignments
            .iter()
            .find(|a| a.project == "light")
            .map(|a| a.device_id.clone())
            .unwrap_or_else(|| "/dev/video0".into()),
        Err(_) => "/dev/video0".into(),
    };

    tracing::info!("gaia-light-capture: using camera device {video_device}");
    args.push("-e".into());
    args.push(format!("VIDEO_DEVICE={video_device}"));

    let mounted = mount_video_devices(args).await;
    if mounted.is_empty() {
        args.push("-v".into());
        args.push(format!("{video_device}:{video_device}"));
        tracing::warn!("gaia-light-capture: no /dev/video* devices found on host");
    } else {
        tracing::info!("gaia-light-capture: mounted devices {:?}", mounted);
    }
}

/// Inject station lat/lon and model slug into gaia-light-processing.
async fn build_light_processing_args(args: &mut Vec<String>, model_slug: &str) {
    let lat = crate::db::get_setting("latitude").await.ok().flatten();
    let lon = crate::db::get_setting("longitude").await.ok().flatten();

    match (lat, lon) {
        (Some(la), Some(lo)) if !la.is_empty() && !lo.is_empty() => {
            tracing::info!("gaia-light-processing ({model_slug}): LATITUDE={la}, LONGITUDE={lo}");
            args.push("-e".into());
            args.push(format!("LATITUDE={la}"));
            args.push("-e".into());
            args.push(format!("LONGITUDE={lo}"));
        }
        _ => {
            tracing::warn!(
                "gaia-light-processing ({model_slug}): no station location configured"
            );
        }
    }

    args.push("-e".into());
    args.push(format!("MODEL_SLUGS={model_slug}"));
    args.push("-e".into());
    args.push(format!("PROCESSING_INSTANCE={model_slug}"));
}

/// Stop a container by name.  Returns Ok(()) on success or an error message.
pub async fn stop(name: &str) -> Result<(), String> {
    let rt = runtime().await;
    let cmd = runtime_cmd(rt);

    set_status(name, "stopped");
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

        match (*enabled, running) {
            (true, true) => {
                // Already running from a previous session.
                set_status(&name, "running");
            }
            (true, false) => {
                // Should be running but isn't -- start in background.
                let name = name.clone();
                tokio::spawn(async move {
                    if let Err(e) = start(&name).await {
                        tracing::warn!("Startup sync: could not start {name}: {e}");
                    }
                });
            }
            (false, true) => {
                if let Err(e) = stop(&name).await {
                    tracing::warn!("Startup sync: could not stop {name}: {e}");
                }
            }
            (false, false) => {
                set_status(&name, "stopped");
            }
        }
    }
    tracing::info!("Container sync complete ({} entries)", states.len());
}

// ── Audio processing model validation ────────────────────────────────────────

/// Wait for an audio processing container to finish initialisation and
/// check whether it successfully loaded its model.
///
/// Reads the container logs for up to ~30 seconds looking for the key
/// phrases the processing server emits:
///   - `"Model ready:"` → success
///   - `"No models loaded"` → manifest was filtered out or model not found
///   - `"Cannot load model"` → model file present but failed to load
///
/// The result is recorded in `set_status()` so the dashboard can show it.
async fn validate_audio_processing(name: &str) {
    let rt = runtime().await;
    let cmd = runtime_cmd(rt);

    // Give the container time to start, discover manifests, and
    // download model files (if needed).  We check logs periodically.
    for attempt in 0..6u32 {
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;

        let output = match Command::new(cmd)
            .args(["logs", "--tail", "50", name])
            .output()
            .await
        {
            Ok(o) => o,
            Err(_) => continue,
        };

        let logs = String::from_utf8_lossy(&output.stdout);
        let stderr_logs = String::from_utf8_lossy(&output.stderr);
        // Podman sends container logs to stderr for some log drivers.
        let combined = format!("{logs}{stderr_logs}");

        // Guard: if the container was stopped while we were waiting,
        // don't overwrite the "stopped" status.
        if !is_running(name).await {
            tracing::info!("[{name}] Container stopped during validation, aborting");
            return;
        }

        if combined.contains("Model ready:") {
            tracing::info!("[{name}] Model loaded successfully");
            set_status(name, "running");
            return;
        }

        if combined.contains("No models loaded") {
            let msg = format!("warning: no models loaded -- check manifest and MODEL_SLUGS");
            tracing::warn!("[{name}] {msg}");
            set_status(name, &format!("running ({msg})"));
            return;
        }

        if combined.contains("Cannot load model") {
            let msg = "warning: model file found but failed to load";
            tracing::warn!("[{name}] {msg}");
            set_status(name, &format!("running ({msg})"));
            return;
        }

        // If the container has exited (crash), report that immediately
        if !is_running(name).await {
            let msg = "error: container exited during startup";
            tracing::error!("[{name}] {msg}");
            set_status(name, msg);
            return;
        }

        tracing::debug!(
            "[{name}] Checking model status (attempt {}/6)...",
            attempt + 1
        );
    }

    // Still no definitive signal after 30s -- probably still downloading.
    tracing::info!(
        "[{name}] Model validation timed out -- container may still be downloading model files"
    );
}
