//! Settings / configuration page.

use leptos::*;

use crate::components::device_list::DeviceList;
use crate::components::mdns_panel::MdnsPanel;
use crate::server_fns::{get_location, get_projects, set_location};

/// Settings page — shows current proxy configuration, port assignments,
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
                        <tr><td colspan="5">"Loading…"</td></tr>
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
        <Suspense fallback=move || view! { <p>"Loading…"</p> }>
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
                        {move || if save_action.pending().get() { "Saving…" } else { "Save" }}
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
