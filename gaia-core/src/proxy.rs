//! Reverse-proxy middleware that forwards `/proxy/{slug}/**` to the upstream service.
//!
//! Each sub-project's web interface is accessible through the Gaia Core
//! dashboard without needing to remember individual ports.
//!
//! **HTML rewriting**: When the upstream returns `text/html`, absolute paths
//! like `/pkg/...` and `/api/...` are rewritten to `/proxy/{slug}/pkg/...` so
//! assets and server-function calls route through the proxy.  A small
//! `<script>` is injected to intercept `fetch()` from the WASM bundle.

use axum::{
    body::Body,
    extract::{Path, State},
    http::{Request, StatusCode},
    response::{IntoResponse, Response},
};
use http_body_util::BodyExt;
use reqwest::Client;
use std::collections::HashMap;

use crate::config::ProjectTarget;

/// Shared state for the proxy layer.
#[derive(Clone)]
pub struct ProxyState {
    pub client: Client,
    /// slug → upstream base URL mapping (always contains **all** projects so
    /// the proxy works immediately when a web container is enabled at
    /// runtime, without restarting gaia-core).
    pub upstreams: HashMap<String, String>,
}

impl ProxyState {
    pub fn from_targets(targets: &[ProjectTarget]) -> Self {
        // Include every project regardless of `web_enabled` so newly-started
        // containers are reachable straight away.
        let upstreams = targets
            .iter()
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

/// Handler for all methods on `/proxy/*path`.
///
/// Parses the slug and remainder from the catch-all, forwards the request
/// (including method, body and content-type) to the upstream service, and
/// optionally rewrites HTML responses so absolute asset paths work.
pub async fn proxy_handler(
    State(state): State<ProxyState>,
    Path(path): Path<String>,
    req: Request<Body>,
) -> Response {
    // path arrives as e.g. "radio", "radio/", "radio/pkg/app.js"
    let trimmed = path.trim_start_matches('/');
    let (slug, rest) = match trimmed.find('/') {
        Some(idx) => (&trimmed[..idx], &trimmed[idx..]),  // slug, /rest...
        None => (trimmed, "/"),                            // slug only → root
    };

    if slug.is_empty() {
        return (StatusCode::BAD_REQUEST, "Missing project slug").into_response();
    }

    let Some(upstream) = state.upstreams.get(slug) else {
        return (
            StatusCode::BAD_GATEWAY,
            format!("Unknown project: {slug}"),
        )
            .into_response();
    };

    let query = req.uri().query().map(|q| format!("?{q}")).unwrap_or_default();
    let upstream_uri = format!("{upstream}{rest}{query}");

    // ── Preserve request method, content-type and body ───────────────
    let (parts, body) = req.into_parts();
    let method = parts.method;
    let req_content_type = parts.headers.get("content-type").cloned();

    // Collect the request body (empty for GET, populated for POST).
    let body_bytes = body
        .collect()
        .await
        .map(|c| c.to_bytes())
        .unwrap_or_default();

    let mut upstream_req = state.client.request(method, &upstream_uri);

    if let Some(ct) = req_content_type {
        upstream_req = upstream_req.header("content-type", ct);
    }
    if !body_bytes.is_empty() {
        upstream_req = upstream_req.body(body_bytes.to_vec());
    }

    // ── Forward ──────────────────────────────────────────────────────
    let upstream_resp = match upstream_req.send().await {
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

    // ── Build the response ───────────────────────────────────────────
    let status = StatusCode::from_u16(upstream_resp.status().as_u16())
        .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);

    let is_html = upstream_resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .map(|ct| ct.contains("text/html"))
        .unwrap_or(false);

    let mut builder = Response::builder().status(status);

    for (key, value) in upstream_resp.headers() {
        // Strip hop-by-hop headers that must not be forwarded by a proxy.
        // The proxy buffers the full response body, so transfer-encoding
        // and content-length from the upstream are invalid for the
        // reconstructed response.  content-encoding is stripped because
        // reqwest auto-decompresses the body.
        let name = key.as_str();
        if matches!(
            name,
            "transfer-encoding" | "connection" | "keep-alive" | "content-encoding"
        ) {
            continue;
        }
        // Drop content-length -- for HTML the proxy rewrites the body so
        // the size changes; for non-HTML reqwest may have decompressed it.
        // Axum/hyper will set the correct value automatically.
        if name == "content-length" {
            continue;
        }
        // Rewrite redirect Location headers to keep the browser in the proxy.
        if key == "location" {
            if let Ok(loc) = value.to_str() {
                if loc.starts_with('/') {
                    let rewritten = format!("/proxy/{slug}{loc}");
                    builder = builder.header(key, rewritten);
                    continue;
                }
            }
        }
        builder = builder.header(key, value);
    }

    let bytes = upstream_resp.bytes().await.unwrap_or_default();

    if is_html {
        let html = String::from_utf8_lossy(&bytes);
        let prefix = format!("/proxy/{slug}");

        // Rewrite absolute asset / API paths so they route through the proxy.
        let html = html
            .replace("=\"/pkg/", &format!("=\"{prefix}/pkg/"))
            .replace("='/pkg/", &format!("='{prefix}/pkg/"))
            .replace("=\"/api/", &format!("=\"{prefix}/api/"))
            .replace("='/api/", &format!("='{prefix}/api/"))
            .replace("=\"/style/", &format!("=\"{prefix}/style/"));

        // Inject a fetch interceptor so WASM server-function calls
        // (POST /api/...) are routed through the proxy prefix.
        let interceptor = format!(
            "<script>(function(){{var p='{prefix}';\
             var F=window.fetch;\
             window.fetch=function(u,o){{\
               if(typeof u==='string'&&u.startsWith('/'))u=p+u;\
               return F.call(this,u,o)\
             }}}})();</script>"
        );
        let html = html.replacen("</head>", &format!("{interceptor}</head>"), 1);

        let html_bytes = html.into_bytes();
        builder
            .header("content-length", html_bytes.len())
            .body(Body::from(html_bytes))
            .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
    } else {
        builder
            .body(Body::from(bytes))
            .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
    }
}

/// Streaming reverse-proxy for the GMN camera preview MJPEG feed.
///
/// Forwards `http://127.0.0.1:8181/stream` so the browser can use the
/// relative URL `/api/camera-stream` regardless of which host/IP the
/// user accesses the dashboard from.
///
/// Retries the upstream connection a few times because the streaming
/// container may have *just* been started by the toggle action.
pub async fn camera_stream_handler() -> Response {
    let client = Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap_or_else(|_| Client::new());

    let upstream_url = "http://127.0.0.1:8181/stream";

    // Retry up to 8 times (~4 s total) to give the container time to boot.
    let mut last_err = String::new();
    for attempt in 0..8u32 {
        if attempt > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }

        match client.get(upstream_url).send().await {
            Ok(resp) if resp.status().is_success() => {
                let status = StatusCode::from_u16(resp.status().as_u16())
                    .unwrap_or(StatusCode::OK);

                let mut builder = Response::builder().status(status);
                for (key, value) in resp.headers() {
                    builder = builder.header(key, value);
                }

                // Stream the MJPEG body without buffering.
                let stream = resp.bytes_stream();
                return builder
                    .body(Body::from_stream(stream))
                    .unwrap_or_else(|_| {
                        StatusCode::INTERNAL_SERVER_ERROR.into_response()
                    });
            }
            Ok(resp) => {
                last_err = format!("upstream returned {}", resp.status());
            }
            Err(e) => {
                last_err = e.to_string();
            }
        }
    }

    (
        StatusCode::BAD_GATEWAY,
        format!("Camera stream unavailable after retries: {last_err}"),
    )
        .into_response()
}
