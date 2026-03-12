//! Reusable card component for displaying a Gaia sub-project with per-container toggles.

use leptos::either::Either;
use leptos::prelude::*;
// Explicit imports for compatibility with Rust ≥1.94 glob re-export changes.
use leptos::prelude::{use_context, Signal};

use crate::components::toggle::ToggleSwitch;
use crate::server_fns::{
    toggle_audio_processing, toggle_container, toggle_light_processing, CaptureHealth,
    ImageUpdate,
};
use crate::config::AudioProcessingNode;

/// Map `(slug, kind)` to the container name used by the runtime.
///
/// Mirrors `containers::container_name()` but is available on both
/// server and client targets.
fn cname(slug: &str, kind: &str) -> String {
    if slug == "gmn" && kind == "processing" {
        "rms".into()
    } else if let Some(model_slug) = kind.strip_prefix("processing:") {
        format!("gaia-{slug}-processing-{model_slug}")
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
                <span class="lifecycle-badge lifecycle-pulling">"Pulling..."</span>
            }
            .into_any(),
            "starting" => view! {
                <span class="lifecycle-badge lifecycle-starting">"Starting..."</span>
            }
            .into_any(),
            s if s.starts_with("error") => view! {
                <span class="lifecycle-badge lifecycle-error">{s.to_string()}</span>
            }
            .into_any(),
            _ => ().into_any(),
        }
    }
}

/// Warning badge shown when the capture container has paused recording
/// because disk usage exceeds the configured threshold.
#[component]
fn DiskBadge(
    /// Project slug used to look up capture health from context.
    slug: String,
) -> impl IntoView {
    let health_list = use_context::<Signal<Vec<CaptureHealth>>>()
        .unwrap_or(Signal::derive(|| vec![]));

    let slug = slug.clone();
    move || {
        let slug = slug.clone();
        let list = health_list.get();
        if let Some(h) = list.iter().find(|h| h.slug == slug) {
            if h.capture_paused {
                return Either::Left(view! {
                    <span
                        class="lifecycle-badge lifecycle-error"
                        title=format!("Disk usage: {:.0}%", h.disk_usage_pct)
                    >
                        "⚠ Disk full – capture paused"
                    </span>
                });
            }
        }
        Either::Right(())
    }
}

/// Small badge that shows whether the camera is currently in day or
/// night mode, based on the brightness probe reported by the capture
/// server.
#[component]
fn CameraModeBadge(
    /// Project slug used to look up capture health.
    slug: String,
) -> impl IntoView {
    let health_list = use_context::<Signal<Vec<CaptureHealth>>>()
        .unwrap_or(Signal::derive(|| vec![]));

    move || {
        let list = health_list.get();
        let mode = list
            .iter()
            .find(|h| h.slug == slug)
            .and_then(|h| h.camera_mode.clone());
        match mode.as_deref() {
            Some("night") => view! {
                <span class="camera-mode-badge night" title="Camera in night / low-light mode">
                    "🌙 Night"
                </span>
            }
            .into_any(),
            Some("day") => view! {
                <span class="camera-mode-badge day" title="Camera in daylight mode">
                    "☀ Day"
                </span>
            }
            .into_any(),
            _ => ().into_any(),
        }
    }
}

/// Small icon that indicates a container image has a pending update.
#[component]
fn UpdateBadge(
    /// Container name to look up in the update status context.
    container_name: String,
) -> impl IntoView {
    let update_list = use_context::<Signal<Vec<ImageUpdate>>>()
        .unwrap_or(Signal::derive(|| vec![]));

    move || {
        let list = update_list.get();
        let has = list
            .iter()
            .any(|u| u.container == container_name && u.has_update);
        if has {
            Either::Left(view! {
                <span class="lifecycle-badge lifecycle-update" title="Image update available">
                    "⬆ Update"
                </span>
            })
        } else {
            Either::Right(())
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

    let (capture, set_capture) = signal(initial_capture);
    let (processing, set_processing) = signal(initial_processing);
    let (web, set_web) = signal(initial_web);

    let slug_for_disk = slug.clone();

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
            Action::new(move |new_state: &bool| {
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
            let (running, set_running) = signal(node.running);
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

    // Build model toggle views (audio and light projects).
    let slug_for_models = slug.clone();
    let model_toggle_views: Vec<_> = model_nodes
        .into_iter()
        .map(|(model_slug, model_name, container_kind, running, set_running)| {
            let model_slug_action = model_slug.clone();
            let is_light = slug_for_models == "light";
            let model_action = Action::new(move |new_state: &bool| {
                let slug = model_slug_action.clone();
                let new_state = *new_state;
                async move {
                    if is_light {
                        let _ = toggle_light_processing(slug, new_state).await;
                    } else {
                        let _ = toggle_audio_processing(slug, new_state).await;
                    }
                }
            });

            let on_model_toggle = Callback::new(move |val: bool| {
                set_running.set(val);
                model_action.dispatch(val);
            });

            // Look up lifecycle status for this model's container.
            let model_container = cname(&slug_for_models, &container_kind);
            let model_status = lookup_status(model_container.clone());

            let label = format!("🧠 {model_name}");

            view! {
                <div class="container-toggle-item">
                    <ToggleSwitch label=label checked=running on_toggle=on_model_toggle />
                    <LifecycleBadge status=model_status />
                    <UpdateBadge container_name=model_container />
                </div>
            }
        })
        .collect();

    view! {
        <div class="project-card">
            <div class="project-card-header">
                <h3 class="project-name">{name.clone()}</h3>
                <span class=status_class>{status_label}</span>
            </div>
            <p class="project-description">{description.clone()}</p>

            <div class="container-toggle-group">
                <div class="container-toggle-item">
                    <ToggleSwitch label="Capture".to_string() checked=capture on_toggle=on_capture />
                    <LifecycleBadge status=cap_status />
                    <UpdateBadge container_name=cname(&slug_for_disk, "capture") />
                    <DiskBadge slug=slug_for_disk.clone() />
                    <CameraModeBadge slug=slug_for_disk.clone() />
                </div>

                // Processing section: per-model toggles or a single toggle.
                {if has_model_nodes {
                    Either::Left(view! {
                        <div class="model-processing-group">
                            <span class="processing-group-label">"Processing"</span>
                            {model_toggle_views}
                            <a href="/settings" class="model-settings-link">"Manage models →"</a>
                        </div>
                    })
                } else {
                    Either::Right(view! {
                        <div class="container-toggle-item">
                            <ToggleSwitch label="Processing".to_string() checked=processing on_toggle=on_processing />
                            <LifecycleBadge status=proc_status />
                            <UpdateBadge container_name=cname(&slug, "processing") />
                        </div>
                    })
                }}

                <div class="container-toggle-item">
                    <ToggleSwitch label="Web".to_string() checked=web on_toggle=on_web />
                    <LifecycleBadge status=web_status />
                    <UpdateBadge container_name=cname(&slug, "web") />
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
