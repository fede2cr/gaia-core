//! Gaia Core - Central web interface and reverse proxy for all Gaia projects.

pub mod app;
pub mod components;
pub mod config;
pub mod pages;
pub mod server_fns;

cfg_if::cfg_if! {
    if #[cfg(feature = "ssr")] {
        pub mod assignments;
        pub mod containers;
        pub mod db;
        pub mod discovery;
        pub mod hardware;
        pub mod proxy;
    }
}

/// Diagnostic helpers only compiled into the WASM bundle.
/// They let us observe hydration progress in the *server* logs (via fetch)
/// and on the page itself (via a red DOM overlay for panics).
#[cfg(feature = "hydrate")]
mod wasm_diag {
    use wasm_bindgen::prelude::*;

    #[wasm_bindgen(inline_js = "
        export function diag_fetch(url) {
            try { fetch(url).catch(function() {}); } catch(e) {}
        }
        export function diag_panic_to_dom(msg) {
            try {
                var pre = document.createElement('pre');
                pre.style.cssText = 'background:red;color:white;padding:1em;position:fixed;top:0;left:0;right:0;z-index:9999;overflow:auto;max-height:50vh';
                pre.textContent = 'WASM PANIC:\\n' + msg;
                document.body.appendChild(pre);
            } catch(e) {}
        }
        export function diag_panic_to_server(msg) {
            try {
                fetch('/api/hydrate-ping?phase=panic&msg=' + encodeURIComponent(msg.substring(0, 500)))
                    .catch(function() {});
            } catch(e) {}
        }
    ")]
    extern "C" {
        pub fn diag_fetch(url: &str);
        pub fn diag_panic_to_dom(msg: &str);
        pub fn diag_panic_to_server(msg: &str);
    }
}

/// Entry-point called from the WASM bundle to hydrate the server-rendered HTML.
#[cfg(feature = "hydrate")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn hydrate() {
    // Custom panic hook: logs to console (stack trace) + DOM overlay + server ping.
    std::panic::set_hook(Box::new(|info| {
        console_error_panic_hook::hook(info);
        let msg = info.to_string();
        wasm_diag::diag_panic_to_dom(&msg);
        wasm_diag::diag_panic_to_server(&msg);
    }));

    // Beacon so we can see in server logs that WASM started.
    wasm_diag::diag_fetch("/api/hydrate-ping?phase=wasm-init");

    leptos::mount_to_body(app::App);

    // If we reach here, hydration succeeded.
    wasm_diag::diag_fetch("/api/hydrate-ping?phase=hydrated");
}
