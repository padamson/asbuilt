//! The architecture section of a deployed snapshot: the output of
//! `asbuilt docs` over this repo's own model, mounted at `architecture/`
//! under the snapshot's base path. The assertions are about the tree
//! being complete, its views inlined and resolving under the real base
//! path (which a wrong `-o` or a missing `render` would break), and its
//! colors following the system's scheme or the visitor's choice, with
//! and without JavaScript.
//!
//! Driven by the same `SNAPSHOT_DIST`, `SNAPSHOT_BASE` and
//! `SNAPSHOT_VERSION` variables as the snapshot smoke test; a missing
//! variable is a failure naming it. Run from the repo root after the
//! snapshot build (`docs/versioned-site.md`, "Running the gates locally").

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use axum::Router;
use playwright_rs::expect;
use playwright_rs::protocol::{
    Browser, BrowserContext, BrowserContextOptions, ColorScheme, EmulateMediaOptions, Page,
    Playwright,
};
use tower_http::services::ServeDir;

/// The page background and a crate's box in the site's palette
/// (crates/site/architecture.css and `[theme]` in asbuilt.toml), as
/// `getComputedStyle` reports them.
const LIGHT_PAGE: &str = "rgb(253, 243, 239)";
const DARK_PAGE: &str = "rgb(26, 20, 16)";
const LIGHT_CRATE: &str = "rgb(206, 66, 43)";
const DARK_CRATE: &str = "rgb(143, 45, 25)";

/// A crate's box in the index view.
const CRATE_BOX: &str = "svg.c4[data-view='index'] .node.c4-k-container > polygon";

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
/// gh-pages does; returns the architecture tree's URL.
async fn serve_snapshot() -> String {
    let dist = PathBuf::from(snapshot_var("SNAPSHOT_DIST"));
    let base = snapshot_var("SNAPSHOT_BASE");
    let _version = snapshot_var("SNAPSHOT_VERSION");
    assert!(
        dist.join("architecture/index.html").exists(),
        "SNAPSHOT_DIST has no architecture/index.html: {} (run `asbuilt docs --no-render -o crates/site/public/architecture` before the Trunk build)",
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
    format!("http://{addr}{base}architecture/")
}

/// A Chromium page, JavaScript on or off, emulating `scheme` as the
/// system's color scheme, with every 4xx/5xx response recorded.
struct Session {
    _pw: Playwright,
    browser: Browser,
    _context: BrowserContext,
    page: Page,
    broken: Arc<Mutex<Vec<String>>>,
}

async fn open_session(javascript: bool, scheme: ColorScheme) -> Session {
    let pw = Playwright::launch().await.expect("launch playwright");
    let browser = pw.chromium().launch().await.expect("launch chromium");
    let context = browser
        .new_context_with_options(
            BrowserContextOptions::builder()
                .javascript_enabled(javascript)
                .build(),
        )
        .await
        .expect("new context");
    let page = context.new_page().await.expect("new page");
    page.emulate_media(EmulateMediaOptions::builder().color_scheme(scheme).build())
        .await
        .expect("emulate the system color scheme");
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
    Session {
        _pw: pw,
        browser,
        _context: context,
        page,
        broken,
    }
}

async fn page_background(page: &Page) -> String {
    page.evaluate::<(), String>(
        "() => getComputedStyle(document.body).backgroundColor",
        None,
    )
    .await
    .expect("read the page background")
}

async fn crate_fill(page: &Page) -> String {
    page.locator(CRATE_BOX)
        .first()
        .evaluate::<String, ()>("el => getComputedStyle(el).fill", None)
        .await
        .expect("read a crate box's fill")
}

#[tokio::test]
#[ignore = "needs a snapshot build with its architecture tree and Chromium; run with: SNAPSHOT_DIST=... SNAPSHOT_BASE=/asbuilt/dev/ SNAPSHOT_VERSION=dev cargo nextest run --manifest-path crates/site-e2e/Cargo.toml --config-file .config/nextest.toml --run-ignored only -E 'test(site_architecture)'"]
async fn site_architecture_section_lists_every_crate_and_embeds_a_view() {
    let containers = committed_containers();
    let root = serve_snapshot().await;
    let session = open_session(true, ColorScheme::Light).await;
    let page = &session.page;

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
    expect(page.locator("svg.c4[data-view='index']"))
        .to_be_visible()
        .await
        .expect("the index inlines the overview view");
    let crates = page
        .locator(CRATE_BOX)
        .count()
        .await
        .expect("count crate boxes");
    assert!(crates > 0, "the overview draws no crate boxes");

    // A crate page: its scoped view is inlined, with its modules drawn,
    // not a placeholder from a missing render.
    let first = &containers[0];
    page.goto(&format!("{root}containers/{first}.html"), None)
        .await
        .expect("navigate to a crate page");
    let view = format!("svg.c4[data-view='view_{first}']");
    expect(page.locator(&view))
        .to_be_visible()
        .await
        .expect("the crate page inlines its scoped view");
    let modules = page
        .locator(format!("{view} .node.c4-k-component"))
        .count()
        .await
        .expect("count module boxes");
    assert!(
        modules > 0,
        "the scoped view for `{first}` draws no modules"
    );
    let missing = page
        .locator("p.missing")
        .count()
        .await
        .expect("count missing-view placeholders");
    assert_eq!(missing, 0, "the crate page has unrendered views");

    let broken = session.broken.lock().unwrap().clone();
    assert!(
        broken.is_empty(),
        "responses at or above 400 under {root}:\n{}",
        broken.join("\n")
    );
    session.browser.close().await.expect("close browser");
}

#[tokio::test]
#[ignore = "needs a snapshot build with its architecture tree and Chromium; run with: SNAPSHOT_DIST=... SNAPSHOT_BASE=/asbuilt/dev/ SNAPSHOT_VERSION=dev cargo nextest run --manifest-path crates/site-e2e/Cargo.toml --config-file .config/nextest.toml --run-ignored only -E 'test(site_architecture)'"]
async fn site_architecture_follows_the_system_scheme_until_the_visitor_chooses_one() {
    let containers = committed_containers();
    let root = serve_snapshot().await;
    let session = open_session(true, ColorScheme::Dark).await;
    let page = &session.page;
    page.goto(&root, None)
        .await
        .expect("navigate to the architecture index");

    assert_eq!(page_background(page).await, DARK_PAGE);
    assert_eq!(crate_fill(page).await, DARK_CRATE);

    let control = page.locator("#scheme");
    expect(control.clone())
        .to_be_visible()
        .await
        .expect("the script reveals the scheme control");
    control
        .select_option("light", None)
        .await
        .expect("choose Light");
    assert_eq!(page_background(page).await, LIGHT_PAGE);
    assert_eq!(crate_fill(page).await, LIGHT_CRATE);

    // The choice is remembered on the tree's other pages.
    page.goto(&format!("{root}containers/{}.html", containers[0]), None)
        .await
        .expect("navigate to a crate page");
    assert_eq!(page_background(page).await, LIGHT_PAGE);

    page.locator("#scheme")
        .select_option("system", None)
        .await
        .expect("choose System");
    assert_eq!(page_background(page).await, DARK_PAGE);
    session.browser.close().await.expect("close browser");
}

#[tokio::test]
#[ignore = "needs a snapshot build with its architecture tree and Chromium; run with: SNAPSHOT_DIST=... SNAPSHOT_BASE=/asbuilt/dev/ SNAPSHOT_VERSION=dev cargo nextest run --manifest-path crates/site-e2e/Cargo.toml --config-file .config/nextest.toml --run-ignored only -E 'test(site_architecture)'"]
async fn site_architecture_without_javascript_follows_the_system_and_shows_no_control() {
    let root = serve_snapshot().await;
    let session = open_session(false, ColorScheme::Dark).await;
    let page = &session.page;
    page.goto(&root, None)
        .await
        .expect("navigate to the architecture index");

    assert_eq!(page_background(page).await, DARK_PAGE);
    assert_eq!(crate_fill(page).await, DARK_CRATE);
    expect(page.locator("label.scheme"))
        .to_be_hidden()
        .await
        .expect("without the script the control stays hidden");
    session.browser.close().await.expect("close browser");
}
