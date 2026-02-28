//! Root Leptos application component with routing.

use leptos::*;
use leptos_meta::*;
use leptos_router::*;

use crate::components::nav::Nav;
use crate::pages::{
    gmn_config::GmnConfigPage, home::Home, projects::ProjectsPage, settings::SettingsPage,
};

/// The root `<App/>` component.
#[component]
pub fn App() -> impl IntoView {
    provide_meta_context();

    view! {
        <Stylesheet id="leptos" href="/pkg/gaia-core.css"/>
        <Title text="Gaia - Central Dashboard"/>
        <Meta name="viewport" content="width=device-width, initial-scale=1"/>
        <Meta name="description" content="Gaia central dashboard for managing and monitoring all Gaia sub-projects"/>

        <Router>
            <Nav/>
            <main class="main-content">
                <Routes>
                    <Route path="/" view=Home/>
                    <Route path="/projects" view=ProjectsPage/>
                    <Route path="/gmn-config" view=GmnConfigPage/>
                    <Route path="/settings" view=SettingsPage/>
                </Routes>
            </main>
        </Router>
    }
}
