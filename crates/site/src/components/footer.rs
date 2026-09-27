use leptos::prelude::*;

const CRATES_IO: &str = "https://crates.io/crates/asbuilt";
const DOCS_RS: &str = "https://docs.rs/asbuilt";
const GITHUB: &str = "https://github.com/padamson/asbuilt";
const LIKEC4: &str = "https://likec4.dev";
const PLAYWRIGHT_RUST: &str = "https://playwright-rust.dev";

#[component]
pub fn Footer() -> impl IntoView {
    let is_dev = crate::version::is_dev();
    view! {
        <footer id="footer" class="mt-8 border-t border-rust-700/30 px-6 py-10">
            <div class="mx-auto flex max-w-5xl flex-col gap-4 text-sm text-rust-50/60">
                <nav class="flex flex-wrap gap-5">
                    <a href=GITHUB class="hover:text-rust-300">"GitHub"</a>
                    <a href="architecture/" class="hover:text-rust-300">"Architecture"</a>
                    {(!is_dev).then(|| view! {
                        <a href=DOCS_RS class="hover:text-rust-300">"Docs"</a>
                        <a href=CRATES_IO class="hover:text-rust-300">"crates.io"</a>
                    })}
                    <a href=LIKEC4 class="hover:text-rust-300">"LikeC4"</a>
                </nav>
                <p id="credits" class="max-w-3xl">
                    "Everything downstream of the model is "
                    <a href=LIKEC4 class="underline hover:text-rust-300">"LikeC4"</a>
                    "'s: validation, export, layout and browsing. This page is tested with "
                    <a href=PLAYWRIGHT_RUST class="underline hover:text-rust-300">"playwright-rs"</a>
                    "."
                </p>
                <p>"Licensed under MIT or Apache-2.0. Built with Leptos and Trunk."</p>
            </div>
        </footer>
    }
}
