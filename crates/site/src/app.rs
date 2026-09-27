use leptos::prelude::*;

use crate::components::{
    DogfoodBanner, Example, Features, Footer, Hero, HowItWorks, Install, VersionSwitcher,
};

/// Root of the landing page. Each section is a component so the view code
/// carries over unchanged if the build ever moves from CSR/Trunk to SSR.
#[component]
pub fn App() -> impl IntoView {
    view! {
        <div class="min-h-screen bg-ink-900 text-rust-50 antialiased">
            <VersionSwitcher/>
            <Hero/>
            <Install/>
            <HowItWorks/>
            <Example/>
            <Features/>
            <DogfoodBanner/>
            <Footer/>
        </div>
    }
}
