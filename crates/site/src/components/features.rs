use leptos::prelude::*;

use super::{CodeBlock, FeatureCard};
use crate::snippets;

/// Six cards. The dogfood test counts them (`[id^='feature-']`), so an
/// addition here needs its count bumped there.
#[component]
pub fn Features() -> impl IntoView {
    view! {
        <section id="features" class="mx-auto max-w-5xl px-6 py-12">
            <h2 class="mb-6 text-2xl font-bold text-rust-300">"What you get"</h2>
            <div class="grid grid-cols-1 gap-5 md:grid-cols-2">
                <FeatureCard
                    id="feature-drift-check"
                    title="A drift check, not a diagram"
                    blurb="check surveys in memory and diffs against the committed model. Exit 1 is a stale model; the diff says which edge."
                >
                    <CodeBlock html=snippets::CARD_DRIFT_DIFF/>
                </FeatureCard>
                <FeatureCard
                    id="feature-reexports"
                    title="Re-exports resolved"
                    blurb="A pub use chain or a glob re-export lands on the module that defines the item, so the edge points where the code lives."
                >
                    <CodeBlock html=snippets::CARD_REEXPORT_RS/>
                </FeatureCard>
                <FeatureCard
                    id="feature-externals"
                    title="Externals from config"
                    blurb="Processes, browsers and services the code talks to are declared once, and a name that matches no module fails the survey."
                >
                    <CodeBlock html=snippets::CARD_EXTERNALS_TOML/>
                </FeatureCard>
                <FeatureCard
                    id="feature-views"
                    title="Curated views over generated ids"
                    blurb="Write the views you want to look at beside the model; validate rejects a stale id."
                >
                    <CodeBlock html=snippets::CARD_VIEWS_C4/>
                </FeatureCard>
                <FeatureCard
                    id="feature-likec4"
                    title="LikeC4 for everything downstream"
                    blurb="Validation, JSON export, Graphviz rendering and a generated documentation site, through the pinned LikeC4 CLI."
                >
                    <CodeBlock html=snippets::CARD_LIKEC4_SH/>
                </FeatureCard>
                <FeatureCard
                    id="feature-rust-first"
                    title="Rust front-end first"
                    blurb="A language-agnostic core with one front-end per language. Rust ships first: cargo metadata for crates, syn for references."
                >
                    <CodeBlock html=snippets::CARD_RUST_FIRST_TOML/>
                </FeatureCard>
            </div>
        </section>
    }
}
