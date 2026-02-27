//! Top-level navigation bar.

use leptos::*;
use leptos_router::*;

#[component]
pub fn Nav() -> impl IntoView {
    view! {
        <nav class="nav">
            <div class="nav-brand">
                <A href="/" class="nav-logo">"🌍 Gaia"</A>
            </div>
            <ul class="nav-links">
                <li><A href="/" exact=true class="nav-link">"Dashboard"</A></li>
                <li><A href="/projects" class="nav-link">"Projects"</A></li>
                <li><A href="/settings" class="nav-link">"Settings"</A></li>
            </ul>
        </nav>
    }
}
