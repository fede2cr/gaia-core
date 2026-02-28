//! Leptos server functions called from UI components, executed on the server.

use leptos::*;
use serde::{Deserialize, Serialize};

// Re-export the device / node types so the UI can use them.
pub use crate::config::ProjectTarget;

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

    // Actually start or stop the container via the runtime CLI.
    let name = crate::containers::container_name(&slug, &container_kind);
    if enabled {
        crate::containers::start(&name)
            .await
            .map_err(|e| ServerFnError::<server_fn::error::NoCustomError>::ServerError(e))?;
    } else {
        // Best-effort stop: don't fail the toggle if the container is
        // already stopped or doesn't exist yet.
        let _ = crate::containers::stop(&name).await;
    }

    tracing::info!(
        "Container '{container_kind}' of project '{slug}' {}",
        if enabled { "enabled" } else { "disabled" }
    );

    get_projects().await
}

/// Return the current list of project targets with persisted container states.
#[server(GetProjects, "/api")]
pub async fn get_projects() -> Result<Vec<ProjectTarget>, ServerFnError> {
    let mut targets = crate::config::default_targets();
    let states = crate::db::all_container_states()
        .await
        .map_err(|e| ServerFnError::<server_fn::error::NoCustomError>::ServerError(e))?;

    for (slug, kind, enabled) in states {
        if let Some(t) = targets.iter_mut().find(|t| t.slug == slug) {
            match kind.as_str() {
                "capture" => t.capture_enabled = enabled,
                "processing" => t.processing_enabled = enabled,
                "web" => t.web_enabled = enabled,
                "config" => t.config_enabled = enabled,
                _ => {}
            }
        }
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
