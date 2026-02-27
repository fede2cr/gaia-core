//! Projects page — detailed view of each sub-project with embedded iframe.

use leptos::*;

use crate::server_fns::get_projects;

/// The projects page shows each enabled project in an iframe via the reverse proxy.
#[component]
pub fn ProjectsPage() -> impl IntoView {
    let targets = create_resource(|| (), |_| get_projects());

    view! {
        <section class="projects-page">
            <h1>"Projects"</h1>
            <p class="page-description">
                "Each project's web interface is served through the Gaia Core reverse proxy. "
                "Click the tabs below to switch between projects."
            </p>

            <Suspense fallback=move || view! { <p class="loading">"Loading projects…"</p> }>
                {move || {
                    targets.get().map(|result| match result {
                        Ok(ts) => {
                            let enabled: Vec<_> = ts.into_iter().filter(|t| t.web_enabled).collect();
                            if enabled.is_empty() {
                                view! {
                                    <p class="empty-state">"No projects are currently enabled. Enable them from the Dashboard."</p>
                                }.into_view()
                            } else {
                                view! {
                                    <div class="project-tabs">
                                        {enabled
                                            .into_iter()
                                            .map(|t| {
                                                let slug = t.slug.clone();
                                                let name = t.name.clone();
                                                let src = format!("/proxy/{slug}/");
                                                view! {
                                                    <details class="project-tab" open=false>
                                                        <summary class="tab-header">{&name}</summary>
                                                        <div class="tab-content">
                                                            <iframe
                                                                src=src
                                                                class="project-iframe"
                                                                title=format!("{name} interface")
                                                            ></iframe>
                                                        </div>
                                                    </details>
                                                }
                                            })
                                            .collect_view()}
                                    </div>
                                }.into_view()
                            }
                        }
                        Err(e) => view! {
                            <p class="error-state">"Error loading projects: " {e.to_string()}</p>
                        }.into_view(),
                    })
                }}
            </Suspense>
        </section>
    }
}
