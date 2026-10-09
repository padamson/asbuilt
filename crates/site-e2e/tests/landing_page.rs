//! The dogfood deploy gate: serve the Trunk-built landing page and drive it
//! with playwright-rs, asserting it works as advertised. The site is a
//! Leptos CSR/WASM app, so these assertions also prove the bundle boots and
//! its interactive widgets react, which a static-HTML check could not.
//!
//! Every test here is `#[ignore]`d because it needs a built site and a
//! Chromium install that a fresh clone lacks. A missing `dist/` is a
//! failure naming the build command, never a skip: a test that returns
//! early reports a pass it did not earn. The Pages job runs them with
//! `--run-ignored only -E 'test(/^site_/)'`.
//!
//! Run after building the site, from the repo root:
//!   cargo run -p asbuilt -- render && cp docs/architecture/views/context.svg crates/site/public/views/
//!   (cd crates/site && SITE_VERSION=dev trunk build --release)
//!   cargo nextest run --manifest-path crates/site-e2e/Cargo.toml --config-file .config/nextest.toml \
//!     --run-ignored only -E 'test(/^site_/) and not test(/^site_(deployed_snapshot|architecture)/)'

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use axum::Router;
use axum::http::{StatusCode, header::CONTENT_TYPE};
use axum::response::IntoResponse;
use playwright_rs::protocol::{
    Animations, AriaSnapshotOptions, Page, Playwright, ScreenshotOptions, StartHarOptions,
    TracingStartOptions, TracingStopOptions,
};
use playwright_rs::{expect, expect_page};
use tower_http::services::ServeDir;

const IGNORE: &str = "needs a Trunk-built site and Chromium";

/// The switcher fetches the manifest from under the site prefix (project
/// Pages), so every stub answers there.
const MANIFEST_PATH: &str = "/asbuilt/versions.json";

/// The built site. Not building it is a failure, not a reason to skip.
fn dist() -> PathBuf {
    let dist = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../site/dist");
    assert!(
        dist.join("index.html").exists(),
        "{} has no index.html: run `SITE_VERSION=dev trunk build --release` in crates/site first",
        dist.display()
    );
    dist
}

fn receipts_dir() -> PathBuf {
    // The site's `public/receipts/` source dir, not `dist/`: Trunk's copy-dir
    // re-copies it into dist on every build, so receipts survive rebuilds.
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../site/public/receipts");
    std::fs::create_dir_all(&dir).expect("create receipts dir");
    dir
}

/// Serve `dist` on an ephemeral port. `overlay` routes are merged ahead of
/// the static fallback, so a test can stub an endpoint the built site
/// fetches without a second server.
async fn serve_with(
    dist: &Path,
    overlay: Option<Router>,
) -> (SocketAddr, tokio::task::JoinHandle<()>) {
    let app = overlay
        .unwrap_or_else(Router::new)
        .fallback_service(ServeDir::new(dist));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind site server");
    let addr = listener.local_addr().expect("local addr");
    let handle = tokio::spawn(async move {
        axum::serve(listener, app).await.expect("serve site");
    });
    (addr, handle)
}

/// What the stubbed backend answers for the manifest: JSON, or `None` for
/// an outage.
type Backend = Arc<Mutex<Option<String>>>;

fn backend_answering(manifest: &str) -> Backend {
    Arc::new(Mutex::new(Some(manifest.to_string())))
}

fn versions_manifest(backend: &Backend) -> Router {
    let answers = backend.clone();
    Router::new().route(
        MANIFEST_PATH,
        axum::routing::get(move || {
            let answer = answers.lock().expect("backend lock").clone();
            async move {
                match answer {
                    Some(json) => ([(CONTENT_TYPE, "application/json")], json).into_response(),
                    None => StatusCode::SERVICE_UNAVAILABLE.into_response(),
                }
            }
        }),
    )
}

/// A fresh Chromium page. The `Playwright` and `Browser` handles come back
/// too: dropping either tears down the browser.
async fn launch_page() -> (Playwright, playwright_rs::protocol::Browser, Page) {
    let pw = Playwright::launch().await.expect("launch playwright");
    let browser = pw.chromium().launch().await.expect("launch chromium");
    let page = browser.new_page().await.expect("new page");
    (pw, browser, page)
}

/// The origin the in-process tests serve the site on. Nothing listens
/// there and the driver never resolves it.
const IN_PROCESS_ORIGIN: &str = "https://asbuilt.test";

/// Open the landing page without a socket, serving `dist` from the test
/// process through `route_service`.
async fn open_site_in_process(
    dist: &Path,
    overlay: Option<Router>,
) -> (Playwright, playwright_rs::protocol::Browser, Page) {
    let app = overlay
        .unwrap_or_else(Router::new)
        .fallback_service(ServeDir::new(dist));
    let (pw, browser, page) = launch_page().await;
    page.route_service(&format!("{IN_PROCESS_ORIGIN}/**"), app)
        .await
        .expect("serve the site in-process");
    page.goto(IN_PROCESS_ORIGIN, None)
        .await
        .expect("navigate to the in-process site");
    (pw, browser, page)
}

/// An element screenshot with animations frozen, written as a receipt.
async fn shot(page: &Page, dir: &Path, file: &str, selector: &str) {
    let opts = ScreenshotOptions::builder()
        .animations(Animations::Disabled)
        .build();
    let bytes = page
        .locator(selector)
        .screenshot(opts)
        .await
        .unwrap_or_else(|e| panic!("screenshot {selector}: {e:?}"));
    std::fs::write(dir.join(file), bytes).unwrap_or_else(|e| panic!("write {file}: {e:?}"));
}

/// The six feature cards and a token unique to each snippet. The count
/// check makes adding a card without adding it here a failure.
const CARDS: [(&str, &str); 7] = [
    ("#feature-drift-check", "is stale"),
    ("#feature-reexports", "pub use"),
    ("#feature-externals", "[[externals]]"),
    ("#feature-views", "view context"),
    ("#feature-likec4", "asbuilt render"),
    ("#feature-viewer", "viewer = true"),
    ("#feature-rust-first", "extra_manifests"),
];

#[tokio::test]
#[ignore = "needs a Trunk-built site and Chromium; run with: cargo nextest run --manifest-path crates/site-e2e/Cargo.toml --config-file .config/nextest.toml --run-ignored only -E 'test(/^site_/)'"]
async fn site_landing_page_works_as_advertised() {
    let _ = IGNORE;
    let dist = dist();
    let receipts = receipts_dir();
    let (addr, server) = serve_with(&dist, None).await;

    let pw = Playwright::launch().await.expect("launch playwright");
    let browser = pw.chromium().launch().await.expect("launch chromium");
    let context = browser.new_context().await.expect("new context");

    // Trace and HAR the whole run; both are published as receipts.
    let tracing = context.tracing().await.expect("tracing handle");
    tracing
        .start(Some(
            TracingStartOptions::default()
                .name("asbuilt landing page dogfood")
                .screenshots(true)
                .snapshots(true),
        ))
        .await
        .expect("start trace");
    tracing
        .start_har(
            receipts.join("dogfood.har").to_string_lossy().into_owned(),
            Some(StartHarOptions::default()),
        )
        .await
        .expect("start HAR recording");

    let page = context.new_page().await.expect("new page");
    page.goto(&format!("http://{addr}"), None)
        .await
        .expect("navigate to site");

    // Asset paths must be relative so they resolve under the snapshot path
    // (/asbuilt/dev/, /asbuilt/vX.Y.Z/) on the deployed site. The gate serves
    // at the root, where both resolve, so the invariant is asserted directly.
    // The switcher's own links carry the prefix by design and are excluded.
    let absolute = page
        .locator("img[src^='/'], a[href^='/receipts'], a[href^='/views'], a[href^='/architecture']")
        .count()
        .await
        .expect("count root-absolute asset paths");
    assert_eq!(
        absolute, 0,
        "asset paths must be relative to resolve under the snapshot path"
    );

    // Step 1: the SPA renders. The locator auto-waits for the WASM app to
    // mount and paint the hero.
    expect(page.locator("#hero-title"))
        .to_have_text("Architecture models that cannot drift")
        .await
        .expect("hero renders once the WASM app boots");
    expect(page.locator("#hero-brand"))
        .to_have_attribute("aria-label", "asbuilt")
        .await
        .expect(
            "the hero opens with the asbuilt mark and wordmark, named for assistive technology",
        );
    expect(page.locator("#hero-brand .wordmark-built"))
        .to_have_css("font-family", "Bungee, system-ui, sans-serif")
        .await
        .expect("the wordmark's face comes from brand/fonts");
    expect(page.locator("#cta-architecture"))
        .to_have_attribute("href", "architecture/")
        .await
        .expect("the primary CTA links the generated architecture docs, relatively");
    expect_page(&page)
        .to_match_aria_snapshot(
            "- banner:\n  - heading \"Architecture models that cannot drift\" [level=1]\n- heading \"Install\" [level=2]\n- heading \"How it works\" [level=2]\n- heading \"Surveying itself\" [level=2]\n- heading \"What you get\" [level=2]",
        )
        .await
        .expect("the page's landmarks are present");
    let aria_tree = page
        .aria_snapshot(Some(AriaSnapshotOptions::default().boxes(true)))
        .await
        .expect("aria snapshot");
    std::fs::write(receipts.join("aria-snapshot.txt"), aria_tree).expect("write aria receipt");
    shot(&page, &receipts, "hero.png", "#hero").await;

    // Step 2: the how-it-works tabs react. Default is the survey command;
    // the check tab shows the drift verdict.
    let how = page.locator("#how-it-works");
    expect(how.clone())
        .to_contain_text("asbuilt survey")
        .await
        .expect("the survey tab is shown by default");
    page.locator("#how-it-works [data-lang='check']")
        .click(None)
        .await
        .expect("click the check tab");
    expect(page.locator("#how-it-works [data-lang='check']"))
        .to_have_attribute("aria-selected", "true")
        .await
        .expect("the check tab becomes selected");
    expect(how.clone())
        .to_contain_text("is stale")
        .await
        .expect("the check tab shows the drift verdict");
    expect(how)
        .not()
        .to_contain_text("git add")
        .await
        .expect("the survey snippet is replaced");

    // Step 3: this repo's own context view is the tree's figure, alive with
    // the viewer, in the page's dark palette, its nodes linking into the
    // tree (relatively).
    expect(page.locator("#example-context figure[data-viewer-active] svg.c4[data-view='context']"))
        .to_be_visible()
        .await
        .expect("the example embeds the tree's context figure and the viewer took it");
    let crate_fill = page
        .evaluate::<(), String>(
            "() => getComputedStyle(document.querySelector(\"#example-context svg.c4 .node.c4-k-container > polygon\")).fill",
            None,
        )
        .await
        .expect("read a crate box's fill");
    assert_eq!(
        crate_fill, "rgb(143, 45, 25)",
        "the figure follows the page's dark palette"
    );
    let into_tree = page
        .locator("#example-context svg.c4 a[href^='architecture/containers/']")
        .count()
        .await
        .expect("count node links into the tree");
    assert!(
        into_tree > 0,
        "a crate's node links to its page in the tree"
    );

    // Step 4: every feature card renders its own highlighted snippet.
    let rendered = page
        .locator("[id^='feature-']")
        .count()
        .await
        .expect("count rendered feature cards");
    assert_eq!(
        rendered,
        CARDS.len(),
        "{rendered} feature cards render but {} are checked; add the new card to CARDS",
        CARDS.len()
    );
    for (id, token) in CARDS {
        expect(page.locator(id))
            .to_be_visible()
            .await
            .unwrap_or_else(|e| panic!("feature card {id} should render: {e:?}"));
        expect(page.locator(id))
            .to_contain_text(token)
            .await
            .unwrap_or_else(|e| panic!("feature card {id} should show its snippet: {e:?}"));
        let colored = page
            .locator(format!("{id} span[style*='color']"))
            .count()
            .await
            .unwrap_or_else(|e| panic!("count colored spans in {id}: {e:?}"));
        assert!(
            colored > 0,
            "feature card {id} should render highlighted code"
        );
    }

    // Step 5: the footer credits what the tool leans on.
    let credits = page.locator("#credits");
    expect(credits.clone())
        .to_contain_text("LikeC4")
        .await
        .expect("footer credits LikeC4");
    expect(credits)
        .to_contain_text("playwright-rs")
        .await
        .expect("footer credits playwright-rs");

    tracing.stop_har().await.expect("write HAR receipt");
    tracing
        .stop(Some(TracingStopOptions::default().path(
            receipts.join("trace.zip").to_string_lossy().into_owned(),
        )))
        .await
        .expect("write trace receipt");

    browser.close().await.expect("close browser");
    server.abort();
}

/// The switcher reads the manifest at runtime from under the site prefix.
/// Served in-process with a fixture manifest, it must list the published
/// versions, warn that this is the dev build, and link the latest release
/// under the prefix.
#[tokio::test]
#[ignore = "needs a Trunk-built site and Chromium; run with: cargo nextest run --manifest-path crates/site-e2e/Cargo.toml --config-file .config/nextest.toml --run-ignored only -E 'test(/^site_/)'"]
async fn site_version_switcher_lists_versions_and_warns_on_dev() {
    let dist = dist();
    let manifest = versions_manifest(&backend_answering(
        r#"{"latest":"9.9.9","versions":["9.9.9","0.1.0"]}"#,
    ));
    let (_pw, browser, page) = open_site_in_process(&dist, Some(manifest)).await;

    expect(page.locator("#version-select"))
        .to_be_visible()
        .await
        .expect("version dropdown visible");
    expect(page.locator("#version-select"))
        .to_contain_text("v0.1.0")
        .await
        .expect("dropdown lists the published version from the manifest");
    expect(page.locator("text=Unreleased dev build"))
        .to_be_visible()
        .await
        .expect("dev build shows the unreleased banner");
    expect(page.locator("#version-switcher a"))
        .to_have_attribute("href", "/asbuilt/v9.9.9/")
        .await
        .expect("the banner links the latest release under the site prefix");

    browser.close().await.expect("close browser");
}

/// The dev (main HEAD) build reflects its unreleased state: it installs
/// from git, the crates.io badge says so, and there is no docs.rs button.
#[tokio::test]
#[ignore = "needs a Trunk-built site and Chromium; run with: cargo nextest run --manifest-path crates/site-e2e/Cargo.toml --config-file .config/nextest.toml --run-ignored only -E 'test(/^site_/)'"]
async fn site_dev_build_reflects_unreleased_state() {
    let dist = dist();
    let (_pw, browser, page) = open_site_in_process(&dist, None).await;

    expect(page.locator("#install"))
        .to_contain_text("cargo install --git https://github.com/padamson/asbuilt asbuilt")
        .await
        .expect("the dev build installs from git");
    let badge = page
        .locator("#hero-badges img[alt='crates.io: unreleased']")
        .count()
        .await
        .expect("count the crates.io badge");
    assert_eq!(
        badge, 1,
        "the dev build shows the unreleased crates.io badge"
    );
    expect(page.locator("#cta-docs"))
        .to_have_count(0)
        .await
        .expect("no docs.rs button before a release");

    browser.close().await.expect("close browser");
}

/// main's roadmap, which every release build fetches from the dev build so
/// any version shows the current one.
const CURRENT_ROADMAP_PATH: &str = "/asbuilt/dev/roadmap.json";

/// A roadmap no build has compiled in, served where main's is published.
/// A release snapshot showing it shows main's, not its own.
fn current_roadmap() -> Router {
    Router::new().route(
        CURRENT_ROADMAP_PATH,
        axum::routing::get(|| async {
            (
                [(CONTENT_TYPE, "application/json")],
                r#"{"milestone": "9.9.0", "next_milestone": "9.10.0", "items": [
                    {"id": "roadmap-now-fetched", "title": "Fetched", "blurb": "From main.", "horizon": "now", "status": "done"},
                    {"id": "roadmap-next-fetched", "title": "Next", "blurb": "", "horizon": "next"},
                    {"id": "roadmap-later-fetched", "title": "Later", "blurb": "", "horizon": "later"}
                ]}"#,
            )
        }),
    )
}

/// Now and Next each say what their release is about, under the heading
/// and above Now's progress bar, the way Later says nothing there is
/// planned yet.
#[tokio::test]
#[ignore = "needs a Trunk-built site and Chromium; run with: cargo nextest run --manifest-path crates/site-e2e/Cargo.toml --config-file .config/nextest.toml --run-ignored only -E 'test(/^site_/)'"]
async fn site_roadmap_columns_say_what_their_release_is_about() {
    let dist = dist();
    let (_pw, browser, page) = open_site_in_process(&dist, None).await;

    for column in ["#roadmap-now", "#roadmap-next"] {
        expect(page.locator(format!("{column} h3 + [data-roadmap-theme]")))
            .to_be_visible()
            .await
            .unwrap_or_else(|e| panic!("{column} shows its theme under its heading: {e:?}"));
    }
    expect(page.locator("#roadmap-later h3 + [data-roadmap-theme]"))
        .to_have_text("Under consideration, not yet planned")
        .await
        .expect("Later says nothing there is planned yet");
    expect(page.locator("#roadmap-now [data-roadmap-theme] + div #roadmap-progress"))
        .to_be_visible()
        .await
        .expect("Now's progress bar follows its theme");

    browser.close().await.expect("close browser");
}

/// The roadmap's progress bar agrees with its Now column: the bar's value
/// is the number of done items and its maximum the number of items.
#[tokio::test]
#[ignore = "needs a Trunk-built site and Chromium; run with: cargo nextest run --manifest-path crates/site-e2e/Cargo.toml --config-file .config/nextest.toml --run-ignored only -E 'test(/^site_/)'"]
async fn site_roadmap_progress_counts_the_done_items_in_now() {
    let dist = dist();
    let (_pw, browser, page) = open_site_in_process(&dist, None).await;

    for column in ["#roadmap-now", "#roadmap-next", "#roadmap-later"] {
        expect(page.locator(column))
            .to_be_visible()
            .await
            .unwrap_or_else(|e| panic!("roadmap column {column} should render: {e:?}"));
    }
    let done = page
        .locator("#roadmap-now li [data-status='Done']")
        .count()
        .await
        .expect("count done items");
    let total = page
        .locator("#roadmap-now li")
        .count()
        .await
        .expect("count Now items");
    let bar = page.locator("#roadmap-progress");
    let value = bar
        .get_attribute("aria-valuenow")
        .await
        .expect("read the bar's value");
    let max = bar
        .get_attribute("aria-valuemax")
        .await
        .expect("read the bar's maximum");
    assert_eq!(
        (value, max),
        (Some(done.to_string()), Some(total.to_string()))
    );

    browser.close().await.expect("close browser");
}

/// The artifact that deploys, not the build the gate above drives: the
/// snapshot built with `--public-url /asbuilt/<dest>/`, served under that
/// path with the manifest at the root, as gh-pages lays it out. Every
/// response must resolve, and the release-only rendering must match the
/// version. Driven by `SNAPSHOT_DIST`, `SNAPSHOT_BASE` and
/// `SNAPSHOT_VERSION`; a missing variable is a failure naming it.
#[tokio::test]
#[ignore = "needs a snapshot build and Chromium; run with: SNAPSHOT_DIST=... SNAPSHOT_BASE=/asbuilt/dev/ SNAPSHOT_VERSION=dev cargo nextest run --manifest-path crates/site-e2e/Cargo.toml --config-file .config/nextest.toml --run-ignored only -E 'test(site_deployed_snapshot)'"]
async fn site_deployed_snapshot_is_sound() {
    let var = |name: &str| {
        std::env::var(name).unwrap_or_else(|_| {
            panic!("{name} must be set (SNAPSHOT_DIST, SNAPSHOT_BASE, SNAPSHOT_VERSION)")
        })
    };
    let dist = PathBuf::from(var("SNAPSHOT_DIST"));
    let base = var("SNAPSHOT_BASE");
    let version = var("SNAPSHOT_VERSION");
    assert!(
        dist.join("index.html").exists(),
        "SNAPSHOT_DIST has no index.html: {}",
        dist.display()
    );

    // The real gh-pages layout: the snapshot under its base path, the
    // manifest at the site prefix shared by every snapshot.
    let mount = base.trim_end_matches('/').to_string();
    let manifest = format!(r#"{{"latest":"{version}","versions":["{version}"]}}"#);
    let mut overlay =
        versions_manifest(&backend_answering(&manifest)).nest_service(&mount, ServeDir::new(&dist));
    // gh-pages always has the dev build beside a release, and a release
    // fetches main's roadmap from it.
    if version != "dev" {
        overlay = overlay.merge(current_roadmap());
    }
    let (addr, server) = serve_with(&dist, Some(overlay)).await;
    let (_pw, browser, page) = launch_page().await;

    // Registered before navigating: any 4xx/5xx is an asset the snapshot
    // build pointed at the wrong place.
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

    page.goto(&format!("http://{addr}{base}"), None)
        .await
        .expect("navigate to the snapshot under its base path");
    expect(page.locator("#hero"))
        .to_be_visible()
        .await
        .expect("the snapshot renders under its base path");

    if version == "dev" {
        expect(page.locator("#install"))
            .to_contain_text("cargo install --git")
            .await
            .expect("the dev snapshot installs from git");
    } else {
        expect(page.locator("#install"))
            .to_contain_text("cargo install asbuilt")
            .await
            .expect("a release snapshot installs from crates.io");
        expect(page.locator("#cta-docs"))
            .to_have_count(1)
            .await
            .expect("a release snapshot links docs.rs");
        let unreleased = page
            .locator("[data-unreleased-badge]")
            .count()
            .await
            .expect("count unreleased badges");
        assert_eq!(unreleased, 0, "unreleased cards are dev-only");
        expect(page.locator("#roadmap-now h3"))
            .to_have_text("Now · 9.9.0")
            .await
            .expect("a release shows main's roadmap, not its own");
        expect(page.locator("#roadmap-now-fetched"))
            .to_be_visible()
            .await
            .expect("a release lists main's items");
        expect(page.locator("#roadmap-current"))
            .to_be_visible()
            .await
            .expect("a release says the roadmap is main's");
    }
    expect(page.locator("#version-select"))
        .to_contain_text(if version == "dev" { "dev (main)" } else { "v" })
        .await
        .expect("the switcher lists the current build");

    let broken = broken.lock().unwrap().clone();
    assert!(
        broken.is_empty(),
        "the snapshot requested assets that do not resolve under {base}: {broken:#?}"
    );

    browser.close().await.expect("close browser");
    server.abort();
}
