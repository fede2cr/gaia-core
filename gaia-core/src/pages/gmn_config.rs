//! GMN configuration page for callsign and camera pre-alignment live stream.

use leptos::either::Either;
use leptos::prelude::*;

use crate::server_fns::{get_gmn_config, set_gmn_callsign, toggle_container};

/// Configuration page for the Global Meteor Network project.
///
/// Reached via the "Config" button on the GMN project card.
#[component]
pub fn GmnConfigPage() -> impl IntoView {
    let config = Resource::new(|| (), |_| get_gmn_config());

    view! {
        <section class="gmn-config-page">
            <a href="/" class="back-link">"← Back to Dashboard"</a>
            <h1>"Global Meteor Network - Configuration"</h1>

            <Suspense fallback=move || view! { <p class="loading">"Loading configuration..."</p> }>
                {move || {
                    config.get().map(|result| match result {
                        Ok(cfg) => {
                            let camera_device = cfg.camera_device.clone();
                            let camera_label = cfg.camera_label.clone();
                            let initial_config_enabled = cfg.config_enabled;

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

                                    {match camera_device {
                                        Some(device) => {
                                            let device_text = camera_label
                                                .unwrap_or_else(|| device.clone());
                                            Either::Left(view! {
                                                <CameraPreview
                                                    device_label=device_text
                                                    initial_enabled=initial_config_enabled
                                                />
                                            })
                                        }
                                        None => Either::Right(view! {
                                            <p class="empty-state">
                                                "No camera assigned to GMN. "
                                                <a href="/">"Go to Dashboard → Capture Devices"</a>
                                                " to assign a camera."
                                            </p>
                                        }),
                                    }}
                                </div>
                            }
                            .into_any()
                        }
                        Err(e) => view! {
                            <p class="error-state">"Error loading config: " {e.to_string()}</p>
                        }
                        .into_any(),
                    })
                }}
            </Suspense>
        </section>
    }
}

// ── Camera preview component ─────────────────────────────────────────────

/// Starts/stops the `gaia-gmn-config` container and shows its MJPEG stream.
#[component]
fn CameraPreview(
    device_label: String,
    initial_enabled: bool,
) -> impl IntoView {
    let (streaming, set_streaming) = signal(initial_enabled);
    let (loading, set_loading) = signal(false);
    let (error_msg, set_error) = signal(Option::<String>::None);
    // A counter appended to the stream URL as a cache-buster so the browser
    // makes a fresh request each time the preview is (re-)started.
    let (stream_epoch, set_stream_epoch) = signal(0u64);

    let toggle_action = Action::new(move |new_state: &bool| {
        let new_state = *new_state;
        async move {
            toggle_container("gmn".into(), "config".into(), new_state).await
        }
    });

    // Handle toggle result.
    Effect::new(move || {
        if let Some(result) = toggle_action.value().get() {
            set_loading.set(false);
            match result {
                Ok(_) => {
                    set_error.set(None);
                    // Bump the epoch so the <img> src changes and the browser
                    // initiates a new request to the (now-running) container.
                    set_stream_epoch.update(|e| *e += 1);
                }
                Err(e) => {
                    // Revert the toggle on error.
                    set_streaming.update(|s| *s = !*s);
                    set_error.set(Some(format!("Failed to toggle stream: {e}")));
                }
            }
        }
    });

    let on_toggle = move |_| {
        let new_state = !streaming.get_untracked();
        set_streaming.set(new_state);
        set_loading.set(true);
        set_error.set(None);
        toggle_action.dispatch(new_state);
    };

    // Reactive stream URL with an epoch cache-buster so the browser
    // re-fetches after each toggle.
    let stream_src = move || {
        let epoch = stream_epoch.get();
        format!("/api/camera-stream?t={epoch}")
    };

    view! {
        <div class="camera-controls">
            <button
                class="btn btn-primary"
                on:click=on_toggle
                disabled=move || loading.get()
            >
                {move || {
                    if loading.get() {
                        if streaming.get() {
                            "Stopping...".to_string()
                        } else {
                            "Starting...".to_string()
                        }
                    } else if streaming.get() {
                        "⏹ Stop Preview".to_string()
                    } else {
                        "▶ Start Preview".to_string()
                    }
                }}
            </button>
            <span class="camera-device-label">{device_label}</span>
        </div>

        {move || error_msg.get().map(|msg| view! {
            <p class="location-status location-error">{msg}</p>
        })}

        <Show when=move || streaming.get() && !loading.get() fallback=|| ()>
            <div class="camera-stream-container">
                <img
                    src=stream_src
                    class="camera-stream"
                    alt="Camera live feed"
                />
                <p class="camera-stream-hint">
                    "Adjust your camera position and focus. "
                    "Stop the preview before starting RMS capture."
                </p>
            </div>
        </Show>
    }
}

// ── Callsign form (inner component) ──────────────────────────────────────

/// Editable callsign field with save button and feedback.
#[component]
fn CallsignForm(
    /// Pre-populated callsign value.
    initial: String,
) -> impl IntoView {
    let (callsign, set_callsign) = signal(initial);
    let save_action = Action::new(move |val: &String| {
        let val = val.clone();
        async move { set_gmn_callsign(val).await }
    });

    let (status_msg, set_status) = signal(Option::<String>::None);

    // Show feedback after save completes.
    Effect::new(move || {
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
                        prop:value=move || callsign.get()
                        on:input=move |ev| set_callsign.set(event_target_value(&ev))
                    />
                </label>
                <button
                    type="submit"
                    class="location-save-btn"
                    disabled=move || save_action.pending().get()
                >
                    {move || if save_action.pending().get() { "Saving..." } else { "Save" }}
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
