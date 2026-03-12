//! Home / dashboard page showing all running Gaia projects.

use leptos::prelude::*;
use leptos::prelude::{
    provide_context, signal, Effect, ElementChild, IntoView, Resource, Signal,
    Suspense,
};
use leptos::web_sys;

use crate::components::device_list::DeviceList;
use crate::components::mdns_panel::MdnsPanel;
use crate::components::project_card::ProjectCard;
use crate::server_fns::{
    get_capture_health, get_container_statuses, get_projects, get_update_status, CaptureHealth,
    ImageUpdate,
};

/// The main dashboard page.
#[component]
pub fn Home() -> impl IntoView {
    let targets = Resource::new(|| (), |_| get_projects());

    // ── Container lifecycle status polling ────────────────────────────
    let (poll_tick, set_poll_tick) = signal(0_u32);
    let status_resource =
        Resource::new(move || poll_tick.get(), |_| get_container_statuses());

    // Only propagate to children when the data actually changes.
    let (status_list, set_status_list) = signal(Vec::<(String, String)>::new());
    Effect::new(move || {
        let new = status_resource
            .get()
            .and_then(|r| r.ok())
            .unwrap_or_default();
        set_status_list.set(new);
    });
    provide_context(Signal::derive(move || status_list.get()));

    // ── Capture health (disk guard) polling ──────────────────────────
    let capture_health_resource =
        Resource::new(move || poll_tick.get(), |_| get_capture_health());

    let (capture_health_list, set_capture_health_list) =
        signal(Vec::<CaptureHealth>::new());
    Effect::new(move || {
        let new = capture_health_resource
            .get()
            .and_then(|r| r.ok())
            .unwrap_or_default();
        set_capture_health_list.set(new);
    });
    provide_context(Signal::derive(move || capture_health_list.get()));

    // ── Image update status polling ──────────────────────────────────
    let update_status_resource =
        Resource::new(move || poll_tick.get(), |_| get_update_status());

    let (update_list, set_update_list) = signal(Vec::<ImageUpdate>::new());
    Effect::new(move || {
        let new = update_status_resource
            .get()
            .and_then(|r| r.ok())
            .unwrap_or_default();
        set_update_list.set(new);
    });
    provide_context(Signal::derive(move || update_list.get()));

    // Poll every 3 s – only in the browser.
    #[cfg(feature = "hydrate")]
    {
        use wasm_bindgen::prelude::*;
        let cb = Closure::<dyn Fn()>::new(move || {
            set_poll_tick.update(|n| *n = n.wrapping_add(1));
        });
        web_sys::window()
            .unwrap()
            .set_interval_with_callback_and_timeout_and_arguments_0(
                cb.as_ref().unchecked_ref(),
                3_000,
            )
            .unwrap();
        cb.forget();
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
                                            processing_models=t.processing_models
                                        />
                                    }
                                }).collect::<Vec<_>>()}
                            </div>
                        }.into_any(),
                        Err(e) => view! {
                            <p class="error-state">"Error: " {e.to_string()}</p>
                        }.into_any(),
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
