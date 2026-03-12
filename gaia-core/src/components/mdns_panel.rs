//! Component that displays remote capture nodes discovered via mDNS
//! with project assignment controls.

use leptos::prelude::*;
use leptos::prelude::{
    signal, Action, Effect, ElementChild, IntoView, Resource,
    ServerFnError, Suspense,
};

use crate::server_fns::{assign_device, discover_nodes, get_assignments, DeviceAssignment, MdnsNode};

/// Panel showing remote capture/processing nodes discovered on the LAN,
/// with a dropdown to assign each node to a Gaia project.
#[component]
pub fn MdnsPanel() -> impl IntoView {
    let nodes = Resource::new(|| (), |_| discover_nodes());
    let assignments = Resource::new(|| (), |_| get_assignments());

    view! {
        <section class="mdns-panel">
            <h2>"Remote Capture Nodes"</h2>
            <p class="panel-subtitle">
                "Other Gaia nodes discovered on the local network via mDNS. "
                "Assign each node to use it as a remote capture source."
            </p>
            <Suspense fallback=move || view! { <p class="loading">"Scanning network..."</p> }>
                {move || {
                    let ns = nodes.get();
                    let asns = assignments.get();
                    match (ns, asns) {
                        (Some(Ok(ns)), Some(Ok(_asns))) if ns.is_empty() => view! {
                            <p class="empty-state">
                                "No remote capture nodes found. Start a capture container on another device."
                            </p>
                        }.into_any(),
                        (Some(Ok(ns)), Some(Ok(asns))) => view! {
                            <div class="node-grid">
                                {ns.into_iter().map(|n| {
                                    let current = asns.iter()
                                        .find(|a| a.device_id == n.instance)
                                        .map(|a| a.project.clone())
                                        .unwrap_or_else(|| n.project_slug.clone());
                                    view! { <NodeRow node=n current_project=current assignments_refetch=assignments /> }
                                }).collect::<Vec<_>>()}
                            </div>
                        }.into_any(),
                        (Some(Err(e)), _) => view! {
                            <p class="error-state">"mDNS discovery error: " {e.to_string()}</p>
                        }.into_any(),
                        (_, Some(Err(e))) => view! {
                            <p class="error-state">"Error loading assignments: " {e.to_string()}</p>
                        }.into_any(),
                        _ => view! {
                            <p class="loading">"Loading..."</p>
                        }.into_any(),
                    }
                }}
            </Suspense>
            <button
                class="btn btn-secondary"
                on:click=move |_| {
                    nodes.refetch();
                    assignments.refetch();
                }
            >
                "↻ Rescan Network"
            </button>
        </section>
    }
}

/// A single row showing one mDNS-discovered node with assignment dropdown.
#[component]
fn NodeRow(
    node: MdnsNode,
    current_project: String,
    assignments_refetch: Resource<Result<Vec<DeviceAssignment>, ServerFnError>>,
) -> impl IntoView {
    let icon = match node.project_slug.as_str() {
        "radio" => "📡",
        "audio" => "🎙️",
        "gmn" => "☄️",
        _ => "🌐",
    };

    let instance = node.instance.clone();
    let (selected, set_selected) = signal(current_project);

    let assign_action = Action::new(move |(dev_id, project): &(String, String)| {
        let dev_id = dev_id.clone();
        let project = project.clone();
        async move {
            let _ = assign_device(dev_id, "remote".to_string(), project).await;
        }
    });

    Effect::new(move || {
        if assign_action.version().get() > 0 {
            assignments_refetch.refetch();
        }
    });

    view! {
        <div class="node-row">
            <span class="device-icon">{icon}</span>
            <div class="device-info">
                <span class="device-label">{node.instance.clone()}</span>
                <span class="device-meta">
                    {node.hostname.clone()}
                    " · " <code>{node.service_type.clone()}</code>
                </span>
            </div>
            <select
                class="device-select"
                on:change=move |ev| {
                    let val = event_target_value(&ev);
                    set_selected.set(val.clone());
                    assign_action.dispatch((instance.clone(), val));
                }
            >
                <option value="none" selected=move || selected.get().is_empty()>"Unassigned"</option>
                <option value="audio" selected=move || selected.get() == "audio">"Gaia Audio"</option>
                <option value="radio" selected=move || selected.get() == "radio">"Gaia Radio"</option>
                <option value="gmn" selected=move || selected.get() == "gmn">"GMN (Meteor)"</option>
            </select>
        </div>
    }
}
