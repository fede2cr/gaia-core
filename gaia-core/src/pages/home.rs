//! Home / dashboard page showing all running Gaia projects.

use leptos::*;

use crate::components::device_list::DeviceList;
use crate::components::mdns_panel::MdnsPanel;
use crate::components::project_card::ProjectCard;
use crate::server_fns::{get_container_statuses, get_projects};

/// The main dashboard page.
#[component]
pub fn Home() -> impl IntoView {
    let targets = create_resource(|| (), |_| get_projects());

    // ── Container lifecycle status polling ────────────────────────────
    let (poll_tick, set_poll_tick) = create_signal(0_u32);
    let status_resource = create_resource(move || poll_tick.get(), |_| get_container_statuses());

    // Flatten the resource into a simple signal for child components.
    let status_list: Signal<Vec<(String, String)>> = Signal::derive(move || {
        status_resource
            .get()
            .and_then(|r| r.ok())
            .unwrap_or_default()
    });
    provide_context(status_list);

    // Poll every 3 s – only in the browser (set_interval is a wasm-bindgen API).
    #[cfg(feature = "hydrate")]
    {
        set_interval(
            move || {
                set_poll_tick.update(|n| *n = n.wrapping_add(1));
            },
            std::time::Duration::from_secs(3),
        );
    }
    // Suppress unused-variable warning on the server build.
    #[cfg(not(feature = "hydrate"))]
    let _ = set_poll_tick;

    view! {
        <section class="dashboard">
            <header class="dashboard-header">
                <h1>"Gaia Dashboard"</h1>
                <p class="dashboard-subtitle">
                    "Central control plane for all Gaia environmental monitoring projects."
                </p>
            </header>

            // ── Applications (with on/off toggles) ──────────────────
            <h2 class="section-heading">"Applications"</h2>
            <Suspense fallback=move || view! { <p class="loading">"Loading projects..."</p> }>
                {move || {
                    targets.get().map(|result| match result {
                        Ok(ts) => view! {
                            <div class="project-grid">
                                {ts.into_iter().map(|t| {
                                    view! {
                                        <ProjectCard
                                            name=t.name
                                            slug=t.slug
                                            description=t.description
                                            port=t.port
                                            initial_capture=t.capture_enabled
                                            initial_processing=t.processing_enabled
                                            initial_web=t.web_enabled
                                        />
                                    }
                                }).collect_view()}
                            </div>
                        }.into_view(),
                        Err(e) => view! {
                            <p class="error-state">"Error: " {e.to_string()}</p>
                        }.into_view(),
                    })
                }}
            </Suspense>

            // ── Capture Devices ──────────────────────────────────────
            <DeviceList/>

            // ── Remote Capture Nodes (mDNS) ─────────────────────────
            <MdnsPanel/>
        </section>
    }
}
