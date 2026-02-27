//! Home / dashboard page — overview of all running Gaia projects.

use leptos::*;

use crate::components::device_list::DeviceList;
use crate::components::mdns_panel::MdnsPanel;
use crate::components::project_card::ProjectCard;
use crate::server_fns::get_projects;

/// The main dashboard page.
#[component]
pub fn Home() -> impl IntoView {
    let targets = create_resource(|| (), |_| get_projects());

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
            <Suspense fallback=move || view! { <p class="loading">"Loading projects…"</p> }>
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
