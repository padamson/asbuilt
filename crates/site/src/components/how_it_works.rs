use leptos::prelude::*;

use super::CodeTabs;
use crate::snippets;

/// Survey, model, check: the loop, and the three rules behind it.
#[component]
pub fn HowItWorks() -> impl IntoView {
    view! {
        <section id="how-it-works" class="mx-auto max-w-5xl px-6 py-12">
            <h2 class="mb-2 text-2xl font-bold text-rust-300">"How it works"</h2>
            <p class="mb-6 max-w-3xl text-rust-50/80">
                <code class="text-rust-300">"asbuilt survey"</code>
                " reads the workspace with cargo and syn and writes the model: crates as "
                "containers, modules as components, and every module-to-module reference as "
                "a relation labeled with the item names it references. "
                <code class="text-rust-300">"asbuilt check"</code>
                " surveys again and exits 1 with a diff when the committed model no longer "
                "matches, so it runs as a pre-commit hook and a CI step. Pin the release in "
                <code class="text-rust-300">"asbuilt.toml"</code>
                " and any other one stops with its install line instead of reporting drift."
            </p>
            <CodeTabs tabs=vec![
                ("survey", snippets::STEP_SURVEY_SH),
                ("model.c4", snippets::STEP_MODEL_C4),
                ("check", snippets::STEP_CHECK_DIFF),
            ]/>
            <div id="rules" class="mt-8 grid grid-cols-1 gap-5 md:grid-cols-3">
                <div class="rounded-xl border border-rust-700/30 bg-ink-800 p-5">
                    <h3 class="font-semibold text-rust-300">"Never edit the model"</h3>
                    <p class="mt-1 text-sm text-rust-50/70">
                        "It is generated. Run the survey and commit the result; that is the "
                        "whole fix for a red check."
                    </p>
                </div>
                <div class="rounded-xl border border-rust-700/30 bg-ink-800 p-5">
                    <h3 class="font-semibold text-rust-300">"Curated views in a sibling file"</h3>
                    <p class="mt-1 text-sm text-rust-50/70">
                        "Hand-written views reference generated ids, and "
                        <code>"asbuilt validate"</code>
                        " runs LikeC4's parser over the directory, so a stale id fails."
                    </p>
                </div>
                <div class="rounded-xl border border-rust-700/30 bg-ink-800 p-5">
                    <h3 class="font-semibold text-rust-300">"Externals in the config"</h3>
                    <p class="mt-1 text-sm text-rust-50/70">
                        "Nothing static says a module spawns a Node process; "
                        <code>"asbuilt.toml"</code>
                        " does, and a name that matches no module is an error, not a "
                        "missing edge."
                    </p>
                </div>
            </div>
        </section>
    }
}
