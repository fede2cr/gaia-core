//! Leptos server functions called from UI components, executed on the server.

use leptos::*;
use serde::{Deserialize, Serialize};

// Re-export the device / node types so the UI can use them.
pub use crate::config::ProjectTarget;
pub use crate::config::AudioProcessingNode;

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
    pub port: u16,
    pub project_slug: String,
}

/// Detect hardware devices on the host (SDR, microphones, cameras).
#[server(DetectHardware, "/api")]
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
#[server(DiscoverNodes, "/api")]
pub async fn discover_nodes() -> Result<Vec<MdnsNode>, ServerFnError> {
    let nodes = crate::discovery::discover_all().await;
    Ok(nodes
        .into_iter()
        .map(|n| MdnsNode {
            service_type: n.service_type,
            instance: n.instance,
            host: n.host,
            port: n.port,
            project_slug: n.project_slug,
        })
        .collect())
}

/// Toggle an individual container (capture / processing / web) within a project.
/// Persists the change to SQLite **and** starts or stops the actual container.
#[server(ToggleContainer, "/api")]
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
        // Start in the background — the UI polls for status updates.
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
#[server(GetContainerStatuses, "/api")]
pub async fn get_container_statuses() -> Result<Vec<(String, String)>, ServerFnError> {
    Ok(crate::containers::all_statuses())
}

/// Return the current list of project targets with persisted container states.
#[server(GetProjects, "/api")]
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
                    // "processing" or "processing:{model}" — handled below
                    // for the audio project, and as a simple flag for others.
                    if !kind.starts_with("processing") {
                        continue;
                    }
                    if slug != "audio" {
                        t.processing_enabled = *enabled;
                    }
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
#[server(GetAssignments, "/api")]
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
#[server(GetLocation, "/api")]
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
#[server(SetLocation, "/api")]
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

/// Assign a device to a project (or "none" to un-assign).
#[server(AssignDevice, "/api")]
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
#[server(GetGmnConfig, "/api")]
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
#[server(SetGmnCallsign, "/api")]
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
#[server(GetAudioModels, "/api")]
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
#[server(ToggleAudioModel, "/api")]
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
        let _ = crate::containers::stop(&name).await;
        tracing::info!("Disabled audio model '{slug}' and stopped container '{name}'");
    } else {
        tracing::info!("Enabled audio model '{slug}'");
    }

    get_audio_models().await
}

/// Toggle a specific audio processing-model container (start / stop).
///
/// Called from the project card per-model processing toggles.
#[server(ToggleAudioProcessing, "/api")]
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
        let _ = crate::containers::stop(&name).await;
    }

    tracing::info!(
        "Audio processing model '{model_slug}' {}",
        if enabled { "enabled" } else { "disabled" }
    );

    get_projects().await
}

/// Return the number of currently-active audio processing nodes.
#[server(GetActiveProcessingNodeCount, "/api")]
pub async fn get_active_processing_node_count() -> Result<usize, ServerFnError> {
    crate::db::active_audio_model_count()
        .await
        .map_err(|e| ServerFnError::<server_fn::error::NoCustomError>::ServerError(e))
}

// ── Recording analysis tracking ──────────────────────────────────────────

/// Register a new recording for analysis by all currently-enabled models.
///
/// Returns the number of models that need to analyse it.
#[server(RegisterRecording, "/api")]
pub async fn register_recording(recording: String) -> Result<usize, ServerFnError> {
    crate::db::register_recording(&recording)
        .await
        .map_err(|e| ServerFnError::<server_fn::error::NoCustomError>::ServerError(e))
}

/// Mark a recording as analysed by a specific model.
#[server(MarkRecordingAnalyzed, "/api")]
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
#[server(FullyAnalyzedRecordings, "/api")]
pub async fn fully_analyzed_recordings() -> Result<Vec<String>, ServerFnError> {
    crate::db::fully_analyzed_recordings()
        .await
        .map_err(|e| ServerFnError::<server_fn::error::NoCustomError>::ServerError(e))
}

/// Remove tracking rows for a recording after the file has been deleted.
#[server(RemoveRecordingTracking, "/api")]
pub async fn remove_recording_tracking(recording: String) -> Result<(), ServerFnError> {
    crate::db::remove_recording_tracking(&recording)
        .await
        .map_err(|e| ServerFnError::<server_fn::error::NoCustomError>::ServerError(e))
}
