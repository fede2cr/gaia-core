//! Persistent device → project assignments.
//!
//! Assignments are stored as a JSON file on disk so they survive restarts.
//! The file path defaults to `$GAIA_CONFIG_DIR/assignments.json` or
//! `./data/assignments.json` when the env var is not set.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::LazyLock;
use tokio::sync::RwLock;

/// In-memory cache of the current assignments (populated from disk on first access).
static ASSIGNMENTS: LazyLock<RwLock<DeviceAssignments>> =
    LazyLock::new(|| RwLock::new(DeviceAssignments::default()));

/// Whether the cache has been initialised from disk.
static INIT: LazyLock<RwLock<bool>> = LazyLock::new(|| RwLock::new(false));

// ── Data model ───────────────────────────────────────────────────────────

/// A single device or mDNS node assignment.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Assignment {
    /// The device id (e.g. "hw:2,0", "/dev/video0", "rtlsdr:0") or mDNS
    /// instance name (e.g. "gaia-radio-capture-01").
    pub device_id: String,
    /// Whether this is a local device or a remote mDNS node.
    pub source: AssignmentSource,
    /// The project slug this device is assigned to ("audio", "radio", "gmn"),
    /// or empty string / "none" if unassigned.
    pub project: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AssignmentSource {
    Local,
    Remote,
}

/// Top-level assignments file.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DeviceAssignments {
    /// device_id → Assignment
    pub devices: HashMap<String, Assignment>,
}

// ── Public API ───────────────────────────────────────────────────────────

/// Return the path to the assignments JSON file.
fn config_path() -> PathBuf {
    let dir = std::env::var("GAIA_CONFIG_DIR").unwrap_or_else(|_| "./data".into());
    PathBuf::from(dir).join("assignments.json")
}

/// Ensure the in-memory cache is populated from disk (idempotent).
async fn ensure_loaded() {
    let mut init = INIT.write().await;
    if *init {
        return;
    }
    let path = config_path();
    if path.exists() {
        match tokio::fs::read_to_string(&path).await {
            Ok(json) => {
                if let Ok(parsed) = serde_json::from_str::<DeviceAssignments>(&json) {
                    *ASSIGNMENTS.write().await = parsed;
                    tracing::info!("Loaded {} device assignment(s) from {}", 
                        ASSIGNMENTS.read().await.devices.len(),
                        path.display()
                    );
                }
            }
            Err(e) => tracing::warn!("Could not read {}: {e}", path.display()),
        }
    }
    *init = true;
}

/// Persist the current in-memory assignments to disk.
async fn save() -> Result<(), String> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| format!("mkdir {}: {e}", parent.display()))?;
    }
    let data = ASSIGNMENTS.read().await;
    let json = serde_json::to_string_pretty(&*data)
        .map_err(|e| format!("serialize: {e}"))?;
    tokio::fs::write(&path, json)
        .await
        .map_err(|e| format!("write {}: {e}", path.display()))?;
    Ok(())
}

/// Get all current assignments.
pub async fn get_all() -> Vec<Assignment> {
    ensure_loaded().await;
    ASSIGNMENTS.read().await.devices.values().cloned().collect()
}

/// Assign (or re-assign) a device to a project.
///
/// Pass `project = "none"` or `project = ""` to un-assign.
pub async fn assign(
    device_id: String,
    source: AssignmentSource,
    project: String,
) -> Result<Vec<Assignment>, String> {
    ensure_loaded().await;

    let project = if project == "none" { String::new() } else { project };

    {
        let mut data = ASSIGNMENTS.write().await;
        if project.is_empty() {
            data.devices.remove(&device_id);
        } else {
            data.devices.insert(
                device_id.clone(),
                Assignment {
                    device_id,
                    source,
                    project,
                },
            );
        }
    }

    save().await?;

    Ok(get_all().await)
}
