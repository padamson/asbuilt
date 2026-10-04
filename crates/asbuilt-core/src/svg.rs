//! A rendered view inlined into a page, so the page's colors reach it.
//!
//! The SVG `dot -Tsvg` wrote is rewritten: the prolog and comments go,
//! and Graphviz's tooltips give way to each known node's own name and a
//! link to the page that documents it; `width` and `height` give way to
//! the page's layout; ids are prefixed with the view's name, so several
//! views share a page; and classes name what `theme.css` colors: each
//! node's and group box's element kind (read from the `.dot` LikeC4
//! wrote beside it), a node's secondary text, and an edge label's
//! backing. Only a LikeC4 render is
//! inlined. Any other SVG is left for an `<img>`, where a script in it
//! cannot run, and the rewrite drops scripts, `foreignObject` and event
//! handlers besides. Pure string work over Graphviz's own output.

use std::collections::BTreeMap;

use crate::docs::escape;

/// A rendered view as `render` left it: the SVG, and the `.dot` beside it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ViewSource {
    pub svg: String,
    pub dot: Option<String>,
}

/// What a page knows about an element a view may draw: its kind, the
/// name its node carries on hover, and a link to where it is
/// documented (`None` for an element the page itself documents).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    pub kind: String,
    pub title: String,
    pub href: Option<String>,
}

/// Whether `dot` was written by `likec4 gen dot`, which names the view in
/// a `likec4_viewId` graph attribute.
pub fn is_likec4_dot(dot: &str) -> bool {
    dot.contains("likec4_viewId=")
}

fn is_dot_id(text: &str) -> bool {
    !text.is_empty() && text.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// An attribute value at the start of `text`: quoted, or up to the next
/// separator.
fn attr_value(text: &str) -> String {
    match text.strip_prefix('"') {
        Some(quoted) => quoted.split('"').next().unwrap_or_default().to_string(),
        None => text
            .split(|c: char| matches!(c, ',' | ']' | ';') || c.is_whitespace())
            .next()
            .unwrap_or_default()
            .to_string(),
    }
}

/// The LikeC4 id of each node and group box in a `.dot` LikeC4 wrote,
/// keyed by the Graphviz name the SVG's `<title>` carries. LikeC4 writes
/// one attribute per line, so a statement's name opens a line (`server
/// [`, `subgraph cluster_app {`) and its `likec4_id` follows; an edge
/// carries no element, and a cluster's own `graph [` line keeps its name.
pub fn likec4_ids(dot: &str) -> BTreeMap<String, String> {
    let mut ids = BTreeMap::new();
    let mut current: Option<String> = None;
    for line in dot.lines().map(str::trim) {
        if let Some(rest) = line.strip_prefix("subgraph ") {
            current = rest
                .split_whitespace()
                .next()
                .filter(|name| is_dot_id(name))
                .map(str::to_string);
        } else if let Some((head, _)) = line.split_once('[') {
            let head = head.trim();
            if head.contains("->") {
                current = None;
            } else if head != "graph" && is_dot_id(head) {
                current = Some(head.to_string());
            }
        }
        if let Some(at) = line.find("likec4_id=")
            && let Some(name) = current.take()
        {
            ids.insert(name, attr_value(&line[at + "likec4_id=".len()..]));
        }
    }
    ids
}

/// The index of the `>` that closes the tag opening `text`, outside
/// any quoted attribute value.
fn tag_end(text: &str) -> Option<usize> {
    let mut quoted = false;
    for (i, c) in text.char_indices() {
        match c {
            '"' => quoted = !quoted,
            '>' if !quoted => return Some(i),
            _ => {}
        }
    }
    None
}

#[derive(Debug)]
struct Tag {
    name: String,
    attrs: Vec<(String, String)>,
    self_closing: bool,
}

impl Tag {
    /// A start tag's name and double-quoted attributes, values kept as
    /// written; `None` for anything else.
    fn parse(inner: &str) -> Option<Tag> {
        let inner = inner.trim_end();
        let (inner, self_closing) = match inner.strip_suffix('/') {
            Some(inner) => (inner, true),
            None => (inner, false),
        };
        let name_end = inner.find(char::is_whitespace).unwrap_or(inner.len());
        let name = inner[..name_end].to_string();
        let mut attrs = Vec::new();
        let mut rest = inner[name_end..].trim_start();
        while !rest.is_empty() {
            let (key, after) = rest.split_once('=')?;
            let quoted = after.trim_start().strip_prefix('"')?;
            let (value, tail) = quoted.split_once('"')?;
            attrs.push((key.trim().to_string(), value.to_string()));
            rest = tail.trim_start();
        }
        Some(Tag {
            name,
            attrs,
            self_closing,
        })
    }

    fn get(&self, key: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    /// Set `key` to `value`, which the caller has escaped.
    fn set(&mut self, key: &str, value: String) {
        match self.attrs.iter_mut().find(|(k, _)| k == key) {
            Some((_, v)) => *v = value,
            None => self.attrs.push((key.to_string(), value)),
        }
    }

    fn remove(&mut self, key: &str) {
        self.attrs.retain(|(k, _)| k != key);
    }

    fn add_class(&mut self, class: &str) {
        let classes = match self.get("class") {
            Some(existing) => format!("{existing} {class}"),
            None => class.to_string(),
        };
        self.set("class", classes);
    }

    /// Write the tag; returns where its `class` value ends in `out`, so
    /// a class learned later can be inserted.
    fn write(&self, out: &mut String) -> Option<usize> {
        out.push('<');
        out.push_str(&self.name);
        let mut class_end = None;
        for (key, value) in &self.attrs {
            out.push(' ');
            out.push_str(key);
            out.push_str("=\"");
            out.push_str(value);
            if key == "class" {
                class_end = Some(out.len());
            }
            out.push('"');
        }
        out.push_str(if self.self_closing { "/>" } else { ">" });
        class_end
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Role {
    Node,
    Cluster,
    Edge,
}

#[derive(Debug)]
struct Group {
    role: Option<Role>,
    /// Where the group's open tag starts in the output, so a node can be
    /// wrapped in a link once its `<title>` names the element.
    open_at: usize,
    /// Where the group's `class` value ends in the output, until its
    /// `<title>` names the element and the kind class goes in there.
    class_end: Option<usize>,
    /// The fill of a node's first text line, its title.
    title_fill: Option<String>,
    /// Whether an `<a>` was opened before the group, to close after it.
    linked: bool,
}

/// An attribute that could run a script: an event handler, or a link to
/// a `javascript:` URL.
fn is_active(key: &str, value: &str) -> bool {
    key.to_ascii_lowercase().starts_with("on")
        || (key.ends_with("href")
            && value
                .trim_start()
                .to_ascii_lowercase()
                .starts_with("javascript:"))
}

/// `source.svg` rewritten for a page, or `None` when it is not a LikeC4
/// render (no `.dot` beside it, or one LikeC4 did not write) or not SVG
/// in the shape Graphviz writes. `nodes` maps a LikeC4 id to what the
/// page knows about its element: a node is classed by its kind, titled
/// by its name (the hover tooltip) and wrapped in a link to its page
/// when it has one; `label` is the diagram's accessible name.
pub fn inline(
    source: &ViewSource,
    view: &str,
    label: &str,
    nodes: &BTreeMap<String, Node>,
) -> Option<String> {
    let dot = source.dot.as_deref().filter(|dot| is_likec4_dot(dot))?;
    let ids = likec4_ids(dot);
    let mut rest = &source.svg[source.svg.find("<svg")?..];
    let mut out = String::with_capacity(rest.len());
    let mut groups: Vec<Group> = Vec::new();
    while let Some(lt) = rest.find('<') {
        out.push_str(&rest[..lt]);
        rest = &rest[lt..];
        if let Some(comment) = rest.strip_prefix("<!--") {
            rest = &comment[comment.find("-->")? + "-->".len()..];
            continue;
        }
        let end = tag_end(rest)?;
        let inner = &rest[1..end];
        rest = &rest[end + 1..];
        if inner.starts_with('?') || inner.starts_with('!') {
            continue;
        }
        if let Some(closing) = inner.strip_prefix('/') {
            if closing.trim() == "g" {
                let group = groups.pop();
                out.push_str("</g>");
                if group.is_some_and(|g| g.linked) {
                    out.push_str("</a>");
                }
                continue;
            }
            out.push('<');
            out.push_str(inner);
            out.push('>');
            continue;
        }
        let mut tag = Tag::parse(inner)?;
        if tag.name == "title" && !tag.self_closing {
            let close = rest.find("</title>")?;
            let name = &rest[..close];
            rest = &rest[close + "</title>".len()..];
            if let Some(group) = groups.last_mut()
                && let Some(at) = group.class_end.take()
                && let Some(node) = ids.get(name).and_then(|id| nodes.get(id))
            {
                out.insert_str(at, &format!(" c4-k-{}", node.kind));
                if group.role == Some(Role::Node) {
                    // The element names itself on hover, and its node is
                    // a link to where the page documents it.
                    out.push_str(&format!("<title>{}</title>", escape(&node.title)));
                    if let Some(href) = &node.href {
                        out.insert_str(group.open_at, &format!("<a href=\"{}\">", escape(href)));
                        group.linked = true;
                    }
                }
            }
            continue;
        }
        if matches!(tag.name.as_str(), "script" | "foreignObject") {
            if !tag.self_closing {
                let close = format!("</{}>", tag.name);
                rest = &rest[rest.find(&close)? + close.len()..];
            }
            continue;
        }
        tag.attrs.retain(|(key, value)| !is_active(key, value));
        if let Some((_, id)) = tag.attrs.iter_mut().find(|(key, _)| key == "id") {
            *id = format!("{}-{id}", escape(view));
        }
        let enclosing = groups.iter().rev().find_map(|g| g.role);
        match tag.name.as_str() {
            "svg" => {
                tag.remove("width");
                tag.remove("height");
                tag.set("class", "c4".to_string());
                tag.set("data-view", escape(view));
                tag.set("role", "img".to_string());
                tag.set("aria-label", escape(label));
            }
            "g" => {
                let role = match tag.get("class") {
                    Some("node") => Some(Role::Node),
                    Some("cluster") => Some(Role::Cluster),
                    Some("edge") => Some(Role::Edge),
                    _ => None,
                };
                let open_at = out.len();
                let class_end = tag.write(&mut out);
                groups.push(Group {
                    role,
                    open_at,
                    class_end: class_end
                        .filter(|_| matches!(role, Some(Role::Node | Role::Cluster))),
                    title_fill: None,
                    linked: false,
                });
                continue;
            }
            "text" if enclosing == Some(Role::Node) => {
                let fill = tag.get("fill").unwrap_or_default().to_string();
                if let Some(node) = groups.iter_mut().rev().find(|g| g.role.is_some()) {
                    match &node.title_fill {
                        None => node.title_fill = Some(fill),
                        Some(title) if *title != fill => tag.add_class("c4-muted"),
                        Some(_) => {}
                    }
                }
            }
            "polygon" if enclosing == Some(Role::Edge) && tag.get("fill-opacity").is_some() => {
                tag.add_class("c4-label-bg");
            }
            _ => {}
        }
        tag.write(&mut out);
    }
    out.push_str(rest);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOT: &str = r##"digraph {
    graph [likec4_viewId=view_app,
        bgcolor=transparent
    ];
    node [color="#2563eb",
        fillcolor="#3b82f6"
    ];
    subgraph cluster_app {
        graph [color="#57130f",
            label=<<FONT POINT-SIZE="11">APP [x]</FONT>>,
            likec4_id=app
        ];
        server [color="#2563eb",
            likec4_id="app.server"];
    }
    driver_1 [likec4_id=node_driver];
    server -> driver_1 [likec4_id="1ab"];
}
"##;

    const SVG: &str = r##"<?xml version="1.0" encoding="UTF-8" standalone="no"?>
<!DOCTYPE svg PUBLIC "-//W3C//DTD SVG 1.1//EN"
 "http://www.w3.org/Graphics/SVG/1.1/DTD/svg11.dtd">
<!-- Generated by graphviz -->
<svg width="300pt" height="200pt"
 viewBox="0.00 0.00 300.00 200.00" xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink">
<g id="graph0" class="graph" transform="scale(1 1)">
<title>G</title>
<g id="clust1" class="cluster">
<title>cluster_app</title>
<polygon fill="#722f24" stroke="#57130f" points="0,0 1,1"/>
<text x="1" y="1" fill="#ffffe5" fill-opacity="0.7">APP</text>
</g>
<!-- server -->
<g id="node1" class="node">
<title>server</title>
<polygon fill="#3b82f6" stroke="#2563eb" points="0,0 1,1"/>
<text x="1" y="1" fill="#eff6ff">server</text>
<text x="1" y="2" fill="#bfdbfe">Where it runs</text>
</g>
<g id="node2" class="node">
<title>driver_1</title>
<polygon fill="#3b82f6" stroke="#2563eb" points="0,0 1,1"/>
<text x="1" y="1" fill="#eff6ff">node</text>
<text x="1" y="2" fill="#eff6ff">driver</text>
</g>
<g id="node3" class="node">
<title>stranger</title>
<polygon fill="#3b82f6" stroke="#2563eb" points="0,0 1,1"/>
</g>
<g id="edge1" class="edge">
<title>server&#45;&gt;driver_1</title>
<path fill="none" stroke="#8d8d8d" d="M0,0"/>
<polygon fill="#8d8d8d" stroke="#8d8d8d" points="0,0 1,1"/>
<polygon fill="#18191b" fill-opacity="0.627451" stroke="none" points="0,0 1,1"/>
<text x="1" y="1" fill="#c9c9c9">spawns</text>
<text x="1" y="2" fill="#aaaaaa">stdio</text>
</g>
</g>
</svg>
"##;

    fn node(kind: &str, title: &str, href: Option<&str>) -> Node {
        Node {
            kind: kind.into(),
            title: title.into(),
            href: href.map(String::from),
        }
    }

    /// The page's view of the fixture's elements: the crate and its
    /// module have pages, the external none.
    fn nodes() -> BTreeMap<String, Node> {
        BTreeMap::from([
            (
                "app".to_string(),
                node("container", "app", Some("containers/app.html")),
            ),
            (
                "app.server".to_string(),
                node(
                    "component",
                    "app.server",
                    Some("containers/app.html#app.server"),
                ),
            ),
            (
                "node_driver".to_string(),
                node("process", "node_driver", None),
            ),
        ])
    }

    fn source() -> ViewSource {
        ViewSource {
            svg: SVG.into(),
            dot: Some(DOT.into()),
        }
    }

    fn inlined() -> String {
        inline(&source(), "view_app", "app", &nodes()).expect("inlined")
    }

    #[test]
    fn likec4_ids_maps_nodes_and_group_boxes_but_not_edges() {
        let ids = likec4_ids(DOT);
        let expected: BTreeMap<String, String> = [
            ("cluster_app", "app"),
            ("driver_1", "node_driver"),
            ("server", "app.server"),
        ]
        .iter()
        .map(|(n, id)| (n.to_string(), id.to_string()))
        .collect();
        assert_eq!(ids, expected);
    }

    #[test]
    fn a_view_without_a_dot_is_not_inlined() {
        let source = ViewSource {
            svg: SVG.into(),
            dot: None,
        };
        assert_eq!(inline(&source, "view_app", "app", &nodes()), None);
    }

    #[test]
    fn a_view_whose_dot_likec4_did_not_write_is_not_inlined() {
        let source = ViewSource {
            svg: SVG.into(),
            dot: Some("digraph { a -> b }".into()),
        };
        assert_eq!(inline(&source, "view_app", "app", &nodes()), None);
    }

    #[test]
    fn the_prolog_comments_and_graphviz_s_tooltips_are_dropped() {
        // Graphviz titles every group with its own name; a node's is
        // replaced by the element's, the rest go.
        let svg = inlined();
        assert!(svg.starts_with("<svg "), "{svg}");
        for gone in [
            "<?xml",
            "<!DOCTYPE",
            "<!--",
            "-->",
            "<title>G</title>",
            "<title>cluster_app</title>",
            "<title>server</title>",
            "<title>driver_1</title>",
            "graphviz",
        ] {
            assert!(!svg.contains(gone), "{gone} in {svg}");
        }
    }

    #[test]
    fn the_root_gives_way_to_the_page_and_names_the_view() {
        let svg = inline(&source(), "view_app", "a<b", &nodes()).unwrap();
        let root = &svg[..svg.find('>').unwrap()];
        assert_eq!(
            root,
            "<svg viewBox=\"0.00 0.00 300.00 200.00\" xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" class=\"c4\" data-view=\"view_app\" role=\"img\" aria-label=\"a&lt;b\""
        );
    }

    #[test]
    fn ids_carry_the_view_name() {
        let svg = inlined();
        assert!(svg.contains("<g id=\"view_app-node1\""), "{svg}");
        assert!(!svg.contains("id=\"node1\""), "{svg}");
    }

    #[test]
    fn nodes_and_group_boxes_are_classed_by_element_kind() {
        let svg = inlined();
        assert!(
            svg.contains("<g id=\"view_app-clust1\" class=\"cluster c4-k-container\">"),
            "{svg}"
        );
        assert!(
            svg.contains("<g id=\"view_app-node1\" class=\"node c4-k-component\">"),
            "{svg}"
        );
        assert!(
            svg.contains("<g id=\"view_app-node2\" class=\"node c4-k-process\">"),
            "{svg}"
        );
    }

    #[test]
    fn a_node_with_a_page_is_a_link_to_it_titled_by_its_name() {
        let out = inlined();
        assert!(
            out.contains("<a href=\"containers/app.html#app.server\"><g id=\"view_app-node"),
            "{out}"
        );
        assert!(
            out.contains("class=\"node c4-k-component\">\n<title>app.server</title>"),
            "{out}"
        );
        assert_eq!(out.matches("</g></a>").count(), 1, "{out}");
    }

    #[test]
    fn a_node_without_a_page_is_titled_but_not_linked() {
        let out = inlined();
        assert!(
            out.contains("class=\"node c4-k-process\">\n<title>node_driver</title>"),
            "{out}"
        );
    }

    #[test]
    fn only_nodes_with_a_page_are_linked() {
        let out = inlined();
        assert_eq!(out.matches("<a ").count(), 1, "{out}");
    }

    #[test]
    fn a_group_box_is_neither_linked_nor_titled() {
        let out = inlined();
        assert!(!out.contains("<title>app</title>"), "{out}");
        assert!(!out.contains("<title>cluster_app</title>"), "{out}");
    }

    #[test]
    fn a_node_the_dot_does_not_name_keeps_its_plain_class() {
        let svg = inlined();
        assert!(
            svg.contains("<g id=\"view_app-node3\" class=\"node\">"),
            "{svg}"
        );
    }

    #[test]
    fn a_node_s_title_keeps_its_class_and_its_other_lines_are_muted() {
        let svg = inlined();
        assert!(
            svg.contains("<text x=\"1\" y=\"1\" fill=\"#eff6ff\">server</text>"),
            "{svg}"
        );
        assert!(
            svg.contains(
                "<text x=\"1\" y=\"2\" fill=\"#bfdbfe\" class=\"c4-muted\">Where it runs</text>"
            ),
            "{svg}"
        );
    }

    #[test]
    fn a_title_that_wraps_keeps_every_line_as_title() {
        let svg = inlined();
        assert!(
            svg.contains("<text x=\"1\" y=\"2\" fill=\"#eff6ff\">driver</text>"),
            "{svg}"
        );
    }

    #[test]
    fn an_edge_label_s_lines_are_never_muted() {
        let svg = inlined();
        assert!(
            svg.contains("<text x=\"1\" y=\"2\" fill=\"#aaaaaa\">stdio</text>"),
            "{svg}"
        );
    }

    #[test]
    fn an_edge_s_id_is_not_given_to_an_unnamed_node_before_it() {
        let ids = likec4_ids(
            "digraph {\n    graph [likec4_viewId=v];\n    helper [shape=point];\n    helper -> server [likec4_id=\"1ab\"];\n}\n",
        );
        assert_eq!(ids, BTreeMap::new());
    }

    #[test]
    fn a_greater_than_sign_inside_an_attribute_value_does_not_end_the_tag() {
        let source = ViewSource {
            svg: SVG.replace(
                "<text x=\"1\" y=\"2\" fill=\"#bfdbfe\">",
                "<text x=\"1\" y=\"2\" data-note=\"a>b\" fill=\"#bfdbfe\">",
            ),
            dot: Some(DOT.into()),
        };
        let svg = inline(&source, "view_app", "app", &nodes()).unwrap();
        assert!(
            svg.contains("data-note=\"a>b\" fill=\"#bfdbfe\" class=\"c4-muted\">"),
            "{svg}"
        );
    }

    #[test]
    fn a_processing_instruction_inside_the_drawing_is_dropped() {
        let source = ViewSource {
            svg: SVG.replace("<g id=\"graph0\"", "<?render x?><g id=\"graph0\""),
            dot: Some(DOT.into()),
        };
        let svg = inline(&source, "view_app", "app", &nodes()).unwrap();
        assert!(!svg.contains("<?render"), "{svg}");
    }

    #[test]
    fn an_edge_label_s_backing_is_classed_and_its_arrowhead_is_not() {
        let svg = inlined();
        assert!(
            svg.contains("<polygon fill=\"#18191b\" fill-opacity=\"0.627451\" stroke=\"none\" points=\"0,0 1,1\" class=\"c4-label-bg\"/>"),
            "{svg}"
        );
        assert!(
            svg.contains("<polygon fill=\"#8d8d8d\" stroke=\"#8d8d8d\" points=\"0,0 1,1\"/>"),
            "{svg}"
        );
    }

    #[test]
    fn scripts_foreign_objects_and_event_handlers_are_dropped() {
        let svg = SVG.replace(
            "<g id=\"edge1\" class=\"edge\">",
            "<script>alert(1)</script><foreignObject><div>x</div></foreignObject><a xlink:href=\"javascript:alert(1)\" onclick=\"alert(1)\" href=\"#ok\"></a><g id=\"edge1\" class=\"edge\">",
        );
        let source = ViewSource {
            svg,
            dot: Some(DOT.into()),
        };
        let out = inline(&source, "view_app", "app", &nodes()).unwrap();
        for gone in ["script", "alert", "foreignObject", "<div>", "onclick"] {
            assert!(!out.contains(gone), "{gone} in {out}");
        }
        assert!(out.contains("<a href=\"#ok\"></a>"), "{out}");
    }

    #[test]
    fn an_svg_that_does_not_parse_is_not_inlined() {
        let source = ViewSource {
            svg: "<svg viewBox=\"0 0 1 1><g>".into(),
            dot: Some(DOT.into()),
        };
        assert_eq!(inline(&source, "view_app", "app", &nodes()), None);
    }
}
