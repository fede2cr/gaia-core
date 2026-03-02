//! SQLite persistence layer for Gaia Core.
//!
//! Stores container on/off state and device-to-project assignments in a
//! single SQLite database.  The DB file lives at
//! `$GAIA_CONFIG_DIR/gaia-core.db` (defaults to `./data/gaia-core.db`).
//!
//! A global connection is created once via [`init`] and accessed through
//! the module-level helper functions.  All calls go through a
//! `tokio::sync::Mutex<Connection>` so they are safe to call from async
//! server functions.

use rusqlite::{params, Connection};
use std::path::PathBuf;
use std::sync::LazyLock;
use tokio::sync::Mutex;

/// Global database connection.
static DB: LazyLock<Mutex<Option<Connection>>> = LazyLock::new(|| Mutex::new(None));

// ── Initialisation ───────────────────────────────────────────────────────

/// Open (or create) the database and run migrations.
///
/// **Must** be called once at server startup (before any server functions).
pub async fn init() {
    let path = db_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).ok();
    }

    let conn = Connection::open(&path).expect("failed to open SQLite database");
    conn.execute_batch("PRAGMA journal_mode = WAL;").ok();

    // ── Migrations ───────────────────────────────────────────────────
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS container_state (
            slug           TEXT NOT NULL,
            container_kind TEXT NOT NULL,   -- 'capture', 'processing', 'web'
            enabled        INTEGER NOT NULL DEFAULT 1,
            PRIMARY KEY (slug, container_kind)
        );

        CREATE TABLE IF NOT EXISTS device_assignments (
            device_id TEXT PRIMARY KEY,
            source    TEXT NOT NULL DEFAULT 'local',  -- 'local' or 'remote'
            project   TEXT NOT NULL DEFAULT ''
        );

        CREATE TABLE IF NOT EXISTS settings (
            key   TEXT PRIMARY KEY,
            value TEXT NOT NULL DEFAULT ''
        );
        ",
    )
    .expect("failed to run DB migrations");

    tracing::info!("SQLite database opened at {}", path.display());
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
    tracing::info!(
        "DB set_container_enabled: slug={slug}, kind={container_kind}, enabled={enabled}"
    );
    let guard = DB.lock().await;
    let conn = guard.as_ref().ok_or("DB not initialised")?;
    let rows = conn.execute(
        "INSERT INTO container_state (slug, container_kind, enabled)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(slug, container_kind) DO UPDATE SET enabled = excluded.enabled",
        params![slug, container_kind, enabled as i32],
    )
    .map_err(|e| format!("DB set_container_enabled: {e}"))?;
    tracing::info!("DB set_container_enabled: {rows} row(s) affected");
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
    let mut stmt = conn
        .prepare_cached(
            "SELECT enabled FROM container_state WHERE slug = ?1 AND container_kind = ?2",
        )
        .map_err(|e| format!("DB prepare: {e}"))?;

    let result: Option<i32> = stmt
        .query_row(params![slug, container_kind], |row| row.get(0))
        .ok();

    Ok(result.map(|v| v != 0))
}

/// Load *all* saved container states into a flat vec.
pub async fn all_container_states() -> Result<Vec<(String, String, bool)>, String> {
    let guard = DB.lock().await;
    let conn = guard.as_ref().ok_or("DB not initialised")?;
    let mut stmt = conn
        .prepare_cached("SELECT slug, container_kind, enabled FROM container_state")
        .map_err(|e| format!("DB prepare: {e}"))?;

    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i32>(2)? != 0,
            ))
        })
        .map_err(|e| format!("DB query: {e}"))?;

    let mut out = Vec::new();
    for r in rows {
        out.push(r.map_err(|e| format!("DB row: {e}"))?);
    }
    tracing::info!("DB all_container_states: {} row(s): {:?}", out.len(), out);
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
    let mut stmt = conn
        .prepare_cached("SELECT device_id, source, project FROM device_assignments")
        .map_err(|e| format!("DB prepare: {e}"))?;

    let rows = stmt
        .query_map([], |row| {
            Ok(AssignmentRow {
                device_id: row.get(0)?,
                source: row.get(1)?,
                project: row.get(2)?,
            })
        })
        .map_err(|e| format!("DB query: {e}"))?;

    let mut out = Vec::new();
    for r in rows {
        out.push(r.map_err(|e| format!("DB row: {e}"))?);
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
    tracing::info!(
        "DB set_assignment: device_id={device_id}, source={source}, project={project}"
    );
    let guard = DB.lock().await;
    let conn = guard.as_ref().ok_or("DB not initialised")?;

    let project = if project == "none" { "" } else { project };

    if project.is_empty() {
        let rows = conn.execute(
            "DELETE FROM device_assignments WHERE device_id = ?1",
            params![device_id],
        )
        .map_err(|e| format!("DB delete assignment: {e}"))?;
        tracing::info!("DB set_assignment: deleted {rows} row(s) for device {device_id}");
    } else {
        let rows = conn.execute(
            "INSERT INTO device_assignments (device_id, source, project)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(device_id) DO UPDATE SET source = excluded.source, project = excluded.project",
            params![device_id, source, project],
        )
        .map_err(|e| format!("DB set_assignment: {e}"))?;
        tracing::info!("DB set_assignment: upserted {rows} row(s) for device {device_id} → project {project}");
    }
    Ok(())
}

// ── Settings (key-value) ──────────────────────────────────────────────

/// Read a setting by key.  Returns `None` if the key has never been stored.
pub async fn get_setting(key: &str) -> Result<Option<String>, String> {
    let guard = DB.lock().await;
    let conn = guard.as_ref().ok_or("DB not initialised")?;
    let mut stmt = conn
        .prepare_cached("SELECT value FROM settings WHERE key = ?1")
        .map_err(|e| format!("DB prepare: {e}"))?;
    let result: Option<String> = stmt
        .query_row(params![key], |row| row.get(0))
        .ok();
    Ok(result)
}

/// Write a setting (upsert).
pub async fn set_setting(key: &str, value: &str) -> Result<(), String> {
    let guard = DB.lock().await;
    let conn = guard.as_ref().ok_or("DB not initialised")?;
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )
    .map_err(|e| format!("DB set_setting: {e}"))?;
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
