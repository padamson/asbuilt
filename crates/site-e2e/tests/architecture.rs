//! The architecture section of a deployed snapshot: the output of
//! `asbuilt docs` over this repo's own model, mounted at `architecture/`
//! under the snapshot's base path. The assertions are about the tree
//! being complete, its views inlined and resolving under the real base
//! path (which a wrong `-o` or a missing `render` would break), and its
//! colors following the system's scheme or the visitor's choice, and
//! with the script blocked.
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
    Browser, ColorScheme, EmulateMediaOptions, Locator, Page, Playwright,
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

/// A Chromium page emulating `scheme` as the system's color scheme, with
/// every 4xx/5xx response recorded.
struct Session {
    _pw: Playwright,
    browser: Browser,
    page: Page,
    broken: Arc<Mutex<Vec<String>>>,
}

async fn open_session(scheme: ColorScheme) -> Session {
    let pw = Playwright::launch().await.expect("launch playwright");
    let browser = pw.chromium().launch().await.expect("launch chromium");
    let page = browser.new_page().await.expect("new page");
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
    let session = open_session(ColorScheme::Light).await;
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
    let session = open_session(ColorScheme::Dark).await;
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
async fn site_architecture_without_its_script_follows_the_system_and_shows_no_control() {
    // The script blocked, as a strict content security policy or a failed
    // load would leave it. playwright-rs 0.19's `javascript_enabled` does
    // not reach the driver (it sends `javascriptEnabled`; Playwright reads
    // `javaScriptEnabled`), so blocking the one script stands in for it.
    let root = serve_snapshot().await;
    let session = open_session(ColorScheme::Dark).await;
    let page = &session.page;
    page.route(
        "**/theme.js",
        |route| async move { route.abort(None).await },
    )
    .await
    .expect("block the scheme script");
    page.route(
        "**/viewer.js",
        |route| async move { route.abort(None).await },
    )
    .await
    .expect("block the viewer script");
    page.goto(&root, None)
        .await
        .expect("navigate to the architecture index");

    assert_eq!(page_background(page).await, DARK_PAGE);
    assert_eq!(crate_fill(page).await, DARK_CRATE);
    expect(page.locator("label.scheme"))
        .to_be_hidden()
        .await
        .expect("without the script the control stays hidden");
    // And the view is scaled to the column, as the stylesheet alone has it.
    expect(page.locator(".viewer-bar").first())
        .to_be_hidden()
        .await
        .expect("without the viewer script its controls stay hidden");
    let fitted = page
        .evaluate::<(), bool>(
            "() => { const svg = document.querySelector('svg.c4'); const f = svg.closest('figure'); return Math.abs(svg.getBoundingClientRect().width - f.clientWidth) < 2; }",
            None,
        )
        .await
        .expect("compare the view's width to its figure's");
    assert!(fitted, "without the viewer script the view fits the column");
    session.browser.close().await.expect("close browser");
}

/// How a page's active views are drawn: how many there are, the smallest
/// and largest ratio of rendered width to natural (viewBox) width, and
/// whether any frame scrolls sideways. A page with none reports zeros,
/// so the caller's claim fails in its own words.
async fn view_scales(page: &Page) -> (usize, f64, f64, bool) {
    let values = page
        .evaluate::<(), Vec<f64>>(
            "() => { \
               const views = [...document.querySelectorAll('figure[data-viewer-active] svg.c4')]; \
               const ratios = views.map(s => s.getBoundingClientRect().width / s.viewBox.baseVal.width); \
               const frames = [...document.querySelectorAll('figure[data-viewer-active] .viewer-frame')]; \
               if (ratios.length === 0) return [0, 0, 0, 0]; \
               return [ratios.length, Math.min(...ratios), Math.max(...ratios), frames.some(f => f.scrollWidth > f.clientWidth + 1) ? 1 : 0]; \
             }",
            None,
        )
        .await
        .expect("measure the page's views");
    assert_eq!(values.len(), 4, "the measurement returns four numbers");
    (values[0] as usize, values[1], values[2], values[3] > 0.5)
}

/// The viewer holds every view between the legibility floor and 1:1, so a
/// wide view scrolls inside its frame rather than shrinking, and Wide
/// takes a figure past the text column and is remembered across pages.
#[tokio::test]
#[ignore = "needs a snapshot build with its architecture tree and Chromium; run with: SNAPSHOT_DIST=... SNAPSHOT_BASE=/asbuilt/dev/ SNAPSHOT_VERSION=dev cargo nextest run --manifest-path crates/site-e2e/Cargo.toml --config-file .config/nextest.toml --run-ignored only -E 'test(site_architecture)'"]
async fn site_architecture_views_are_legible_and_can_go_wide() {
    let containers = committed_containers();
    let root = serve_snapshot().await;
    let session = open_session(ColorScheme::Light).await;
    let page = &session.page;

    let mut any_scrolls = false;
    for path in std::iter::once("index.html".to_string())
        .chain(containers.iter().map(|id| format!("containers/{id}.html")))
    {
        page.goto(&format!("{root}{path}"), None)
            .await
            .unwrap_or_else(|e| panic!("navigate to {path}: {e:?}"));
        expect(page.locator(".viewer-bar").first())
            .to_be_visible()
            .await
            .unwrap_or_else(|e| panic!("{path}: the script reveals the viewer controls: {e:?}"));
        let (count, min, max, scrolls) = view_scales(page).await;
        assert!(count > 0, "{path}: no view got the viewer");
        assert!(
            (0.69..=1.01).contains(&min) && max <= 1.01,
            "{path}: view scales run {min:.2}..{max:.2}; the viewer holds them in 0.7..1"
        );
        any_scrolls |= scrolls;
    }
    assert!(
        any_scrolls,
        "this repo's widest views exceed the column, so some frame must scroll"
    );

    // Wide: past the text column, and remembered on the next page.
    let column = page
        .evaluate::<(), f64>("() => document.querySelector('main').clientWidth", None)
        .await
        .expect("the text column's width");
    page.locator("[data-viewer-wide]")
        .first()
        .click(None)
        .await
        .expect("press Wide");
    let figure = page
        .evaluate::<(), f64>(
            "() => document.querySelector('figure[data-viewer-active]').getBoundingClientRect().width",
            None,
        )
        .await
        .expect("the wide figure's width");
    assert!(
        figure > column + 40.0,
        "Wide should take the figure past the column ({figure:.0}px vs {column:.0}px)"
    );
    page.goto(&format!("{root}index.html"), None)
        .await
        .expect("navigate to the index");
    expect(page.locator("[data-viewer-wide]").first())
        .to_have_attribute("aria-pressed", "true")
        .await
        .expect("Wide is remembered on the next page");
    session.browser.close().await.expect("close browser");
}

/// The rendered width of a located element.
async fn width_of(locator: &Locator) -> f64 {
    locator
        .bounding_box()
        .await
        .expect("measure the element")
        .expect("the element has a box")
        .width
}

/// Wait, bounded, until `pass` holds for the element's width; the wheel
/// and key handlers run after Playwright's input call returns.
async fn wait_for_width(locator: &Locator, pass: impl Fn(f64) -> bool) -> f64 {
    let mut last = width_of(locator).await;
    for _ in 0..40 {
        if pass(last) {
            return last;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        last = width_of(locator).await;
    }
    last
}

async fn frame_scroll_left(page: &Page) -> f64 {
    page.evaluate::<(), f64>(
        "() => document.querySelector('figure[data-viewer-active] .viewer-frame').scrollLeft",
        None,
    )
    .await
    .expect("the frame's scroll position")
}

/// The first active view of a crate page, with what its claims measure
/// against: the view's width on load and the frame's center.
struct ViewPage {
    session: Session,
    svg: Locator,
    frame: Locator,
    initial: f64,
    cx: f64,
    cy: f64,
}

async fn open_first_view(root: &str) -> ViewPage {
    let containers = committed_containers();
    let session = open_session(ColorScheme::Light).await;
    let page = &session.page;
    page.goto(&format!("{root}containers/{}.html", containers[0]), None)
        .await
        .expect("navigate to a crate page");
    let frame = page
        .locator("figure[data-viewer-active] .viewer-frame")
        .first();
    let svg = page.locator("figure[data-viewer-active] svg.c4").first();
    expect(frame.clone())
        .to_be_visible()
        .await
        .expect("the first view has its frame");
    let initial = width_of(&svg).await;
    let frame_box = frame
        .bounding_box()
        .await
        .expect("measure the frame")
        .expect("the frame has a box");
    let cx = frame_box.x + frame_box.width / 2.0;
    let cy = frame_box.y + frame_box.height / 2.0;
    page.mouse()
        .move_to(cx, cy, None)
        .await
        .expect("point at the view");
    ViewPage {
        session,
        svg,
        frame,
        initial,
        cx,
        cy,
    }
}

/// A wheel step up with `modifier` held (none when empty), as a trackpad
/// pinch or a mouse wheel with the key sends it.
async fn wheel_up_with(page: &Page, modifier: &str) {
    if !modifier.is_empty() {
        page.keyboard()
            .down(modifier)
            .await
            .expect("hold the modifier");
    }
    page.mouse().wheel(0.0, -200.0).await.expect("wheel");
    if !modifier.is_empty() {
        page.keyboard()
            .up(modifier)
            .await
            .expect("release the modifier");
    }
}

async fn press_zoom_in(page: &Page, times: usize) {
    for _ in 0..times {
        page.locator("[data-viewer-zoom='in']")
            .first()
            .click(None)
            .await
            .expect("press +");
    }
}

#[tokio::test]
#[ignore = "needs a snapshot build with its architecture tree and Chromium; run with: SNAPSHOT_DIST=... SNAPSHOT_BASE=/asbuilt/dev/ SNAPSHOT_VERSION=dev cargo nextest run --manifest-path crates/site-e2e/Cargo.toml --config-file .config/nextest.toml --run-ignored only -E 'test(site_architecture)'"]
async fn site_architecture_a_plain_wheel_does_not_zoom() {
    let root = serve_snapshot().await;
    let v = open_first_view(&root).await;
    wheel_up_with(&v.session.page, "").await;
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    let after = width_of(&v.svg).await;
    assert!(
        (after - v.initial).abs() < 1.0,
        "a plain wheel must not zoom ({:.0} -> {after:.0})",
        v.initial
    );
    v.session.browser.close().await.expect("close browser");
}

#[tokio::test]
#[ignore = "needs a snapshot build with its architecture tree and Chromium; run with: SNAPSHOT_DIST=... SNAPSHOT_BASE=/asbuilt/dev/ SNAPSHOT_VERSION=dev cargo nextest run --manifest-path crates/site-e2e/Cargo.toml --config-file .config/nextest.toml --run-ignored only -E 'test(site_architecture)'"]
async fn site_architecture_control_wheel_zooms_in() {
    let root = serve_snapshot().await;
    let v = open_first_view(&root).await;
    wheel_up_with(&v.session.page, "Control").await;
    let zoomed = wait_for_width(&v.svg, |w| w > v.initial * 1.2).await;
    assert!(
        zoomed > v.initial * 1.2,
        "Control+wheel up zooms in ({:.0} -> {zoomed:.0})",
        v.initial
    );
    v.session.browser.close().await.expect("close browser");
}

#[tokio::test]
#[ignore = "needs a snapshot build with its architecture tree and Chromium; run with: SNAPSHOT_DIST=... SNAPSHOT_BASE=/asbuilt/dev/ SNAPSHOT_VERSION=dev cargo nextest run --manifest-path crates/site-e2e/Cargo.toml --config-file .config/nextest.toml --run-ignored only -E 'test(site_architecture)'"]
async fn site_architecture_meta_wheel_zooms_in() {
    let root = serve_snapshot().await;
    let v = open_first_view(&root).await;
    wheel_up_with(&v.session.page, "Meta").await;
    let zoomed = wait_for_width(&v.svg, |w| w > v.initial * 1.2).await;
    assert!(
        zoomed > v.initial * 1.2,
        "Meta+wheel up zooms in ({:.0} -> {zoomed:.0})",
        v.initial
    );
    v.session.browser.close().await.expect("close browser");
}

#[tokio::test]
#[ignore = "needs a snapshot build with its architecture tree and Chromium; run with: SNAPSHOT_DIST=... SNAPSHOT_BASE=/asbuilt/dev/ SNAPSHOT_VERSION=dev cargo nextest run --manifest-path crates/site-e2e/Cargo.toml --config-file .config/nextest.toml --run-ignored only -E 'test(site_architecture)'"]
async fn site_architecture_the_plus_button_steps_the_zoom_up() {
    let root = serve_snapshot().await;
    let v = open_first_view(&root).await;
    press_zoom_in(&v.session.page, 1).await;
    let stepped = wait_for_width(&v.svg, |w| w > v.initial * 1.2).await;
    assert!(
        stepped > v.initial * 1.2,
        "+ steps the zoom up by a quarter ({:.0} -> {stepped:.0})",
        v.initial
    );
    v.session.browser.close().await.expect("close browser");
}

#[tokio::test]
#[ignore = "needs a snapshot build with its architecture tree and Chromium; run with: SNAPSHOT_DIST=... SNAPSHOT_BASE=/asbuilt/dev/ SNAPSHOT_VERSION=dev cargo nextest run --manifest-path crates/site-e2e/Cargo.toml --config-file .config/nextest.toml --run-ignored only -E 'test(site_architecture)'"]
async fn site_architecture_the_zero_key_returns_to_the_floor_scale() {
    let root = serve_snapshot().await;
    let v = open_first_view(&root).await;
    press_zoom_in(&v.session.page, 2).await;
    let stepped = wait_for_width(&v.svg, |w| w > v.initial * 1.5).await;
    assert!(stepped > v.initial * 1.5, "zoomed in first ({stepped:.0})");
    v.frame.focus().await.expect("focus the frame");
    v.frame.press("0", None).await.expect("press 0");
    let restored = wait_for_width(&v.svg, |w| (w - v.initial).abs() < 1.0).await;
    assert!(
        (restored - v.initial).abs() < 1.0,
        "0 returns to the floor rule ({stepped:.0} -> {restored:.0}, was {:.0})",
        v.initial
    );
    v.session.browser.close().await.expect("close browser");
}

#[tokio::test]
#[ignore = "needs a snapshot build with its architecture tree and Chromium; run with: SNAPSHOT_DIST=... SNAPSHOT_BASE=/asbuilt/dev/ SNAPSHOT_VERSION=dev cargo nextest run --manifest-path crates/site-e2e/Cargo.toml --config-file .config/nextest.toml --run-ignored only -E 'test(site_architecture)'"]
async fn site_architecture_a_drag_pans_the_frame() {
    let root = serve_snapshot().await;
    let v = open_first_view(&root).await;
    let page = &v.session.page;
    // Zoom in so the frame overflows, then pull the view left.
    press_zoom_in(page, 3).await;
    let zoomed = wait_for_width(&v.svg, |w| w > v.initial * 1.5).await;
    assert!(zoomed > v.initial * 1.5, "zoomed in first ({zoomed:.0})");
    let before = frame_scroll_left(page).await;
    page.mouse()
        .move_to(v.cx, v.cy, None)
        .await
        .expect("point at the view");
    page.mouse().down(None).await.expect("press");
    page.mouse()
        .move_to(v.cx - 60.0, v.cy, None)
        .await
        .expect("drag");
    page.mouse()
        .move_to(v.cx - 120.0, v.cy, None)
        .await
        .expect("drag further");
    page.mouse().up(None).await.expect("release");
    let after = frame_scroll_left(page).await;
    assert!(
        after > before + 100.0,
        "dragging 120px left pans the frame ({before:.0} -> {after:.0})"
    );
    v.session.browser.close().await.expect("close browser");
}

/// Safari reports a trackpad pinch as gesture events with a scale
/// relative to the gesture's start; the viewer zooms by that scale. The
/// gate runs Chromium, so the events are synthesized: this proves the
/// handler, not Safari.
#[tokio::test]
#[ignore = "needs a snapshot build with its architecture tree and Chromium; run with: SNAPSHOT_DIST=... SNAPSHOT_BASE=/asbuilt/dev/ SNAPSHOT_VERSION=dev cargo nextest run --manifest-path crates/site-e2e/Cargo.toml --config-file .config/nextest.toml --run-ignored only -E 'test(site_architecture)'"]
async fn site_architecture_a_pinch_gesture_zooms_the_view() {
    let containers = committed_containers();
    let root = serve_snapshot().await;
    let session = open_session(ColorScheme::Light).await;
    let page = &session.page;
    page.goto(&format!("{root}containers/{}.html", containers[0]), None)
        .await
        .expect("navigate to a crate page");
    let svg = page.locator("figure[data-viewer-active] svg.c4").first();
    expect(svg.clone())
        .to_be_visible()
        .await
        .expect("the first view is active");
    let initial = width_of(&svg).await;
    page.evaluate::<(), ()>(
        "() => { \
           const frame = document.querySelector('figure[data-viewer-active] .viewer-frame'); \
           const rect = frame.getBoundingClientRect(); \
           frame.dispatchEvent(new Event('gesturestart', { cancelable: true })); \
           const change = new Event('gesturechange', { cancelable: true }); \
           change.scale = 1.5; \
           change.clientX = rect.left + rect.width / 2; \
           change.clientY = rect.top + rect.height / 2; \
           frame.dispatchEvent(change); \
           frame.dispatchEvent(new Event('gestureend')); \
         }",
        None,
    )
    .await
    .expect("synthesize a pinch");
    let pinched = wait_for_width(&svg, |w| w > initial * 1.4).await;
    assert!(
        (pinched - initial * 1.5).abs() < initial * 0.05,
        "a pinch to 1.5 scales the view by 1.5 ({initial:.0} -> {pinched:.0})"
    );
    session.browser.close().await.expect("close browser");
}

/// The bar's hint as the viewer sees the platform: the page is loaded
/// with `navigator.platform` reporting `platform`. The index view fits
/// its frame, so the hint offers no pan.
async fn hint_on(platform: &str, root: &str) -> String {
    let session = open_session(ColorScheme::Light).await;
    let page = &session.page;
    page.add_init_script(&format!(
        "Object.defineProperty(navigator, 'platform', {{ get: () => '{platform}' }});"
    ))
    .await
    .expect("report the platform");
    page.goto(root, None)
        .await
        .expect("navigate to the architecture index");
    let hint = page.locator(".viewer-hint").first();
    expect(hint.clone())
        .to_be_visible()
        .await
        .expect("the script fills the hint");
    let text = hint
        .text_content()
        .await
        .expect("read the hint")
        .unwrap_or_default();
    session.browser.close().await.expect("close browser");
    text
}

#[tokio::test]
#[ignore = "needs a snapshot build with its architecture tree and Chromium; run with: SNAPSHOT_DIST=... SNAPSHOT_BASE=/asbuilt/dev/ SNAPSHOT_VERSION=dev cargo nextest run --manifest-path crates/site-e2e/Cargo.toml --config-file .config/nextest.toml --run-ignored only -E 'test(site_architecture)'"]
async fn site_architecture_hint_names_cmd_on_a_mac() {
    let root = serve_snapshot().await;
    assert_eq!(hint_on("MacIntel", &root).await, "\u{2318} scroll to zoom");
}

#[tokio::test]
#[ignore = "needs a snapshot build with its architecture tree and Chromium; run with: SNAPSHOT_DIST=... SNAPSHOT_BASE=/asbuilt/dev/ SNAPSHOT_VERSION=dev cargo nextest run --manifest-path crates/site-e2e/Cargo.toml --config-file .config/nextest.toml --run-ignored only -E 'test(site_architecture)'"]
async fn site_architecture_hint_names_ctrl_elsewhere() {
    let root = serve_snapshot().await;
    assert_eq!(hint_on("Win32", &root).await, "Ctrl scroll to zoom");
}

/// The first node link on the index and where it points.
async fn first_node_link(page: &Page) -> (Locator, String) {
    // A crate's node, not an external's (which links to an index row).
    let link = page
        .locator("svg.c4[data-view='index'] a[href^='containers/']")
        .first();
    expect(link.clone())
        .to_be_visible()
        .await
        .expect("the index view has a linked node");
    let href = link
        .get_attribute("href")
        .await
        .expect("read the link")
        .expect("the link has an href");
    (link, href)
}

#[tokio::test]
#[ignore = "needs a snapshot build with its architecture tree and Chromium; run with: SNAPSHOT_DIST=... SNAPSHOT_BASE=/asbuilt/dev/ SNAPSHOT_VERSION=dev cargo nextest run --manifest-path crates/site-e2e/Cargo.toml --config-file .config/nextest.toml --run-ignored only -E 'test(site_architecture)'"]
async fn site_architecture_a_node_links_to_its_page() {
    let root = serve_snapshot().await;
    let session = open_session(ColorScheme::Light).await;
    let page = &session.page;
    page.goto(&root, None)
        .await
        .expect("navigate to the architecture index");
    let (link, href) = first_node_link(page).await;
    link.click(None).await.expect("click the node");
    page.wait_for_load_state(None)
        .await
        .expect("the crate page loads");
    assert!(
        page.url().ends_with(&href),
        "clicking the node opens {href}, not {}",
        page.url()
    );
    session.browser.close().await.expect("close browser");
}

#[tokio::test]
#[ignore = "needs a snapshot build with its architecture tree and Chromium; run with: SNAPSHOT_DIST=... SNAPSHOT_BASE=/asbuilt/dev/ SNAPSHOT_VERSION=dev cargo nextest run --manifest-path crates/site-e2e/Cargo.toml --config-file .config/nextest.toml --run-ignored only -E 'test(site_architecture)'"]
async fn site_architecture_a_node_names_itself_by_its_id() {
    let root = serve_snapshot().await;
    let session = open_session(ColorScheme::Light).await;
    let page = &session.page;
    page.goto(&root, None)
        .await
        .expect("navigate to the architecture index");
    let (link, href) = first_node_link(page).await;
    let title = link
        .locator("title")
        .first()
        .text_content()
        .await
        .expect("read the node's title")
        .unwrap_or_default();
    assert_eq!(
        href,
        format!("containers/{title}.html"),
        "a crate's node is titled by its id, which names its page"
    );
    session.browser.close().await.expect("close browser");
}

#[tokio::test]
#[ignore = "needs a snapshot build with its architecture tree and Chromium; run with: SNAPSHOT_DIST=... SNAPSHOT_BASE=/asbuilt/dev/ SNAPSHOT_VERSION=dev cargo nextest run --manifest-path crates/site-e2e/Cargo.toml --config-file .config/nextest.toml --run-ignored only -E 'test(site_architecture)'"]
async fn site_architecture_a_drag_from_a_node_pans_without_following_its_link() {
    let root = serve_snapshot().await;
    let v = open_first_view(&root).await;
    let page = &v.session.page;
    press_zoom_in(page, 3).await;
    let zoomed = wait_for_width(&v.svg, |w| w > v.initial * 1.5).await;
    assert!(zoomed > v.initial * 1.5, "zoomed in first ({zoomed:.0})");
    // Zoomed about the center, the first node may sit outside the
    // frame's clip; bring it in, then measure.
    let link = page.locator("figure[data-viewer-active] svg.c4 a").first();
    link.scroll_into_view_if_needed()
        .await
        .expect("scroll the node into the frame");
    let node = link
        .bounding_box()
        .await
        .expect("measure a node")
        .expect("a node is drawn");
    let (x, y) = (node.x + node.width / 2.0, node.y + node.height / 2.0);
    let url = page.url();
    let before = frame_scroll_left(page).await;
    page.mouse()
        .move_to(x, y, None)
        .await
        .expect("point at the node");
    page.mouse().down(None).await.expect("press");
    page.mouse().move_to(x - 60.0, y, None).await.expect("drag");
    page.mouse()
        .move_to(x - 120.0, y, None)
        .await
        .expect("drag further");
    page.mouse().up(None).await.expect("release");
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    let after = frame_scroll_left(page).await;
    assert!(
        after > before + 100.0,
        "the drag panned ({before:.0} -> {after:.0})"
    );
    assert_eq!(page.url(), url, "the node's link was not followed");
    v.session.browser.close().await.expect("close browser");
}

async fn frame_overflows(page: &Page) -> bool {
    page.evaluate::<(), bool>(
        "() => { const f = document.querySelector('figure[data-viewer-active] .viewer-frame'); return f.scrollWidth > f.clientWidth + 1 || f.scrollHeight > f.clientHeight + 1; }",
        None,
    )
    .await
    .expect("whether the frame overflows")
}

async fn frame_cursor(page: &Page) -> String {
    page.evaluate::<(), String>(
        "() => getComputedStyle(document.querySelector('figure[data-viewer-active] .viewer-frame')).cursor",
        None,
    )
    .await
    .expect("the frame's cursor")
}

/// A view that fits its frame has nowhere to pan to, so the frame drops
/// the grab cursor.
#[tokio::test]
#[ignore = "needs a snapshot build with its architecture tree and Chromium; run with: SNAPSHOT_DIST=... SNAPSHOT_BASE=/asbuilt/dev/ SNAPSHOT_VERSION=dev cargo nextest run --manifest-path crates/site-e2e/Cargo.toml --config-file .config/nextest.toml --run-ignored only -E 'test(site_architecture)'"]
async fn site_architecture_a_view_that_fits_offers_no_pan() {
    let root = serve_snapshot().await;
    let v = open_first_view(&root).await;
    let page = &v.session.page;
    page.locator("[data-viewer-mode='fit']")
        .first()
        .click(None)
        .await
        .expect("press Fit");
    let mut overflows = true;
    for _ in 0..40 {
        overflows = frame_overflows(page).await;
        if !overflows {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert!(!overflows, "after Fit the view sits inside its frame");
    assert_ne!(
        frame_cursor(page).await,
        "grab",
        "a fitted view is not pannable"
    );
    v.session.browser.close().await.expect("close browser");
}

#[tokio::test]
#[ignore = "needs a snapshot build with its architecture tree and Chromium; run with: SNAPSHOT_DIST=... SNAPSHOT_BASE=/asbuilt/dev/ SNAPSHOT_VERSION=dev cargo nextest run --manifest-path crates/site-e2e/Cargo.toml --config-file .config/nextest.toml --run-ignored only -E 'test(site_architecture)'"]
async fn site_architecture_a_view_larger_than_its_frame_offers_a_pan() {
    let root = serve_snapshot().await;
    let v = open_first_view(&root).await;
    let page = &v.session.page;
    press_zoom_in(page, 3).await;
    let zoomed = wait_for_width(&v.svg, |w| w > v.initial * 1.5).await;
    assert!(zoomed > v.initial * 1.5, "zoomed in first ({zoomed:.0})");
    assert_eq!(
        frame_cursor(page).await,
        "grab",
        "an overflowing view is pannable"
    );
    v.session.browser.close().await.expect("close browser");
}
