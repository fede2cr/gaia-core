//! Reusable card component for displaying a Gaia sub-project with per-container toggles.

use leptos::*;

use crate::components::toggle::ToggleSwitch;
use crate::server_fns::toggle_container;

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
                    let _ = toggle_container(slug, kind, new_state).await;
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
                <ToggleSwitch label="Capture".to_string()   checked=capture    on_toggle=on_capture />
                <ToggleSwitch label="Processing".to_string() checked=processing on_toggle=on_processing />
                <ToggleSwitch label="Web".to_string()        checked=web        on_toggle=on_web />
            </div>

            <div class="project-card-footer">
                <span class="project-port">"Port: " {port}</span>
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
    }
}
