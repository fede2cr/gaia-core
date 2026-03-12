//! Server entry-point - Axum + Leptos SSR + reverse proxy.

#[cfg(feature = "ssr")]
#[tokio::main]
async fn main() {
    use axum::{
        routing::{any, get},
        Router,
    };
    use leptos::prelude::*;
    use leptos::prelude::ElementChild;
    use leptos_axum::{generate_route_list, LeptosRoutes};
    use tower_http::services::ServeDir;

    use gaia_core::app::{shell, App};
    use gaia_core::config;
    use gaia_core::proxy::{self, ProxyState};

    // ── Tracing ──────────────────────────────────────────────────────────
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "gaia_core=info,tower_http=info".into()),
        )
        .init();

    if std::env::var("RUST_LOG").map_or(false, |v| v.contains("debug")) {
        tracing::info!("🔍 Debug logging ENABLED (RUST_LOG={})", std::env::var("RUST_LOG").unwrap_or_default());
    }

    // ── Configuration ────────────────────────────────────────────────────
    let conf = get_configuration(None).unwrap();
    let leptos_options = conf.leptos_options.clone();
    let addr = leptos_options.site_addr;
    let site_root = leptos_options.site_root.clone();

    // ── Database ─────────────────────────────────────────────────────────
    gaia_core::db::init().await;
    gaia_core::db::migrate_legacy_json().await;

    // ── Sync containers with persisted toggle state ─────────────────────
    // Spawned in background so the web UI starts immediately.
    // Container lifecycle statuses are tracked and polled by the dashboard.
    tokio::spawn(gaia_core::containers::sync_with_db());
    // ── Background container update checker ──────────────────────────
    // Periodically compares local vs Docker Hub digests and stores
    // results in-memory for the UI to poll.
    gaia_core::updates::spawn_background_loop();
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

    // Build a self-contained sub-router for proxy routes.  Calling
    // .with_state() here converts Router<ProxyState> → Router<()> so it
    // can be nested inside the main router that carries LeptosOptions.
    let proxy_router = Router::new()
        .route("/{*path}", any(proxy::proxy_handler))
        .with_state(proxy_state);

    let app = Router::new()
        // Camera-stream MJPEG proxy (must be before Leptos catch-all).
        .route("/api/camera-stream", get(proxy::camera_stream_handler))
        // Reverse-proxy sub-router, mounted before the Leptos catch-all.
        .nest("/proxy", proxy_router)
        // Leptos SSR routes.
        .leptos_routes(&leptos_options, routes, {
            let options = leptos_options.clone();
            move || shell(options.clone())
        })
        // Serve compiled WASM bundle, CSS, and static assets.
        .nest_service(
            "/pkg",
            ServeDir::new(format!("{}/pkg", site_root.to_string())),
        )
        .fallback(leptos_axum::file_and_error_handler(shell))
        .with_state(leptos_options);

    // ── Start server ─────────────────────────────────────────────────────
    tracing::info!("Gaia Core listening on http://{addr}");

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app.into_make_service())
        .await
        .unwrap();
}

/// When compiled without the `ssr` feature, main is a no-op stub.
#[cfg(not(feature = "ssr"))]
pub fn main() {
    // Intentionally empty, the WASM library entry is in lib.rs.
}
