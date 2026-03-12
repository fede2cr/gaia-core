//! Settings / configuration page.

use leptos::either::Either;
use leptos::prelude::*;

use crate::components::device_list::DeviceList;
use crate::components::mdns_panel::MdnsPanel;
use crate::components::toggle::ToggleSwitch;
use crate::server_fns::{
    check_for_updates, get_audio_models, get_debug_settings, get_location, get_node_name,
    get_processing_threads, get_projects, get_update_check_interval, get_update_status,
    set_location, set_node_name, set_processing_threads, set_update_check_interval,
    toggle_audio_model, toggle_debug_logging, DebugState, ImageUpdate,
};

/// Settings page showing current proxy configuration, port assignments,
/// detected hardware and remote nodes.
#[component]
pub fn SettingsPage() -> impl IntoView {
    let targets = Resource::new(|| (), |_| get_projects());

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
                                            <td>{t.name.clone()}</td>
                                            <td><code>{t.slug.clone()}</code></td>
                                            <td><code>{t.upstream_url.clone()}</code></td>
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
                                                        .collect::<Vec<_>>()}
                                                </span>
                                            </td>
                                        </tr>
                                    }
                                }).collect::<Vec<_>>().into_any(),
                                Err(_) => view! {
                                    <tr><td colspan="5">"Error loading projects"</td></tr>
                                }.into_any(),
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

            // ── Node Name ────────────────────────────────────────────
            <h2>"Node Name"</h2>
            <p class="page-description">
                "A friendly identifier for this station, shown in all web interfaces "
                "instead of the IP address. Falls back to the system hostname when empty."
            </p>
            <NodeNameSettings/>

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

            // ── Container Updates ────────────────────────────────────
            <h2>"Container Updates"</h2>
            <p class="page-description">
                "Periodically checks Docker Hub for newer container images. "
                "Set the automatic check interval, or use the manual button for development."
            </p>
            <UpdateCheckSettings/>

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
    let location = Resource::new(|| (), |_| get_location());
    let save_action = Action::new(move |(lat, lon): &(String, String)| {
        let lat = lat.clone();
        let lon = lon.clone();
        async move { set_location(lat, lon).await }
    });

    // Local signals for the input fields.
    let (lat, set_lat) = signal(String::new());
    let (lon, set_lon) = signal(String::new());
    let (status_msg, set_status) = signal(Option::<String>::None);

    // Populate fields once the resource loads.
    Effect::new(move || {
        if let Some(Ok(loc)) = location.get() {
            set_lat.set(loc.latitude);
            set_lon.set(loc.longitude);
        }
    });

    // Show feedback after the action completes.
    Effect::new(move || {
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
                            prop:value=move || lat.get()
                            on:input=move |ev| set_lat.set(event_target_value(&ev))
                        />
                    </label>
                    <label class="location-label">
                        "Longitude"
                        <input
                            type="text"
                            class="location-input"
                            placeholder="e.g. -58.3816"
                            prop:value=move || lon.get()
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

/// Node Name input — a friendly identifier for this station.
#[component]
fn NodeNameSettings() -> impl IntoView {
    let name_res = Resource::new(|| (), |_| get_node_name());
    let (name, set_name) = signal(String::new());
    let (status_msg, set_status) = signal(Option::<String>::None);

    let save_action = Action::new(move |val: &String| {
        let val = val.clone();
        async move { set_node_name(val).await }
    });

    Effect::new(move || {
        if let Some(Ok(n)) = name_res.get() {
            set_name.set(n);
        }
    });

    Effect::new(move || {
        if let Some(result) = save_action.value().get() {
            match result {
                Ok(n) => {
                    set_name.set(n);
                    set_status.set(Some("Saved. Restart containers to apply.".into()));
                }
                Err(e) => set_status.set(Some(format!("Error: {e}"))),
            }
        }
    });

    let on_submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        set_status.set(None);
        save_action.dispatch(name.get());
    };

    view! {
        <Suspense fallback=move || view! { <p>"Loading..."</p> }>
            <form class="location-form" on:submit=on_submit>
                <div class="location-fields">
                    <label class="location-label">
                        "Name"
                        <input
                            type="text"
                            class="location-input"
                            placeholder="e.g. garden-station"
                            prop:value=move || name.get()
                            on:input=move |ev| set_name.set(event_target_value(&ev))
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
    let models = Resource::new(|| (), |_| get_audio_models());
    let (model_list, set_model_list) = signal(Vec::<crate::server_fns::AudioModelInfo>::new());

    // Populate local state when the resource loads.
    Effect::new(move || {
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

                        let (enabled, set_enabled) = signal(model.enabled);
                        let toggle_action = {
                            let slug = slug.clone();
                            Action::new(move |new_state: &bool| {
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
    let debug_res = Resource::new(|| (), |_| get_debug_settings());
    let (items, set_items) = signal(Vec::<DebugState>::new());

    Effect::new(move || {
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

                        let (enabled, set_enabled) = signal(item.enabled);
                        let toggle_action = {
                            let slug = slug.clone();
                            Action::new(move |new_state: &bool| {
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
    let threads_res = Resource::new(|| (), |_| get_processing_threads());
    let (threads, set_threads) = signal(1u32);
    let (status_msg, set_status) = signal(Option::<String>::None);

    let save_action = Action::new(move |val: &u32| {
        let val = *val;
        async move { set_processing_threads(val).await }
    });

    // Populate from DB.
    Effect::new(move || {
        if let Some(Ok(n)) = threads_res.get() {
            set_threads.set(n);
        }
    });

    // Show feedback.
    Effect::new(move || {
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

/// Container update check: interval setting, "Check Now" button, and results list.
#[component]
fn UpdateCheckSettings() -> impl IntoView {
    // ── Interval setting ─────────────────────────────────────────────
    let interval_res = Resource::new(|| (), |_| get_update_check_interval());
    let (interval, set_interval_val) = signal(24u64);
    let (interval_msg, set_interval_msg) = signal(Option::<String>::None);

    let save_interval = Action::new(move |val: &u64| {
        let val = *val;
        async move { set_update_check_interval(val).await }
    });

    Effect::new(move || {
        if let Some(Ok(n)) = interval_res.get() {
            set_interval_val.set(n);
        }
    });

    Effect::new(move || {
        if let Some(result) = save_interval.value().get() {
            match result {
                Ok(n) => {
                    set_interval_val.set(n);
                    set_interval_msg.set(Some(format!("Saved ({n}h). Takes effect on next cycle.")));
                }
                Err(e) => set_interval_msg.set(Some(format!("Error: {e}"))),
            }
        }
    });

    let on_interval_submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        set_interval_msg.set(None);
        save_interval.dispatch(interval.get());
    };

    // ── Manual check ─────────────────────────────────────────────────
    let (updates, set_updates) = signal(Vec::<ImageUpdate>::new());
    let (check_msg, set_check_msg) = signal(Option::<String>::None);

    // Load cached status on page load.
    let status_res = Resource::new(|| (), |_| get_update_status());
    Effect::new(move || {
        if let Some(Ok(list)) = status_res.get() {
            set_updates.set(list);
        }
    });

    let check_action = Action::new(move |_: &()| async move {
        check_for_updates().await
    });

    Effect::new(move || {
        if let Some(result) = check_action.value().get() {
            match result {
                Ok(list) => {
                    let n = list.iter().filter(|u| u.has_update).count();
                    set_check_msg.set(Some(format!(
                        "Check complete: {n} update(s) available."
                    )));
                    set_updates.set(list);
                }
                Err(e) => set_check_msg.set(Some(format!("Error: {e}"))),
            }
        }
    });

    view! {
        <Suspense fallback=move || view! { <p>"Loading..."</p> }>
            // Interval form
            <form class="location-form" on:submit=on_interval_submit>
                <div class="location-fields">
                    <label class="location-label">
                        "Check interval (hours)"
                        <input
                            type="number"
                            class="location-input"
                            min="1"
                            max="168"
                            prop:value=move || interval.get().to_string()
                            on:input=move |ev| {
                                if let Ok(v) = event_target_value(&ev).parse::<u64>() {
                                    set_interval_val.set(v.max(1).min(168));
                                }
                            }
                        />
                    </label>
                    <button type="submit" class="location-save-btn"
                        disabled=move || save_interval.pending().get()
                    >
                        {move || if save_interval.pending().get() { "Saving..." } else { "Save" }}
                    </button>
                    <button type="button" class="location-save-btn update-check-btn"
                        disabled=move || check_action.pending().get()
                        on:click=move |_| { check_action.dispatch(()); }
                    >
                        {move || if check_action.pending().get() {
                            "Checking..."
                        } else {
                            "Check Now"
                        }}
                    </button>
                </div>
                {move || interval_msg.get().map(|msg| {
                    let cls = if msg.starts_with("Error") {
                        "location-status location-error"
                    } else {
                        "location-status location-ok"
                    };
                    view! { <p class=cls>{msg}</p> }
                })}
                {move || check_msg.get().map(|msg| {
                    let cls = if msg.starts_with("Error") {
                        "location-status location-error"
                    } else {
                        "location-status location-ok"
                    };
                    view! { <p class=cls>{msg}</p> }
                })}
            </form>

            // Update results list
            {move || {
                let list = updates.get();
                if list.is_empty() {
                    view! {
                        <p class="empty-state">"No update data yet. Click \"Check Now\" or wait for the next scheduled check."</p>
                    }.into_any()
                } else {
                    let has_any = list.iter().any(|u| u.has_update);
                    view! {
                        <div class="update-results">
                            {if has_any {
                                Either::Left(view! {
                                    <p class="update-summary update-available">
                                        "⬆ Updates available for some containers. "
                                        "Restart them from the Dashboard to pull the latest images."
                                    </p>
                                })
                            } else {
                                Either::Right(view! {
                                    <p class="update-summary update-current">
                                        "✓ All container images are up to date."
                                    </p>
                                })
                            }}
                            <div class="update-list">
                                {list.into_iter().map(|u| {
                                    let badge_cls = if u.has_update {
                                        "update-badge update-badge-new"
                                    } else {
                                        "update-badge update-badge-ok"
                                    };
                                    let badge_text = if u.has_update { "Update" } else { "Current" };
                                    view! {
                                        <div class="update-row">
                                            <span class="update-container-name">{u.container.clone()}</span>
                                            <span class=badge_cls>{badge_text}</span>
                                            <span class="update-last-checked">
                                                {if u.last_checked == "unknown" {
                                                    String::new()
                                                } else {
                                                    format!("checked {}", u.last_checked)
                                                }}
                                            </span>
                                        </div>
                                    }
                                }).collect::<Vec<_>>()}
                            </div>
                        </div>
                    }.into_any()
                }
            }}
        </Suspense>
    }
}