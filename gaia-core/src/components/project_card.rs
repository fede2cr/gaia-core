//! Reusable card component for displaying a Gaia sub-project with per-container toggles.

use leptos::*;

use crate::components::toggle::ToggleSwitch;
use crate::server_fns::toggle_container;

/// Map `(slug, kind)` to the container name used by the runtime.
///
/// Mirrors `containers::container_name()` but is available on both
/// server and client targets.
fn cname(slug: &str, kind: &str) -> String {
    if slug == "gmn" && kind == "processing" {
        "rms".into()
    } else {
        format!("gaia-{slug}-{kind}")
    }
}

/// Inline lifecycle badge shown next to a toggle when the container is
/// in a transitional state (pulling / starting / error).
#[component]
fn LifecycleBadge(
    /// Reactive status string for this container.
    #[prop(into)]
    status: Signal<String>,
) -> impl IntoView {
    move || {
        let s = status.get();
        match s.as_str() {
            "pulling" => view! {
                <span class="lifecycle-badge lifecycle-pulling">"Pulling…"</span>
            }
            .into_view(),
            "starting" => view! {
                <span class="lifecycle-badge lifecycle-starting">"Starting…"</span>
            }
            .into_view(),
            s if s.starts_with("error") => view! {
                <span class="lifecycle-badge lifecycle-error">{s.to_string()}</span>
            }
            .into_view(),
            _ => ().into_view(),
        }
    }
}

/// A card showing project name, description, and individual toggles for
/// capture / processing / web containers.
#[component]
pub fn ProjectCard(
    /// Human-readable project name.
    name: String,
    /// Short project slug (used in the proxy path).
    slug: String,
    /// Brief description.
    description: String,
    /// TCP port the upstream listens on.
    port: u16,
    /// Initial enabled state for the capture container.
    initial_capture: bool,
    /// Initial enabled state for the processing container.
    initial_processing: bool,
    /// Initial enabled state for the web container.
    initial_web: bool,
) -> impl IntoView {
    let (capture, set_capture) = create_signal(initial_capture);
    let (processing, set_processing) = create_signal(initial_processing);
    let (web, set_web) = create_signal(initial_web);

    let slug_for_link = slug.clone();
    let slug_for_config = slug.clone();

    // ── Lifecycle status from context (polled by Home) ───────────────
    let status_list = use_context::<Signal<Vec<(String, String)>>>()
        .unwrap_or(Signal::derive(|| vec![]));

    let cap_name = cname(&slug, "capture");
    let proc_name = cname(&slug, "processing");
    let web_name = cname(&slug, "web");

    let lookup_status = move |name: String| {
        let name = name.clone();
        Signal::derive(move || {
            status_list
                .get()
                .iter()
                .find(|(n, _)| *n == name)
                .map(|(_, s)| s.clone())
                .unwrap_or_default()
        })
    };

    let cap_status = lookup_status(cap_name);
    let proc_status = lookup_status(proc_name);
    let web_status = lookup_status(web_name);

    // Helper: create a toggle action for a given container kind.
    let make_action = {
        let slug = slug.clone();
        move |kind: &'static str| {
            let slug = slug.clone();
            create_action(move |new_state: &bool| {
                let slug = slug.clone();
                let new_state = *new_state;
                let kind = kind.to_string();
                async move {
                    leptos::logging::log!(
                        "[toggle] calling server fn: slug={}, kind={}, enabled={}",
                        &slug, &kind, new_state
                    );
                    match toggle_container(slug, kind, new_state).await {
                        Ok(_) => {
                            leptos::logging::log!("[toggle] server fn succeeded");
                        }
                        Err(e) => {
                            leptos::logging::error!(
                                "[toggle] server fn FAILED: {:?}", e
                            );
                        }
                    }
                }
            })
        }
    };

    let capture_action = make_action("capture");
    let processing_action = make_action("processing");
    let web_action = make_action("web");

    let on_capture = Callback::new(move |val: bool| {
        set_capture.set(val);
        capture_action.dispatch(val);
    });
    let on_processing = Callback::new(move |val: bool| {
        set_processing.set(val);
        processing_action.dispatch(val);
    });
    let on_web = Callback::new(move |val: bool| {
        set_web.set(val);
        web_action.dispatch(val);
    });

    let any_active = move || capture.get() || processing.get() || web.get();
    let status_class = move || {
        if any_active() {
            "status-badge status-active"
        } else {
            "status-badge status-disabled"
        }
    };
    let status_label = move || if any_active() { "Active" } else { "Disabled" };

    let proxy_href = format!("/proxy/{slug_for_link}/");

    view! {
        <div class="project-card">
            <div class="project-card-header">
                <h3 class="project-name">{&name}</h3>
                <span class=status_class>{status_label}</span>
            </div>
            <p class="project-description">{&description}</p>

            <div class="container-toggle-group">
                <div class="container-toggle-item">
                    <ToggleSwitch label="Capture".to_string()   checked=capture    on_toggle=on_capture />
                    <LifecycleBadge status=cap_status />
                </div>
                <div class="container-toggle-item">
                    <ToggleSwitch label="Processing".to_string() checked=processing on_toggle=on_processing />
                    <LifecycleBadge status=proc_status />
                </div>
                <div class="container-toggle-item">
                    <ToggleSwitch label="Web".to_string()        checked=web        on_toggle=on_web />
                    <LifecycleBadge status=web_status />
                </div>
            </div>

            <div class="project-card-footer">
                <span class="project-port">"Port: " {port}</span>
                <div class="project-card-actions">
                    {(slug_for_config == "gmn").then(|| view! {
                        <a href="/gmn-config" class="btn btn-secondary btn-sm">"⚙ Config"</a>
                    })}
                    <Show
                        when=move || web.get()
                        fallback=|| view! {
                            <span class="project-link-disabled">"Web disabled"</span>
                        }
                    >
                        <a href={proxy_href.clone()} class="project-link" target="_blank">
                            "Open Interface →"
                        </a>
                    </Show>
                </div>
            </div>
        </div>
    }
}
