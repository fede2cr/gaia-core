//! Toggle switch component.

use leptos::prelude::*;
use leptos::prelude::{Callback, ElementChild, IntoView, ReadSignal};

/// A CSS-only toggle switch with a label.
#[component]
pub fn ToggleSwitch(
    /// Label shown next to the toggle.
    label: String,
    /// Current on/off state.
    checked: ReadSignal<bool>,
    /// Callback invoked when the user toggles.
    on_toggle: Callback<bool>,
) -> impl IntoView {
    let label_clone = label.clone();
    view! {
        <label class="toggle-switch">
            <input
                type="checkbox"
                class="toggle-input"
                prop:checked=move || checked.get()
                on:change=move |ev| {
                    let val = event_target_checked(&ev);
                    on_toggle.run(val);
                }
            />
            <span class="toggle-slider"></span>
            <span class="toggle-label">{label_clone}</span>
        </label>
    }
}
