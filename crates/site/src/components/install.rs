use leptos::prelude::*;

use super::CodeBlock;
use crate::snippets;

#[component]
pub fn Install() -> impl IntoView {
    // The dev (main HEAD) build is unreleased, so it installs from git, not
    // the crates.io version.
    let is_dev = crate::version::is_dev();

    view! {
        <section id="install" class="mx-auto max-w-3xl px-6 py-12">
            <h2 class="mb-4 text-2xl font-bold text-rust-300">"Install"</h2>
            {if is_dev {
                view! {
                    <CodeBlock
                        html=snippets::INSTALL_DEV_SH
                        caption="Unreleased — installs from GitHub main HEAD"
                    />
                }
                    .into_any()
            } else {
                view! { <CodeBlock html=snippets::INSTALL_SH/> }.into_any()
            }}
            <p class="mt-4 text-sm text-rust-50/70">
                <code class="text-rust-300">"survey"</code>
                " and "
                <code class="text-rust-300">"check"</code>
                " need only cargo. "
                <code class="text-rust-300">"validate"</code>
                ", "
                <code class="text-rust-300">"export json"</code>
                ", "
                <code class="text-rust-300">"render"</code>
                " and "
                <code class="text-rust-300">"docs"</code>
                " shell out to a pinned "
                <code class="text-rust-300">"npx likec4"</code>
                " (Node), and rendering also needs Graphviz "
                <code class="text-rust-300">"dot"</code>
                "."
            </p>
        </section>
    }
}
