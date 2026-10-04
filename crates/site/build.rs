//! Highlights the code snippets in `snippets/` at build time with syntect and
//! emits an `OUT_DIR/snippets.rs` module of `&str` constants holding the
//! highlighted inner HTML (token `<span>`s with inline colors, no outer
//! `<pre>`). The browser ships none of syntect; `src/snippets.rs` includes the
//! generated file and the components render the constants via `inner_html`.

use std::{env, fs, path::Path};

use syntect::highlighting::ThemeSet;
use syntect::html::highlighted_html_for_string;
use syntect::parsing::SyntaxSet;

/// The `context` figure of the tree's views page: its caption dropped (the
/// page has its own heading) and every link into the tree, in the markup
/// and in the edge data, re-based from the tree's root to `architecture/`.
fn context_figure(views_html: &str) -> Option<String> {
    let caption = "<figcaption>context</figcaption>";
    let at = views_html.find(caption)?;
    let start = views_html[..at].rfind("<figure ")?;
    let end = at + views_html[at..].find("</figure>")? + "</figure>".len();
    Some(
        views_html[start..end]
            .replacen(caption, "", 1)
            .replace("\"containers/", "\"architecture/containers/")
            .replace("\"index.html", "\"architecture/index.html"),
    )
}

fn main() {
    println!("cargo:rerun-if-changed=snippets");

    let ss = SyntaxSet::load_defaults_newlines();
    let ts = ThemeSet::load_defaults();
    let theme = &ts.themes["base16-ocean.dark"];

    let mut paths: Vec<_> = fs::read_dir("snippets")
        .expect("snippets/ dir")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_file())
        .collect();
    paths.sort();

    let mut generated = String::new();
    for path in paths {
        println!("cargo:rerun-if-changed={}", path.display());
        let code = fs::read_to_string(&path).expect("read snippet");
        let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
        let syntax = ss
            .find_syntax_by_extension(ext)
            .unwrap_or_else(|| ss.find_syntax_plain_text());
        let html = highlighted_html_for_string(&code, &ss, syntax, theme).expect("highlight");

        // Drop syntect's outer `<pre style=...>` / `</pre>`; the component
        // supplies its own styled <pre>. The first '>' closes the <pre> tag.
        // Trim the newline syntect emits right after `<pre>` (and before
        // `</pre>`) so the block has no leading/trailing blank line.
        let after_open = html.find('>').map(|i| &html[i + 1..]).unwrap_or(&html);
        let inner = after_open
            .trim()
            .strip_suffix("</pre>")
            .unwrap_or(after_open)
            .trim()
            .to_string();

        let stem = path.file_stem().unwrap().to_str().unwrap();
        let name = format!("{stem}_{ext}")
            .to_uppercase()
            .replace(['-', '.'], "_");
        generated.push_str(&format!("pub const {name}: &str = r####\"{inner}\"####;\n"));
    }

    // This repo's own context view, lifted from the generated architecture
    // tree (`asbuilt docs` runs before Trunk in preview.sh and pages.yml),
    // so the landing page shows the same figure the tree does: inlined,
    // in the page's palette, with the viewer. A tree not yet generated (a
    // plain clippy or test run) leaves it empty and the page shows the
    // rendered image instead.
    let views_page = Path::new("public/architecture/views.html");
    println!("cargo:rerun-if-changed={}", views_page.display());
    let figure = fs::read_to_string(views_page)
        .ok()
        .and_then(|html| context_figure(&html))
        .unwrap_or_default();
    generated.push_str(&format!(
        "pub const CONTEXT_FIGURE: &str = r####\"{figure}\"####;\n"
    ));

    let dest = Path::new(&env::var("OUT_DIR").unwrap()).join("snippets.rs");
    fs::write(dest, generated).expect("write snippets.rs");

    // Identify which snapshot this build is. Set by the deploy workflow:
    // `SITE_VERSION=0.14.0` for a release snapshot, left unset (→ "dev") for the
    // main-HEAD build. Read in-app via `env!("SITE_VERSION")`.
    println!("cargo:rerun-if-env-changed=SITE_VERSION");
    let version = env::var("SITE_VERSION").unwrap_or_else(|_| "dev".to_string());
    println!("cargo:rustc-env=SITE_VERSION={version}");

    // The LikeC4 release the CLI pins, for the hero's badge. Read from the
    // constant, since `crates/asbuilt/src/lib.rs` is the one place the
    // release is named. Read in-app via `env!("LIKEC4_VERSION")`.
    let lib_rs = "../asbuilt/src/lib.rs";
    println!("cargo:rerun-if-changed={lib_rs}");
    let source = fs::read_to_string(lib_rs).expect("read crates/asbuilt/src/lib.rs");
    let likec4 = source
        .lines()
        .find_map(|line| {
            line.strip_prefix("pub const LIKEC4_VERSION: &str = \"")?
                .strip_suffix("\";")
        })
        .expect("LIKEC4_VERSION in crates/asbuilt/src/lib.rs");
    println!("cargo:rustc-env=LIKEC4_VERSION={likec4}");
}
