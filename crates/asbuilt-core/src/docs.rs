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
    /// first in every page's header trail. Relative
    /// values (`../`) are resolved from each page's own depth, so they
    /// hold under a versioned snapshot; absolute URLs are used verbatim.
    pub home_url: Option<String>,
    /// The text of that link; `home_url` itself when absent. Every
    /// page's `<title>` starts with it, so a tab names the host.
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
    /// with it `theme.js`.
    pub scheme_toggle: bool,
    /// Whether every inlined view gets the viewer (`viewer.js`): a frame
    /// at a readable scale, zoom and pan, and Fit, 1:1, Wide and
    /// Fullscreen controls.
    pub viewer: bool,
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

/// The asbuilt mark as SVG path data for a `0 0 24 24` viewBox, a copy of
/// `brand/mark.path` (kept identical by `scripts/check-brand-copy.sh`).
pub const MARK_PATH: &str = include_str!("mark.path");

/// The mark as every page's header draws it, before the tree's title: the
/// section asbuilt generated, in the page's accent. Decorative, since the
/// title beside it is the link's name.
fn mark_svg() -> String {
    format!(
        "<svg class=\"mark\" viewBox=\"0 0 24 24\" aria-hidden=\"true\" focusable=\"false\"><path d=\"{}\"/></svg>",
        MARK_PATH.trim()
    )
}

/// The script behind the scheme control, linked in every page's head.
pub const SCHEME_SCRIPT: &str = include_str!("theme.js");

/// `viewer.js`: the diagram viewer, when `viewer` is on.
pub const VIEWER_SCRIPT: &str = include_str!("viewer.js");

/// `viewer.css`: the viewer's own styles, beside `style.css` so a host
/// page can embed a figure without the tree's page styles.
pub const VIEWER_STYLESHEET: &str = include_str!("viewer.css");

/// The controls of a viewer figure. Hidden until the script reveals
/// them, so a page without JavaScript shows no dead buttons. The scale
/// readout is visual only: it changes on every resize, and the pressed
/// buttons already say the mode. The hint names the zoom modifier for
/// the platform, which only the script knows.
const VIEWER_BAR: &str = "<div class=\"viewer-bar\" hidden><span class=\"viewer-scale\"></span><button type=\"button\" data-viewer-zoom=\"out\" aria-label=\"Zoom out\" title=\"Zoom out (\u{2212})\">\u{2212}</button><button type=\"button\" data-viewer-zoom=\"in\" aria-label=\"Zoom in\" title=\"Zoom in (+)\">+</button><button type=\"button\" data-viewer-mode=\"fit\" aria-pressed=\"false\">Fit</button><button type=\"button\" data-viewer-mode=\"one\" aria-pressed=\"false\">1:1</button><button type=\"button\" data-viewer-wide aria-pressed=\"false\">Wide</button><button type=\"button\" data-viewer-full>Fullscreen</button><span class=\"viewer-hint\"></span></div>";

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

/// A JSON string literal; `<` escaped too, so the text can sit inside a
/// `<script>` element.
fn json(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '<' => out.push_str("\\u003c"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn json_opt(text: Option<String>) -> String {
    text.map(|t| json(&t)).unwrap_or_else(|| "null".to_string())
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

/// Which page a layout is for, so the header can mark it current.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Here<'a> {
    Index,
    Container(&'a [String]),
    Views,
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
    /// The rendered views that are neither `index` nor generated, which
    /// `views.html` shows; every header links that page when there are any.
    curated: Vec<String>,
    /// Each element by its LikeC4 id: what a view's nodes and edges name.
    by_likec4: BTreeMap<String, &'a Element>,
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

    /// The `<title>`, in the header trail's order: the host when
    /// `home_title` names it, the tree, then the page. The index is the
    /// tree's own page, so it ends at the tree.
    fn document_title(&self, page: Option<&str>) -> String {
        self.options
            .home_title
            .as_deref()
            .into_iter()
            .chain([self.options.title.as_str()])
            .chain(page)
            .map(escape)
            .collect::<Vec<_>>()
            .join(" · ")
    }

    /// A page: the header's trail (the home link, when there is one, and
    /// the tree's index) with the scheme control, the row of containers
    /// with the current one unlinked, then `body`. Every page's own
    /// heading names it, so nothing below the header repeats the trail.
    fn layout(&self, depth: usize, here: Here<'_>, page_title: Option<&str>, body: &str) -> String {
        let prefix = up(depth);
        let mut containers = String::new();
        for element in children_of(self.model, &[]) {
            if is_external(element) {
                continue;
            }
            if here == Here::Container(&element.id) {
                let _ = write!(
                    containers,
                    "<span aria-current=\"page\">{}</span>",
                    escape(&element.title)
                );
            } else {
                let _ = write!(
                    containers,
                    r#"<a href="{prefix}{}">{}</a>"#,
                    container_page(&element.id),
                    escape(&element.title)
                );
            }
        }
        let containers = if containers.is_empty() {
            String::new()
        } else {
            format!("<nav class=\"containers\" aria-label=\"Containers\">{containers}</nav>")
        };
        let home = self
            .home_link(depth)
            .map(|link| format!("{link}<span class=\"sep\" aria-hidden=\"true\">/</span>"))
            .unwrap_or_default();
        let index_current = if here == Here::Index {
            " aria-current=\"page\""
        } else {
            ""
        };
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
        let mut script = String::new();
        if toggle {
            let _ = writeln!(script, "<script src=\"{prefix}theme.js\"></script>");
        }
        let mut viewer_css = String::new();
        if self.options.viewer {
            let _ = writeln!(
                viewer_css,
                "<link rel=\"stylesheet\" href=\"{prefix}viewer.css\">"
            );
            let _ = writeln!(script, "<script src=\"{prefix}viewer.js\" defer></script>");
        }
        let control = if toggle { SCHEME_CONTROL } else { "" };
        let curated = if self.curated.is_empty() {
            String::new()
        } else if here == Here::Views {
            "<nav class=\"views\" aria-label=\"Views\"><span aria-current=\"page\">Curated views</span></nav>".to_string()
        } else {
            format!(
                "<nav class=\"views\" aria-label=\"Views\"><a href=\"{prefix}views.html\">Curated views</a></nav>"
            )
        };
        format!(
            "<!doctype html>\n<html lang=\"en\"{root_attrs}>\n<head>\n<meta charset=\"utf-8\">\n<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n{GENERATOR_META}\n<title>{title}</title>\n<link rel=\"stylesheet\" href=\"{prefix}style.css\">\n<link rel=\"stylesheet\" href=\"{prefix}theme.css\">\n{viewer_css}{script}{host_css}</head>\n<body>\n<header><div class=\"bar\"><nav class=\"trail\" aria-label=\"Breadcrumb\">{home}<a class=\"site\" href=\"{prefix}index.html\"{index_current}>{mark}{site}</a></nav>{curated}{control}</div>{containers}</header>\n<main>\n{body}</main>\n</body>\n</html>\n",
            title = self.document_title(page_title),
            site = escape(&self.options.title),
            mark = mark_svg(),
        )
    }

    /// A view as the page shows it: inlined when LikeC4 drew it, so the
    /// page's colors reach it, else an image; `None` when there is no SVG
    /// for it.
    /// What the inliner needs per element for the page `here` at `depth`:
    /// its kind, its dotted id as the name a node shows on hover, and a
    /// link to where it is documented, except for the page's own element.
    /// Built once per page and shared by its views.
    fn nodes(&self, depth: usize, here: Here<'_>) -> BTreeMap<String, svg::Node> {
        let this_page = match here {
            Here::Container(id) => Some(container_page(id)),
            Here::Index | Here::Views => None,
        };
        self.by_likec4
            .iter()
            .map(|(key, element)| {
                let place = self.places.get(&element.id);
                let here = place.is_some_and(|p| {
                    p.fragment.is_none() && this_page.as_deref() == Some(p.page.as_str())
                });
                let href = if here {
                    None
                } else {
                    place.map(|p| p.href(depth))
                };
                (
                    key.clone(),
                    svg::Node {
                        kind: element.kind.keyword().to_string(),
                        title: dotted(&element.id),
                        href,
                    },
                )
            })
            .collect()
    }

    fn view_markup(
        &self,
        depth: usize,
        nodes: &BTreeMap<String, svg::Node>,
        view: &str,
        alt: &str,
    ) -> Option<svg::Inlined> {
        let source = self.options.views.get(view)?;
        Some(match svg::inline(source, view, alt, nodes) {
            Some(mut inlined) => {
                inlined.html.truncate(inlined.html.trim_end().len());
                inlined
            }
            None => svg::Inlined {
                html: format!(
                    "<img src=\"{}views/{}.svg\" alt=\"{}\">",
                    up(depth),
                    escape(view),
                    escape(alt)
                ),
                edges: Vec::new(),
            },
        })
    }

    /// The relations behind each edge a view draws, as JSON for the
    /// viewer's popover: keyed `from->to` by LikeC4 id, each relation
    /// with its endpoints' ids and links (the page's own, from `nodes`,
    /// so its own element is named but not linked here either), its
    /// kind, items and technology. Empty when no edge has any. The
    /// relations are sorted by source, so one source prefix is one range.
    fn edge_data(&self, edges: &[(String, String)], nodes: &BTreeMap<String, svg::Node>) -> String {
        let href = |id: &Id| json_opt(nodes.get(&sanitize_id(id)).and_then(|n| n.href.clone()));
        let relations = &self.model.relations;
        let mut entries = Vec::new();
        for (from, to) in edges {
            let (Some(from_el), Some(to_el)) = (self.by_likec4.get(from), self.by_likec4.get(to))
            else {
                continue;
            };
            let (from_id, to_id) = (&from_el.id, &to_el.id);
            let start = relations.partition_point(|r| r.source.as_slice() < from_id.as_slice());
            let behind: Vec<String> = relations[start..]
                .iter()
                .take_while(|r| r.source.starts_with(from_id))
                .filter(|r| r.target.starts_with(to_id))
                .map(|r| {
                    format!(
                        "{{\"source\":{},\"source_href\":{},\"target\":{},\"target_href\":{},\"kind\":{},\"items\":[{}],\"technology\":{}}}",
                        json(&dotted(&r.source)),
                        href(&r.source),
                        json(&dotted(&r.target)),
                        href(&r.target),
                        json(r.kind.keyword()),
                        r.items.iter().map(|i| json(i)).collect::<Vec<_>>().join(","),
                        json_opt(r.technology.clone()),
                    )
                })
                .collect();
            if behind.is_empty() {
                continue;
            }
            entries.push(format!(
                "{}:[{}]",
                json(&format!("{from}->{to}")),
                behind.join(",")
            ));
        }
        if entries.is_empty() {
            return String::new();
        }
        format!(
            "<script type=\"application/json\" class=\"viewer-edges\">{{{}}}</script>",
            entries.join(",")
        )
    }

    /// A view's `<figure>`: with the viewer, an inlined SVG sits in a
    /// frame (a group named for the view; the script makes it a tab stop
    /// when it takes it over) under the controls, with the relations
    /// behind its edges beside it; an `<img>` (a view LikeC4 did not
    /// draw) and a tree without the viewer keep the bare figure.
    fn figure(&self, label: &str, caption: &str, markup: &str, edges: &str) -> String {
        if self.options.viewer && markup.starts_with("<svg") {
            format!(
                "<figure class=\"view\" data-viewer>{caption}{VIEWER_BAR}{edges}<div class=\"viewer-frame\" role=\"group\" aria-label=\"{}: diagram\">{markup}</div></figure>",
                escape(label)
            )
        } else {
            format!("<figure class=\"view\">{caption}{markup}</figure>")
        }
    }

    /// The `<figure>` for a view, or the placeholder when its SVG is
    /// missing (recorded for the report).
    fn view_figure(
        &mut self,
        depth: usize,
        nodes: &BTreeMap<String, svg::Node>,
        view: &str,
        alt: &str,
    ) -> String {
        match self.view_markup(depth, nodes, view, alt) {
            Some(inlined) => {
                let edges = self.edge_data(&inlined.edges, nodes);
                format!("{}\n", self.figure(alt, "", &inlined.html, &edges))
            }
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
        let mut body = String::new();
        let _ = writeln!(body, "<h1>{}</h1>", escape(&self.options.title));
        let nodes = self.nodes(0, Here::Index);
        body.push_str(&self.view_figure(0, &nodes, "index", "Overview"));
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
        self.layout(0, Here::Index, None, &body)
    }

    fn container_page_html(&mut self, element: &Element) -> String {
        let nodes = self.nodes(1, Here::Container(&element.id));
        let mut body = String::new();
        let _ = writeln!(body, "<h1>{}</h1>", escape(&element.title));
        body.push_str(&self.meta_html(element));
        if let Some(view) = self.views.get(&element.id).cloned() {
            body.push_str(&self.view_figure(1, &nodes, &view, &element.title));
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
                body.push_str(&self.view_figure(1, &nodes, &view, &dotted(&descendant.id)));
            }
            body.push_str(&self.relation_tables(&descendant.id, 1));
            body.push_str("</section>\n");
        }
        self.layout(1, Here::Container(&element.id), Some(&element.title), &body)
    }

    /// The curated views on one page; `None` when there are none.
    fn views_page(&mut self) -> Option<String> {
        if self.curated.is_empty() {
            return None;
        }
        let mut body = String::from("<h1>Curated views</h1>\n");
        let nodes = self.nodes(0, Here::Views);
        for view in &self.curated {
            let inlined = self
                .view_markup(0, &nodes, view, view)
                .unwrap_or(svg::Inlined {
                    html: String::new(),
                    edges: Vec::new(),
                });
            let caption = format!("<figcaption>{}</figcaption>", escape(view));
            let edges = self.edge_data(&inlined.edges, &nodes);
            let _ = writeln!(
                body,
                "{}",
                self.figure(view, &caption, &inlined.html, &edges)
            );
        }
        Some(self.layout(0, Here::Views, Some("Curated views"), &body))
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

    let kind_names: BTreeSet<String> = model
        .elements
        .iter()
        .map(|e| e.kind.keyword().to_string())
        .collect();
    let by_likec4: BTreeMap<String, &Element> = model
        .elements
        .iter()
        .map(|e| (sanitize_id(&e.id), e))
        .collect();
    let generated: BTreeSet<&String> = views.values().collect();
    let curated: Vec<String> = options
        .views
        .keys()
        .filter(|v| v.as_str() != "index" && !generated.contains(v))
        .cloned()
        .collect();
    let mut ctx = Ctx {
        model: &model,
        options,
        views,
        places,
        curated,
        by_likec4,
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
    if options.viewer {
        pages.insert("viewer.css".to_string(), VIEWER_STYLESHEET.to_string());
        pages.insert("viewer.js".to_string(), VIEWER_SCRIPT.to_string());
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
        let mut tests = element("app.tests", ElementKind::Tests);
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
            svg: "<svg width=\"10pt\" height=\"10pt\" viewBox=\"0 0 10 10\" xmlns=\"http://www.w3.org/2000/svg\">\n<g id=\"node1\" class=\"node\">\n<title>app</title>\n<polygon fill=\"#3b82f6\" points=\"0,0 1,1\"/>\n</g>\n<g id=\"node2\" class=\"node\">\n<title>lib</title>\n<polygon fill=\"#3b82f6\" points=\"0,0 1,1\"/>\n</g>\n<g id=\"node3\" class=\"node\">\n<title>driver_1</title>\n<polygon fill=\"#3b82f6\" points=\"0,0 1,1\"/>\n</g>\n<g id=\"edge1\" class=\"edge\">\n<title>app&#45;&gt;lib</title>\n<path fill=\"none\" stroke=\"#8d8d8d\" d=\"M0,0\"/>\n</g>\n<g id=\"edge2\" class=\"edge\">\n<title>lib&#45;&gt;driver_1</title>\n<path fill=\"none\" stroke=\"#8d8d8d\" d=\"M0,0\"/>\n</g>\n</svg>\n".into(),
            dot: Some("digraph {\n    graph [likec4_viewId=index];\n    app [likec4_id=app];\n    lib [likec4_id=lib];\n    driver_1 [likec4_id=node_driver];\n    app -> lib [likec4_id=\"1ab\"];\n    lib -> driver_1 [likec4_id=\"1ac\"];\n}\n".into()),
        }
    }

    /// A view with one node and no edge, for claims about the figure's
    /// shape around a view.
    fn likec4_bare_view() -> ViewSource {
        ViewSource {
            svg: "<svg width=\"10pt\" height=\"10pt\" viewBox=\"0 0 10 10\" xmlns=\"http://www.w3.org/2000/svg\">\n<g id=\"node1\" class=\"node\">\n<title>app</title>\n<polygon fill=\"#3b82f6\" points=\"0,0 1,1\"/>\n</g>\n</svg>\n".into(),
            dot: Some("digraph {\n    graph [likec4_viewId=index];\n    app [likec4_id=app];\n}\n".into()),
        }
    }
    /// `view_app` as LikeC4 would draw it: the crate itself, its `server`
    /// module and the external it spawns, each a node.
    fn likec4_scoped_view() -> ViewSource {
        ViewSource {
            svg: "<svg width=\"10pt\" height=\"10pt\" viewBox=\"0 0 10 10\" xmlns=\"http://www.w3.org/2000/svg\">\n<g id=\"node1\" class=\"node\">\n<title>app</title>\n<polygon fill=\"#3b82f6\" points=\"0,0 1,1\"/>\n</g>\n<g id=\"node2\" class=\"node\">\n<title>server</title>\n<polygon fill=\"#3b82f6\" points=\"0,0 1,1\"/>\n</g>\n<g id=\"node3\" class=\"node\">\n<title>driver_1</title>\n<polygon fill=\"#3b82f6\" points=\"0,0 1,1\"/>\n</g>\n<g id=\"edge1\" class=\"edge\">\n<title>server&#45;&gt;driver_1</title>\n<path fill=\"none\" stroke=\"#8d8d8d\" d=\"M0,0\"/>\n</g>\n</svg>\n".into(),
            dot: Some("digraph {\n    graph [likec4_viewId=view_app];\n    app [likec4_id=app];\n    server [likec4_id=\"app.server\"];\n    driver_1 [likec4_id=node_driver];\n    server -> driver_1 [likec4_id=\"1ab\"];\n}\n".into()),
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

    fn app_page_with_scoped_view() -> String {
        let mut opts = options();
        opts.viewer = true;
        opts.views.insert("view_app".into(), likec4_scoped_view());
        let site = generate(&sample(), &opts);
        page(&site, "containers/app.html").to_string()
    }

    #[test]
    fn a_figure_carries_the_relations_behind_each_edge_it_draws() {
        let app = app_page_with_scoped_view();
        assert!(
            app.contains("<script type=\"application/json\" class=\"viewer-edges\">{\"app.server->node_driver\":[{\"source\":\"app.server\",\"source_href\":\"../containers/app.html#app.server\",\"target\":\"node_driver\",\"target_href\":\"../index.html#node_driver\",\"kind\":\"uses\",\"items\":[\"spawns\"],\"technology\":\"stdio\"}]}</script><div class=\"viewer-frame\""),
            "{app}"
        );
    }

    #[test]
    fn an_edge_between_crates_lists_the_relations_between_their_modules() {
        let mut opts = options();
        opts.viewer = true;
        opts.views.insert("index".into(), likec4_view());
        let site = generate(&sample(), &opts);
        let index = page(&site, "index.html");
        assert!(
            index.contains("\"app->lib\":[{\"source\":\"app.server\",\"source_href\":\"containers/app.html#app.server\",\"target\":\"lib.util\",\"target_href\":\"containers/lib.html#lib.util\",\"kind\":\"names\",\"items\":[\"Helper\"],\"technology\":null}]"),
            "{index}"
        );
    }

    #[test]
    fn the_popover_does_not_link_the_page_s_own_crate_either() {
        // The sample has lib.util -> app; on lib's page an edge lib->app
        // would list it with app, the other page, linked; on app's page
        // the same relation's target is the page itself.
        let mut opts = options();
        opts.viewer = true;
        let mut scoped = likec4_scoped_view();
        scoped.svg = scoped.svg.replace(
            "<title>server&#45;&gt;driver_1</title>",
            "<title>driver_1&#45;&gt;app</title>",
        );
        scoped.dot = scoped.dot.map(|d| {
            d.replace(
                "server -> driver_1 [likec4_id=\"1ab\"];",
                "driver_1 -> app [likec4_id=\"1ab\"];",
            )
        });
        let mut model = sample();
        model
            .relations
            .push(relation("node_driver", "app", RelationKind::Uses, &["App"]));
        opts.views.insert("view_app".into(), scoped);
        let site = generate(&model, &opts);
        let app = page(&site, "containers/app.html");
        assert!(
            app.contains("\"target\":\"app\",\"target_href\":null"),
            "{app}"
        );
    }

    #[test]
    fn an_edge_with_no_relation_behind_it_is_left_out_of_the_data() {
        let mut opts = options();
        opts.viewer = true;
        opts.views.insert("index".into(), likec4_view());
        let site = generate(&sample(), &opts);
        let index = page(&site, "index.html");
        assert!(index.contains("viewer-edges"), "{index}");
        assert!(!index.contains("lib->node_driver"), "{index}");
    }

    #[test]
    fn json_escapes_what_would_break_a_string_or_a_script_element() {
        assert_eq!(json("a\"b\\c\nd<e"), "\"a\\\"b\\\\c\\nd\\u003ce\"");
    }

    #[test]
    fn an_absent_text_is_json_null() {
        assert_eq!(json_opt(None), "null");
        assert_eq!(json_opt(Some("x".into())), "\"x\"");
    }

    #[test]
    fn json_escapes_every_control_character_and_nothing_printable() {
        assert_eq!(json("\u{1}x\u{1f}"), "\"\\u0001x\\u001f\"");
        assert_eq!(json(" ~\u{7f}é"), "\" ~\u{7f}é\"");
    }

    #[test]
    fn on_a_crate_page_a_module_node_links_to_its_section() {
        let app = app_page_with_scoped_view();
        assert!(
            app.contains("<a href=\"../containers/app.html#app.server\"><g id=\"view_app-node2\" class=\"node c4-k-component\">\n<title>app.server</title>"),
            "{app}"
        );
    }

    #[test]
    fn on_a_crate_page_its_own_node_is_named() {
        let app = app_page_with_scoped_view();
        assert!(
            app.contains(
                "<g id=\"view_app-node1\" class=\"node c4-k-container\">\n<title>app</title>"
            ),
            "{app}"
        );
    }

    #[test]
    fn on_a_crate_page_its_own_node_is_not_linked_to_itself() {
        let app = app_page_with_scoped_view();
        assert!(
            !app.contains("<a href=\"../containers/app.html\">"),
            "{app}"
        );
    }

    #[test]
    fn on_a_crate_page_an_external_node_links_to_its_row_on_the_index() {
        let app = app_page_with_scoped_view();
        assert!(
            app.contains("<a href=\"../index.html#node_driver\"><g id=\"view_app-node3\" class=\"node c4-k-process\">\n<title>node_driver</title>"),
            "{app}"
        );
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
        let kinds: BTreeSet<String> = ["component", "container", "process", "tests"]
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
            index.contains("<a href=\"containers/app.html\"><g id=\"index-node1\" class=\"node c4-k-container\">\n<title>app</title>"),
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
            app.contains(&format!("</nav>{SCHEME_CONTROL}</div>")),
            "{app}"
        );
        assert!(
            app.contains("<html lang=\"en\" data-theme-default=\"system\">"),
            "{app}"
        );
    }

    #[test]
    fn the_viewer_wraps_an_inlined_view_in_a_frame_under_hidden_controls() {
        let mut opts = options();
        opts.viewer = true;
        opts.views.insert("index".into(), likec4_bare_view());
        let site = generate(&sample(), &opts);
        let index = page(&site, "index.html");
        assert!(
            index.contains(&format!(
                "<figure class=\"view\" data-viewer>{VIEWER_BAR}<div class=\"viewer-frame\" role=\"group\" aria-label=\"Overview: diagram\"><svg viewBox="
            )),
            "{index}"
        );
    }

    #[test]
    fn the_viewer_controls_ship_hidden() {
        assert!(VIEWER_BAR.starts_with("<div class=\"viewer-bar\" hidden>"));
    }

    #[test]
    fn the_viewer_links_its_script_deferred_and_adds_it_to_the_page_set() {
        let mut opts = options();
        opts.viewer = true;
        let site = generate(&sample(), &opts);
        assert!(
            page(&site, "containers/app.html")
                .contains("<script src=\"../viewer.js\" defer></script>\n</head>"),
            "{}",
            page(&site, "containers/app.html")
        );
        assert_eq!(page(&site, "viewer.js"), VIEWER_SCRIPT);
    }

    #[test]
    fn the_viewer_links_its_own_stylesheet_after_the_tree_s_and_adds_it_to_the_page_set() {
        let mut opts = options();
        opts.viewer = true;
        let site = generate(&sample(), &opts);
        assert!(
            page(&site, "containers/app.html").contains(
                "<link rel=\"stylesheet\" href=\"../theme.css\">\n<link rel=\"stylesheet\" href=\"../viewer.css\">\n"
            ),
            "{}",
            page(&site, "containers/app.html")
        );
        assert_eq!(page(&site, "viewer.css"), VIEWER_STYLESHEET);
    }

    #[test]
    fn the_viewer_s_rules_live_in_its_own_stylesheet_not_the_page_s() {
        assert!(VIEWER_STYLESHEET.contains(".viewer-frame"));
        assert!(VIEWER_STYLESHEET.contains(".viewer-popover"));
        assert!(!STYLESHEET.contains("viewer-"), "{STYLESHEET}");
    }

    #[test]
    fn the_viewer_leaves_an_image_fallback_as_a_bare_figure() {
        let mut opts = options();
        opts.viewer = true;
        opts.views.insert("context".into(), plain_view());
        let site = generate(&sample(), &opts);
        let views = page(&site, "views.html");
        assert!(
            views.contains("<figure class=\"view\"><figcaption>context</figcaption><img "),
            "{views}"
        );
    }

    #[test]
    fn a_curated_view_puts_its_caption_before_the_controls() {
        let mut opts = options();
        opts.viewer = true;
        opts.views.insert("context".into(), likec4_bare_view());
        let site = generate(&sample(), &opts);
        let views = page(&site, "views.html");
        assert!(
            views.contains(&format!(
                "<figure class=\"view\" data-viewer><figcaption>context</figcaption>{VIEWER_BAR}<div class=\"viewer-frame\" role=\"group\" aria-label=\"context: diagram\">"
            )),
            "{views}"
        );
    }

    #[test]
    fn without_the_viewer_there_is_no_script_and_no_frame() {
        let mut opts = options();
        opts.views.insert("index".into(), likec4_view());
        let site = generate(&sample(), &opts);
        assert!(!site.pages.contains_key("viewer.js"));
        assert!(!site.pages.contains_key("viewer.css"));
        for (path, contents) in site.pages.iter().filter(|(p, _)| p.ends_with(".html")) {
            assert!(!contents.contains("viewer.js"), "{path}: {contents}");
            assert!(!contents.contains("viewer.css"), "{path}: {contents}");
            assert!(!contents.contains("data-viewer"), "{path}: {contents}");
            assert!(!contents.contains("viewer-frame"), "{path}: {contents}");
            assert!(!contents.contains("viewer-edges"), "{path}: {contents}");
        }
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
            app.contains(&format!(
                "<a class=\"site\" href=\"../index.html\">{MARK}Sample</a>",
                MARK = mark_svg()
            )),
            "{app}"
        );
        assert!(app.contains("<title>Sample · app</title>"), "{app}");
        let index = page(&site, "index.html");
        assert!(
            index.contains("<link rel=\"stylesheet\" href=\"style.css\">"),
            "{index}"
        );
        assert!(
            index.contains(&format!(
                "<a class=\"site\" href=\"index.html\" aria-current=\"page\">{MARK}Sample</a>",
                MARK = mark_svg()
            )),
            "{index}"
        );
    }

    #[test]
    fn the_tree_title_carries_the_asbuilt_mark_hidden_from_screen_readers() {
        assert_eq!(
            mark_svg(),
            format!(
                "<svg class=\"mark\" viewBox=\"0 0 24 24\" aria-hidden=\"true\" focusable=\"false\"><path d=\"{}\"/></svg>",
                MARK_PATH.trim()
            )
        );
        assert!(
            MARK_PATH.starts_with("M14.24 3.21L17.61 1.26"),
            "{MARK_PATH}"
        );
        assert!(!MARK_PATH.trim().contains('\n'));
    }

    #[test]
    fn a_container_page_s_header_is_the_trail_then_the_containers_with_this_one_current() {
        let site = generate(&sample(), &options());
        assert!(
            page(&site, "containers/app.html").contains(&format!("<header><div class=\"bar\"><nav class=\"trail\" aria-label=\"Breadcrumb\"><a class=\"site\" href=\"../index.html\">{MARK}Sample</a></nav></div><nav class=\"containers\" aria-label=\"Containers\"><span aria-current=\"page\">app</span><a href=\"../containers/lib.html\">lib</a></nav></header>", MARK = mark_svg())),
            "{}",
            page(&site, "containers/app.html")
        );
    }

    #[test]
    fn no_page_repeats_the_trail_below_the_header() {
        let mut opts = options();
        opts.home_url = Some("../".into());
        let site = generate(&sample(), &opts);
        for (path, contents) in &site.pages {
            assert!(!contents.contains("crumbs"), "{path}: {contents}");
            assert_eq!(
                contents.matches("aria-label=\"Breadcrumb\"").count(),
                usize::from(path.ends_with(".html")),
                "{path}"
            );
        }
    }

    #[test]
    fn a_model_without_containers_has_no_containers_row() {
        let mut model = sample();
        model.elements.retain(is_external);
        model.relations.clear();
        let site = generate(&model, &options());
        assert!(
            !page(&site, "index.html").contains("class=\"containers\""),
            "{}",
            page(&site, "index.html")
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
    fn a_home_url_opens_the_trail_resolved_from_each_depth() {
        let mut opts = options();
        opts.home_url = Some("../".into());
        opts.home_title = Some("Home".into());
        let site = generate(&sample(), &opts);
        let index = page(&site, "index.html");
        assert!(
            index.contains(&format!("<nav class=\"trail\" aria-label=\"Breadcrumb\"><a class=\"home\" href=\"../\">Home</a><span class=\"sep\" aria-hidden=\"true\">/</span><a class=\"site\" href=\"index.html\" aria-current=\"page\">{MARK}Sample</a></nav>", MARK = mark_svg())),
            "{index}"
        );
        let app = page(&site, "containers/app.html");
        assert!(
            app.contains(&format!("<nav class=\"trail\" aria-label=\"Breadcrumb\"><a class=\"home\" href=\"../../\">Home</a><span class=\"sep\" aria-hidden=\"true\">/</span><a class=\"site\" href=\"../index.html\">{MARK}Sample</a></nav>", MARK = mark_svg())),
            "{app}"
        );
    }

    #[test]
    fn the_index_title_names_the_tree_once() {
        let site = generate(&sample(), &options());
        let index = page(&site, "index.html");
        assert!(index.contains("<title>Sample</title>"), "{index}");
    }

    #[test]
    fn a_home_title_starts_the_index_title() {
        let mut opts = options();
        opts.home_title = Some("Host".into());
        let site = generate(&sample(), &opts);
        let index = page(&site, "index.html");
        assert!(index.contains("<title>Host · Sample</title>"), "{index}");
    }

    #[test]
    fn a_home_title_starts_a_container_page_title() {
        let mut opts = options();
        opts.home_title = Some("Host".into());
        let site = generate(&sample(), &opts);
        let app = page(&site, "containers/app.html");
        assert!(app.contains("<title>Host · Sample · app</title>"), "{app}");
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
    fn without_a_home_url_the_trail_is_the_tree_s_title_alone() {
        let site = generate(&sample(), &options());
        let index = page(&site, "index.html");
        assert!(!index.contains("class=\"home\""), "{index}");
        assert!(!index.contains("class=\"sep\""), "{index}");
        assert!(
            index.contains(&format!("<nav class=\"trail\" aria-label=\"Breadcrumb\"><a class=\"site\" href=\"index.html\" aria-current=\"page\">{MARK}Sample</a></nav>", MARK = mark_svg())),
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
            curated: vec![],
            by_likec4: BTreeMap::new(),
            missing: BTreeSet::new(),
        };
        assert_eq!(
            ctx.view_figure(0, &BTreeMap::new(), "a<b", "alt"),
            "<p class=\"missing\">No diagram for <code>a&lt;b</code>: run <code>asbuilt render</code>.</p>\n"
        );
    }

    #[test]
    fn a_page_below_the_root_links_the_curated_views_through_the_parent() {
        let mut opts = options();
        opts.views.insert("context".into(), plain_view());
        let site = generate(&sample(), &opts);
        let app = page(&site, "containers/app.html");
        assert!(
            app.contains("<nav class=\"views\" aria-label=\"Views\"><a href=\"../views.html\">Curated views</a></nav>"),
            "{app}"
        );
    }

    #[test]
    fn the_views_page_marks_its_own_header_link_current() {
        let mut opts = options();
        opts.views.insert("context".into(), plain_view());
        let site = generate(&sample(), &opts);
        let views = page(&site, "views.html");
        assert!(
            views.contains("<nav class=\"views\" aria-label=\"Views\"><span aria-current=\"page\">Curated views</span></nav>"),
            "{views}"
        );
    }

    #[test]
    fn a_tree_with_no_curated_views_has_no_views_link() {
        let site = generate(&sample(), &options());
        assert!(
            site.pages.values().all(|p| !p.contains("class=\"views\"")),
            "a page links a views page that does not exist"
        );
    }

    #[test]
    fn the_views_page_trail_links_the_index_without_marking_it_current() {
        let mut opts = options();
        opts.views.insert("context".into(), plain_view());
        let site = generate(&sample(), &opts);
        assert!(
            page(&site, "views.html").contains(&format!(
                "<a class=\"site\" href=\"index.html\">{MARK}Sample</a></nav>",
                MARK = mark_svg()
            )),
            "{}",
            page(&site, "views.html")
        );
    }

    #[test]
    fn the_header_s_container_row_is_a_labeled_navigation_region() {
        let site = generate(&sample(), &options());
        assert!(
            page(&site, "index.html")
                .contains("<nav class=\"containers\" aria-label=\"Containers\"><a href=\"containers/app.html\">app</a><a href=\"containers/lib.html\">lib</a></nav>"),
            "{}",
            page(&site, "index.html")
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
