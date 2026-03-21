//! Async Redis / Valkey helper for gaia-core.
//!
//! Communicates with the `gaia-audio-coordinator` Valkey instance to
//! signal model enable/disable changes to the processing container.
//!
//! ## Data model
//!
//! | Key                      | Type | Purpose                              |
//! |--------------------------|------|--------------------------------------|
//! | `audio:enabled_models`   | SET  | Slugs of enabled audio models        |
//! | `light:enabled_models`   | SET  | Slugs of enabled light models        |

use redis::AsyncCommands;
use std::sync::OnceLock;

static CLIENT: OnceLock<redis::Client> = OnceLock::new();

fn redis_url() -> String {
    std::env::var("REDIS_URL")
        .unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string())
}

/// Lazily create a Redis client (no actual connection until first use).
fn client() -> &'static redis::Client {
    CLIENT.get_or_init(|| {
        let url = redis_url();
        tracing::info!("kv: Redis client URL = {url}");
        redis::Client::open(url.as_str()).expect("Cannot parse REDIS_URL")
    })
}

/// Get an async multiplexed connection.
async fn conn() -> Result<redis::aio::MultiplexedConnection, String> {
    client()
        .get_multiplexed_async_connection()
        .await
        .map_err(|e| format!("Redis connection failed: {e}"))
}

// ── Audio model management ───────────────────────────────────────────────

const AUDIO_KEY: &str = "audio:enabled_models";

/// Replace the set of enabled audio model slugs.
pub async fn set_enabled_audio_models(slugs: &[String]) -> Result<(), String> {
    let mut c = conn().await?;
    // Atomic: delete + re-add inside a pipeline.
    let mut pipe = redis::pipe();
    pipe.del(AUDIO_KEY);
    if !slugs.is_empty() {
        pipe.sadd(AUDIO_KEY, slugs.to_vec());
    }
    pipe.query_async::<()>(&mut c)
        .await
        .map_err(|e| format!("Redis set_enabled_audio_models: {e}"))?;
    tracing::info!("kv: audio enabled models = {slugs:?}");
    Ok(())
}

/// Add a single model slug to the enabled set.
pub async fn enable_audio_model(slug: &str) -> Result<(), String> {
    let mut c = conn().await?;
    c.sadd::<_, _, ()>(AUDIO_KEY, slug)
        .await
        .map_err(|e| format!("Redis enable_audio_model: {e}"))?;
    tracing::info!("kv: enabled audio model '{slug}'");
    Ok(())
}

/// Remove a single model slug from the enabled set.
pub async fn disable_audio_model(slug: &str) -> Result<(), String> {
    let mut c = conn().await?;
    c.srem::<_, _, ()>(AUDIO_KEY, slug)
        .await
        .map_err(|e| format!("Redis disable_audio_model: {e}"))?;
    tracing::info!("kv: disabled audio model '{slug}'");
    Ok(())
}

/// Read the current set of enabled audio model slugs.
///
/// Returns an empty vec if the key doesn't exist (treated as "nothing
/// enabled yet" by the server_fns layer which seeds from the DB).
pub async fn get_enabled_audio_models() -> Result<Vec<String>, String> {
    let mut c = conn().await?;
    let members: Vec<String> = c
        .smembers(AUDIO_KEY)
        .await
        .map_err(|e| format!("Redis get_enabled_audio_models: {e}"))?;
    Ok(members)
}

// ── Light model management ───────────────────────────────────────────────

const LIGHT_KEY: &str = "light:enabled_models";

/// Add a light model slug to the enabled set.
pub async fn enable_light_model(slug: &str) -> Result<(), String> {
    let mut c = conn().await?;
    c.sadd::<_, _, ()>(LIGHT_KEY, slug)
        .await
        .map_err(|e| format!("Redis enable_light_model: {e}"))?;
    tracing::info!("kv: enabled light model '{slug}'");
    Ok(())
}

/// Remove a light model slug from the enabled set.
pub async fn disable_light_model(slug: &str) -> Result<(), String> {
    let mut c = conn().await?;
    c.srem::<_, _, ()>(LIGHT_KEY, slug)
        .await
        .map_err(|e| format!("Redis disable_light_model: {e}"))?;
    tracing::info!("kv: disabled light model '{slug}'");
    Ok(())
}

/// Read the current set of enabled light model slugs.
pub async fn get_enabled_light_models() -> Result<Vec<String>, String> {
    let mut c = conn().await?;
    let members: Vec<String> = c
        .smembers(LIGHT_KEY)
        .await
        .map_err(|e| format!("Redis get_enabled_light_models: {e}"))?;
    Ok(members)
}

// ── Sync DB → Redis ──────────────────────────────────────────────────────

/// Seed the Redis enabled-model sets from the SQLite database.
///
/// Called once at startup (after the coordinator is running) so the
/// processing container sees the correct set of enabled models.
pub async fn seed_from_db() -> Result<(), String> {
    // Audio models
    let audio_states = crate::db::all_audio_model_states().await.unwrap_or_default();
    let defaults = crate::config::default_audio_models();
    let enabled: Vec<String> = defaults
        .iter()
        .filter(|m| {
            audio_states
                .iter()
                .find(|(s, _)| s == &m.slug)
                .map(|(_, e)| *e)
                .unwrap_or(m.enabled)
        })
        .map(|m| m.slug.clone())
        .collect();
    set_enabled_audio_models(&enabled).await?;

    // Light models
    let light_states = crate::db::all_light_model_states().await.unwrap_or_default();
    let light_defaults = crate::config::default_light_models();
    let light_enabled: Vec<String> = light_defaults
        .iter()
        .filter(|m| {
            light_states
                .iter()
                .find(|(s, _)| s == &m.slug)
                .map(|(_, e)| *e)
                .unwrap_or(m.enabled)
        })
        .map(|m| m.slug.clone())
        .collect();

    let mut c = conn().await?;
    let mut pipe = redis::pipe();
    pipe.del(LIGHT_KEY);
    if !light_enabled.is_empty() {
        pipe.sadd(LIGHT_KEY, light_enabled.clone());
    }
    pipe.query_async::<()>(&mut c)
        .await
        .map_err(|e| format!("Redis seed light models: {e}"))?;
    tracing::info!("kv: light enabled models = {light_enabled:?}");

    Ok(())
}
