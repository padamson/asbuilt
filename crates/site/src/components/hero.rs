use leptos::prelude::*;

use super::icons::{self, Icon};

const CRATES_IO: &str = "https://crates.io/crates/asbuilt";
const DOCS_RS: &str = "https://docs.rs/asbuilt";
const GITHUB: &str = "https://github.com/padamson/asbuilt";
const CI_BADGE: &str = "https://github.com/padamson/asbuilt/actions/workflows/test.yml/badge.svg";
/// The LikeC4 release every `npx likec4` call is pinned to; bump with
/// `LIKEC4_VERSION` in `crates/asbuilt/src/lib.rs`.
const LIKEC4_VERSION: &str = "1.59.3";

#[component]
pub fn Hero() -> impl IntoView {
    let is_dev = crate::version::is_dev();

    view! {
        <header id="hero" class="flex flex-col items-center px-6 pt-24 pb-16 text-center">
            // The mark and the wordmark (brand/wordmark.css), named once for
            // assistive technology and drawn for everyone else.
            <p
                id="hero-brand"
                role="img"
                aria-label="asbuilt"
                class="mb-8 inline-flex items-center gap-3 text-4xl text-rust-50 sm:text-5xl"
            >
                <svg
                    class="h-12 w-12 text-rust-500 sm:h-14 sm:w-14"
                    viewBox="0 0 24 24"
                    fill="currentColor"
                    aria-hidden="true"
                >
                    <path d=icons::ASBUILT.trim()/>
                </svg>
                <span class="wordmark" aria-hidden="true">
                    <span class="wordmark-as">"AS"</span>
                    <span class="wordmark-built">"BUILT"</span>
                </span>
            </p>
            <h1
                id="hero-title"
                class="text-5xl font-bold tracking-tight text-rust-500 sm:text-6xl"
            >
                "Architecture models that cannot drift"
            </h1>
            <p id="hero-tagline" class="mt-5 max-w-2xl text-lg text-rust-50/80">
                "asbuilt surveys a Rust code base into a LikeC4 model, the way as-built "
                "drawings describe a building as constructed rather than as designed, and "
                "fails CI when the committed model no longer matches the code."
            </p>
            <p id="hero-rules" class="mt-3 text-xs text-rust-50/50">
                "Nothing in the model is hand-written. Nothing in the code is annotated."
            </p>

            <div
                id="hero-badges"
                class="mt-7 flex flex-wrap items-center justify-center gap-2"
            >
                <a href=CRATES_IO>
                    {if is_dev {
                        view! {
                            <img
                                alt="crates.io: unreleased"
                                src="https://img.shields.io/badge/crates.io-unreleased-inactive"
                            />
                        }
                            .into_any()
                    } else {
                        view! {
                            <img alt="crates.io" src="https://img.shields.io/crates/v/asbuilt.svg"/>
                        }
                            .into_any()
                    }}
                </a>
                <a href=GITHUB>
                    <img alt="CI" src=CI_BADGE/>
                </a>
                <img
                    alt=format!("LikeC4 {LIKEC4_VERSION}")
                    src=format!("https://img.shields.io/badge/LikeC4-{LIKEC4_VERSION}-45ba4b")
                />
            </div>

            <div class="mt-9 flex flex-wrap items-center justify-center gap-3">
                <a
                    id="cta-architecture"
                    href="architecture/"
                    class="inline-flex items-center gap-2 rounded-lg bg-rust-500 px-5 py-2.5 font-semibold text-rust-50 transition hover:bg-rust-600"
                >
                    <Icon path=icons::ASBUILT.trim()/>
                    "See asbuilt's own architecture"
                </a>
                <a
                    id="cta-github"
                    href=GITHUB
                    class="inline-flex items-center gap-2 rounded-lg border border-rust-700/50 px-5 py-2.5 font-semibold text-rust-50 transition hover:border-rust-500"
                >
                    <Icon path=icons::GITHUB/>
                    "GitHub"
                </a>
                {(!is_dev).then(|| view! {
                    <a
                        id="cta-docs"
                        href=DOCS_RS
                        class="inline-flex items-center gap-2 rounded-lg border border-rust-700/50 px-5 py-2.5 font-semibold text-rust-50 transition hover:border-rust-500"
                    >
                        <Icon path=icons::DOCS_RS/>
                        "Docs"
                    </a>
                })}
            </div>
        </header>
    }
}
