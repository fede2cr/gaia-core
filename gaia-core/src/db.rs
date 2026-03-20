//! SQLite persistence layer for Gaia Core (via libsql / Turso).
//!
//! Stores container on/off state and device-to-project assignments in a
//! single SQLite database.  The DB file lives at
//! `$GAIA_CONFIG_DIR/gaia-core.db` (defaults to `./data/gaia-core.db`).
//!
//! A global connection is created once via [`init`] and accessed through
//! the module-level helper functions.  All calls go through a
//! `tokio::sync::Mutex<Connection>` so they are safe to call from async
//! server functions.

use libsql::params;
use std::path::PathBuf;
use std::sync::LazyLock;
use tokio::sync::Mutex;

/// Global database connection.
static DB: LazyLock<Mutex<Option<libsql::Connection>>> = LazyLock::new(|| Mutex::new(None));

/// Keep the `Database` handle alive for the program lifetime so the
/// connection returned by `db.connect()` stays valid.
static DB_HANDLE: LazyLock<Mutex<Option<libsql::Database>>> = LazyLock::new(|| Mutex::new(None));

// ── Initialisation ───────────────────────────────────────────────────────

/// Open (or create) the database and run migrations.
///
/// **Must** be called once at server startup (before any server functions).
pub async fn init() {
    let path = db_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).ok();
    }

    let db = libsql::Builder::new_local(
        path.to_str().expect("Non-UTF-8 database path"),
    )
    .build()
    .await
    .expect("failed to open SQLite database");

    let conn = db.connect().expect("failed to connect to database");
    conn.execute_batch("PRAGMA journal_mode = WAL;")
        .await
        .ok();

    // ── Migrations ───────────────────────────────────────────────────
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS container_state (
            slug           TEXT NOT NULL,
            container_kind TEXT NOT NULL,
            enabled        INTEGER NOT NULL DEFAULT 1,
            PRIMARY KEY (slug, container_kind)
        );

        CREATE TABLE IF NOT EXISTS device_assignments (
            device_id TEXT PRIMARY KEY,
            source    TEXT NOT NULL DEFAULT 'local',
            project   TEXT NOT NULL DEFAULT ''
        );

        CREATE TABLE IF NOT EXISTS settings (
            key   TEXT PRIMARY KEY,
            value TEXT NOT NULL DEFAULT ''
        );

        CREATE TABLE IF NOT EXISTS audio_models (
            slug    TEXT PRIMARY KEY,
            enabled INTEGER NOT NULL DEFAULT 0
        );

        CREATE TABLE IF NOT EXISTS light_models (
            slug    TEXT PRIMARY KEY,
            enabled INTEGER NOT NULL DEFAULT 0
        );

        CREATE TABLE IF NOT EXISTS recording_analysis (
            recording  TEXT NOT NULL,
            model_slug TEXT NOT NULL,
            completed  INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY (recording, model_slug)
        );
        ",
    )
    .await
    .expect("failed to run DB migrations");

    tracing::info!("SQLite database opened at {}", path.display());

    // Store both the Database handle (keeps SQLite alive) and the Connection.
    *DB_HANDLE.lock().await = Some(db);
    *DB.lock().await = Some(conn);
}

fn db_path() -> PathBuf {
    let dir = std::env::var("GAIA_CONFIG_DIR").unwrap_or_else(|_| "./data".into());
    PathBuf::from(dir).join("gaia-core.db")
}

// ── Container state ──────────────────────────────────────────────────────

/// Persist a container toggle.
pub async fn set_container_enabled(
    slug: &str,
    container_kind: &str,
    enabled: bool,
) -> Result<(), String> {
    let guard = DB.lock().await;
    let conn = guard.as_ref().ok_or("DB not initialised")?;
    conn.execute(
        "INSERT INTO container_state (slug, container_kind, enabled)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(slug, container_kind) DO UPDATE SET enabled = excluded.enabled",
        params![slug.to_string(), container_kind.to_string(), enabled as i32],
    )
    .await
    .map_err(|e| format!("DB set_container_enabled: {e}"))?;
    Ok(())
}

/// Load the saved enabled state for a (slug, container_kind) pair.
/// Returns `None` if the pair has never been persisted (caller should
/// fall back to the compile-time default).
pub async fn get_container_enabled(
    slug: &str,
    container_kind: &str,
) -> Result<Option<bool>, String> {
    let guard = DB.lock().await;
    let conn = guard.as_ref().ok_or("DB not initialised")?;
    let mut rows = conn
        .query(
            "SELECT enabled FROM container_state WHERE slug = ?1 AND container_kind = ?2",
            params![slug.to_string(), container_kind.to_string()],
        )
        .await
        .map_err(|e| format!("DB query: {e}"))?;

    match rows.next().await {
        Ok(Some(row)) => {
            let v: i32 = row.get(0).map_err(|e| format!("DB row: {e}"))?;
            Ok(Some(v != 0))
        }
        _ => Ok(None),
    }
}

/// Load *all* saved container states into a flat vec.
pub async fn all_container_states() -> Result<Vec<(String, String, bool)>, String> {
    let guard = DB.lock().await;
    let conn = guard.as_ref().ok_or("DB not initialised")?;
    let mut rows = conn
        .query(
            "SELECT slug, container_kind, enabled FROM container_state",
            (),
        )
        .await
        .map_err(|e| format!("DB query: {e}"))?;

    let mut out = Vec::new();
    while let Some(row) = rows.next().await.map_err(|e| format!("DB row: {e}"))? {
        out.push((
            row.get::<String>(0).map_err(|e| format!("DB row: {e}"))?,
            row.get::<String>(1).map_err(|e| format!("DB row: {e}"))?,
            row.get::<i32>(2).map_err(|e| format!("DB row: {e}"))? != 0,
        ));
    }
    Ok(out)
}

// ── Device assignments ───────────────────────────────────────────────────

/// A device-to-project assignment row.
#[derive(Clone, Debug)]
pub struct AssignmentRow {
    pub device_id: String,
    pub source: String,
    pub project: String,
}

/// Get all saved assignments.
pub async fn get_all_assignments() -> Result<Vec<AssignmentRow>, String> {
    let guard = DB.lock().await;
    let conn = guard.as_ref().ok_or("DB not initialised")?;
    let mut rows = conn
        .query(
            "SELECT device_id, source, project FROM device_assignments",
            (),
        )
        .await
        .map_err(|e| format!("DB query: {e}"))?;

    let mut out = Vec::new();
    while let Some(row) = rows.next().await.map_err(|e| format!("DB row: {e}"))? {
        out.push(AssignmentRow {
            device_id: row.get(0).map_err(|e| format!("DB row: {e}"))?,
            source: row.get(1).map_err(|e| format!("DB row: {e}"))?,
            project: row.get(2).map_err(|e| format!("DB row: {e}"))?,
        });
    }
    Ok(out)
}

/// Assign a device to a project.  Pass `project = ""` or `"none"` to
/// un-assign (deletes the row).
pub async fn set_assignment(
    device_id: &str,
    source: &str,
    project: &str,
) -> Result<(), String> {
    let guard = DB.lock().await;
    let conn = guard.as_ref().ok_or("DB not initialised")?;

    let project = if project == "none" { "" } else { project };

    if project.is_empty() {
        conn.execute(
            "DELETE FROM device_assignments WHERE device_id = ?1",
            params![device_id.to_string()],
        )
        .await
        .map_err(|e| format!("DB delete assignment: {e}"))?;
    } else {
        conn.execute(
            "INSERT INTO device_assignments (device_id, source, project)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(device_id) DO UPDATE SET source = excluded.source, project = excluded.project",
            params![device_id.to_string(), source.to_string(), project.to_string()],
        )
        .await
        .map_err(|e| format!("DB set_assignment: {e}"))?;
    }
    Ok(())
}

// ── Settings (key-value) ──────────────────────────────────────────────

/// Read a setting by key.  Returns `None` if the key has never been stored.
pub async fn get_setting(key: &str) -> Result<Option<String>, String> {
    let guard = DB.lock().await;
    let conn = guard.as_ref().ok_or("DB not initialised")?;
    let mut rows = conn
        .query(
            "SELECT value FROM settings WHERE key = ?1",
            params![key.to_string()],
        )
        .await
        .map_err(|e| format!("DB query: {e}"))?;

    match rows.next().await {
        Ok(Some(row)) => Ok(Some(row.get(0).map_err(|e| format!("DB row: {e}"))?)),
        _ => Ok(None),
    }
}

/// Write a setting (upsert).
pub async fn set_setting(key: &str, value: &str) -> Result<(), String> {
    let guard = DB.lock().await;
    let conn = guard.as_ref().ok_or("DB not initialised")?;
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key.to_string(), value.to_string()],
    )
    .await
    .map_err(|e| format!("DB set_setting: {e}"))?;
    Ok(())
}

// ── Debug logging ────────────────────────────────────────────────────────

/// Check whether debug logging is enabled for the given project slug.
pub async fn is_debug_enabled(slug: &str) -> bool {
    get_setting(&format!("debug:{slug}"))
        .await
        .ok()
        .flatten()
        .map(|v| v == "1")
        .unwrap_or(false)
}

/// Set the debug logging flag for a project.
pub async fn set_debug_enabled(slug: &str, enabled: bool) -> Result<(), String> {
    set_setting(&format!("debug:{slug}"), if enabled { "1" } else { "0" }).await
}

/// Return the debug-logging state for every known project slug.
pub async fn all_debug_states() -> Result<Vec<(String, bool)>, String> {
    let slugs = ["audio", "radio", "gmn", "light"];
    let mut out = Vec::with_capacity(slugs.len());
    for slug in &slugs {
        out.push((slug.to_string(), is_debug_enabled(slug).await));
    }
    Ok(out)
}

// ── Audio model state ─────────────────────────────────────────────────────

/// Persist the enabled state of an audio model.
pub async fn set_audio_model_enabled(slug: &str, enabled: bool) -> Result<(), String> {
    let guard = DB.lock().await;
    let conn = guard.as_ref().ok_or("DB not initialised")?;
    conn.execute(
        "INSERT INTO audio_models (slug, enabled)
         VALUES (?1, ?2)
         ON CONFLICT(slug) DO UPDATE SET enabled = excluded.enabled",
        params![slug.to_string(), enabled as i32],
    )
    .await
    .map_err(|e| format!("DB set_audio_model_enabled: {e}"))?;
    Ok(())
}

/// Load the enabled state for an audio model.
/// Returns `None` if the slug has never been persisted.
pub async fn get_audio_model_enabled(slug: &str) -> Result<Option<bool>, String> {
    let guard = DB.lock().await;
    let conn = guard.as_ref().ok_or("DB not initialised")?;
    let mut rows = conn
        .query(
            "SELECT enabled FROM audio_models WHERE slug = ?1",
            params![slug.to_string()],
        )
        .await
        .map_err(|e| format!("DB query: {e}"))?;

    match rows.next().await {
        Ok(Some(row)) => {
            let v: i32 = row.get(0).map_err(|e| format!("DB row: {e}"))?;
            Ok(Some(v != 0))
        }
        _ => Ok(None),
    }
}

/// Load all audio model enabled states.
pub async fn all_audio_model_states() -> Result<Vec<(String, bool)>, String> {
    let guard = DB.lock().await;
    let conn = guard.as_ref().ok_or("DB not initialised")?;
    let mut rows = conn
        .query("SELECT slug, enabled FROM audio_models", ())
        .await
        .map_err(|e| format!("DB query: {e}"))?;

    let mut out = Vec::new();
    while let Some(row) = rows.next().await.map_err(|e| format!("DB row: {e}"))? {
        out.push((
            row.get::<String>(0).map_err(|e| format!("DB row: {e}"))?,
            row.get::<i32>(1).map_err(|e| format!("DB row: {e}"))? != 0,
        ));
    }
    Ok(out)
}

/// Count how many audio models are currently enabled.
pub async fn active_audio_model_count() -> Result<usize, String> {
    let guard = DB.lock().await;
    let conn = guard.as_ref().ok_or("DB not initialised")?;
    let mut rows = conn
        .query(
            "SELECT COUNT(*) FROM audio_models WHERE enabled = 1",
            (),
        )
        .await
        .map_err(|e| format!("DB query: {e}"))?;

    let count: i32 = rows
        .next()
        .await
        .map_err(|e| format!("DB row: {e}"))?
        .map(|r| r.get::<i32>(0))
        .transpose()
        .map_err(|e| format!("DB row: {e}"))?
        .unwrap_or(0);
    Ok(count as usize)
}

// ── Light model state ──────────────────────────────────────────────────────

/// Persist the enabled state of a light (camera-trap) model.
pub async fn set_light_model_enabled(slug: &str, enabled: bool) -> Result<(), String> {
    let guard = DB.lock().await;
    let conn = guard.as_ref().ok_or("DB not initialised")?;
    conn.execute(
        "INSERT INTO light_models (slug, enabled)
         VALUES (?1, ?2)
         ON CONFLICT(slug) DO UPDATE SET enabled = excluded.enabled",
        params![slug.to_string(), enabled as i32],
    )
    .await
    .map_err(|e| format!("DB set_light_model_enabled: {e}"))?;
    Ok(())
}

/// Load the enabled state for a light model.
/// Returns `None` if the slug has never been persisted.
pub async fn get_light_model_enabled(slug: &str) -> Result<Option<bool>, String> {
    let guard = DB.lock().await;
    let conn = guard.as_ref().ok_or("DB not initialised")?;
    let mut rows = conn
        .query(
            "SELECT enabled FROM light_models WHERE slug = ?1",
            params![slug.to_string()],
        )
        .await
        .map_err(|e| format!("DB query: {e}"))?;

    match rows.next().await {
        Ok(Some(row)) => {
            let v: i32 = row.get(0).map_err(|e| format!("DB row: {e}"))?;
            Ok(Some(v != 0))
        }
        _ => Ok(None),
    }
}

/// Load all light model enabled states.
pub async fn all_light_model_states() -> Result<Vec<(String, bool)>, String> {
    let guard = DB.lock().await;
    let conn = guard.as_ref().ok_or("DB not initialised")?;
    let mut rows = conn
        .query("SELECT slug, enabled FROM light_models", ())
        .await
        .map_err(|e| format!("DB query: {e}"))?;

    let mut out = Vec::new();
    while let Some(row) = rows.next().await.map_err(|e| format!("DB row: {e}"))? {
        out.push((
            row.get::<String>(0).map_err(|e| format!("DB row: {e}"))?,
            row.get::<i32>(1).map_err(|e| format!("DB row: {e}"))? != 0,
        ));
    }
    Ok(out)
}

/// Count how many light models are currently enabled.
pub async fn active_light_model_count() -> Result<usize, String> {
    let guard = DB.lock().await;
    let conn = guard.as_ref().ok_or("DB not initialised")?;
    let mut rows = conn
        .query(
            "SELECT COUNT(*) FROM light_models WHERE enabled = 1",
            (),
        )
        .await
        .map_err(|e| format!("DB query: {e}"))?;

    let count: i32 = rows
        .next()
        .await
        .map_err(|e| format!("DB row: {e}"))?
        .map(|r| r.get::<i32>(0))
        .transpose()
        .map_err(|e| format!("DB row: {e}"))?
        .unwrap_or(0);
    Ok(count as usize)
}

// ── Recording analysis tracking ──────────────────────────────────────────

/// Register a recording for analysis by all currently-enabled models.
///
/// Called when a new recording is captured.  Creates one row per enabled
/// model so each processing node knows it needs to analyse the file.
pub async fn register_recording(recording: &str) -> Result<usize, String> {
    let guard = DB.lock().await;
    let conn = guard.as_ref().ok_or("DB not initialised")?;

    let mut rows = conn
        .query(
            "SELECT slug FROM audio_models WHERE enabled = 1",
            (),
        )
        .await
        .map_err(|e| format!("DB query: {e}"))?;

    let mut slugs = Vec::new();
    while let Some(row) = rows.next().await.map_err(|e| format!("DB row: {e}"))? {
        let slug: String = row.get(0).map_err(|e| format!("DB row: {e}"))?;
        slugs.push(slug);
    }

    let count = slugs.len();
    for slug in &slugs {
        conn.execute(
            "INSERT OR IGNORE INTO recording_analysis (recording, model_slug, completed)
             VALUES (?1, ?2, 0)",
            params![recording.to_string(), slug.clone()],
        )
        .await
        .map_err(|e| format!("DB register_recording: {e}"))?;
    }
    Ok(count)
}

/// Mark a recording as analysed by a specific model.
pub async fn mark_recording_analyzed(
    recording: &str,
    model_slug: &str,
) -> Result<(), String> {
    let guard = DB.lock().await;
    let conn = guard.as_ref().ok_or("DB not initialised")?;
    conn.execute(
        "UPDATE recording_analysis SET completed = 1
         WHERE recording = ?1 AND model_slug = ?2",
        params![recording.to_string(), model_slug.to_string()],
    )
    .await
    .map_err(|e| format!("DB mark_recording_analyzed: {e}"))?;
    Ok(())
}

/// Check whether a recording has been analysed by all registered models.
pub async fn is_recording_fully_analyzed(recording: &str) -> Result<bool, String> {
    let guard = DB.lock().await;
    let conn = guard.as_ref().ok_or("DB not initialised")?;
    let mut rows = conn
        .query(
            "SELECT COUNT(*) FROM recording_analysis
             WHERE recording = ?1 AND completed = 0",
            params![recording.to_string()],
        )
        .await
        .map_err(|e| format!("DB query: {e}"))?;

    let pending: i32 = rows
        .next()
        .await
        .map_err(|e| format!("DB row: {e}"))?
        .map(|r| r.get::<i32>(0))
        .transpose()
        .map_err(|e| format!("DB row: {e}"))?
        .unwrap_or(0);
    Ok(pending == 0)
}

/// Return all recordings that have been fully analysed by every registered
/// model and can safely be deleted.
pub async fn fully_analyzed_recordings() -> Result<Vec<String>, String> {
    let guard = DB.lock().await;
    let conn = guard.as_ref().ok_or("DB not initialised")?;
    let mut rows = conn
        .query(
            "SELECT recording FROM recording_analysis
             GROUP BY recording
             HAVING COUNT(*) > 0 AND SUM(completed = 0) = 0",
            (),
        )
        .await
        .map_err(|e| format!("DB query: {e}"))?;

    let mut out = Vec::new();
    while let Some(row) = rows.next().await.map_err(|e| format!("DB row: {e}"))? {
        out.push(row.get(0).map_err(|e| format!("DB row: {e}"))?);
    }
    Ok(out)
}

/// Remove tracking rows for a recording after the file has been deleted.
pub async fn remove_recording_tracking(recording: &str) -> Result<(), String> {
    let guard = DB.lock().await;
    let conn = guard.as_ref().ok_or("DB not initialised")?;
    conn.execute(
        "DELETE FROM recording_analysis WHERE recording = ?1",
        params![recording.to_string()],
    )
    .await
    .map_err(|e| format!("DB remove_recording_tracking: {e}"))?;
    Ok(())
}

// ── Legacy migration ─────────────────────────────────────────────────────

/// Migrate data from the legacy `assignments.json` file, if it exists
/// and the DB assignments table is empty.
pub async fn migrate_legacy_json() {
    let dir = std::env::var("GAIA_CONFIG_DIR").unwrap_or_else(|_| "./data".into());
    let json_path = PathBuf::from(&dir).join("assignments.json");
    if !json_path.exists() {
        return;
    }

    // Only migrate if table is empty.
    let existing = get_all_assignments().await.unwrap_or_default();
    if !existing.is_empty() {
        return;
    }

    match std::fs::read_to_string(&json_path) {
        Ok(json) => {
            #[derive(serde::Deserialize)]
            struct Legacy {
                devices: std::collections::HashMap<String, LegacyAssignment>,
            }
            #[derive(serde::Deserialize)]
            struct LegacyAssignment {
                device_id: String,
                source: String,
                project: String,
            }

            // The JSON format wraps assignments as { "devices": { ... } } or
            // the Assignment struct had a `source` enum serialised as lowercase.
            if let Ok(legacy) = serde_json::from_str::<Legacy>(&json) {
                let mut count = 0u32;
                for a in legacy.devices.values() {
                    let src = &a.source;
                    if set_assignment(&a.device_id, src, &a.project).await.is_ok() {
                        count += 1;
                    }
                }
                tracing::info!(
                    "Migrated {count} assignment(s) from legacy {}",
                    json_path.display()
                );
            } else {
                tracing::warn!("Could not parse legacy {}", json_path.display());
            }
        }
        Err(e) => tracing::warn!("Could not read legacy {}: {e}", json_path.display()),
    }
}
