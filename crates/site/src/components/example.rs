use leptos::prelude::*;

use super::CodeBlock;
use crate::snippets;

/// This repo's own model: the context view as the architecture tree shows
/// it (the tree's figure, lifted at build time, with its viewer; the
/// rendered image when no tree was generated), the config that produced
/// it, and the hook that keeps it honest.
#[component]
pub fn Example() -> impl IntoView {
    view! {
        <section id="example" class="mx-auto max-w-5xl px-6 py-12">
            <h2 class="mb-2 text-2xl font-bold text-rust-300">"Surveying itself"</h2>
            <p class="mb-6 max-w-3xl text-rust-50/80">
                "This is asbuilt's own context view, rendered from the committed model on "
                "every deploy. The full generated documentation, one page per crate with "
                "every module and relation, is at "
                <a id="example-architecture" href="architecture/" class="text-rust-300 underline hover:text-rust-500">
                    "architecture/"
                </a>
                ". Open a crate's page to zoom and pan its diagram, follow a node to its "
                "page, or click an edge for the relations behind it."
            </p>
            {if snippets::CONTEXT_FIGURE.is_empty() {
                view! {
                    <figure class="rounded-xl border border-rust-700/30 bg-white p-2">
                        <img
                            id="example-context"
                            src="views/context.svg"
                            alt="asbuilt: the three crates and what they run"
                            class="mx-auto max-h-[28rem] w-auto"
                        />
                    </figure>
                }
                    .into_any()
            } else {
                view! {
                    <div
                        id="example-context"
                        class="rounded-xl border border-rust-700/30 p-3"
                        inner_html=snippets::CONTEXT_FIGURE
                    ></div>
                }
                    .into_any()
            }}
            <div class="mt-8 grid grid-cols-1 gap-5 md:grid-cols-2">
                <CodeBlock html=snippets::ASBUILT_TOML caption="asbuilt.toml: the externals nothing static can state"/>
                <CodeBlock html=snippets::PRE_COMMIT_YAML caption="the pre-commit hook and CI step"/>
            </div>
        </section>
    }
}
