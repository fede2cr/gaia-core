//! Top-level navigation bar.

use leptos::*;
use leptos_router::*;

use crate::server_fns::get_update_count;

#[component]
pub fn Nav() -> impl IntoView {
    // Poll for update count every 60 s so the badge stays current.
    let (poll_tick, set_poll_tick) = create_signal(0u32);
    let update_count = create_local_resource(move || poll_tick.get(), |_| get_update_count());

    #[cfg(feature = "hydrate")]
    {
        set_interval(
            move || set_poll_tick.update(|n| *n = n.wrapping_add(1)),
            std::time::Duration::from_secs(300),
        );
    }
    #[cfg(not(feature = "hydrate"))]
    let _ = set_poll_tick;

    view! {
        <nav class="nav">
            <div class="nav-brand">
                <A href="/" class="nav-logo">"🌍 Gaia"</A>
            </div>
            <ul class="nav-links">
                <li><A href="/" exact=true class="nav-link">"Dashboard"</A></li>
                <li><A href="/projects" class="nav-link">"Projects"</A></li>
                <li>
                    <A href="/settings" class="nav-link">
                        "Settings"
                        {move || {
                            let count = update_count
                                .get()
                                .and_then(|r| r.ok())
                                .unwrap_or(0);
                            if count > 0 {
                                view! {
                                    <span class="nav-update-badge" title=format!("{count} update(s) available")>
                                        {count.to_string()}
                                    </span>
                                }.into_view()
                            } else {
                                ().into_view()
                            }
                        }}
                    </A>
                </li>
            </ul>
        </nav>
    }
}
