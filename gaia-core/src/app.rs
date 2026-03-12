//! Root Leptos application component with routing.

use leptos::prelude::*;
use leptos_meta::*;
use leptos_router::{
    components::{FlatRoutes, Route, Router},
    StaticSegment,
};

use crate::components::nav::Nav;
use crate::pages::{
    gmn_config::GmnConfigPage, home::Home, projects::ProjectsPage, settings::SettingsPage,
};

/// Server-side HTML shell wrapping the app.
pub fn shell(options: leptos::config::LeptosOptions) -> impl IntoView {
    view! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <meta charset="utf-8"/>
                <meta name="viewport" content="width=device-width, initial-scale=1"/>
                <meta name="description" content="Gaia central dashboard for managing and monitoring all Gaia sub-projects"/>
                <AutoReload options=options.clone()/>
                <HydrationScripts options/>
                <MetaTags/>
                <link rel="stylesheet" id="leptos" href="/pkg/gaia-core.css"/>
                <title>"Gaia - Central Dashboard"</title>
            </head>
            <body>
                <App/>
            </body>
        </html>
    }
}

/// The root `<App/>` component.
#[component]
pub fn App() -> impl IntoView {
    view! {
        <Router>
            <Nav/>
            <main class="main-content">
                <FlatRoutes fallback=|| "Page not found.">
                    <Route path=StaticSegment("") view=Home/>
                    <Route path=StaticSegment("projects") view=ProjectsPage/>
                    <Route path=StaticSegment("gmn-config") view=GmnConfigPage/>
                    <Route path=StaticSegment("settings") view=SettingsPage/>
                </FlatRoutes>
            </main>
        </Router>
    }
}
