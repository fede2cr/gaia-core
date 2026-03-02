//! Server entry-point - Axum + Leptos SSR + reverse proxy.

#[cfg(feature = "ssr")]
#[tokio::main]
async fn main() {
    use axum::{
        extract::State,
        response::{IntoResponse, Response},
        routing::{any, get},
        Router,
    };
    use leptos::*;
    use leptos_axum::{generate_route_list, LeptosRoutes};
    use tower_http::services::ServeDir;
    use tower_http::trace::TraceLayer;

    use gaia_core::app::App;
    use gaia_core::config;
    use gaia_core::proxy::{self, ProxyState};

    // ── Tracing ──────────────────────────────────────────────────────────
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "gaia_core=info,tower_http=info".into()),
        )
        .init();

    // ── Configuration ────────────────────────────────────────────────────
    let conf = get_configuration(None).await.unwrap();
    let leptos_options = conf.leptos_options.clone();
    let addr = leptos_options.site_addr;
    let site_root = leptos_options.site_root.clone();

    // ── Database ─────────────────────────────────────────────────────────
    gaia_core::db::init().await;
    gaia_core::db::migrate_legacy_json().await;

    // ── Proxy targets (with persisted container states) ──────────────────
    let mut targets = config::default_targets();
    if let Ok(states) = gaia_core::db::all_container_states().await {
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
    }
    let proxy_state = ProxyState::from_targets(&targets);

    for t in &targets {
        tracing::info!(
            "Proxy: /proxy/{} → {} (port {}) [web={}]",
            t.slug,
            t.upstream_url,
            t.port,
            if t.web_enabled { "on" } else { "off" },
        );
    }

    // ── Leptos routes ────────────────────────────────────────────────────
    let routes = generate_route_list(App);

    // Log registered server functions so we can verify they're available.
    let sf_count = server_fn::axum::server_fn_paths().count();
    tracing::info!("Registered {sf_count} server function(s)");
    for (path, method) in server_fn::axum::server_fn_paths() {
        tracing::info!("  server_fn: {method} {path}");
    }

    // Build a self-contained sub-router for proxy routes.  Calling
    // .with_state() here converts Router<ProxyState> → Router<()> so it
    // can be nested inside the main router that carries LeptosOptions.
    let proxy_router = Router::new()
        .route("/*path", any(proxy::proxy_handler))
        .with_state(proxy_state);

    let app = Router::new()
        // Camera-stream MJPEG proxy (must be before Leptos catch-all).
        .route("/api/camera-stream", get(proxy::camera_stream_handler))
        // Reverse-proxy sub-router, mounted before the Leptos catch-all.
        .nest("/proxy", proxy_router)
        // Leptos SSR routes (also registers server function endpoints).
        .leptos_routes(&leptos_options, routes, App)
        // Serve compiled WASM bundle, CSS, and static assets.
        .nest_service(
            "/pkg",
            ServeDir::new(format!("{}/pkg", site_root.to_string())),
        )
        // Log all HTTP requests for diagnostics.
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(tower_http::trace::DefaultMakeSpan::new().level(tracing::Level::INFO))
                .on_response(tower_http::trace::DefaultOnResponse::new().level(tracing::Level::INFO))
        )
        .fallback(fallback_handler)
        .with_state(leptos_options);

    // ── Start server ─────────────────────────────────────────────────────
    // Bind the TCP port first so the web UI is reachable immediately.
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    tracing::info!("Gaia Core listening on http://{addr}");

    // Port is open — now sync persisted container toggle states in the
    // background.  Containers whose toggle was left "on" in a previous
    // session will be (re-)started, those marked "off" will be stopped.
    tokio::spawn(gaia_core::containers::sync_with_db());

    axum::serve(listener, app.into_make_service())
        .await
        .unwrap();

    /// Fallback: try to serve a static file, otherwise return 404.
    async fn fallback_handler(
        State(options): State<LeptosOptions>,
        req: axum::http::Request<axum::body::Body>,
    ) -> Response {
        let root = options.site_root.clone();
        let (parts, _body) = req.into_parts();
        let path = format!("{}{}", root, parts.uri.path());
        match tokio::fs::read(&path).await {
            Ok(bytes) => {
                let mime = mime_guess::from_path(&path)
                    .first_raw()
                    .unwrap_or("application/octet-stream");
                ([("content-type", mime)], bytes).into_response()
            }
            Err(_) => (
                axum::http::StatusCode::NOT_FOUND,
                "404 - Not Found",
            )
                .into_response(),
        }
    }
}

/// When compiled without the `ssr` feature, main is a no-op stub.
#[cfg(not(feature = "ssr"))]
pub fn main() {
    // Intentionally empty, the WASM library entry is in lib.rs.
}
