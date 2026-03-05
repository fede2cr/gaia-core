//! Component that displays detected local hardware devices with project assignment.

use leptos::*;

use crate::server_fns::{assign_device, detect_hardware, get_assignments, DeviceAssignment, HwDevice};

/// Panel showing locally-detected SDR dongles, microphones and cameras,
/// with a dropdown to assign each device to a Gaia project.
#[component]
pub fn DeviceList() -> impl IntoView {
    let devices = create_resource(|| (), |_| detect_hardware());
    let assignments = create_resource(|| (), |_| get_assignments());

    view! {
        <section class="device-panel">
            <h2>"Capture Devices"</h2>
            <p class="panel-subtitle">
                "Detected capture devices on this host. Assign each device to a project."
            </p>
            <Suspense fallback=move || view! { <p class="loading">"Scanning devices..."</p> }>
                {move || {
                    let devs = devices.get();
                    let asns = assignments.get();
                    match (devs, asns) {
                        (Some(Ok(devs)), Some(Ok(_asns))) if devs.is_empty() => view! {
                            <p class="empty-state">
                                "No capture devices detected. Attach an SDR dongle, microphone or camera."
                            </p>
                        }.into_view(),
                        (Some(Ok(devs)), Some(Ok(asns))) => view! {
                            <div class="device-grid">
                                {devs.into_iter().map(|d| {
                                    let current = asns.iter()
                                        .find(|a| a.device_id == d.id)
                                        .map(|a| a.project.clone())
                                        .unwrap_or_default();
                                    view! { <DeviceRow device=d current_project=current assignments_refetch=assignments /> }
                                }).collect_view()}
                            </div>
                        }.into_view(),
                        (Some(Err(e)), _) => view! {
                            <p class="error-state">"Error detecting devices: " {e.to_string()}</p>
                        }.into_view(),
                        (_, Some(Err(e))) => view! {
                            <p class="error-state">"Error loading assignments: " {e.to_string()}</p>
                        }.into_view(),
                        _ => view! {
                            <p class="loading">"Loading..."</p>
                        }.into_view(),
                    }
                }}
            </Suspense>
            <button
                class="btn btn-secondary"
                on:click=move |_| {
                    devices.refetch();
                    assignments.refetch();
                }
            >
                "↻ Rescan"
            </button>
        </section>
    }
}

/// A single row showing one detected device with an assignment dropdown.
#[component]
fn DeviceRow(
    device: HwDevice,
    current_project: String,
    assignments_refetch: Resource<(), Result<Vec<DeviceAssignment>, ServerFnError>>,
) -> impl IntoView {
    let icon = match device.kind.as_str() {
        "Sdr" => "📡",
        "Microphone" => "🎙️",
        "Camera" => "📷",
        _ => "❓",
    };
    let kind_label = match device.kind.as_str() {
        "Sdr" => "SDR Dongle",
        "Microphone" => "Microphone",
        "Camera" => "Camera",
        _ => "Unknown",
    };

    let device_id = device.id.clone();
    let (selected, set_selected) = create_signal(current_project);

    let assign_action = create_action(move |(dev_id, project): &(String, String)| {
        let dev_id = dev_id.clone();
        let project = project.clone();
        async move {
            let _ = assign_device(dev_id, "local".to_string(), project).await;
        }
    });

    // Refetch assignments when the action completes.
    create_effect(move |_| {
        if assign_action.version().get() > 0 {
            assignments_refetch.refetch();
        }
    });

    view! {
        <div class="device-row">
            <span class="device-icon">{icon}</span>
            <div class="device-info">
                <span class="device-label">{&device.label}</span>
                <span class="device-meta">
                    {kind_label} " · " <code>{&device.id}</code>
                </span>
            </div>
            <select
                class="device-select"
                on:change=move |ev| {
                    let val = event_target_value(&ev);
                    set_selected.set(val.clone());
                    assign_action.dispatch((device_id.clone(), val));
                }
            >
                <option value="none" selected=move || selected.get().is_empty()>"Unassigned"</option>
                <option value="audio" selected=move || selected.get() == "audio">"Gaia Audio"</option>
                <option value="light" selected=move || selected.get() == "light">"Gaia Light"</option>
                <option value="radio" selected=move || selected.get() == "radio">"Gaia Radio"</option>
                <option value="gmn" selected=move || selected.get() == "gmn">"GMN (Meteor)"</option>
            </select>
        </div>
    }
}
