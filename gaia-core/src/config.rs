//! Configuration for upstream proxy targets and project metadata.

use serde::{Deserialize, Serialize};

/// Describes one Gaia sub-project that can be proxied.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProjectTarget {
    /// Human-readable name shown in the dashboard.
    pub name: String,
    /// Short identifier used in URL paths, e.g. "audio", "radio".
    pub slug: String,
    /// A brief description of the project.
    pub description: String,
    /// Base URL of the upstream web interface (e.g. "http://localhost:3000").
    pub upstream_url: String,
    /// The TCP port the upstream listens on (for display purposes).
    pub port: u16,
    /// Whether the capture container is enabled.
    pub capture_enabled: bool,
    /// Whether the processing container is enabled.
    ///
    /// For the "audio" project this reflects whether *any* model processing
    /// node is running.  Use [`processing_models`] for per-model detail.
    pub processing_enabled: bool,
    /// Whether the web interface container is enabled.
    pub web_enabled: bool,
    /// Whether the config container is enabled (e.g. GMN camera pre-align).
    pub config_enabled: bool,
    /// TCP port for the config service (0 = none).
    pub config_port: u16,
    /// Audio processing models with per-model running state.
    ///
    /// Non-empty only for the "audio" project.  Empty for other projects
    /// that use the traditional single processing toggle.
    pub processing_models: Vec<AudioProcessingNode>,
}

/// An audio processing node: one running container for one model.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AudioProcessingNode {
    /// Model slug, e.g. "birdnet".
    pub model_slug: String,
    /// Human-readable model name.
    pub model_name: String,
    /// The container_kind used in the DB, e.g. "processing" or "processing:perch".
    pub container_kind: String,
    /// Whether this processing node's container is currently enabled.
    pub running: bool,
}

impl ProjectTarget {
    /// Returns `true` if any container in this project is enabled.
    pub fn any_enabled(&self) -> bool {
        self.capture_enabled || self.processing_enabled || self.web_enabled || self.config_enabled
    }
}

// ── Audio model definitions ──────────────────────────────────────────────

/// An audio processing model that can be enabled and run as a processing node.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AudioModel {
    /// Short identifier, e.g. "birdnet", "perch".
    pub slug: String,
    /// Human-readable model name, e.g. "BirdNET V2.4".
    pub name: String,
    /// Brief description of what the model detects.
    pub description: String,
    /// Whether the model is enabled (available for use).
    pub enabled: bool,
    /// The `container_kind` value used in the container_state table.
    ///
    /// - `"processing"` for the default model (BirdNET, backward compatible)
    /// - `"processing:{slug}"` for additional models
    pub container_kind: String,
}

/// Returns the built-in list of known audio models.
///
/// Only BirdNET V2.4 is included initially.  Additional models will be
/// added in future releases.  The `enabled` field defaults to `false`;
/// actual state is loaded from the database at runtime.
pub fn default_audio_models() -> Vec<AudioModel> {
    vec![
        AudioModel {
            slug: "birdnet".into(),
            name: "BirdNET V2.4".into(),
            description: "Bird song classification — ~6,500 species worldwide".into(),
            enabled: false,
            container_kind: "processing".into(),
        },
        AudioModel {
            slug: "perch".into(),
            name: "Google Perch 2.0".into(),
            description: "Multi-taxa wildlife classifier — ~15,000 species (birds, frogs, insects, mammals)".into(),
            enabled: false,
            container_kind: "processing:perch".into(),
        },
    ]
}

/// Derive the `container_kind` value for a model slug.
///
/// BirdNET uses `"processing"` for backward compatibility with existing
/// deployments.  All other models use `"processing:{slug}"`.
pub fn model_container_kind(model_slug: &str) -> String {
    if model_slug == "birdnet" {
        "processing".into()
    } else {
        format!("processing:{model_slug}")
    }
}

// ── Project targets ──────────────────────────────────────────────────────

/// Returns the default list of upstream project targets.
///
/// These can be overridden via a future configuration file or environment
/// variables; for now they reflect the standard Gaia port allocation.
pub fn default_targets() -> Vec<ProjectTarget> {
    vec![
        ProjectTarget {
            name: "Gaia Audio".into(),
            slug: "audio".into(),
            description: "Bioacoustic species monitoring: bird songs, insects, bats and more."
                .into(),
            upstream_url: std::env::var("GAIA_AUDIO_URL")
                .unwrap_or_else(|_| "http://localhost:3000".into()),
            port: 3000,
            capture_enabled: false,
            processing_enabled: false,
            web_enabled: false,
            config_enabled: false,
            config_port: 0,
            processing_models: vec![],
        },
        ProjectTarget {
            name: "Gaia Radio".into(),
            slug: "radio".into(),
            description:
                "ADS-B flight tracking with CO₂ estimation and aircraft type identification."
                    .into(),
            upstream_url: std::env::var("GAIA_RADIO_URL")
                .unwrap_or_else(|_| "http://localhost:8080".into()),
            port: 8080,
            capture_enabled: false,
            processing_enabled: false,
            web_enabled: false,
            config_enabled: false,
            config_port: 0,
            processing_models: vec![],
        },
        ProjectTarget {
            name: "Global Meteor Network".into(),
            slug: "gmn".into(),
            description:
                "Meteor detection using video cameras, capture and processing via RMS.".into(),
            upstream_url: std::env::var("GAIA_GMN_URL")
                .unwrap_or_else(|_| "http://localhost:8180".into()),
            port: 8180,
            capture_enabled: false,
            processing_enabled: false,
            web_enabled: false,
            config_enabled: false,
            config_port: 8181,
            processing_models: vec![],
        },
    ]
}
