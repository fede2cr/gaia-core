//! GMN configuration page — callsign + camera pre-alignment live stream.

use leptos::*;

use crate::server_fns::{get_gmn_config, set_gmn_callsign};

/// Configuration page for the Global Meteor Network project.
///
/// Reached via the "Config" button on the GMN project card.
#[component]
pub fn GmnConfigPage() -> impl IntoView {
    let config = create_resource(|| (), |_| get_gmn_config());
    let (streaming, set_streaming) = create_signal(false);

    view! {
        <section class="gmn-config-page">
            <a href="/" class="back-link">"← Back to Dashboard"</a>
            <h1>"Global Meteor Network — Configuration"</h1>

            <Suspense fallback=move || view! { <p class="loading">"Loading configuration…"</p> }>
                {move || {
                    config.get().map(|result| match result {
                        Ok(cfg) => {
                            let camera_device = cfg.camera_device.clone();
                            let camera_label = cfg.camera_label.clone();
                            let stream_url = camera_device
                                .as_ref()
                                .map(|d| format!("/api/camera-stream?device={d}"));

                            view! {
                                // ── Callsign ─────────────────────────
                                <div class="config-section">
                                    <h2>"Station Callsign"</h2>
                                    <p class="section-description">
                                        "Your GMN station identifier (e.g. "
                                        <code>"US000A"</code>
                                        "). Assigned by the Global Meteor Network."
                                    </p>
                                    <CallsignForm initial=cfg.callsign.clone() />
                                </div>

                                // ── Camera Pre-align ─────────────────
                                <div class="config-section">
                                    <h2>"Camera Pre-align"</h2>
                                    <p class="section-description">
                                        "Live preview for aiming and focusing the camera before starting capture."
                                    </p>

                                    {match stream_url {
                                        Some(url) => {
                                            let device_text = camera_label
                                                .unwrap_or_else(|| camera_device.unwrap_or_default());
                                            view! {
                                                <div class="camera-controls">
                                                    <button
                                                        class="btn btn-primary"
                                                        on:click=move |_| set_streaming.update(|s| *s = !*s)
                                                    >
                                                        {move || if streaming.get() { "⏹ Stop Preview" } else { "▶ Start Preview" }}
                                                    </button>
                                                    <span class="camera-device-label">{device_text}</span>
                                                </div>
                                                <Show when=move || streaming.get() fallback=|| ()>
                                                    <div class="camera-stream-container">
                                                        <img
                                                            src=url.clone()
                                                            class="camera-stream"
                                                            alt="Camera live feed"
                                                        />
                                                    </div>
                                                </Show>
                                            }.into_view()
                                        }
                                        None => view! {
                                            <p class="empty-state">
                                                "No camera assigned to GMN. "
                                                <a href="/">"Go to Dashboard → Capture Devices"</a>
                                                " to assign a camera."
                                            </p>
                                        }.into_view(),
                                    }}
                                </div>
                            }
                            .into_view()
                        }
                        Err(e) => view! {
                            <p class="error-state">"Error loading config: " {e.to_string()}</p>
                        }
                        .into_view(),
                    })
                }}
            </Suspense>
        </section>
    }
}

// ── Callsign form (inner component) ──────────────────────────────────────

/// Editable callsign field with save button and feedback.
#[component]
fn CallsignForm(
    /// Pre-populated callsign value.
    initial: String,
) -> impl IntoView {
    let (callsign, set_callsign) = create_signal(initial);
    let save_action = create_action(move |val: &String| {
        let val = val.clone();
        async move { set_gmn_callsign(val).await }
    });

    let (status_msg, set_status) = create_signal(Option::<String>::None);

    // Show feedback after save completes.
    create_effect(move |_| {
        if let Some(result) = save_action.value().get() {
            match result {
                Ok(_) => set_status.set(Some("Callsign saved.".into())),
                Err(e) => set_status.set(Some(format!("Error: {e}"))),
            }
        }
    });

    let on_submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        set_status.set(None);
        save_action.dispatch(callsign.get_untracked());
    };

    view! {
        <form class="callsign-form" on:submit=on_submit>
            <div class="callsign-fields">
                <label class="field-label">
                    "Callsign"
                    <input
                        type="text"
                        class="field-input"
                        placeholder="e.g. US000A"
                        prop:value=callsign
                        on:input=move |ev| set_callsign.set(event_target_value(&ev))
                    />
                </label>
                <button
                    type="submit"
                    class="location-save-btn"
                    disabled=move || save_action.pending().get()
                >
                    {move || if save_action.pending().get() { "Saving…" } else { "Save" }}
                </button>
            </div>
            {move || {
                status_msg.get().map(|msg| {
                    let cls = if msg.starts_with("Error") {
                        "location-status location-error"
                    } else {
                        "location-status location-ok"
                    };
                    view! { <p class=cls>{msg}</p> }
                })
            }}
        </form>
    }
}
