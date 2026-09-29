//! The documentation generator: a [`Model`] and the SVGs `asbuilt render`
//! produced, to a static HTML tree that mounts anywhere. A view LikeC4
//! drew is inlined into its page ([`crate::svg`]) and colored by
//! `theme.css` ([`crate::theme`]) for the system's light or dark scheme or
//! a visitor's choice; `theme.js`, the tree's one script, carries that
//! choice and is left out when `scheme_toggle` is off.
//!
//! Pure: no filesystem, no process. LikeC4 and Graphviz supply the
//! pictures; this module supplies what only the model knows (each crate
//! and module, its path and doc paragraph, what it references and what
//! references it). Every link is relative and no page is deeper than one
//! directory, so the tree serves from `file://`, a GitHub Pages
//! subdirectory, or a versioned snapshot unchanged.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

use pulldown_cmark::{Event, Options, Parser, html};

use crate::config::{ColorScheme, ThemeColor};
use crate::emit::{children_of, link_encode, view_ids};
use crate::model::{Element, ElementKind, Id, Model, Relation, dotted, sanitize_id};
use crate::svg::{self, ViewSource};
use crate::theme;

/// What the pages need beyond the model.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DocsOptions {
    pub title: String,
    /// A prefix that turns a repo-relative path into a link
    /// (`https://github.com/owner/repo/blob/main/`); paths are plain
    /// text when absent.
    pub source_url: Option<String>,
    /// Where the tree is mounted: a link back to the site that hosts it,
    /// as the first crumb and a header link on every page. Relative
    /// values (`../`) are resolved from each page's own depth, so they
    /// hold under a versioned snapshot; absolute URLs are used verbatim.
    pub home_url: Option<String>,
    /// The text of that link; `home_url` itself when absent.
    pub home_title: Option<String>,
    /// A stylesheet linked last in every page's head, after the tree's
    /// own, so a host site can restate the page tokens in its palette.
    /// Resolved like `home_url`.
    pub stylesheet: Option<String>,
    /// The rendered views under `views/`, by file stem (`index`,
    /// `view_app`, `context`): the SVG and the `.dot` beside it. A view
    /// with a LikeC4 `.dot` is inlined into its page; any other SVG is an
    /// image.
    pub views: BTreeMap<String, ViewSource>,
    /// A color per element kind from `[theme]`, the source of `theme.css`.
    pub theme: BTreeMap<String, ThemeColor>,
    /// The scheme the pages show before a visitor chooses one.
    pub color_scheme: ColorScheme,
    /// Whether every page carries a System / Light / Dark control, and
    /// with it `theme.js`, the tree's one script.
    pub scheme_toggle: bool,
}

/// The `<meta name="generator">` every page carries. `asbuilt docs` looks
/// for it before clearing an output directory: a tree that has it was
/// written by a previous run and is safe to replace.
pub const GENERATOR_META: &str = "<meta name=\"generator\" content=\"asbuilt docs\">";

/// The generated tree.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Site {
    /// Relative `/`-separated path to contents, `style.css` included.
    pub pages: BTreeMap<String, String>,
    /// Views a page wanted and `options.views` lacked, sorted.
    pub missing_views: Vec<String>,
}

/// The stylesheet every page links.
pub const STYLESHEET: &str = include_str!("docs.css");

/// The script behind the scheme control, linked in every page's head.
pub const SCHEME_SCRIPT: &str = include_str!("theme.js");

/// The visitor's scheme control. It ships hidden, and `theme.js` reveals
/// it, so a page without the script shows no dead control.
const SCHEME_CONTROL: &str = "<label class=\"scheme\" hidden>Theme <select id=\"scheme\"><option value=\"system\">System</option><option value=\"light\">Light</option><option value=\"dark\">Dark</option></select></label>";

/// HTML-escape the five characters that matter in text and attributes.
///
/// ```
/// assert_eq!(asbuilt_core::docs::escape("a < b & \"c\""), "a &lt; b &amp; &quot;c&quot;");
/// ```
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

/// A description paragraph as HTML. Raw HTML in the source is rendered
/// as text: the pages are published, and a `<script>` in a doc comment
/// must not become one here.
pub fn markdown(text: &str) -> String {
    let events = Parser::new_ext(text, Options::empty()).map(|event| match event {
        Event::Html(raw) | Event::InlineHtml(raw) => Event::Text(raw),
        other => other,
    });
    let mut out = String::new();
    html::push_html(&mut out, events);
    out
}

/// The page of a top-level element.
pub fn container_page(id: &[String]) -> String {
    format!("containers/{}.html", sanitize_id(id))
}

/// The fragment id of an element within its container's page.
pub fn anchor(id: &[String]) -> String {
    sanitize_id(id)
}

/// Where an element is documented: its page, and the fragment when it
/// is not the page's own subject.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Place {
    page: String,
    fragment: Option<String>,
}

impl Place {
    fn href(&self, depth: usize) -> String {
        let mut out = format!("{}{}", up(depth), self.page);
        if let Some(fragment) = &self.fragment {
            out.push('#');
            out.push_str(fragment);
        }
        out
    }
}

/// The relative prefix from a page at `depth` back to the tree root.
fn up(depth: usize) -> String {
    "../".repeat(depth)
}

/// A configured URL as a page at `depth` should write it: absolute URLs
/// and root-relative paths verbatim, anything else relative to the tree
/// root and so prefixed by the climb.
fn resolve(depth: usize, url: &str) -> String {
    if url.contains("://") || url.starts_with('/') {
        url.to_string()
    } else {
        format!("{}{url}", up(depth))
    }
}

fn is_external(element: &Element) -> bool {
    matches!(element.kind, ElementKind::External(_))
}

struct Ctx<'a> {
    model: &'a Model,
    options: &'a DocsOptions,
    /// Generated view name per element with children.
    views: BTreeMap<Id, String>,
    /// Where every element is documented.
    places: BTreeMap<Id, Place>,
    /// The element kind of each LikeC4 id, for the classes an inlined
    /// view's nodes carry.
    kinds: BTreeMap<String, String>,
    missing: BTreeSet<String>,
}

impl Ctx<'_> {
    /// The home link as `<a class="home">`, when there is a home.
    fn home_link(&self, depth: usize) -> Option<String> {
        let url = self.options.home_url.as_deref()?;
        let title = self.options.home_title.as_deref().unwrap_or(url);
        Some(format!(
            "<a class=\"home\" href=\"{}\">{}</a>",
            escape(&resolve(depth, url)),
            escape(title)
        ))
    }

    /// The crumbs of a page: home (when there is one), the tree's index
    /// unless this is the index, then `trail` as plain text. Empty when
    /// there is nothing to climb to.
    fn crumbs(&self, depth: usize, trail: &[&str]) -> String {
        let mut items: Vec<String> = self.home_link(depth).into_iter().collect();
        if trail.is_empty() {
            items.push(escape(&self.options.title));
        } else {
            items.push(format!(
                "<a href=\"{}index.html\">{}</a>",
                up(depth),
                escape(&self.options.title)
            ));
            items.extend(trail.iter().map(|t| escape(t)));
        }
        if items.len() < 2 {
            return String::new();
        }
        format!("<nav class=\"crumbs\">{}</nav>\n", items.join(" / "))
    }

    fn layout(&self, depth: usize, page_title: &str, body: &str) -> String {
        let prefix = up(depth);
        let mut nav = String::new();
        for element in children_of(self.model, &[]) {
            if is_external(element) {
                continue;
            }
            let _ = write!(
                nav,
                r#"<a href="{prefix}{}">{}</a> "#,
                container_page(&element.id),
                escape(&element.title)
            );
        }
        let home = self.home_link(depth).unwrap_or_default();
        let host_css = self
            .options
            .stylesheet
            .as_deref()
            .map(|css| {
                format!(
                    "<link rel=\"stylesheet\" href=\"{}\">\n",
                    escape(&resolve(depth, css))
                )
            })
            .unwrap_or_default();
        let scheme = self.options.color_scheme;
        let toggle = self.options.scheme_toggle;
        let mut root_attrs = String::new();
        if scheme != ColorScheme::System {
            let _ = write!(root_attrs, " data-theme=\"{}\"", scheme.as_str());
        }
        if toggle {
            let _ = write!(root_attrs, " data-theme-default=\"{}\"", scheme.as_str());
        }
        let script = if toggle {
            format!("<script src=\"{prefix}theme.js\"></script>\n")
        } else {
            String::new()
        };
        let control = if toggle { SCHEME_CONTROL } else { "" };
        format!(
            "<!doctype html>\n<html lang=\"en\"{root_attrs}>\n<head>\n<meta charset=\"utf-8\">\n<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n{GENERATOR_META}\n<title>{page} · {site}</title>\n<link rel=\"stylesheet\" href=\"{prefix}style.css\">\n<link rel=\"stylesheet\" href=\"{prefix}theme.css\">\n{script}{host_css}</head>\n<body>\n<header>{home}<a class=\"site\" href=\"{prefix}index.html\">{site}</a><nav>{nav}</nav>{control}</header>\n<main>\n{body}</main>\n</body>\n</html>\n",
            page = escape(page_title),
            site = escape(&self.options.title),
        )
    }

    /// A view as the page shows it: inlined when LikeC4 drew it, so the
    /// page's colors reach it, else an image; `None` when there is no SVG
    /// for it.
    fn view_markup(&self, depth: usize, view: &str, alt: &str) -> Option<String> {
        let source = self.options.views.get(view)?;
        Some(match svg::inline(source, view, alt, &self.kinds) {
            Some(inline) => inline.trim_end().to_string(),
            None => format!(
                "<img src=\"{}views/{}.svg\" alt=\"{}\">",
                up(depth),
                escape(view),
                escape(alt)
            ),
        })
    }

    /// The `<figure>` for a view, or the placeholder when its SVG is
    /// missing (recorded for the report).
    fn view_figure(&mut self, depth: usize, view: &str, alt: &str) -> String {
        match self.view_markup(depth, view, alt) {
            Some(markup) => format!("<figure class=\"view\">{markup}</figure>\n"),
            None => {
                self.missing.insert(view.to_string());
                format!(
                    "<p class=\"missing\">No diagram for <code>{}</code>: run <code>asbuilt render</code>.</p>\n",
                    escape(view)
                )
            }
        }
    }

    fn path_html(&self, path: &str) -> String {
        match &self.options.source_url {
            Some(base) => {
                let target = if path == "." {
                    String::new()
                } else {
                    link_encode(path)
                };
                format!(
                    "<a href=\"{}{}\"><code>{}</code></a>",
                    escape(base),
                    target,
                    escape(path)
                )
            }
            None => format!("<code>{}</code>", escape(path)),
        }
    }

    /// Technology, tags and path on one muted line, then the description.
    fn meta_html(&self, element: &Element) -> String {
        let mut out = String::new();
        let mut parts: Vec<String> = Vec::new();
        if let Some(technology) = &element.technology {
            parts.push(escape(technology));
        }
        for tag in &element.tags {
            parts.push(format!("<span class=\"tag\">{}</span>", escape(tag)));
        }
        if let Some(path) = &element.path {
            parts.push(self.path_html(path));
        }
        if !parts.is_empty() {
            let _ = writeln!(out, "<p class=\"meta\">{}</p>", parts.join(" · "));
        }
        if let Some(description) = &element.description {
            out.push_str(&markdown(description));
        }
        out
    }

    fn link_to(&self, id: &Id, depth: usize) -> String {
        match self.places.get(id) {
            Some(place) => format!(
                "<a href=\"{}\">{}</a>",
                place.href(depth),
                escape(&dotted(id))
            ),
            None => escape(&dotted(id)),
        }
    }

    fn relation_row(&self, relation: &Relation, other: &Id, depth: usize) -> String {
        format!(
            "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>\n",
            relation.kind.keyword(),
            self.link_to(other, depth),
            escape(&relation.items.join(", ")),
            relation
                .technology
                .as_deref()
                .map(escape)
                .unwrap_or_default()
        )
    }

    /// "Uses" (this element as source) and "Used by" (as target).
    fn relation_tables(&self, id: &Id, depth: usize) -> String {
        let mut out = String::new();
        let uses: Vec<&Relation> = self
            .model
            .relations
            .iter()
            .filter(|r| &r.source == id)
            .collect();
        let used_by: Vec<&Relation> = self
            .model
            .relations
            .iter()
            .filter(|r| &r.target == id)
            .collect();
        for (heading, rows, column) in [("Uses", uses, "Target"), ("Used by", used_by, "Source")] {
            if rows.is_empty() {
                continue;
            }
            let _ = writeln!(
                out,
                "<h3>{heading}</h3>\n<table><thead><tr><th>Kind</th><th>{column}</th><th>Items</th><th>Technology</th></tr></thead><tbody>"
            );
            for relation in rows {
                let other = if heading == "Uses" {
                    &relation.target
                } else {
                    &relation.source
                };
                out.push_str(&self.relation_row(relation, other, depth));
            }
            out.push_str("</tbody></table>\n");
        }
        out
    }

    /// The nested list of an element's descendants, as anchors.
    fn module_list(&self, parent: &Id) -> String {
        let children = children_of(self.model, parent);
        if children.is_empty() {
            return String::new();
        }
        let mut out = String::from("<ul>\n");
        for child in children {
            let _ = writeln!(
                out,
                "<li><a href=\"#{}\">{}</a>{}</li>",
                anchor(&child.id),
                escape(&child.title),
                self.module_list(&child.id)
            );
        }
        out.push_str("</ul>\n");
        out
    }

    fn index_page(&mut self) -> String {
        let mut body = self.crumbs(0, &[]);
        let _ = writeln!(body, "<h1>{}</h1>", escape(&self.options.title));
        body.push_str(&self.view_figure(0, "index", "Overview"));
        let top = children_of(self.model, &[]);
        let containers: Vec<&Element> = top.iter().copied().filter(|e| !is_external(e)).collect();
        let externals: Vec<&Element> = top.iter().copied().filter(|e| is_external(e)).collect();
        if !containers.is_empty() {
            body.push_str("<h2>Containers</h2>\n<table><thead><tr><th>Name</th><th>Technology</th><th>Description</th></tr></thead><tbody>\n");
            for element in &containers {
                let _ = writeln!(
                    body,
                    "<tr><td><a href=\"{}\">{}</a></td><td>{}</td><td>{}</td></tr>",
                    container_page(&element.id),
                    escape(&element.title),
                    element
                        .technology
                        .as_deref()
                        .map(escape)
                        .unwrap_or_default(),
                    element
                        .description
                        .as_deref()
                        .map(markdown)
                        .unwrap_or_default()
                );
            }
            body.push_str("</tbody></table>\n");
        }
        if !externals.is_empty() {
            body.push_str("<h2>Externals</h2>\n<table><thead><tr><th>Name</th><th>Kind</th><th>Technology</th><th>Description</th><th>Used by</th></tr></thead><tbody>\n");
            for element in &externals {
                let kind = match &element.kind {
                    ElementKind::External(kind) => kind.as_str(),
                    _ => "",
                };
                let users: Vec<String> = self
                    .model
                    .relations
                    .iter()
                    .filter(|r| r.target == element.id)
                    .map(|r| {
                        format!(
                            "{} ({}: {})",
                            self.link_to(&r.source, 0),
                            r.kind.keyword(),
                            escape(&r.items.join(", "))
                        )
                    })
                    .collect();
                let _ = writeln!(
                    body,
                    "<tr id=\"{}\"><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                    anchor(&element.id),
                    escape(&element.title),
                    escape(kind),
                    element
                        .technology
                        .as_deref()
                        .map(escape)
                        .unwrap_or_default(),
                    element
                        .description
                        .as_deref()
                        .map(markdown)
                        .unwrap_or_default(),
                    users.join("<br>")
                );
            }
            body.push_str("</tbody></table>\n");
        }
        let title = self.options.title.clone();
        self.layout(0, &title, &body)
    }

    fn container_page_html(&mut self, element: &Element) -> String {
        let mut body = self.crumbs(1, &[&element.title]);
        let _ = writeln!(body, "<h1>{}</h1>", escape(&element.title));
        body.push_str(&self.meta_html(element));
        if let Some(view) = self.views.get(&element.id).cloned() {
            body.push_str(&self.view_figure(1, &view, &element.title));
        }
        let modules = self.module_list(&element.id);
        if !modules.is_empty() {
            let _ = write!(
                body,
                "<h2>Modules</h2>\n<div class=\"modules\">{modules}</div>\n"
            );
        }
        body.push_str(&self.relation_tables(&element.id, 1));
        let descendants: Vec<&Element> = self
            .model
            .elements
            .iter()
            .filter(|e| e.id.len() > element.id.len() && e.id.starts_with(&element.id))
            .collect();
        for descendant in descendants {
            let _ = writeln!(
                body,
                "<section id=\"{}\">\n<h2>{} <small>{}</small></h2>",
                anchor(&descendant.id),
                escape(descendant.id.last().map(String::as_str).unwrap_or_default()),
                escape(&dotted(&descendant.id))
            );
            body.push_str(&self.meta_html(descendant));
            if let Some(view) = self.views.get(&descendant.id).cloned() {
                body.push_str(&self.view_figure(1, &view, &dotted(&descendant.id)));
            }
            body.push_str(&self.relation_tables(&descendant.id, 1));
            body.push_str("</section>\n");
        }
        self.layout(1, &element.title, &body)
    }

    /// Curated views: stems that are neither `index` nor generated.
    fn views_page(&mut self) -> Option<String> {
        let generated: BTreeSet<&String> = self.views.values().collect();
        let options = self.options;
        let curated: Vec<&String> = options
            .views
            .keys()
            .filter(|v| v.as_str() != "index" && !generated.contains(v))
            .collect();
        if curated.is_empty() {
            return None;
        }
        let mut body = self.crumbs(0, &["Curated views"]);
        body.push_str("<h1>Curated views</h1>\n");
        for view in curated {
            let markup = self.view_markup(0, view, view).unwrap_or_default();
            let _ = writeln!(
                body,
                "<figure class=\"view\"><figcaption>{}</figcaption>{markup}</figure>",
                escape(view)
            );
        }
        Some(self.layout(0, "Curated views", &body))
    }
}

/// The whole tree for a model. The model is normalized on a copy first,
/// so the caller's element order does not matter.
pub fn generate(model: &Model, options: &DocsOptions) -> Site {
    let mut model = model.clone();
    model.normalize();
    let views = view_ids(&model);

    let mut places: BTreeMap<Id, Place> = BTreeMap::new();
    for element in children_of(&model, &[]) {
        if is_external(element) {
            places.insert(
                element.id.clone(),
                Place {
                    page: "index.html".into(),
                    fragment: Some(anchor(&element.id)),
                },
            );
            continue;
        }
        let page = container_page(&element.id);
        places.insert(
            element.id.clone(),
            Place {
                page: page.clone(),
                fragment: None,
            },
        );
        for descendant in model
            .elements
            .iter()
            .filter(|e| e.id.len() > element.id.len() && e.id.starts_with(&element.id))
        {
            places.insert(
                descendant.id.clone(),
                Place {
                    page: page.clone(),
                    fragment: Some(anchor(&descendant.id)),
                },
            );
        }
    }

    let kinds: BTreeMap<String, String> = model
        .elements
        .iter()
        .map(|e| (sanitize_id(&e.id), e.kind.keyword().to_string()))
        .collect();
    let kind_names: BTreeSet<String> = kinds.values().cloned().collect();
    let mut ctx = Ctx {
        model: &model,
        options,
        views,
        places,
        kinds,
        missing: BTreeSet::new(),
    };
    let mut pages = BTreeMap::new();
    pages.insert("index.html".to_string(), ctx.index_page());
    let containers: Vec<Element> = children_of(&model, &[])
        .into_iter()
        .filter(|e| !is_external(e))
        .cloned()
        .collect();
    for element in &containers {
        pages.insert(
            container_page(&element.id),
            ctx.container_page_html(element),
        );
    }
    if let Some(views_page) = ctx.views_page() {
        pages.insert("views.html".to_string(), views_page);
    }
    pages.insert("style.css".to_string(), STYLESHEET.to_string());
    pages.insert(
        "theme.css".to_string(),
        theme::stylesheet(&kind_names, &options.theme),
    );
    if options.scheme_toggle {
        pages.insert("theme.js".to_string(), SCHEME_SCRIPT.to_string());
    }
    Site {
        pages,
        missing_views: ctx.missing.into_iter().collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::emit::{EmitOptions, emit};
    use crate::model::RelationKind;

    fn id(s: &str) -> Id {
        s.split('.').map(str::to_string).collect()
    }

    fn element(s: &str, kind: ElementKind) -> Element {
        Element {
            id: id(s),
            kind,
            title: s.rsplit('.').next().unwrap().to_string(),
            description: None,
            technology: None,
            path: None,
            tags: vec![],
        }
    }

    fn relation(source: &str, target: &str, kind: RelationKind, items: &[&str]) -> Relation {
        Relation {
            source: id(source),
            target: id(target),
            kind,
            items: items.iter().map(|s| s.to_string()).collect(),
            technology: None,
        }
    }

    /// `app` (with `server`, `view` → `view.element`, `tests`), `lib`
    /// (with `util`), and an external `node_driver`.
    fn sample() -> Model {
        let mut app = element("app", ElementKind::Container);
        app.technology = Some("library crate".into());
        app.path = Some("crates/app".into());
        app.description = Some("The app. It's `here`.".into());
        let mut server = element("app.server", ElementKind::Component);
        server.description = Some("Where `Page` is *sent*.".into());
        server.path = Some("crates/app/src/server.rs".into());
        let view = element("app.view", ElementKind::Component);
        let view_element = element("app.view.element", ElementKind::Component);
        let mut tests = element("app.tests", ElementKind::Component);
        tests.tags = vec!["tests".into()];
        let mut lib = element("lib", ElementKind::Container);
        lib.path = Some(".".into());
        let util = element("lib.util", ElementKind::Component);
        let mut driver = element("node_driver", ElementKind::External("process".into()));
        driver.tags = vec!["external".into()];
        driver.technology = Some("Node.js".into());
        driver.description = Some("The driver.".into());
        let mut spawns = relation("app.server", "node_driver", RelationKind::Uses, &["spawns"]);
        spawns.technology = Some("stdio".into());
        Model {
            elements: vec![driver, util, lib, tests, view_element, view, server, app],
            relations: vec![
                relation(
                    "app.tests",
                    "app.server",
                    RelationKind::Calls,
                    &["Server", "connect"],
                ),
                spawns,
                relation("app.view", "app.server", RelationKind::NamesType, &[]),
                relation(
                    "app.view.element",
                    "app.server",
                    RelationKind::Constructs,
                    &["Page"],
                ),
                relation(
                    "app.server",
                    "lib.util",
                    RelationKind::NamesType,
                    &["Helper"],
                ),
                relation("lib.util", "app", RelationKind::Uses, &["App"]),
            ],
            ..Default::default()
        }
    }

    /// Views drawn by something other than LikeC4 (no `.dot`), so each
    /// is an image; `likec4_view` makes one LikeC4 drew.
    fn all_views() -> BTreeMap<String, ViewSource> {
        ["index", "view_app", "view_app_view_", "view_lib"]
            .iter()
            .map(|s| (s.to_string(), plain_view()))
            .collect()
    }

    fn plain_view() -> ViewSource {
        ViewSource {
            svg: "<svg xmlns=\"http://www.w3.org/2000/svg\"/>".into(),
            dot: None,
        }
    }

    /// The `index` view as LikeC4 and Graphviz write it: one node, `app`.
    fn likec4_view() -> ViewSource {
        ViewSource {
            svg: "<svg width=\"10pt\" height=\"10pt\" viewBox=\"0 0 10 10\" xmlns=\"http://www.w3.org/2000/svg\">\n<g id=\"node1\" class=\"node\">\n<title>app</title>\n<polygon fill=\"#3b82f6\" points=\"0,0 1,1\"/>\n</g>\n</svg>\n".into(),
            dot: Some("digraph {\n    graph [likec4_viewId=index];\n    app [likec4_id=app];\n}\n".into()),
        }
    }

    fn options() -> DocsOptions {
        DocsOptions {
            title: "Sample".into(),
            views: all_views(),
            ..Default::default()
        }
    }

    fn page<'a>(site: &'a Site, path: &str) -> &'a str {
        site.pages
            .get(path)
            .unwrap_or_else(|| panic!("no page {path}; have {:?}", site.pages.keys()))
    }

    #[test]
    fn the_page_set_is_index_the_stylesheet_and_one_page_per_container() {
        let site = generate(&sample(), &options());
        let keys: Vec<&String> = site.pages.keys().collect();
        assert_eq!(
            keys,
            [
                "containers/app.html",
                "containers/lib.html",
                "index.html",
                "style.css",
                "theme.css"
            ]
        );
        assert_eq!(page(&site, "style.css"), STYLESHEET);
    }

    #[test]
    fn the_scheme_toggle_adds_the_script_to_the_page_set() {
        let mut opts = options();
        opts.scheme_toggle = true;
        let site = generate(&sample(), &opts);
        assert_eq!(page(&site, "theme.js"), SCHEME_SCRIPT);
    }

    #[test]
    fn theme_css_colors_every_kind_the_model_has() {
        let mut opts = options();
        opts.theme = BTreeMap::from([(
            "container".to_string(),
            ThemeColor {
                light: "#ce422b".into(),
                dark: None,
            },
        )]);
        let site = generate(&sample(), &opts);
        let kinds: BTreeSet<String> = ["component", "container", "process"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(
            page(&site, "theme.css"),
            theme::stylesheet(&kinds, &opts.theme)
        );
    }

    #[test]
    fn a_view_likec4_drew_is_inlined_with_its_nodes_classed_by_kind() {
        let mut opts = options();
        opts.views.insert("index".into(), likec4_view());
        let site = generate(&sample(), &opts);
        let index = page(&site, "index.html");
        assert!(
            index.contains("<figure class=\"view\"><svg viewBox=\"0 0 10 10\" xmlns=\"http://www.w3.org/2000/svg\" class=\"c4\" data-view=\"index\" role=\"img\" aria-label=\"Overview\">"),
            "{index}"
        );
        assert!(
            index.contains("<g id=\"index-node1\" class=\"node c4-k-container\">"),
            "{index}"
        );
        assert!(!index.contains("<img src=\"views/index.svg\""), "{index}");
    }

    #[test]
    fn a_curated_view_likec4_drew_is_inlined_under_its_caption() {
        let mut opts = options();
        opts.views.insert("context".into(), likec4_view());
        let site = generate(&sample(), &opts);
        assert!(
            page(&site, "views.html")
                .contains("<figure class=\"view\"><figcaption>context</figcaption><svg "),
            "{}",
            page(&site, "views.html")
        );
    }

    #[test]
    fn every_page_links_the_theme_stylesheet_between_its_own_and_the_host_s() {
        let mut opts = options();
        opts.stylesheet = Some("site.css".into());
        let site = generate(&sample(), &opts);
        assert!(
            page(&site, "containers/app.html").contains("<link rel=\"stylesheet\" href=\"../style.css\">\n<link rel=\"stylesheet\" href=\"../theme.css\">\n<link rel=\"stylesheet\" href=\"../site.css\">\n</head>"),
            "{}",
            page(&site, "containers/app.html")
        );
    }

    #[test]
    fn the_scheme_toggle_links_the_script_and_puts_a_hidden_control_in_the_header() {
        let mut opts = options();
        opts.scheme_toggle = true;
        let site = generate(&sample(), &opts);
        let app = page(&site, "containers/app.html");
        assert!(
            app.contains("<link rel=\"stylesheet\" href=\"../theme.css\">\n<script src=\"../theme.js\"></script>\n</head>"),
            "{app}"
        );
        assert!(
            app.contains(&format!("</nav>{SCHEME_CONTROL}</header>")),
            "{app}"
        );
        assert!(
            app.contains("<html lang=\"en\" data-theme-default=\"system\">"),
            "{app}"
        );
    }

    #[test]
    fn without_the_scheme_toggle_there_is_no_script_and_no_control() {
        let site = generate(&sample(), &options());
        for (path, contents) in &site.pages {
            assert!(!contents.contains("theme.js"), "{path}: {contents}");
            assert!(!contents.contains("class=\"scheme\""), "{path}: {contents}");
        }
    }

    #[test]
    fn a_fixed_color_scheme_is_written_on_the_root() {
        let mut opts = options();
        opts.color_scheme = ColorScheme::Dark;
        let site = generate(&sample(), &opts);
        assert!(
            page(&site, "index.html").contains("<html lang=\"en\" data-theme=\"dark\">"),
            "{}",
            page(&site, "index.html")
        );
        opts.scheme_toggle = true;
        let site = generate(&sample(), &opts);
        assert!(
            page(&site, "index.html")
                .contains("<html lang=\"en\" data-theme=\"dark\" data-theme-default=\"dark\">"),
            "{}",
            page(&site, "index.html")
        );
    }

    #[test]
    fn the_system_scheme_writes_no_scheme_on_the_root() {
        let site = generate(&sample(), &options());
        assert!(
            page(&site, "index.html").contains("<html lang=\"en\">"),
            "{}",
            page(&site, "index.html")
        );
    }

    #[test]
    fn the_index_links_every_container_and_lists_the_externals_with_who_uses_them() {
        let site = generate(&sample(), &options());
        let index = page(&site, "index.html");
        // Table rows, not the header nav, which links the same pages.
        assert!(index.contains("<h2>Containers</h2>"), "{index}");
        assert!(
            index.contains(
                "<tr><td><a href=\"containers/app.html\">app</a></td><td>library crate</td>"
            ),
            "{index}"
        );
        assert!(
            index.contains("<tr><td><a href=\"containers/lib.html\">lib</a></td>"),
            "{index}"
        );
        assert!(!index.contains("containers/node_driver.html"), "{index}");
        assert!(
            index.contains(
                "<tr id=\"node_driver\"><td>node_driver</td><td>process</td><td>Node.js</td>"
            ),
            "{index}"
        );
        assert!(
            index.contains(
                "<a href=\"containers/app.html#app.server\">app.server</a> (uses: spawns)"
            ),
            "{index}"
        );
        assert!(index.contains("<h1>Sample</h1>"), "{index}");
        assert!(index.contains("<img src=\"views/index.svg\""), "{index}");
    }

    #[test]
    fn a_container_page_lists_its_modules_as_a_nested_list_of_anchors() {
        let site = generate(&sample(), &options());
        let app = page(&site, "containers/app.html");
        let expected = "<ul>\n<li><a href=\"#app.server\">server</a></li>\n<li><a href=\"#app.tests\">tests</a></li>\n<li><a href=\"#app.view_\">view</a><ul>\n<li><a href=\"#app.view_.element\">element</a></li>\n</ul>\n</li>\n</ul>\n";
        assert!(app.contains(expected), "{app}");
        assert!(app.contains("<section id=\"app.view_.element\">\n<h2>element <small>app.view.element</small></h2>"), "{app}");
    }

    #[test]
    fn an_element_section_lists_outgoing_and_incoming_relations_with_kind_items_and_technology() {
        let site = generate(&sample(), &options());
        let app = page(&site, "containers/app.html");
        let server = app
            .split("<section id=\"app.server\">")
            .nth(1)
            .unwrap()
            .split("</section>")
            .next()
            .unwrap();
        assert!(server.contains("<h3>Uses</h3>"), "{server}");
        assert!(server.contains("<tr><td>uses</td><td><a href=\"../index.html#node_driver\">node_driver</a></td><td>spawns</td><td>stdio</td></tr>"), "{server}");
        assert!(server.contains("<h3>Used by</h3>"), "{server}");
        assert!(server.contains("<tr><td>calls</td><td><a href=\"../containers/app.html#app.tests\">app.tests</a></td><td>Server, connect</td><td></td></tr>"), "{server}");
        // Empty items and no technology render as empty cells.
        assert!(server.contains("<tr><td>names</td><td><a href=\"../containers/app.html#app.view_\">app.view</a></td><td></td><td></td></tr>"), "{server}");
    }

    #[test]
    fn a_relation_to_an_element_on_another_page_links_across_with_its_anchor() {
        let site = generate(&sample(), &options());
        let app = page(&site, "containers/app.html");
        assert!(
            app.contains(
                "<a href=\"../containers/lib.html#lib.util\">lib.util</a></td><td>Helper</td>"
            ),
            "{app}"
        );
        let lib = page(&site, "containers/lib.html");
        assert!(lib.contains("<h3>Used by</h3>"), "{lib}");
        assert!(
            lib.contains("<a href=\"../containers/app.html#app.server\">app.server</a>"),
            "{lib}"
        );
    }

    #[test]
    fn a_model_with_only_externals_has_no_containers_table() {
        let mut model = sample();
        model.elements.retain(is_external);
        model.relations.clear();
        let mut opts = options();
        opts.views = BTreeMap::from([("index".to_string(), plain_view())]);
        let site = generate(&model, &opts);
        let index = page(&site, "index.html");
        assert!(!index.contains("<h2>Containers</h2>"), "{index}");
        assert!(index.contains("<h2>Externals</h2>"), "{index}");
        assert_eq!(
            site.pages.keys().collect::<Vec<_>>(),
            ["index.html", "style.css", "theme.css"]
        );
    }

    #[test]
    fn a_model_with_only_containers_has_no_externals_table() {
        let mut model = sample();
        model.elements.retain(|e| !is_external(e));
        model.relations.retain(|r| r.target != id("node_driver"));
        let site = generate(&model, &options());
        assert!(!page(&site, "index.html").contains("<h2>Externals</h2>"));
    }

    #[test]
    fn a_container_page_documents_its_own_descendants_only() {
        let site = generate(&sample(), &options());
        let app = page(&site, "containers/app.html");
        assert!(!app.contains("<section id=\"app\">"), "{app}");
        assert!(!app.contains("<section id=\"lib.util\">"), "{app}");
        assert!(!app.contains("<section id=\"lib\">"), "{app}");
        assert_eq!(app.matches("<section id=").count(), 4, "{app}");
    }

    #[test]
    fn a_relation_to_a_container_links_its_page_without_a_fragment() {
        let site = generate(&sample(), &options());
        let lib = page(&site, "containers/lib.html");
        assert!(
            lib.contains("<tr><td>uses</td><td><a href=\"../containers/app.html\">app</a></td><td>App</td><td></td></tr>"),
            "{lib}"
        );
        let app = page(&site, "containers/app.html");
        let top = app.split("<section").next().unwrap();
        assert!(top.contains("<h3>Used by</h3>"), "{top}");
        assert!(
            top.contains(
                "<a href=\"../containers/lib.html#lib.util\">lib.util</a></td><td>App</td>"
            ),
            "{top}"
        );
    }

    #[test]
    fn an_element_without_relations_has_no_tables() {
        let site = generate(&sample(), &options());
        let lib = page(&site, "containers/lib.html");
        let top = lib.split("<section").next().unwrap();
        assert!(
            !top.contains("<h3>Uses</h3>") && !top.contains("<h3>Used by</h3>"),
            "{top}"
        );
    }

    #[test]
    fn an_element_with_children_embeds_its_scoped_view_and_a_leaf_does_not() {
        let site = generate(&sample(), &options());
        let app = page(&site, "containers/app.html");
        assert!(
            app.contains("<img src=\"../views/view_app.svg\" alt=\"app\""),
            "{app}"
        );
        assert!(
            app.contains("<img src=\"../views/view_app_view_.svg\" alt=\"app.view\""),
            "{app}"
        );
        let server = app
            .split("<section id=\"app.server\">")
            .nth(1)
            .unwrap()
            .split("</section>")
            .next()
            .unwrap();
        assert!(!server.contains("<img"), "{server}");
    }

    #[test]
    fn a_reserved_word_id_uses_the_same_view_file_name_the_emitter_declares() {
        let model = sample();
        let text = emit(&model, &EmitOptions::for_output_path("m.c4"));
        assert!(
            text.contains("view view_app_view_ of app.view_ {"),
            "{text}"
        );
        let site = generate(&model, &options());
        assert!(page(&site, "containers/app.html").contains("views/view_app_view_.svg"));
    }

    #[test]
    fn a_missing_view_is_a_placeholder_and_is_reported() {
        let mut opts = options();
        opts.views.remove("view_app");
        opts.views.remove("index");
        let site = generate(&sample(), &opts);
        assert_eq!(site.missing_views, ["index", "view_app"]);
        let app = page(&site, "containers/app.html");
        assert!(app.contains("<p class=\"missing\">No diagram for <code>view_app</code>: run <code>asbuilt render</code>.</p>"), "{app}");
        assert!(!app.contains("views/view_app.svg"), "{app}");
        assert!(page(&site, "index.html").contains("No diagram for <code>index</code>"));
    }

    #[test]
    fn a_curated_svg_gets_a_section_on_the_views_page_and_none_means_no_page() {
        let mut opts = options();
        opts.views.insert("context".into(), plain_view());
        let site = generate(&sample(), &opts);
        let views = page(&site, "views.html");
        assert!(
            views.contains("<figcaption>context</figcaption><img src=\"views/context.svg\""),
            "{views}"
        );
        assert!(!views.contains("views/view_app.svg"), "{views}");
        assert!(
            !generate(&sample(), &options())
                .pages
                .contains_key("views.html")
        );
    }

    #[test]
    fn escape_handles_the_five_characters() {
        assert_eq!(
            escape("<a href=\"x\">&'</a>"),
            "&lt;a href=&quot;x&quot;&gt;&amp;&#39;&lt;/a&gt;"
        );
        assert_eq!(escape("plain"), "plain");
    }

    #[test]
    fn a_lt_in_a_title_is_escaped() {
        let mut model = sample();
        model
            .elements
            .iter_mut()
            .find(|e| e.id == ["lib"])
            .unwrap()
            .title = "a<b".into();
        let site = generate(&model, &options());
        assert!(page(&site, "index.html").contains(">a&lt;b</a>"));
        assert!(page(&site, "containers/lib.html").contains("<h1>a&lt;b</h1>"));
    }

    #[test]
    fn raw_html_in_a_description_is_text_inline_and_as_a_block() {
        assert_eq!(
            markdown("say <b>hi</b>"),
            "<p>say &lt;b&gt;hi&lt;/b&gt;</p>\n"
        );
        assert_eq!(
            markdown("<div>\nblock\n</div>"),
            "&lt;div&gt;\nblock\n&lt;/div&gt;"
        );
    }

    #[test]
    fn backticks_and_emphasis_in_a_description_become_markup() {
        let site = generate(&sample(), &options());
        assert!(
            page(&site, "containers/app.html")
                .contains("<p>Where <code>Page</code> is <em>sent</em>.</p>")
        );
    }

    #[test]
    fn a_source_url_makes_paths_links_and_its_absence_leaves_them_as_text() {
        let mut opts = options();
        opts.source_url = Some("https://github.com/o/r/blob/main/".into());
        let site = generate(&sample(), &opts);
        let app = page(&site, "containers/app.html");
        assert!(app.contains("<a href=\"https://github.com/o/r/blob/main/crates/app/src/server.rs\"><code>crates/app/src/server.rs</code></a>"), "{app}");
        // The root path links to the base itself.
        assert!(
            page(&site, "containers/lib.html")
                .contains("<a href=\"https://github.com/o/r/blob/main/\"><code>.</code></a>")
        );
        let plain = generate(&sample(), &options());
        assert!(
            page(&plain, "containers/app.html").contains("<code>crates/app/src/server.rs</code>")
        );
        assert!(!page(&plain, "containers/app.html").contains("blob/main"));
    }

    #[test]
    fn a_space_in_a_path_is_percent_encoded_in_the_link_only() {
        let mut model = sample();
        model
            .elements
            .iter_mut()
            .find(|e| e.id == ["lib"])
            .unwrap()
            .path = Some("my lib".into());
        let mut opts = options();
        opts.source_url = Some("https://x/".into());
        let site = generate(&model, &opts);
        assert!(
            page(&site, "containers/lib.html")
                .contains("<a href=\"https://x/my%20lib\"><code>my lib</code></a>")
        );
    }

    #[test]
    fn pages_below_the_root_link_the_stylesheet_and_the_index_through_the_parent() {
        let site = generate(&sample(), &options());
        let app = page(&site, "containers/app.html");
        assert!(
            app.contains("<link rel=\"stylesheet\" href=\"../style.css\">"),
            "{app}"
        );
        assert!(
            app.contains("<a class=\"site\" href=\"../index.html\">Sample</a>"),
            "{app}"
        );
        assert!(
            app.contains("<nav class=\"crumbs\"><a href=\"../index.html\">Sample</a> / app</nav>"),
            "{app}"
        );
        assert!(app.contains("<title>app · Sample</title>"), "{app}");
        let index = page(&site, "index.html");
        assert!(
            index.contains("<link rel=\"stylesheet\" href=\"style.css\">"),
            "{index}"
        );
        assert!(
            index.contains("<a class=\"site\" href=\"index.html\">Sample</a>"),
            "{index}"
        );
    }

    #[test]
    fn every_page_carries_the_generator_meta() {
        let site = generate(&sample(), &options());
        for (path, contents) in &site.pages {
            if path.ends_with(".html") {
                assert!(contents.contains(GENERATOR_META), "{path}: {contents}");
            }
        }
    }

    #[test]
    fn no_image_is_lazy() {
        let site = generate(&sample(), &options());
        for (path, contents) in &site.pages {
            assert!(!contents.contains("loading="), "{path}: {contents}");
        }
    }

    #[test]
    fn a_home_url_is_the_first_crumb_and_a_header_link_resolved_from_each_depth() {
        let mut opts = options();
        opts.home_url = Some("../".into());
        opts.home_title = Some("Home".into());
        let site = generate(&sample(), &opts);
        let index = page(&site, "index.html");
        assert!(
            index.contains("<header><a class=\"home\" href=\"../\">Home</a><a class=\"site\""),
            "{index}"
        );
        assert!(
            index.contains(
                "<nav class=\"crumbs\"><a class=\"home\" href=\"../\">Home</a> / Sample</nav>"
            ),
            "{index}"
        );
        let app = page(&site, "containers/app.html");
        assert!(
            app.contains("<header><a class=\"home\" href=\"../../\">Home</a><a class=\"site\""),
            "{app}"
        );
        assert!(
            app.contains("<nav class=\"crumbs\"><a class=\"home\" href=\"../../\">Home</a> / <a href=\"../index.html\">Sample</a> / app</nav>"),
            "{app}"
        );
    }

    #[test]
    fn an_absolute_home_url_is_verbatim_and_names_itself_without_a_title() {
        let mut opts = options();
        opts.home_url = Some("https://example.test/".into());
        let site = generate(&sample(), &opts);
        let app = page(&site, "containers/app.html");
        assert!(
            app.contains(
                "<a class=\"home\" href=\"https://example.test/\">https://example.test/</a>"
            ),
            "{app}"
        );
    }

    #[test]
    fn without_a_home_url_there_is_no_home_link_and_the_index_has_no_crumbs() {
        let site = generate(&sample(), &options());
        let index = page(&site, "index.html");
        assert!(!index.contains("class=\"home\""), "{index}");
        assert!(!index.contains("class=\"crumbs\""), "{index}");
        assert!(
            index.contains("<header><a class=\"site\" href=\"index.html\">Sample</a>"),
            "{index}"
        );
    }

    #[test]
    fn a_stylesheet_is_linked_last_in_the_head_and_resolved_from_each_depth() {
        let mut opts = options();
        opts.stylesheet = Some("site.css".into());
        let site = generate(&sample(), &opts);
        let index = page(&site, "index.html");
        assert!(
            index.contains("<link rel=\"stylesheet\" href=\"theme.css\">\n<link rel=\"stylesheet\" href=\"site.css\">\n</head>"),
            "{index}"
        );
        let app = page(&site, "containers/app.html");
        assert!(
            app.contains("<link rel=\"stylesheet\" href=\"../theme.css\">\n<link rel=\"stylesheet\" href=\"../site.css\">\n</head>"),
            "{app}"
        );
        let mut opts = options();
        opts.stylesheet = Some("/theme.css".into());
        let site = generate(&sample(), &opts);
        assert!(
            page(&site, "containers/app.html").contains("href=\"/theme.css\">\n</head>"),
            "root-relative is verbatim"
        );
        assert!(
            !page(&site, "index.html").contains("site.css"),
            "no host link without a stylesheet"
        );
    }

    #[test]
    fn the_stylesheet_defines_the_page_tokens_for_light_and_dark_and_no_white_slab() {
        for token in ["--bg:", "--fg:", "--muted:", "--accent:", "--figure-bg:"] {
            assert!(STYLESHEET.contains(token), "{token}");
        }
        assert!(STYLESHEET.contains("@media (prefers-color-scheme: dark)"));
        assert!(!STYLESHEET.contains("background: #fff"), "{STYLESHEET}");
        assert!(STYLESHEET.contains(".modules ul"), "{STYLESHEET}");
    }

    #[test]
    fn a_missing_view_placeholder_escapes_the_view_name() {
        // Generated view names are sanitized to identifier characters, so
        // only a direct call can hand the placeholder a name to escape.
        let model = sample();
        let opts = options();
        let mut ctx = Ctx {
            model: &model,
            options: &opts,
            views: BTreeMap::new(),
            places: BTreeMap::new(),
            kinds: BTreeMap::new(),
            missing: BTreeSet::new(),
        };
        assert_eq!(
            ctx.view_figure(0, "a<b", "alt"),
            "<p class=\"missing\">No diagram for <code>a&lt;b</code>: run <code>asbuilt render</code>.</p>\n"
        );
    }

    #[test]
    fn the_header_lists_every_container_but_no_external() {
        let site = generate(&sample(), &options());
        let index = page(&site, "index.html");
        let header = index
            .split("<header>")
            .nth(1)
            .unwrap()
            .split("</header>")
            .next()
            .unwrap();
        assert!(
            header.contains("<a href=\"containers/app.html\">app</a>"),
            "{header}"
        );
        assert!(
            header.contains("<a href=\"containers/lib.html\">lib</a>"),
            "{header}"
        );
        assert!(!header.contains("node_driver"), "{header}");
    }

    #[test]
    fn technology_tags_and_path_share_the_meta_line() {
        let site = generate(&sample(), &options());
        let app = page(&site, "containers/app.html");
        assert!(
            app.contains("<p class=\"meta\">library crate · <code>crates/app</code></p>"),
            "{app}"
        );
        assert!(
            app.contains("<p class=\"meta\"><span class=\"tag\">tests</span></p>"),
            "{app}"
        );
    }

    #[test]
    fn an_element_with_nothing_to_say_has_no_meta_line() {
        let site = generate(&sample(), &options());
        let app = page(&site, "containers/app.html");
        let view = app
            .split("<section id=\"app.view_\">")
            .nth(1)
            .unwrap()
            .split("<section")
            .next()
            .unwrap();
        assert!(!view.contains("class=\"meta\""), "{view}");
    }

    #[test]
    fn output_does_not_depend_on_input_order() {
        let a = generate(&sample(), &options());
        let mut shuffled = sample();
        shuffled.elements.reverse();
        shuffled.relations.reverse();
        let b = generate(&shuffled, &options());
        assert_eq!(a, b);
    }

    #[test]
    fn a_relative_prefix_climbs_one_directory_per_depth() {
        assert_eq!(up(0), "");
        assert_eq!(up(2), "../../");
        let place = Place {
            page: "index.html".into(),
            fragment: Some("x".into()),
        };
        assert_eq!(place.href(1), "../index.html#x");
        assert_eq!(container_page(&id("my-app")), "containers/my_app.html");
    }
}
