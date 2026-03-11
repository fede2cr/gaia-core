//! Container image update checker.
//!
//! Periodically compares the locally-pulled image digest against the
//! remote Docker Hub digest for every image listed in the container
//! configuration.  Results are stored in-memory and exposed via server
//! functions so the UI can show an update indicator.
//!
//! ## How it works
//!
//! 1. **Local digest** – obtained via
//!    `podman/docker image inspect --format '{{index .RepoDigests 0}}'`.
//!    This returns the repo digest of the last pull (e.g.
//!    `docker.io/fede2/gaia-audio-capture@sha256:abc…`).
//!
//! 2. **Remote digest** – obtained from the Docker Hub v2 registry API:
//!    - Acquire a short-lived bearer token from `auth.docker.io`.
//!    - `HEAD /v2/{repo}/manifests/latest` with the token and the
//!      correct `Accept` header.  The `Docker-Content-Digest` response
//!      header contains the current digest.
//!
//! 3. If the two digests differ (or the image has never been pulled),
//!    the container is marked as having an update available.

use std::collections::HashMap;
use std::sync::OnceLock;

use tokio::sync::RwLock;
use serde::{Deserialize, Serialize};

use crate::containers::{config, runtime, runtime_cmd};

// ── Public types ─────────────────────────────────────────────────────────

/// Update status for a single container image.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ImageUpdateStatus {
    /// Container name (e.g. "gaia-audio-capture").
    pub container: String,
    /// Image reference (e.g. "docker.io/fede2/gaia-audio-capture").
    pub image: String,
    /// `true` when the remote digest differs from the local one.
    pub has_update: bool,
    /// Local repo digest, if the image has been pulled.
    pub local_digest: Option<String>,
    /// Remote digest from Docker Hub.
    pub remote_digest: Option<String>,
    /// ISO-8601 timestamp of the last check.
    pub last_checked: String,
}

// ── In-memory state ──────────────────────────────────────────────────────

static UPDATE_STATE: OnceLock<RwLock<HashMap<String, ImageUpdateStatus>>> = OnceLock::new();

fn state() -> &'static RwLock<HashMap<String, ImageUpdateStatus>> {
    UPDATE_STATE.get_or_init(|| RwLock::new(HashMap::new()))
}

/// Return a snapshot of all image update statuses.
pub async fn all_update_statuses() -> Vec<ImageUpdateStatus> {
    state()
        .read()
        .await
        .values()
        .cloned()
        .collect()
}

/// Return how many images have updates available.
pub async fn update_count() -> usize {
    state()
        .read()
        .await
        .values()
        .filter(|s| s.has_update)
        .count()
}

// ── Check logic ──────────────────────────────────────────────────────────

/// Run an update check for **running** container images.
///
/// Only containers whose lifecycle status is `"running"` are checked.
/// Stopped containers are skipped because they pull the latest image
/// automatically when they start.
///
/// This is called both by the background loop and by the manual
/// "Check Now" button.
pub async fn check_all() -> Vec<ImageUpdateStatus> {
    let cfg = config();
    let rt = runtime().await;
    let cmd = runtime_cmd(rt);

    // Collect unique images only for containers that are currently running.
    // Stopped containers will pull the latest image on next start, so
    // there is no need to query the registry for them.
    let mut image_to_containers: HashMap<String, Vec<String>> = HashMap::new();
    for (name, spec) in &cfg.containers {
        let status = crate::containers::get_status(name);
        if status != "running" {
            continue;
        }
        image_to_containers
            .entry(spec.image.clone())
            .or_default()
            .push(name.clone());
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .unwrap_or_default();

    let now = now_iso8601();
    let mut results: Vec<ImageUpdateStatus> = Vec::new();

    for (image, containers) in &image_to_containers {
        let local = local_image_digest(cmd, image).await;
        let remote = remote_image_digest(&client, image).await;

        tracing::debug!(
            image,
            ?local,
            ?remote,
            "Digest comparison for update check"
        );

        let has_update = match (&local, &remote) {
            (Some(l), Some(r)) => l != r,
            // Image never pulled → treat as needing update.
            (None, Some(_)) => true,
            // Cannot reach registry → unknown, keep previous state or false.
            _ => false,
        };

        for cname in containers {
            let status = ImageUpdateStatus {
                container: cname.clone(),
                image: image.clone(),
                has_update,
                local_digest: local.clone(),
                remote_digest: remote.clone(),
                last_checked: now.clone(),
            };
            results.push(status.clone());
            state().write().await.insert(cname.clone(), status);
        }
    }

    tracing::info!(
        "Update check complete: {}/{} images have updates",
        results.iter().filter(|s| s.has_update).count(),
        image_to_containers.len(),
    );

    results
}

// ── Local digest ─────────────────────────────────────────────────────────

/// Get the repo digest of a locally-pulled image.
///
/// Returns `Some("sha256:abc…")` or `None` if the image isn't present.
async fn local_image_digest(cmd: &str, image: &str) -> Option<String> {
    let output = tokio::process::Command::new(cmd)
        .args(["image", "inspect", "--format", "{{index .RepoDigests 0}}", image])
        .output()
        .await
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let raw = String::from_utf8_lossy(&output.stdout).trim().to_string();
    // Format: "docker.io/fede2/gaia-audio-capture@sha256:abc…"
    // Extract just the "sha256:…" part.
    raw.split('@').nth(1).map(|s| s.to_string())
}

// ── Remote digest (Docker Hub v2 API) ────────────────────────────────────

/// Docker Hub auth token response.
#[derive(Deserialize)]
struct TokenResponse {
    token: String,
}

/// Fetch the remote digest of `image:latest` from Docker Hub.
///
/// Returns `Some("sha256:abc…")` or `None` on any failure.
async fn remote_image_digest(client: &reqwest::Client, image: &str) -> Option<String> {
    // Parse "docker.io/fede2/gaia-audio-capture" → "fede2/gaia-audio-capture"
    let repo = image
        .strip_prefix("docker.io/")
        .unwrap_or(image);

    // 1. Get bearer token.
    let token_url = format!(
        "https://auth.docker.io/token?service=registry.docker.io&scope=repository:{repo}:pull"
    );
    let token_resp = client
        .get(&token_url)
        .send()
        .await
        .ok()?
        .json::<TokenResponse>()
        .await
        .ok()?;

    // 2. HEAD the manifest to get the digest.
    //
    // Accept manifest-list / OCI-index types **first** so the registry
    // returns the fat-manifest digest — that is what Podman/Docker store
    // in `RepoDigests` after a pull.  If the image is single-arch the
    // registry will fall back to the plain manifest type.
    let manifest_url = format!(
        "https://registry-1.docker.io/v2/{repo}/manifests/latest"
    );
    let resp = client
        .head(&manifest_url)
        .header("Authorization", format!("Bearer {}", token_resp.token))
        .header(
            "Accept",
            "application/vnd.oci.image.index.v1+json, \
             application/vnd.docker.distribution.manifest.list.v2+json, \
             application/vnd.oci.image.manifest.v1+json, \
             application/vnd.docker.distribution.manifest.v2+json",
        )
        .send()
        .await
        .ok()?;

    resp.headers()
        .get("docker-content-digest")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
}

// ── Background loop ──────────────────────────────────────────────────────

/// Default check interval: 24 hours.
const DEFAULT_INTERVAL_HOURS: u64 = 24;

/// Spawn the background update-check loop.
///
/// Reads the `update_check_interval` setting from the DB (hours, default
/// 24) and sleeps between checks.  The first check runs after a short
/// delay to let containers start first.
pub fn spawn_background_loop() {
    tokio::spawn(async {
        // Wait a bit before the first check so the system has time to
        // start containers and pull images.
        tokio::time::sleep(std::time::Duration::from_secs(60)).await;

        loop {
            tracing::info!("Running scheduled container update check");
            check_all().await;

            let interval_hours = crate::db::get_setting("update_check_interval")
                .await
                .ok()
                .flatten()
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(DEFAULT_INTERVAL_HOURS)
                .max(1);

            tracing::info!("Next update check in {interval_hours}h");
            tokio::time::sleep(std::time::Duration::from_secs(interval_hours * 3600)).await;
        }
    });
}

// ── Helpers ──────────────────────────────────────────────────────────────

fn now_iso8601() -> String {
    // Use a simple approach without pulling in chrono.
    let output = std::process::Command::new("date")
        .args(["--iso-8601=seconds"])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "unknown".into());
    output
}
