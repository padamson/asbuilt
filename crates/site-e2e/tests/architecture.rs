//! The architecture section of a deployed snapshot: the output of
//! `asbuilt docs` over this repo's own model, mounted at `architecture/`
//! under the snapshot's base path. Static HTML with relative links, so
//! the assertions are about the tree being complete and its figures
//! resolving under the real base path, which is what a wrong `-o` or a
//! missing `render` would break.
//!
//! Driven by the same `SNAPSHOT_DIST`, `SNAPSHOT_BASE` and
//! `SNAPSHOT_VERSION` variables as the snapshot smoke test; a missing
//! variable is a failure naming it. Run from the repo root after the
//! snapshot build (`docs/versioned-site.md`, "Running the gates locally").

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use axum::Router;
use playwright_rs::expect;
use playwright_rs::protocol::{Page, Playwright};
use tower_http::services::ServeDir;

/// The top-level container ids of the committed model, in file order.
/// Two-space indentation is the `model {` block's own level; components
/// sit deeper. Reading the `.c4` rather than the survey keeps this test
/// free of the asbuilt crates: it asserts against what was committed.
fn committed_containers() -> Vec<String> {
    let model = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/architecture/model.c4");
    let text =
        std::fs::read_to_string(&model).unwrap_or_else(|e| panic!("read {}: {e}", model.display()));
    let ids: Vec<String> = text
        .lines()
        .filter_map(|line| {
            let rest = line.strip_prefix("  ")?;
            if rest.starts_with(' ') {
                return None;
            }
            let (id, kind) = rest.split_once(" = ")?;
            kind.starts_with("container ").then(|| id.to_string())
        })
        .collect();
    assert!(!ids.is_empty(), "no containers in {}", model.display());
    ids
}

fn snapshot_var(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| {
        panic!("{name} must be set (SNAPSHOT_DIST, SNAPSHOT_BASE, SNAPSHOT_VERSION)")
    })
}

/// Serve the snapshot under its base path on an ephemeral port, as
/// gh-pages does, and open a Chromium page with every 4xx/5xx response
/// recorded.
async fn open_snapshot() -> (
    Playwright,
    playwright_rs::protocol::Browser,
    Page,
    String,
    Arc<Mutex<Vec<String>>>,
) {
    let dist = PathBuf::from(snapshot_var("SNAPSHOT_DIST"));
    let base = snapshot_var("SNAPSHOT_BASE");
    let _version = snapshot_var("SNAPSHOT_VERSION");
    assert!(
        dist.join("architecture/index.html").exists(),
        "SNAPSHOT_DIST has no architecture/index.html: {} (run `asbuilt docs --no-render -o <dist>/architecture`)",
        dist.display()
    );
    let mount = base.trim_end_matches('/').to_string();
    let app = Router::new().nest_service(&mount, ServeDir::new(&dist));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind snapshot server");
    let addr = listener.local_addr().expect("local addr");
    tokio::spawn(async move {
        axum::serve(listener, app).await.expect("serve snapshot");
    });

    let pw = Playwright::launch().await.expect("launch playwright");
    let browser = pw.chromium().launch().await.expect("launch chromium");
    let page = browser.new_page().await.expect("new page");
    let broken: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let sink = broken.clone();
    page.on_response(move |resp| {
        let sink = sink.clone();
        let (status, url) = (resp.status(), resp.url().to_string());
        async move {
            if status >= 400 {
                sink.lock().unwrap().push(format!("{status} {url}"));
            }
            Ok(())
        }
    })
    .await
    .expect("register response listener");
    (
        pw,
        browser,
        page,
        format!("http://{addr}{base}architecture/"),
        broken,
    )
}

#[tokio::test]
#[ignore = "needs a snapshot build with its architecture tree and Chromium; run with: SNAPSHOT_DIST=... SNAPSHOT_BASE=/asbuilt/dev/ SNAPSHOT_VERSION=dev cargo nextest run --manifest-path crates/site-e2e/Cargo.toml --config-file .config/nextest.toml --run-ignored only -E 'test(site_architecture)'"]
async fn site_architecture_section_lists_every_crate_and_embeds_a_view() {
    let containers = committed_containers();
    let (_pw, browser, page, root, broken) = open_snapshot().await;

    page.goto(&root, None)
        .await
        .expect("navigate to the architecture index under the base path");
    expect(page.locator("h1"))
        .to_be_visible()
        .await
        .expect("the architecture index renders");
    for id in &containers {
        let link = page.locator(format!("a[href='containers/{id}.html']"));
        let count = link.count().await.expect("count container links");
        assert!(
            count >= 1,
            "the index does not link crate `{id}` (containers/{id}.html)"
        );
    }
    let overview = page.locator("img[src='views/index.svg']");
    expect(overview)
        .to_be_visible()
        .await
        .expect("the index embeds the overview view");

    // A crate page: its scoped view is a real SVG the browser decoded,
    // not a placeholder from a missing render and not a 404 under the
    // base path.
    let first = &containers[0];
    page.goto(&format!("{root}containers/{first}.html"), None)
        .await
        .expect("navigate to a crate page");
    let view = format!("img[src='../views/view_{first}.svg']");
    expect(page.locator(&view))
        .to_be_visible()
        .await
        .expect("the crate page embeds its scoped view");
    let decoded: bool = page
        .locator(&view)
        .evaluate::<bool, ()>("img => img.complete && img.naturalWidth > 0", None)
        .await
        .expect("probe the view image");
    assert!(decoded, "the scoped view for `{first}` did not decode");
    let missing = page
        .locator("p.missing")
        .count()
        .await
        .expect("count missing-view placeholders");
    assert_eq!(missing, 0, "the crate page has unrendered views");

    let broken = broken.lock().unwrap().clone();
    assert!(
        broken.is_empty(),
        "responses at or above 400 under {root}:\n{}",
        broken.join("\n")
    );
    browser.close().await.expect("close browser");
}
