//! Settings / configuration page.

use leptos::*;

use crate::components::device_list::DeviceList;
use crate::components::mdns_panel::MdnsPanel;
use crate::components::toggle::ToggleSwitch;
use crate::server_fns::{
    get_audio_models, get_debug_settings, get_location, get_processing_threads, get_projects,
    set_location, set_processing_threads, toggle_audio_model, toggle_debug_logging, DebugState,
};

/// Settings page showing current proxy configuration, port assignments,
/// detected hardware and remote nodes.
#[component]
pub fn SettingsPage() -> impl IntoView {
    let targets = create_resource(|| (), |_| get_projects());

    view! {
        <section class="settings-page">
            <h1>"Settings"</h1>
            <p class="page-description">
                "Current proxy configuration. In a future release you will be able to edit "
                "these settings and run the setup wizard from here."
            </p>

            <h2>"Port Allocation"</h2>
            <table class="port-table">
                <thead>
                    <tr>
                        <th>"Project"</th>
                        <th>"Slug"</th>
                        <th>"Upstream URL"</th>
                        <th>"Port"</th>
                        <th>"Status"</th>
                    </tr>
                </thead>
                <tbody>
                    <tr>
                        <td>"Gaia Core"</td>
                        <td>"-"</td>
                        <td>"(this server)"</td>
                        <td>"3100"</td>
                        <td><span class="status-badge status-active">"Active"</span></td>
                    </tr>
                    <Suspense fallback=move || view! {
                        <tr><td colspan="5">"Loading..."</td></tr>
                    }>
                        {move || {
                            targets.get().map(|result| match result {
                                Ok(ts) => ts.into_iter().map(|t| {
                                    let status_class = if t.any_enabled() {
                                        "status-badge status-active"
                                    } else {
                                        "status-badge status-disabled"
                                    };
                                    let status_text = if t.any_enabled() { "Active" } else { "Disabled" };
                                    let containers = [
                                        ("C", t.capture_enabled),
                                        ("P", t.processing_enabled),
                                        ("W", t.web_enabled),
                                        ("Cfg", t.config_enabled),
                                    ];
                                    view! {
                                        <tr>
                                            <td>{&t.name}</td>
                                            <td><code>{&t.slug}</code></td>
                                            <td><code>{&t.upstream_url}</code></td>
                                            <td>{t.port}</td>
                                            <td>
                                                <span class=status_class>{status_text}</span>
                                                <span class="container-badges">
                                                    {containers
                                                        .into_iter()
                                                        .map(|(label, on)| {
                                                            let cls = if on {
                                                                "container-badge container-on"
                                                            } else {
                                                                "container-badge container-off"
                                                            };
                                                            view! { <span class=cls>{label}</span> }
                                                        })
                                                        .collect_view()}
                                                </span>
                                            </td>
                                        </tr>
                                    }
                                }).collect_view(),
                                Err(_) => view! {
                                    <tr><td colspan="5">"Error loading projects"</td></tr>
                                }.into_view(),
                            })
                        }}
                    </Suspense>
                </tbody>
            </table>

            <h2>"Environment Variables"</h2>
            <p>"Override upstream URLs with these environment variables:"</p>
            <table class="env-table">
                <thead>
                    <tr>
                        <th>"Variable"</th>
                        <th>"Default"</th>
                        <th>"Description"</th>
                    </tr>
                </thead>
                <tbody>
                    <tr>
                        <td><code>"GAIA_AUDIO_URL"</code></td>
                        <td><code>"http://localhost:3000"</code></td>
                        <td>"Gaia Audio web interface"</td>
                    </tr>
                    <tr>
                        <td><code>"GAIA_RADIO_URL"</code></td>
                        <td><code>"http://localhost:8080"</code></td>
                        <td>"Gaia Radio flight tracker"</td>
                    </tr>
                    <tr>
                        <td><code>"GAIA_GMN_URL"</code></td>
                        <td><code>"http://localhost:8180"</code></td>
                        <td>"Global Meteor Network (RMS)"</td>
                    </tr>
                </tbody>
            </table>

            // ── Station Location ─────────────────────────────────────
            <h2>"Station Location"</h2>
            <p class="page-description">
                "Latitude and longitude of your monitoring station. "
                "Used by gaia-audio (BirdNET location filter) and RMS (meteor trajectory calculation)."
            </p>
            <LocationForm/>

            // ── Audio Models ─────────────────────────────────────────
            <h2>"Audio Models"</h2>
            <p class="page-description">
                "Enable bioacoustic models to make them available as audio processing nodes. "
                "Each enabled model can run as a separate processing container, sharing "
                "captured recordings and only deleting files once all models have analysed them."
            </p>
            <AudioModelSettings/>

            // ── Processing Performance ───────────────────────────────
            <h2>"Processing Performance"</h2>
            <p class="page-description">
                "Number of parallel threads for audio analysis. "
                "Higher values process recordings faster but use more CPU and RAM "
                "(each thread loads its own copy of the model). Takes effect on next container restart."
            </p>
            <ProcessingThreadsSettings/>

            // ── Debug Logging ────────────────────────────────────────
            <h2>"Debug Logging"</h2>
            <p class="page-description">
                "Enable verbose debug logs for each project's containers. "
                "Useful for diagnosing issues like files not being deleted or "
                "disk space not being freed. Takes effect on the next container restart."
            </p>
            <DebugLoggingSettings/>

            // ── Hardware & Network Discovery ────────────────────────
            <h2>"Hardware & Network"</h2>
            <DeviceList/>
            <MdnsPanel/>
        </section>
    }
}

/// Inline component for the latitude / longitude form.
#[component]
fn LocationForm() -> impl IntoView {
    let location = create_resource(|| (), |_| get_location());
    let save_action = create_action(move |(lat, lon): &(String, String)| {
        let lat = lat.clone();
        let lon = lon.clone();
        async move { set_location(lat, lon).await }
    });

    // Local signals for the input fields.
    let (lat, set_lat) = create_signal(String::new());
    let (lon, set_lon) = create_signal(String::new());
    let (status_msg, set_status) = create_signal(Option::<String>::None);

    // Populate fields once the resource loads.
    create_effect(move |_| {
        if let Some(Ok(loc)) = location.get() {
            set_lat.set(loc.latitude);
            set_lon.set(loc.longitude);
        }
    });

    // Show feedback after the action completes.
    create_effect(move |_| {
        if let Some(result) = save_action.value().get() {
            match result {
                Ok(_) => set_status.set(Some("Location saved.".into())),
                Err(e) => set_status.set(Some(format!("Error: {e}"))),
            }
        }
    });

    let on_submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        set_status.set(None);
        save_action.dispatch((lat.get(), lon.get()));
    };

    view! {
        <Suspense fallback=move || view! { <p>"Loading..."</p> }>
            <form class="location-form" on:submit=on_submit>
                <div class="location-fields">
                    <label class="location-label">
                        "Latitude"
                        <input
                            type="text"
                            class="location-input"
                            placeholder="e.g. -34.6037"
                            prop:value=lat
                            on:input=move |ev| set_lat.set(event_target_value(&ev))
                        />
                    </label>
                    <label class="location-label">
                        "Longitude"
                        <input
                            type="text"
                            class="location-input"
                            placeholder="e.g. -58.3816"
                            prop:value=lon
                            on:input=move |ev| set_lon.set(event_target_value(&ev))
                        />
                    </label>
                    <button type="submit" class="location-save-btn"
                        disabled=move || save_action.pending().get()
                    >
                        {move || if save_action.pending().get() { "Saving..." } else { "Save" }}
                    </button>
                </div>
                {move || status_msg.get().map(|msg| {
                    let cls = if msg.starts_with("Error") {
                        "location-status location-error"
                    } else {
                        "location-status location-ok"
                    };
                    view! { <p class=cls>{msg}</p> }
                })}
            </form>
        </Suspense>
    }
}

/// Audio model toggles for the Settings page.
#[component]
fn AudioModelSettings() -> impl IntoView {
    let models = create_resource(|| (), |_| get_audio_models());
    let (model_list, set_model_list) = create_signal(Vec::<crate::server_fns::AudioModelInfo>::new());

    // Populate local state when the resource loads.
    create_effect(move |_| {
        if let Some(Ok(ms)) = models.get() {
            set_model_list.set(ms);
        }
    });

    view! {
        <Suspense fallback=move || view! { <p class="loading">"Loading models..."</p> }>
            <div class="audio-models-list">
                <For
                    each=move || model_list.get()
                    key=|m| m.slug.clone()
                    children=move |model| {
                        let slug = model.slug.clone();
                        let name = model.name.clone();
                        let description = model.description.clone();

                        let (enabled, set_enabled) = create_signal(model.enabled);
                        let toggle_action = {
                            let slug = slug.clone();
                            create_action(move |new_state: &bool| {
                                let slug = slug.clone();
                                let new_state = *new_state;
                                async move {
                                    let _ = toggle_audio_model(slug, new_state).await;
                                }
                            })
                        };

                        let on_toggle = Callback::new(move |val: bool| {
                            set_enabled.set(val);
                            toggle_action.dispatch(val);
                        });

                        view! {
                            <div class="audio-model-row">
                                <div class="audio-model-info">
                                    <span class="audio-model-name">{name}</span>
                                    <span class="audio-model-description">{description}</span>
                                </div>
                                <ToggleSwitch
                                    label=slug
                                    checked=enabled
                                    on_toggle=on_toggle
                                />
                            </div>
                        }
                    }
                />
                {move || {
                    if model_list.get().is_empty() {
                        Some(view! {
                            <p class="empty-state">"No audio models configured."</p>
                        })
                    } else {
                        None
                    }
                }}
            </div>
        </Suspense>
    }
}

/// Per-project debug logging toggles for the Settings page.
#[component]
fn DebugLoggingSettings() -> impl IntoView {
    let debug_res = create_resource(|| (), |_| get_debug_settings());
    let (items, set_items) = create_signal(Vec::<DebugState>::new());

    create_effect(move |_| {
        if let Some(Ok(ds)) = debug_res.get() {
            set_items.set(ds);
        }
    });

    view! {
        <Suspense fallback=move || view! { <p class="loading">"Loading..."</p> }>
            <div class="debug-logging-list">
                <For
                    each=move || items.get()
                    key=|d| d.slug.clone()
                    children=move |item| {
                        let slug = item.slug.clone();
                        let name = item.name.clone();

                        let (enabled, set_enabled) = create_signal(item.enabled);
                        let toggle_action = {
                            let slug = slug.clone();
                            create_action(move |new_state: &bool| {
                                let slug = slug.clone();
                                let new_state = *new_state;
                                async move {
                                    let _ = toggle_debug_logging(slug, new_state).await;
                                }
                            })
                        };

                        let on_toggle = Callback::new(move |val: bool| {
                            set_enabled.set(val);
                            toggle_action.dispatch(val);
                        });

                        view! {
                            <div class="audio-model-row">
                                <div class="audio-model-info">
                                    <span class="audio-model-name">{name}</span>
                                    <span class="audio-model-description">
                                        "Sets RUST_LOG=debug on next container restart"
                                    </span>
                                </div>
                                <ToggleSwitch
                                    label=slug
                                    checked=enabled
                                    on_toggle=on_toggle
                                />
                            </div>
                        }
                    }
                />
            </div>
        </Suspense>
    }
}

/// Numeric input for the parallel processing thread count.
#[component]
fn ProcessingThreadsSettings() -> impl IntoView {
    let threads_res = create_resource(|| (), |_| get_processing_threads());
    let (threads, set_threads) = create_signal(1u32);
    let (status_msg, set_status) = create_signal(Option::<String>::None);

    let save_action = create_action(move |val: &u32| {
        let val = *val;
        async move { set_processing_threads(val).await }
    });

    // Populate from DB.
    create_effect(move |_| {
        if let Some(Ok(n)) = threads_res.get() {
            set_threads.set(n);
        }
    });

    // Show feedback.
    create_effect(move |_| {
        if let Some(result) = save_action.value().get() {
            match result {
                Ok(n) => {
                    set_threads.set(n);
                    set_status.set(Some(format!("Saved ({n} threads). Restart processing containers to apply.")));
                }
                Err(e) => set_status.set(Some(format!("Error: {e}"))),
            }
        }
    });

    let on_submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        set_status.set(None);
        save_action.dispatch(threads.get());
    };

    view! {
        <Suspense fallback=move || view! { <p>"Loading..."</p> }>
            <form class="location-form" on:submit=on_submit>
                <div class="location-fields">
                    <label class="location-label">
                        "Threads"
                        <input
                            type="number"
                            class="location-input"
                            min="1"
                            max="8"
                            prop:value=move || threads.get().to_string()
                            on:input=move |ev| {
                                if let Ok(v) = event_target_value(&ev).parse::<u32>() {
                                    set_threads.set(v.max(1).min(8));
                                }
                            }
                        />
                    </label>
                    <button type="submit" class="location-save-btn"
                        disabled=move || save_action.pending().get()
                    >
                        {move || if save_action.pending().get() { "Saving..." } else { "Save" }}
                    </button>
                </div>
                {move || status_msg.get().map(|msg| {
                    let cls = if msg.starts_with("Error") {
                        "location-status location-error"
                    } else {
                        "location-status location-ok"
                    };
                    view! { <p class=cls>{msg}</p> }
                })}
            </form>
        </Suspense>
    }
}