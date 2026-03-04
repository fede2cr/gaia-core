//! Reusable card component for displaying a Gaia sub-project with per-container toggles.

use leptos::*;

use crate::components::toggle::ToggleSwitch;
use crate::server_fns::{toggle_audio_processing, toggle_container};
use crate::config::AudioProcessingNode;

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
///
/// For the audio project, `processing_models` contains per-model toggles
/// that replace the single "Processing" toggle.
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
    /// Per-model processing nodes (audio project only).
    /// When non-empty, replaces the single "Processing" toggle.
    #[prop(optional, default = vec![])]
    processing_models: Vec<AudioProcessingNode>,
) -> impl IntoView {
    let has_model_nodes = !processing_models.is_empty();

    let (capture, set_capture) = create_signal(initial_capture);
    let (processing, set_processing) = create_signal(initial_processing);
    let (web, set_web) = create_signal(initial_web);

    let slug_for_link = slug.clone();
    let slug_for_config = slug.clone();

    // ── Lifecycle status from context (polled by Home) ───────────────
    let status_list = use_context::<Signal<Vec<(String, String)>>>()
        .unwrap_or(Signal::derive(|| vec![]));

    let cap_name = cname(&slug, "capture");
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
    let web_status = lookup_status(web_name.clone());

    // Traditional single-processing status (for non-audio projects).
    let proc_name = cname(&slug, "processing");
    let proc_status = lookup_status(proc_name);

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

    // ── Per-model processing node signals and actions ────────────────
    let model_nodes: Vec<(
        String,   // model_slug
        String,   // model_name
        String,   // container_kind
        ReadSignal<bool>,
        WriteSignal<bool>,
    )> = processing_models
        .iter()
        .map(|node| {
            let (running, set_running) = create_signal(node.running);
            (
                node.model_slug.clone(),
                node.model_name.clone(),
                node.container_kind.clone(),
                running,
                set_running,
            )
        })
        .collect();

    // Track whether ANY model node is running (for the status badge).
    let model_running_signals: Vec<ReadSignal<bool>> =
        model_nodes.iter().map(|(_, _, _, r, _)| *r).collect();
    let any_model_running = move || {
        model_running_signals
            .iter()
            .any(|sig| sig.get())
    };

    let any_active = Signal::derive(move || {
        capture.get()
            || web.get()
            || if has_model_nodes {
                any_model_running()
            } else {
                processing.get()
            }
    });
    let status_class = move || {
        if any_active.get() {
            "status-badge status-active"
        } else {
            "status-badge status-disabled"
        }
    };
    let status_label = move || if any_active.get() { "Active" } else { "Disabled" };

    let proxy_href = format!("/proxy/{slug_for_link}/");

    // Build model toggle views (audio project only).
    let model_toggle_views: Vec<_> = model_nodes
        .into_iter()
        .map(|(model_slug, model_name, container_kind, running, set_running)| {
            let model_slug_action = model_slug.clone();
            let model_action = create_action(move |new_state: &bool| {
                let slug = model_slug_action.clone();
                let new_state = *new_state;
                async move {
                    let _ = toggle_audio_processing(slug, new_state).await;
                }
            });

            let on_model_toggle = Callback::new(move |val: bool| {
                set_running.set(val);
                model_action.dispatch(val);
            });

            // Look up lifecycle status for this model's container.
            let model_container = cname("audio", &container_kind);
            let model_status = lookup_status(model_container);

            let label = format!("🧠 {model_name}");

            view! {
                <div class="container-toggle-item">
                    <ToggleSwitch label=label checked=running on_toggle=on_model_toggle />
                    <LifecycleBadge status=model_status />
                </div>
            }
        })
        .collect();

    view! {
        <div class="project-card">
            <div class="project-card-header">
                <h3 class="project-name">{&name}</h3>
                <span class=status_class>{status_label}</span>
            </div>
            <p class="project-description">{&description}</p>

            <div class="container-toggle-group">
                <div class="container-toggle-item">
                    <ToggleSwitch label="Capture".to_string() checked=capture on_toggle=on_capture />
                    <LifecycleBadge status=cap_status />
                </div>

                // Processing section: per-model toggles or a single toggle.
                {if has_model_nodes {
                    view! {
                        <div class="model-processing-group">
                            <span class="processing-group-label">"Processing"</span>
                            {model_toggle_views.clone()}
                            <a href="/settings" class="model-settings-link">"Manage models →"</a>
                        </div>
                    }.into_view()
                } else {
                    view! {
                        <div class="container-toggle-item">
                            <ToggleSwitch label="Processing".to_string() checked=processing on_toggle=on_processing />
                            <LifecycleBadge status=proc_status />
                        </div>
                    }.into_view()
                }}

                <div class="container-toggle-item">
                    <ToggleSwitch label="Web".to_string() checked=web on_toggle=on_web />
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
