//! Reverse-proxy middleware — forwards `/proxy/{slug}/**` to the upstream service.
//!
//! Each sub-project's web interface is accessible through the Gaia Core
//! dashboard without needing to remember individual ports.

use axum::{
    body::Body,
    extract::{Path, State},
    http::{Request, StatusCode},
    response::{IntoResponse, Response},
};
use reqwest::Client;
use std::collections::HashMap;

use crate::config::ProjectTarget;

/// Shared state for the proxy layer.
#[derive(Clone)]
pub struct ProxyState {
    pub client: Client,
    /// slug → upstream base URL mapping.
    pub upstreams: HashMap<String, String>,
}

impl ProxyState {
    pub fn from_targets(targets: &[ProjectTarget]) -> Self {
        let upstreams = targets
            .iter()
            .filter(|t| t.web_enabled)
            .map(|t| (t.slug.clone(), t.upstream_url.clone()))
            .collect();

        Self {
            client: Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .expect("failed to build HTTP client"),
            upstreams,
        }
    }
}

/// Handler for `GET /proxy/:slug/*rest`.
///
/// Strips the `/proxy/:slug` prefix and forwards the remainder to the
/// upstream service.  Headers, query strings and the response body are
/// forwarded transparently.
pub async fn proxy_handler(
    State(state): State<ProxyState>,
    Path((slug, rest)): Path<(String, String)>,
    req: Request<Body>,
) -> Response {
    let Some(upstream) = state.upstreams.get(&slug) else {
        return (
            StatusCode::BAD_GATEWAY,
            format!("Unknown project: {slug}"),
        )
            .into_response();
    };

    // Strip /proxy/{slug} prefix to get the path the upstream expects.
    let upstream_path = format!("/{rest}");
    let query = req.uri().query().map(|q| format!("?{q}")).unwrap_or_default();

    let upstream_uri = format!("{upstream}{upstream_path}{query}");

    // Forward the request.
    let upstream_resp = match state.client.get(&upstream_uri).send().await {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("Proxy error for {slug}: {e}");
            return (
                StatusCode::BAD_GATEWAY,
                format!("Upstream {slug} unreachable: {e}"),
            )
                .into_response();
        }
    };

    // Convert reqwest::Response → axum::Response.
    let status = StatusCode::from_u16(upstream_resp.status().as_u16())
        .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);

    let mut builder = Response::builder().status(status);

    // Copy response headers, rewriting Location headers for redirects
    // so the browser stays within /proxy/{slug}/…
    for (key, value) in upstream_resp.headers() {
        if key == "location" {
            if let Ok(loc) = value.to_str() {
                // If the redirect is a relative path, prefix it.
                if loc.starts_with('/') {
                    let rewritten = format!("/proxy/{slug}{loc}");
                    builder = builder.header(key, rewritten);
                    continue;
                }
            }
        }
        builder = builder.header(key, value);
    }

    let bytes = upstream_resp
        .bytes()
        .await
        .unwrap_or_default();

    builder
        .body(Body::from(bytes))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

/// Handler for `/proxy/:slug` (no trailing path) — redirects to `/proxy/:slug/`.
pub async fn proxy_root_handler(
    State(state): State<ProxyState>,
    Path(slug): Path<String>,
    req: Request<Body>,
) -> Response {
    // Delegate to the main handler with an empty rest path.
    let rest = String::new();
    proxy_handler(State(state), Path((slug, rest)), req).await
}
