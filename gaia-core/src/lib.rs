//! Gaia Core – Central web interface and reverse proxy for all Gaia projects.

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

/// Entry-point called from the WASM bundle to hydrate the server-rendered HTML.
#[cfg(feature = "hydrate")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn hydrate() {
    console_error_panic_hook::set_once();
    leptos::mount_to_body(app::App);
}
