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
    pub processing_enabled: bool,
    /// Whether the web interface container is enabled.
    pub web_enabled: bool,
}

impl ProjectTarget {
    /// Returns `true` if any container in this project is enabled.
    pub fn any_enabled(&self) -> bool {
        self.capture_enabled || self.processing_enabled || self.web_enabled
    }
}

/// Returns the default list of upstream project targets.
///
/// These can be overridden via a future configuration file or environment
/// variables; for now they reflect the standard Gaia port allocation.
pub fn default_targets() -> Vec<ProjectTarget> {
    vec![
        ProjectTarget {
            name: "Gaia Audio".into(),
            slug: "audio".into(),
            description: "Bioacoustic species monitoring — bird songs, insects, bats and more."
                .into(),
            upstream_url: std::env::var("GAIA_AUDIO_URL")
                .unwrap_or_else(|_| "http://localhost:3000".into()),
            port: 3000,
            capture_enabled: false,
            processing_enabled: false,
            web_enabled: false,
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
        },
        ProjectTarget {
            name: "Global Meteor Network".into(),
            slug: "gmn".into(),
            description:
                "Meteor detection using video cameras — capture and processing via RMS.".into(),
            upstream_url: std::env::var("GAIA_GMN_URL")
                .unwrap_or_else(|_| "http://localhost:8180".into()),
            port: 8180,
            capture_enabled: false,
            processing_enabled: false,
            web_enabled: false, // RMS has no web UI yet
        },
    ]
}
