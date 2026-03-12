//! Leptos server functions called from UI components, executed on the server.

use leptos::prelude::*;
use leptos::prelude::ServerFnError;
use serde::{Deserialize, Serialize};

// Re-export the device / node types so the UI can use them.
pub use crate::config::ProjectTarget;
pub use crate::config::AudioProcessingNode;

/// Read the system hostname (best-effort).
///
/// Tries `$HOSTNAME` env var first, then `/etc/hostname`, then `"unknown"`.
#[cfg(feature = "ssr")]
fn system_hostname() -> String {
    if let Ok(h) = std::env::var("HOSTNAME") {
        if !h.is_empty() {
            return h;
        }
    }
    if let Ok(h) = std::fs::read_to_string("/etc/hostname") {
        let h = h.trim().to_string();
        if !h.is_empty() {
            return h;
        }
    }
    "unknown".into()
}

/// A hardware device detected on the local host.
/// (Mirrors `hardware::HwDevice` but is always compiled for both targets.)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HwDevice {
    pub kind: String,
    pub id: String,
    pub label: String,
    pub suggested_project: String,
}

/// A remote capture node discovered via mDNS.
/// (Mirrors `discovery::MdnsNode`.)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MdnsNode {
    pub service_type: String,
    pub instance: String,
    pub host: String,
    pub hostname: String,
    pub port: u16,
    pub project_slug: String,
}

/// Detect hardware devices on the host (SDR, microphones, cameras).
#[server(prefix = "/api")]
pub async fn detect_hardware() -> Result<Vec<HwDevice>, ServerFnError> {
    let devices = crate::hardware::detect_all().await;
    Ok(devices
        .into_iter()
        .map(|d| HwDevice {
            kind: format!("{:?}", d.kind),
            id: d.id,
            label: d.label,
            suggested_project: d.suggested_project,
        })
        .collect())
}

/// Discover remote capture nodes via mDNS.
#[server(prefix = "/api")]
pub async fn discover_nodes() -> Result<Vec<MdnsNode>, ServerFnError> {
    let nodes = crate::discovery::discover_all().await;
    Ok(nodes
        .into_iter()
        .map(|n| MdnsNode {
            service_type: n.service_type,
            instance: n.instance,
            host: n.host,
            hostname: n.hostname,
            port: n.port,
            project_slug: n.project_slug,
        })
        .collect())
}

/// Toggle an individual container (capture / processing / web) within a project.
/// Persists the change to SQLite **and** starts or stops the actual container.
#[server(prefix = "/api")]
pub async fn toggle_container(
    slug: String,
    container_kind: String,
    enabled: bool,
) -> Result<Vec<ProjectTarget>, ServerFnError> {
    crate::db::set_container_enabled(&slug, &container_kind, enabled)
        .await
        .map_err(|e| ServerFnError::<server_fn::error::NoCustomError>::ServerError(e))?;

    let name = crate::containers::container_name(&slug, &container_kind);
    if enabled {
        // Start in the background -- the UI polls for status updates.
        let name_bg = name.clone();
        tokio::spawn(async move {
            if let Err(e) = crate::containers::start(&name_bg).await {
                tracing::error!("Background start of '{name_bg}' failed: {e}");
            }
        });
    } else {
        // Stop is quick enough to await (10 s timeout max).
        let _ = crate::containers::stop(&name).await;
    }

    tracing::info!(
        "Container '{container_kind}' of project '{slug}' {}",
        if enabled { "enabled" } else { "disabled" }
    );

    get_projects().await
}

/// Poll the lifecycle status of all managed containers.
///
/// Returns `(container_name, status)` pairs where status is one of:
/// `"stopped"`, `"pulling"`, `"starting"`, `"running"`, or `"error: <msg>"`.
#[server(prefix = "/api")]
pub async fn get_container_statuses() -> Result<Vec<(String, String)>, ServerFnError> {
    Ok(crate::containers::all_statuses())
}

/// Return the current list of project targets with persisted container states.
#[server(prefix = "/api")]
pub async fn get_projects() -> Result<Vec<ProjectTarget>, ServerFnError> {
    let mut targets = crate::config::default_targets();
    let states = crate::db::all_container_states()
        .await
        .map_err(|e| ServerFnError::<server_fn::error::NoCustomError>::ServerError(e))?;

    for (slug, kind, enabled) in &states {
        if let Some(t) = targets.iter_mut().find(|t| t.slug == *slug) {
            match kind.as_str() {
                "capture" => t.capture_enabled = *enabled,
                "web" => t.web_enabled = *enabled,
                "config" => t.config_enabled = *enabled,
                _ => {
                    // "processing" or "processing:{model}" -- handled below
                    // for the audio project, and as a simple flag for others.
                    if !kind.starts_with("processing") {
                        continue;
                    }
                    if slug == "audio" || slug == "light" {
                        // Handled by per-model blocks below.
                        continue;
                    }
                    t.processing_enabled = *enabled;
                }
            }
        }
    }

    // Build per-model processing nodes for the audio project.
    if let Some(audio) = targets.iter_mut().find(|t| t.slug == "audio") {
        let models = crate::config::default_audio_models();
        let model_states = crate::db::all_audio_model_states()
            .await
            .unwrap_or_default();

        for model in &models {
            // Only show models that are enabled in Settings.
            let model_enabled = model_states
                .iter()
                .find(|(s, _)| s == &model.slug)
                .map(|(_, e)| *e)
                .unwrap_or(model.enabled);

            if !model_enabled {
                continue;
            }

            // Check if this model's processing container is toggled on.
            let container_running = states
                .iter()
                .find(|(s, k, _)| s == "audio" && *k == model.container_kind)
                .map(|(_, _, e)| *e)
                .unwrap_or(false);

            audio.processing_models.push(
                crate::config::AudioProcessingNode {
                    model_slug: model.slug.clone(),
                    model_name: model.name.clone(),
                    container_kind: model.container_kind.clone(),
                    running: container_running,
                },
            );
        }

        // processing_enabled is true if ANY model node is running.
        audio.processing_enabled = audio
            .processing_models
            .iter()
            .any(|n| n.running);
    }

    // Build per-model processing nodes for the light (camera-trap) project.
    if let Some(light) = targets.iter_mut().find(|t| t.slug == "light") {
        let models = crate::config::default_light_models();
        let model_states = crate::db::all_light_model_states()
            .await
            .unwrap_or_default();

        for model in &models {
            let model_enabled = model_states
                .iter()
                .find(|(s, _)| s == &model.slug)
                .map(|(_, e)| *e)
                .unwrap_or(model.enabled);

            if !model_enabled {
                continue;
            }

            let container_running = states
                .iter()
                .find(|(s, k, _)| s == "light" && *k == model.container_kind)
                .map(|(_, _, e)| *e)
                .unwrap_or(false);

            light.processing_models.push(
                crate::config::AudioProcessingNode {
                    model_slug: model.slug.clone(),
                    model_name: model.name.clone(),
                    container_kind: model.container_kind.clone(),
                    running: container_running,
                },
            );
        }

        light.processing_enabled = light
            .processing_models
            .iter()
            .any(|n| n.running);
    }

    Ok(targets)
}

// ── Device assignment types & functions ──────────────────────────────────

/// A device-to-project assignment (shared between SSR and WASM).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DeviceAssignment {
    pub device_id: String,
    pub source: String, // "local" or "remote"
    pub project: String, // project slug or empty
}

/// Get all current device → project assignments.
#[server(prefix = "/api")]
pub async fn get_assignments() -> Result<Vec<DeviceAssignment>, ServerFnError> {
    let rows = crate::db::get_all_assignments()
        .await
        .map_err(|e| ServerFnError::<server_fn::error::NoCustomError>::ServerError(e))?;
    Ok(rows
        .into_iter()
        .map(|r| DeviceAssignment {
            device_id: r.device_id,
            source: r.source,
            project: r.project,
        })
        .collect())
}

// ── Station location ─────────────────────────────────────────────────────

/// Station location (latitude + longitude).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StationLocation {
    pub latitude: String,
    pub longitude: String,
}

/// Get the saved station location.
#[server(prefix = "/api")]
pub async fn get_location() -> Result<StationLocation, ServerFnError> {
    let lat = crate::db::get_setting("latitude")
        .await
        .map_err(|e| ServerFnError::<server_fn::error::NoCustomError>::ServerError(e))?
        .unwrap_or_default();
    let lon = crate::db::get_setting("longitude")
        .await
        .map_err(|e| ServerFnError::<server_fn::error::NoCustomError>::ServerError(e))?
        .unwrap_or_default();
    Ok(StationLocation {
        latitude: lat,
        longitude: lon,
    })
}

/// Save the station location.
#[server(prefix = "/api")]
pub async fn set_location(
    latitude: String,
    longitude: String,
) -> Result<StationLocation, ServerFnError> {
    // Basic validation: must parse as f64 and be in range (or empty to clear).
    if !latitude.is_empty() {
        let v: f64 = latitude
            .parse()
            .map_err(|_| ServerFnError::<server_fn::error::NoCustomError>::ServerError("Invalid latitude".into()))?;
        if !(-90.0..=90.0).contains(&v) {
            return Err(ServerFnError::ServerError("Latitude must be between -90 and 90".into()));
        }
    }
    if !longitude.is_empty() {
        let v: f64 = longitude
            .parse()
            .map_err(|_| ServerFnError::<server_fn::error::NoCustomError>::ServerError("Invalid longitude".into()))?;
        if !(-180.0..=180.0).contains(&v) {
            return Err(ServerFnError::ServerError("Longitude must be between -180 and 180".into()));
        }
    }

    crate::db::set_setting("latitude", &latitude)
        .await
        .map_err(|e| ServerFnError::<server_fn::error::NoCustomError>::ServerError(e))?;
    crate::db::set_setting("longitude", &longitude)
        .await
        .map_err(|e| ServerFnError::<server_fn::error::NoCustomError>::ServerError(e))?;

    tracing::info!("Station location updated: lat={latitude}, lon={longitude}");
    Ok(StationLocation {
        latitude,
        longitude,
    })
}

// ── Processing Threads ───────────────────────────────────────────────────

/// Get the configured number of parallel audio processing threads.
#[server(prefix = "/api")]
pub async fn get_processing_threads() -> Result<u32, ServerFnError> {
    let val = crate::db::get_setting("processing_threads")
        .await
        .map_err(|e| ServerFnError::<server_fn::error::NoCustomError>::ServerError(e))?
        .unwrap_or_default();
    Ok(val.parse::<u32>().unwrap_or(1).max(1))
}

/// Set the number of parallel audio processing threads.
#[server(prefix = "/api")]
pub async fn set_processing_threads(threads: u32) -> Result<u32, ServerFnError> {
    let threads = threads.max(1).min(8);
    crate::db::set_setting("processing_threads", &threads.to_string())
        .await
        .map_err(|e| ServerFnError::<server_fn::error::NoCustomError>::ServerError(e))?;
    tracing::info!("Processing threads updated to {threads}");
    Ok(threads)
}

// ── Node Name ────────────────────────────────────────────────────────────

/// Get the configured friendly node name.
/// Falls back to the system hostname when no name has been set.
#[server(prefix = "/api")]
pub async fn get_node_name() -> Result<String, ServerFnError> {
    let val = crate::db::get_setting("node_name")
        .await
        .map_err(|e| ServerFnError::<server_fn::error::NoCustomError>::ServerError(e))?;
    match val {
        Some(v) if !v.is_empty() => Ok(v),
        _ => {
            // Fall back to system hostname.
            Ok(system_hostname())
        }
    }
}

/// Set the friendly node name.
#[server(prefix = "/api")]
pub async fn set_node_name(name: String) -> Result<String, ServerFnError> {
    let trimmed = name.trim().to_string();
    crate::db::set_setting("node_name", &trimmed)
        .await
        .map_err(|e| ServerFnError::<server_fn::error::NoCustomError>::ServerError(e))?;
    tracing::info!("Node name updated to '{trimmed}'");
    Ok(trimmed)
}

/// Assign a device to a project (or "none" to un-assign).
#[server(prefix = "/api")]
pub async fn assign_device(
    device_id: String,
    source: String,
    project: String,
) -> Result<Vec<DeviceAssignment>, ServerFnError> {
    crate::db::set_assignment(&device_id, &source, &project)
        .await
        .map_err(|e| ServerFnError::<server_fn::error::NoCustomError>::ServerError(e))?;
    get_assignments().await
}

// ── GMN configuration ────────────────────────────────────────────────────

/// Configuration state for the Global Meteor Network project.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GmnConfig {
    /// Station callsign (e.g. "US000A").
    pub callsign: String,
    /// Device path of the camera assigned to GMN, if any.
    pub camera_device: Option<String>,
    /// Human-readable camera label, if available.
    pub camera_label: Option<String>,
    /// Whether the config container (camera stream) is running.
    pub config_enabled: bool,
    /// Port the config (camera stream) container listens on.
    pub config_port: u16,
}

/// Load the current GMN configuration (callsign + assigned camera).
#[server(prefix = "/api")]
pub async fn get_gmn_config() -> Result<GmnConfig, ServerFnError> {
    let callsign = crate::db::get_setting("gmn_callsign")
        .await
        .map_err(|e| ServerFnError::<server_fn::error::NoCustomError>::ServerError(e))?
        .unwrap_or_default();

    // Look up camera device assigned to GMN.
    let assignments = crate::db::get_all_assignments()
        .await
        .map_err(|e| ServerFnError::<server_fn::error::NoCustomError>::ServerError(e))?;

    let camera = assignments.iter().find(|a| a.project == "gmn");

    // Try to get the human-readable label from the hardware detector.
    let camera_label = if let Some(cam) = &camera {
        let hw = crate::hardware::detect_cameras().await;
        hw.into_iter()
            .find(|d| d.id == cam.device_id)
            .map(|d| d.label)
    } else {
        None
    };

    Ok(GmnConfig {
        callsign,
        camera_device: camera.map(|c| c.device_id.clone()),
        camera_label,
        config_enabled: crate::db::get_container_enabled("gmn", "config")
            .await
            .unwrap_or(None)
            .unwrap_or(false),
        config_port: 8181,
    })
}

/// Save the GMN station callsign.
#[server(prefix = "/api")]
pub async fn set_gmn_callsign(callsign: String) -> Result<String, ServerFnError> {
    let trimmed = callsign.trim().to_string();
    crate::db::set_setting("gmn_callsign", &trimmed)
        .await
        .map_err(|e| ServerFnError::<server_fn::error::NoCustomError>::ServerError(e))?;
    tracing::info!("GMN callsign set to: {trimmed}");
    Ok(trimmed)
}

// ── Audio model management ───────────────────────────────────────────────

/// An audio model with its current enabled state (shared type for SSR + WASM).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AudioModelInfo {
    pub slug: String,
    pub name: String,
    pub description: String,
    pub enabled: bool,
}

/// Return all known audio models with their persisted enabled state.
#[server(prefix = "/api")]
pub async fn get_audio_models() -> Result<Vec<AudioModelInfo>, ServerFnError> {
    let models = crate::config::default_audio_models();
    let db_states = crate::db::all_audio_model_states()
        .await
        .map_err(|e| ServerFnError::<server_fn::error::NoCustomError>::ServerError(e))?;

    Ok(models
        .into_iter()
        .map(|m| {
            let enabled = db_states
                .iter()
                .find(|(s, _)| s == &m.slug)
                .map(|(_, e)| *e)
                .unwrap_or(m.enabled);
            AudioModelInfo {
                slug: m.slug,
                name: m.name,
                description: m.description,
                enabled,
            }
        })
        .collect())
}

/// Enable or disable an audio model in Settings.
///
/// When a model is disabled, its processing container is also stopped.
#[server(prefix = "/api")]
pub async fn toggle_audio_model(
    slug: String,
    enabled: bool,
) -> Result<Vec<AudioModelInfo>, ServerFnError> {
    crate::db::set_audio_model_enabled(&slug, enabled)
        .await
        .map_err(|e| ServerFnError::<server_fn::error::NoCustomError>::ServerError(e))?;

    if !enabled {
        // Stop the processing container for this model.
        let kind = crate::config::model_container_kind(&slug);
        let name = crate::containers::container_name("audio", &kind);
        crate::db::set_container_enabled("audio", &kind, false)
            .await
            .ok();
        if let Err(e) = crate::containers::stop(&name).await {
            tracing::error!("Failed to stop container '{name}' for audio model '{slug}': {e}");
        }
        tracing::info!("Disabled audio model '{slug}' and stopped container '{name}'");
    } else {
        tracing::info!("Enabled audio model '{slug}'");
    }

    get_audio_models().await
}

/// Toggle a specific audio processing-model container (start / stop).
///
/// Called from the project card per-model processing toggles.
#[server(prefix = "/api")]
pub async fn toggle_audio_processing(
    model_slug: String,
    enabled: bool,
) -> Result<Vec<ProjectTarget>, ServerFnError> {
    let kind = crate::config::model_container_kind(&model_slug);
    crate::db::set_container_enabled("audio", &kind, enabled)
        .await
        .map_err(|e| ServerFnError::<server_fn::error::NoCustomError>::ServerError(e))?;

    let name = crate::containers::container_name("audio", &kind);
    if enabled {
        let name_bg = name.clone();
        tokio::spawn(async move {
            if let Err(e) = crate::containers::start(&name_bg).await {
                tracing::error!("Background start of '{name_bg}' failed: {e}");
            }
        });
    } else {
        if let Err(e) = crate::containers::stop(&name).await {
            tracing::error!("Failed to stop container '{name}': {e}");
        }
    }

    tracing::info!(
        "Audio processing model '{model_slug}' {}",
        if enabled { "enabled" } else { "disabled" }
    );

    get_projects().await
}

// ── Debug logging ────────────────────────────────────────────────────────

/// Debug logging state for a single project.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DebugState {
    pub slug: String,
    pub name: String,
    pub enabled: bool,
}

/// Return the debug-logging toggle state for every project.
#[server(prefix = "/api")]
pub async fn get_debug_settings() -> Result<Vec<DebugState>, ServerFnError> {
    let states = crate::db::all_debug_states()
        .await
        .map_err(|e| ServerFnError::<server_fn::error::NoCustomError>::ServerError(e))?;

    let names: std::collections::HashMap<&str, &str> = [
        ("audio", "Gaia Audio"),
        ("radio", "Gaia Radio"),
        ("gmn", "Global Meteor Network"),
        ("light", "Gaia Light"),
    ]
    .into_iter()
    .collect();

    Ok(states
        .into_iter()
        .map(|(slug, enabled)| DebugState {
            name: names.get(slug.as_str()).unwrap_or(&slug.as_str()).to_string(),
            slug,
            enabled,
        })
        .collect())
}

/// Toggle debug logging for a project.
///
/// The change takes effect the next time a container in this project is
/// (re)started — running containers are not affected until restart.
#[server(prefix = "/api")]
pub async fn toggle_debug_logging(
    slug: String,
    enabled: bool,
) -> Result<Vec<DebugState>, ServerFnError> {
    crate::db::set_debug_enabled(&slug, enabled)
        .await
        .map_err(|e| ServerFnError::<server_fn::error::NoCustomError>::ServerError(e))?;

    tracing::info!(
        "Debug logging for project '{slug}' {}",
        if enabled { "enabled" } else { "disabled" }
    );

    get_debug_settings().await
}

/// Return the number of currently-active audio processing nodes.
#[server(prefix = "/api")]
pub async fn get_active_processing_node_count() -> Result<usize, ServerFnError> {
    crate::db::active_audio_model_count()
        .await
        .map_err(|e| ServerFnError::<server_fn::error::NoCustomError>::ServerError(e))
}

// ── Light model management ───────────────────────────────────────────────

/// A light (camera-trap) model with its current enabled state.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LightModelInfo {
    pub slug: String,
    pub name: String,
    pub description: String,
    pub enabled: bool,
}

/// Return all known light models with their persisted enabled state.
#[server(prefix = "/api")]
pub async fn get_light_models() -> Result<Vec<LightModelInfo>, ServerFnError> {
    let models = crate::config::default_light_models();
    let db_states = crate::db::all_light_model_states()
        .await
        .map_err(|e| ServerFnError::<server_fn::error::NoCustomError>::ServerError(e))?;

    Ok(models
        .into_iter()
        .map(|m| {
            let enabled = db_states
                .iter()
                .find(|(s, _)| s == &m.slug)
                .map(|(_, e)| *e)
                .unwrap_or(m.enabled);
            LightModelInfo {
                slug: m.slug,
                name: m.name,
                description: m.description,
                enabled,
            }
        })
        .collect())
}

/// Enable or disable a light model in Settings.
///
/// When a model is disabled, its processing container is also stopped.
#[server(prefix = "/api")]
pub async fn toggle_light_model(
    slug: String,
    enabled: bool,
) -> Result<Vec<LightModelInfo>, ServerFnError> {
    crate::db::set_light_model_enabled(&slug, enabled)
        .await
        .map_err(|e| ServerFnError::<server_fn::error::NoCustomError>::ServerError(e))?;

    if !enabled {
        let kind = crate::config::light_model_container_kind(&slug);
        let name = crate::containers::container_name("light", &kind);
        crate::db::set_container_enabled("light", &kind, false)
            .await
            .ok();
        if let Err(e) = crate::containers::stop(&name).await {
            tracing::error!("Failed to stop container '{name}' for light model '{slug}': {e}");
        }
        tracing::info!("Disabled light model '{slug}' and stopped container '{name}'");
    } else {
        tracing::info!("Enabled light model '{slug}'");
    }

    get_light_models().await
}

/// Toggle a specific light processing-model container (start / stop).
#[server(prefix = "/api")]
pub async fn toggle_light_processing(
    model_slug: String,
    enabled: bool,
) -> Result<Vec<ProjectTarget>, ServerFnError> {
    let kind = crate::config::light_model_container_kind(&model_slug);
    crate::db::set_container_enabled("light", &kind, enabled)
        .await
        .map_err(|e| ServerFnError::<server_fn::error::NoCustomError>::ServerError(e))?;

    let name = crate::containers::container_name("light", &kind);
    if enabled {
        let name_bg = name.clone();
        tokio::spawn(async move {
            if let Err(e) = crate::containers::start(&name_bg).await {
                tracing::error!("Background start of '{name_bg}' failed: {e}");
            }
        });
    } else {
        if let Err(e) = crate::containers::stop(&name).await {
            tracing::error!("Failed to stop container '{name}': {e}");
        }
    }

    tracing::info!(
        "Light processing model '{model_slug}' {}",
        if enabled { "enabled" } else { "disabled" }
    );

    get_projects().await
}

// ── Recording analysis tracking ──────────────────────────────────────────

/// Register a new recording for analysis by all currently-enabled models.
///
/// Returns the number of models that need to analyse it.
#[server(prefix = "/api")]
pub async fn register_recording(recording: String) -> Result<usize, ServerFnError> {
    crate::db::register_recording(&recording)
        .await
        .map_err(|e| ServerFnError::<server_fn::error::NoCustomError>::ServerError(e))
}

/// Mark a recording as analysed by a specific model.
#[server(prefix = "/api")]
pub async fn mark_recording_analyzed(
    recording: String,
    model_slug: String,
) -> Result<bool, ServerFnError> {
    crate::db::mark_recording_analyzed(&recording, &model_slug)
        .await
        .map_err(|e| ServerFnError::<server_fn::error::NoCustomError>::ServerError(e))?;

    // Return whether the recording is now fully analysed.
    crate::db::is_recording_fully_analyzed(&recording)
        .await
        .map_err(|e| ServerFnError::<server_fn::error::NoCustomError>::ServerError(e))
}

/// Return the list of recordings that have been fully analysed by all
/// models and can safely be deleted.
#[server(prefix = "/api")]
pub async fn fully_analyzed_recordings() -> Result<Vec<String>, ServerFnError> {
    crate::db::fully_analyzed_recordings()
        .await
        .map_err(|e| ServerFnError::<server_fn::error::NoCustomError>::ServerError(e))
}

/// Remove tracking rows for a recording after the file has been deleted.
#[server(prefix = "/api")]
pub async fn remove_recording_tracking(recording: String) -> Result<(), ServerFnError> {
    crate::db::remove_recording_tracking(&recording)
        .await
        .map_err(|e| ServerFnError::<server_fn::error::NoCustomError>::ServerError(e))
}

// ── Capture health (disk guard) ─────────────────────────────────────────

/// Capture health information reported by a capture container.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CaptureHealth {
    /// Project slug, e.g. "audio".
    pub slug: String,
    /// Current disk usage percentage (0–100).
    pub disk_usage_pct: f64,
    /// `true` when capture is paused due to disk pressure.
    pub capture_paused: bool,
    /// Camera mode: `Some("day")` or `Some("night")` when the capture
    /// server reports brightness info, `None` otherwise.
    pub camera_mode: Option<String>,
}

/// Poll the `/api/health` endpoint of all capture containers that expose
/// an HTTP API.  Returns one [`CaptureHealth`] per reachable container.
///
/// Containers with `capture_port == 0` or that are unreachable are silently
/// skipped.
#[server(prefix = "/api")]
pub async fn get_capture_health() -> Result<Vec<CaptureHealth>, ServerFnError> {
    let targets = crate::config::default_targets();
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(2))
        .build()
        .unwrap_or_default();

    let mut results = Vec::new();

    for t in &targets {
        if t.capture_port == 0 {
            continue;
        }
        let url = format!("http://localhost:{}/api/health", t.capture_port);
        match client.get(&url).send().await {
            Ok(resp) if resp.status().is_success() => {
                // Parse as a generic JSON value so we tolerate older capture
                // images that don't include the disk fields yet.
                if let Ok(body) = resp.json::<serde_json::Value>().await {
                    let disk_usage_pct = body
                        .get("disk_usage_pct")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0);
                    let capture_paused = body
                        .get("capture_paused")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    // Try to fetch camera brightness info from the
                    // capture server's /api/camera endpoint.
                    let camera_mode = match client
                        .get(format!(
                            "http://localhost:{}/api/camera",
                            t.capture_port
                        ))
                        .send()
                        .await
                    {
                        Ok(r) if r.status().is_success() => r
                            .json::<serde_json::Value>()
                            .await
                            .ok()
                            .and_then(|v| {
                                let is_dark = v.get("is_dark")?.as_bool()?;
                                Some(
                                    if is_dark { "night" } else { "day" }.to_string(),
                                )
                            }),
                        _ => None,
                    };

                    results.push(CaptureHealth {
                        slug: t.slug.clone(),
                        disk_usage_pct,
                        capture_paused,
                        camera_mode,
                    });
                }
            }
            _ => {
                // Container unreachable — skip.
            }
        }
    }

    Ok(results)
}

// ── Container image updates ──────────────────────────────────────────────

/// Update status for a single container image (shared SSR + WASM type).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ImageUpdate {
    pub container: String,
    pub image: String,
    pub has_update: bool,
    pub last_checked: String,
}

/// Get the cached update status for all container images.
///
/// Returns one entry per container.  `has_update` is `true` when the
/// remote Docker Hub digest differs from the locally-pulled one.
#[server(prefix = "/api")]
pub async fn get_update_status() -> Result<Vec<ImageUpdate>, ServerFnError> {
    let statuses = crate::updates::all_update_statuses().await;
    Ok(statuses
        .into_iter()
        .map(|s| ImageUpdate {
            container: s.container,
            image: s.image,
            has_update: s.has_update,
            last_checked: s.last_checked,
        })
        .collect())
}

/// Get the number of images that have updates available.
#[server(prefix = "/api")]
pub async fn get_update_count() -> Result<usize, ServerFnError> {
    Ok(crate::updates::update_count().await)
}

/// Manually trigger an update check (for development / impatient users).
///
/// Returns the fresh status list.
#[server(prefix = "/api")]
pub async fn check_for_updates() -> Result<Vec<ImageUpdate>, ServerFnError> {
    let statuses = crate::updates::check_all().await;
    Ok(statuses
        .into_iter()
        .map(|s| ImageUpdate {
            container: s.container,
            image: s.image,
            has_update: s.has_update,
            last_checked: s.last_checked,
        })
        .collect())
}

/// Get the update check interval in hours.
#[server(prefix = "/api")]
pub async fn get_update_check_interval() -> Result<u64, ServerFnError> {
    let val = crate::db::get_setting("update_check_interval")
        .await
        .map_err(|e| ServerFnError::<server_fn::error::NoCustomError>::ServerError(e))?
        .unwrap_or_default();
    Ok(val.parse::<u64>().unwrap_or(24).max(1))
}

/// Set the update check interval in hours.
#[server(prefix = "/api")]
pub async fn set_update_check_interval(hours: u64) -> Result<u64, ServerFnError> {
    let hours = hours.max(1).min(168); // 1h to 1 week
    crate::db::set_setting("update_check_interval", &hours.to_string())
        .await
        .map_err(|e| ServerFnError::<server_fn::error::NoCustomError>::ServerError(e))?;
    tracing::info!("Update check interval set to {hours}h");
    Ok(hours)
}
