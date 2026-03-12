//! Top-level navigation bar.

use leptos::either::Either;
use leptos::prelude::*;
use leptos::web_sys;

use crate::server_fns::get_update_count;

#[component]
pub fn Nav() -> impl IntoView {
    // Poll for update count every 300 s so the badge stays current.
    let (poll_tick, set_poll_tick) = signal(0u32);
    let update_count = Resource::new(move || poll_tick.get(), |_| get_update_count());

    #[cfg(feature = "hydrate")]
    {
        use wasm_bindgen::prelude::*;
        let cb = Closure::<dyn Fn()>::new(move || {
            set_poll_tick.update(|n| *n = n.wrapping_add(1));
        });
        web_sys::window()
            .unwrap()
            .set_interval_with_callback_and_timeout_and_arguments_0(
                cb.as_ref().unchecked_ref(),
                300_000,
            )
            .unwrap();
        cb.forget();
    }
    #[cfg(not(feature = "hydrate"))]
    let _ = set_poll_tick;

    view! {
        <nav class="nav">
            <div class="nav-brand">
                <a href="/" class="nav-logo">"🌍 Gaia"</a>
            </div>
            <ul class="nav-links">
                <li><a href="/" class="nav-link">"Dashboard"</a></li>
                <li><a href="/projects" class="nav-link">"Projects"</a></li>
                <li>
                    <a href="/settings" class="nav-link">
                        "Settings"
                        {move || {
                            let count = update_count
                                .get()
                                .and_then(|r| r.ok())
                                .unwrap_or(0);
                            if count > 0 {
                                Either::Left(view! {
                                    <span class="nav-update-badge" title=format!("{count} update(s) available")>
                                        {count.to_string()}
                                    </span>
                                })
                            } else {
                                Either::Right(())
                            }
                        }}
                    </a>
                </li>
            </ul>
        </nav>
    }
}
