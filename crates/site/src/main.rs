use leptos::mount::mount_to_body;

mod app;
mod components;
mod roadmap;
mod snippets;
#[cfg(test)]
mod style;
mod version;

fn main() {
    // Keep main thin: a future CSR->SSR switch only touches this entry and
    // the build config, never the components.
    mount_to_body(app::App);
}
