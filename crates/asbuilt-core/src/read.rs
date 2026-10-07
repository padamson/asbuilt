//! A model asbuilt wrote, read back: the elements and relations in the
//! `model` block of a `.c4` file, as the file spells them. The inverse of
//! [`emit`](crate::emit) for the model block it writes, so the drift
//! check can say what changed rather than only which lines did. The
//! specification and views are derived from the model block and are not
//! read.
//!
//! The model block must have emit's shape: its statements, each element's
//! fields in emit's order and at most once, every element before the first
//! relation, and every relation joining two elements the block declares.
//! Anything else (a hand edit, another tool's model, a release that wrote
//! a different shape) reads as `None`, never as a partial model: a summary
//! that silently missed part of a file would be worse than none.

use std::collections::BTreeMap;
use std::iter::Peekable;
use std::vec;

use crate::check::normalize;
use crate::emit::HEADER;

/// The elements and relations of a written model. Ids are dotted and as
/// the file writes them (LikeC4 identifiers); kinds are the keywords the
/// specification declares.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Written {
    pub elements: BTreeMap<String, WrittenElement>,
    /// Keyed by source and target id: the survey records one relation per
    /// pair.
    pub relations: BTreeMap<(String, String), WrittenRelation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WrittenElement {
    pub kind: String,
    pub title: String,
    pub description: Option<String>,
    pub technology: Option<String>,
    pub path: Option<String>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WrittenRelation {
    pub kind: String,
    /// The label as written: a surveyed relation's item names joined with
    /// `, `, or an external relation's title, which is free text and may
    /// hold `, ` itself, so it is not split back.
    pub label: Option<String>,
    pub technology: Option<String>,
}

/// The model block of `text`, or `None` when `text` is not a model
/// asbuilt wrote. Line endings may be `\r\n`.
pub fn read(text: &str) -> Option<Written> {
    let text = normalize(text);
    let rest = text.strip_prefix(HEADER)?.strip_prefix('\n')?;
    let start = rest.find("\nmodel {\n")? + "\nmodel {\n".len();
    let mut parser = Parser {
        tokens: tokenize(&rest[start..])?.into_iter().peekable(),
        written: Written::default(),
    };
    parser.model_body()?;
    let written = parser.written;
    written
        .relations
        .keys()
        .all(|(from, to)| written.elements.contains_key(from) && written.elements.contains_key(to))
        .then_some(written)
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    Word(String),
    Quoted(String),
    Open,
    Close,
}

/// Words, single-quoted strings (unescaped) and braces, up to the brace
/// that closes the model block; the views after it are not read. Every
/// pass takes a character, so malformed text ends the loop.
fn tokenize(text: &str) -> Option<Vec<Token>> {
    let mut tokens = Vec::new();
    let mut depth = 1usize;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' => {
                depth += 1;
                tokens.push(Token::Open);
            }
            '}' => {
                tokens.push(Token::Close);
                depth -= 1;
                if depth == 0 {
                    return Some(tokens);
                }
            }
            '\'' => {
                let mut value = String::new();
                loop {
                    match chars.next()? {
                        '\'' => break,
                        '\\' => value.push(chars.next()?),
                        c => value.push(c),
                    }
                }
                tokens.push(Token::Quoted(value));
            }
            c if c.is_whitespace() => {}
            c => {
                let mut word = String::from(c);
                while let Some(&c) = chars.peek() {
                    if c.is_whitespace() || matches!(c, '{' | '}' | '\'') {
                        break;
                    }
                    word.push(c);
                    chars.next();
                }
                tokens.push(Token::Word(word));
            }
        }
    }
    None
}

/// Takes each token as it reads it, so every step of every loop consumes.
struct Parser {
    tokens: Peekable<vec::IntoIter<Token>>,
    written: Written,
}

/// Where an element body is in emit's order: tags, description,
/// technology, metadata, link, children.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Field {
    Tags,
    Description,
    Technology,
    Metadata,
    Link,
    Children,
}

impl Field {
    /// Tags share a line and children follow one another; every other
    /// field comes once.
    fn repeats(self) -> bool {
        matches!(self, Field::Tags | Field::Children)
    }
}

impl Parser {
    fn next(&mut self) -> Option<Token> {
        self.tokens.next()
    }

    fn word(&mut self) -> Option<String> {
        match self.next()? {
            Token::Word(word) => Some(word),
            _ => None,
        }
    }

    fn quoted(&mut self) -> Option<String> {
        match self.next()? {
            Token::Quoted(value) => Some(value),
            _ => None,
        }
    }

    fn expect(&mut self, token: Token) -> Option<()> {
        (self.next()? == token).then_some(())
    }

    fn expect_word(&mut self, word: &str) -> Option<()> {
        (self.word()? == word).then_some(())
    }

    /// Top-level elements, then relations, then the closing brace.
    fn model_body(&mut self) -> Option<()> {
        let mut in_relations = false;
        loop {
            let first = match self.next()? {
                Token::Close => return Some(()),
                Token::Word(word) => word,
                _ => return None,
            };
            let second = self.word()?;
            if second == "=" {
                if in_relations {
                    return None;
                }
                self.element("", first)?;
            } else {
                in_relations = true;
                self.relation(first, &second)?;
            }
        }
    }

    /// After `name =`: `kind 'title'`, then the body when one follows.
    fn element(&mut self, parent: &str, name: String) -> Option<()> {
        let kind = self.word()?;
        let title = self.quoted()?;
        let id = if parent.is_empty() {
            name
        } else {
            format!("{parent}.{name}")
        };
        let mut element = WrittenElement {
            kind,
            title,
            ..Default::default()
        };
        if self.tokens.peek() == Some(&Token::Open) {
            self.next();
            self.element_body(&id, &mut element)?;
        }
        self.written
            .elements
            .insert(id, element)
            .is_none()
            .then_some(())
    }

    fn element_body(&mut self, id: &str, element: &mut WrittenElement) -> Option<()> {
        let mut at: Option<Field> = None;
        loop {
            let word = match self.next()? {
                Token::Close => return Some(()),
                Token::Word(word) => word,
                _ => return None,
            };
            let field = match word.as_str() {
                w if w.starts_with('#') => Field::Tags,
                "description" => Field::Description,
                "technology" => Field::Technology,
                "metadata" => Field::Metadata,
                // emit writes a link only right after the metadata; one
                // anywhere else falls to a child, which wants `=`.
                "link" if at == Some(Field::Metadata) => Field::Link,
                _ => Field::Children,
            };
            if at.is_some_and(|at| field < at || (field == at && !field.repeats())) {
                return None;
            }
            at = Some(field);
            match field {
                Field::Tags => element.tags.push(word[1..].to_string()),
                Field::Description => element.description = Some(self.quoted()?),
                Field::Technology => element.technology = Some(self.quoted()?),
                Field::Metadata => {
                    self.expect(Token::Open)?;
                    self.expect_word("path")?;
                    element.path = Some(self.quoted()?);
                    self.expect(Token::Close)?;
                }
                // Derived from the path and the file's place; not compared.
                Field::Link => {
                    self.word()?;
                }
                Field::Children => {
                    self.expect_word("=")?;
                    self.element(id, word)?;
                }
            }
        }
    }

    /// After `from`: `-[kind]-> to`, the label when there is one, and a
    /// technology block when there is one.
    fn relation(&mut self, from: String, arrow: &str) -> Option<()> {
        let kind = arrow.strip_prefix("-[")?.strip_suffix("]->")?.to_string();
        let to = self.word()?;
        let mut relation = WrittenRelation {
            kind,
            ..Default::default()
        };
        if matches!(self.tokens.peek(), Some(Token::Quoted(_))) {
            relation.label = Some(self.quoted()?);
        }
        if self.tokens.peek() == Some(&Token::Open) {
            self.next();
            self.expect_word("technology")?;
            relation.technology = Some(self.quoted()?);
            self.expect(Token::Close)?;
        }
        self.written
            .relations
            .insert((from, to), relation)
            .is_none()
            .then_some(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::emit::{EmitOptions, emit};
    use crate::model::{Element, ElementKind, Model, Relation, RelationKind};

    fn id(dotted: &str) -> Vec<String> {
        dotted.split('.').map(str::to_string).collect()
    }

    fn element(dotted: &str, kind: ElementKind) -> Element {
        Element {
            id: id(dotted),
            kind,
            title: dotted.rsplit('.').next().unwrap().to_string(),
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

    /// A crate with a described module, a bin, a leaf module with no body,
    /// an external with a technology, and relations with and without items.
    fn sample() -> Model {
        let mut app = element("app", ElementKind::Container);
        app.technology = Some("library crate".into());
        app.path = Some("app".into());
        let mut server = element("app.server", ElementKind::Component);
        server.description =
            Some("Serves 'quoted' text,\nover two lines, with \\ and {braces}.".into());
        server.path = Some("app/src/server.rs".into());
        let mut bin = element("app.main", ElementKind::Bin);
        bin.tags = vec!["bin".into()];
        bin.path = Some("app/src/main.rs".into());
        let leaf = element("app.leaf", ElementKind::Component);
        let mut driver = element("node_driver", ElementKind::External("process".into()));
        driver.title = "Driver process".into();
        driver.tags = vec!["external".into()];
        driver.technology = Some("Node.js".into());
        let mut spawns = relation("app.server", "node_driver", RelationKind::Uses, &["spawns"]);
        spawns.technology = Some("stdio".into());
        Model {
            elements: vec![app, server, bin, leaf, driver],
            relations: vec![
                relation(
                    "app.main",
                    "app.server",
                    RelationKind::Calls,
                    &["Server", "serve"],
                ),
                relation("app.leaf", "app.server", RelationKind::NamesType, &[]),
                spawns,
            ],
            ..Default::default()
        }
    }

    fn sample_written() -> Written {
        read(&emit(
            &sample(),
            &EmitOptions::for_output_path("docs/architecture/model.c4"),
        ))
        .unwrap()
    }

    #[test]
    fn every_element_is_read_back_under_its_full_id() {
        assert_eq!(
            sample_written().elements.keys().collect::<Vec<_>>(),
            ["app", "app.leaf", "app.main", "app.server", "node_driver"]
        );
    }

    #[test]
    fn an_element_s_fields_are_read_back_as_written() {
        assert_eq!(
            sample_written().elements["app.server"],
            WrittenElement {
                kind: "component".into(),
                title: "server".into(),
                description: Some(
                    "Serves 'quoted' text,\nover two lines, with \\ and {braces}.".into()
                ),
                technology: None,
                path: Some("app/src/server.rs".into()),
                tags: vec![],
            }
        );
    }

    #[test]
    fn a_crate_s_technology_is_read_back() {
        assert_eq!(
            sample_written().elements["app"].technology.as_deref(),
            Some("library crate")
        );
    }

    #[test]
    fn a_bin_s_tag_is_read_back() {
        assert_eq!(sample_written().elements["app.main"].tags, ["bin"]);
    }

    #[test]
    fn an_external_is_read_back_with_its_own_kind() {
        assert_eq!(
            sample_written().elements["node_driver"],
            WrittenElement {
                kind: "process".into(),
                title: "Driver process".into(),
                description: None,
                technology: Some("Node.js".into()),
                path: None,
                tags: vec!["external".into()],
            }
        );
    }

    #[test]
    fn a_leaf_with_no_body_is_read_back_with_only_its_kind_and_title() {
        assert_eq!(
            sample_written().elements["app.leaf"],
            WrittenElement {
                kind: "component".into(),
                title: "leaf".into(),
                ..Default::default()
            }
        );
    }

    fn pair(from: &str, to: &str) -> (String, String) {
        (from.to_string(), to.to_string())
    }

    #[test]
    fn a_relation_is_read_back_with_its_kind_and_label() {
        assert_eq!(
            sample_written().relations[&pair("app.main", "app.server")],
            WrittenRelation {
                kind: "calls".into(),
                label: Some("Server, serve".into()),
                technology: None,
            }
        );
    }

    #[test]
    fn a_relation_without_items_has_no_label() {
        assert_eq!(
            sample_written().relations[&pair("app.leaf", "app.server")].label,
            None
        );
    }

    #[test]
    fn a_relation_s_technology_is_read_back() {
        assert_eq!(
            sample_written().relations[&pair("app.server", "node_driver")],
            WrittenRelation {
                kind: "uses".into(),
                label: Some("spawns".into()),
                technology: Some("stdio".into()),
            }
        );
    }

    #[test]
    fn an_external_title_holding_a_comma_reads_back_whole() {
        let mut model = sample();
        model.relations[2].items = vec!["reads, writes".into()];
        let written = read(&emit(&model, &EmitOptions::for_output_path("m.c4"))).unwrap();
        assert_eq!(
            written.relations[&pair("app.server", "node_driver")]
                .label
                .as_deref(),
            Some("reads, writes")
        );
    }

    #[test]
    fn crlf_line_endings_read_the_same() {
        let text = emit(&sample(), &EmitOptions::for_output_path("m.c4"));
        assert_eq!(read(&text.replace('\n', "\r\n")), read(&text));
    }

    #[test]
    fn a_file_without_the_header_is_not_read() {
        let text = emit(&sample(), &EmitOptions::for_output_path("m.c4"));
        let edited = text.replacen("// GENERATED by asbuilt.", "// mine", 1);
        assert_eq!(read(&edited), None);
    }

    #[test]
    fn a_statement_emit_would_not_write_is_not_read() {
        let text = emit(&sample(), &EmitOptions::for_output_path("m.c4"));
        let edited = text.replacen(
            "    technology 'library crate'",
            "    style { color red }",
            1,
        );
        assert_eq!(read(&edited), None);
    }

    #[test]
    fn an_unterminated_string_is_not_read() {
        let text = emit(&sample(), &EmitOptions::for_output_path("m.c4"));
        let edited = text.replacen("'Driver process'", "'Driver process", 1);
        assert_eq!(read(&edited), None);
    }

    #[test]
    fn a_relation_written_twice_is_not_read() {
        let text = emit(&sample(), &EmitOptions::for_output_path("m.c4"));
        let line = "  app.main -[calls]-> app.server 'Server, serve'\n";
        assert_eq!(
            read(&text.replacen(line, &format!("{line}{line}"), 1)),
            None
        );
    }

    #[test]
    fn an_element_written_twice_is_not_read() {
        let text = emit(&sample(), &EmitOptions::for_output_path("m.c4"));
        let line = "    leaf = component 'leaf'\n";
        assert_eq!(
            read(&text.replacen(line, &format!("{line}{line}"), 1)),
            None
        );
    }

    fn edited(from: &str, to: &str) -> Option<Written> {
        let text = emit(&sample(), &EmitOptions::for_output_path("m.c4"));
        assert!(text.contains(from), "{from:?} not in\n{text}");
        read(&text.replacen(from, to, 1))
    }

    #[test]
    fn a_header_line_with_more_on_it_is_not_read() {
        assert_eq!(edited("survey.\n", "survey. And mine.\n"), None);
    }

    #[test]
    fn fields_out_of_emit_s_order_are_not_read() {
        assert_eq!(
            edited(
                "    technology 'library crate'\n    metadata {\n      path 'app'\n    }\n",
                "    metadata {\n      path 'app'\n    }\n    technology 'library crate'\n"
            ),
            None
        );
    }

    #[test]
    fn a_field_written_twice_is_not_read() {
        assert_eq!(
            edited(
                "    technology 'library crate'\n",
                "    technology 'library crate'\n    technology 'binary crate'\n"
            ),
            None
        );
    }

    #[test]
    fn a_link_without_the_metadata_before_it_is_not_read() {
        assert_eq!(
            edited("    metadata {\n      path 'app'\n    }\n", ""),
            None
        );
    }

    #[test]
    fn an_element_after_a_relation_is_not_read() {
        assert_eq!(
            edited(
                "  app.main -[calls]-> ",
                "  extra = component 'extra'\n  app.main -[calls]-> "
            ),
            None
        );
    }

    #[test]
    fn a_relation_to_an_element_the_file_does_not_declare_is_not_read() {
        // An orphan: emit writes `a.b` at the top level as `b`, while the
        // relation names `a.b`.
        let mut model = sample();
        model.elements.push(element("a.b", ElementKind::Component));
        model
            .relations
            .push(relation("app.leaf", "a.b", RelationKind::Uses, &[]));
        assert_eq!(
            read(&emit(&model, &EmitOptions::for_output_path("m.c4"))),
            None
        );
    }
}
