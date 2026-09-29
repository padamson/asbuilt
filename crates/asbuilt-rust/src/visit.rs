//! What one parsed file references, per module: every `use`, every
//! path in a type, expression or pattern, every `impl Trait for`, and
//! the names the module defines. Pure over a `syn::File`; the tests
//! parse strings. A macro invocation's body is parsed as Rust where it
//! is Rust (an expression, a comma-separated list of them, or items), so
//! `fuzz_target!`, `assert_eq!` and `vec!` yield their references; a
//! body in another syntax (Leptos `view!`, `macro_rules!` arms) yields
//! only the macro's own path. Method calls on values are not paths and
//! produce no reference.

use std::collections::{BTreeMap, BTreeSet};

use asbuilt_core::RelationKind;
use syn::punctuated::Punctuated;
use syn::visit::{self, Visit};
use syn::{Expr, Item, ItemMod, Token, UseTree, Visibility};

use crate::walk::{ModulePath, is_cfg_test};

/// How a path starts.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Anchor {
    /// `crate::`
    Crate,
    /// `self::`
    SelfMod,
    /// `super::` this many times
    Super(usize),
    /// `::x::` (an explicit extern crate)
    Extern,
    /// a plain identifier
    Bare,
}

/// A path as written, minus its anchor tokens.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct RawPath {
    pub anchor: Anchor,
    pub segments: Vec<String>,
}

impl RawPath {
    /// From the identifiers of a path. `None` for a `Self::` path (it
    /// names the enclosing impl's type, a local) and for an empty path.
    pub fn from_segments(leading_colon: bool, segments: Vec<String>) -> Option<Self> {
        if leading_colon {
            return (!segments.is_empty()).then_some(RawPath {
                anchor: Anchor::Extern,
                segments,
            });
        }
        let mut segments = segments.into_iter().peekable();
        let first = segments.peek()?.clone();
        let anchor = match first.as_str() {
            "crate" => {
                segments.next();
                Anchor::Crate
            }
            "self" => {
                segments.next();
                Anchor::SelfMod
            }
            "super" => {
                let mut n = 0;
                while segments.peek().is_some_and(|s| s == "super") {
                    segments.next();
                    n += 1;
                }
                Anchor::Super(n)
            }
            "Self" => return None,
            _ => Anchor::Bare,
        };
        Some(RawPath {
            anchor,
            segments: segments.collect(),
        })
    }

    /// From `crate::a::B`-style text; the test-side spelling.
    pub fn parse(text: &str) -> Option<Self> {
        let leading = text.starts_with("::");
        let segments: Vec<String> = text
            .trim_start_matches("::")
            .split("::")
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect();
        Self::from_segments(leading, segments)
    }

    fn from_syn(path: &syn::Path) -> Option<Self> {
        Self::from_segments(
            path.leading_colon.is_some(),
            path.segments.iter().map(|s| s.ident.to_string()).collect(),
        )
    }
}

/// One name a `use` brings into scope (or, for a glob, a module whose
/// names it brings in).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UseEntry {
    pub path: RawPath,
    /// The local name; for a glob, the last segment of the path.
    pub alias: String,
    pub glob: bool,
    /// Any visibility but the default: this module re-exports the name.
    pub reexport: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawRef {
    pub path: RawPath,
    pub kind: RelationKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ModuleFacts {
    /// Items this module defines, by name.
    pub defines: BTreeSet<String>,
    pub uses: Vec<UseEntry>,
    pub refs: Vec<RawRef>,
}

/// Associated functions treated as construction when called on a type.
pub const CONSTRUCTOR_NAMES: &[&str] = &["builder", "default", "from", "new", "try_new"];
const CONSTRUCTOR_PREFIXES: &[&str] = &["from_", "new_", "with_"];

/// Whether `Type::name()` constructs a `Type`.
pub fn is_constructor_name(name: &str) -> bool {
    CONSTRUCTOR_NAMES.contains(&name) || CONSTRUCTOR_PREFIXES.iter().any(|p| name.starts_with(p))
}

/// The kind for a call of `path(...)`: `Type::new()` constructs,
/// `Type::parse()` and `module::f()` call.
fn call_kind(path: &syn::Path) -> RelationKind {
    let n = path.segments.len();
    if n >= 2 {
        let type_name = path.segments[n - 2].ident.to_string();
        let f = path.segments[n - 1].ident.to_string();
        if type_name.chars().next().is_some_and(char::is_uppercase) && is_constructor_name(&f) {
            return RelationKind::Constructs;
        }
    }
    RelationKind::Calls
}

struct Collector {
    module: ModulePath,
    facts: BTreeMap<ModulePath, ModuleFacts>,
}

impl Collector {
    fn here(&mut self) -> &mut ModuleFacts {
        self.facts.entry(self.module.clone()).or_default()
    }

    fn record(&mut self, path: &syn::Path, kind: RelationKind) {
        if let Some(path) = RawPath::from_syn(path) {
            self.here().refs.push(RawRef { path, kind });
        }
    }

    fn define(&mut self, ident: &syn::Ident) {
        self.here().defines.insert(ident.to_string());
    }
}

fn item_attrs(item: &Item) -> &[syn::Attribute] {
    match item {
        Item::Const(i) => &i.attrs,
        Item::Enum(i) => &i.attrs,
        Item::ExternCrate(i) => &i.attrs,
        Item::Fn(i) => &i.attrs,
        Item::ForeignMod(i) => &i.attrs,
        Item::Impl(i) => &i.attrs,
        Item::Macro(i) => &i.attrs,
        Item::Mod(i) => &i.attrs,
        Item::Static(i) => &i.attrs,
        Item::Struct(i) => &i.attrs,
        Item::Trait(i) => &i.attrs,
        Item::TraitAlias(i) => &i.attrs,
        Item::Type(i) => &i.attrs,
        Item::Union(i) => &i.attrs,
        Item::Use(i) => &i.attrs,
        _ => &[],
    }
}

/// Expand a use tree into `(segments, alias, glob)` triples.
fn expand_use(tree: &UseTree, prefix: Vec<String>, out: &mut Vec<(Vec<String>, String, bool)>) {
    match tree {
        UseTree::Path(p) => {
            let mut prefix = prefix;
            prefix.push(p.ident.to_string());
            expand_use(&p.tree, prefix, out);
        }
        UseTree::Name(n) => {
            let ident = n.ident.to_string();
            if ident == "self" {
                let alias = prefix.last().cloned().unwrap_or_default();
                out.push((prefix, alias, false));
            } else {
                let mut path = prefix;
                path.push(ident.clone());
                out.push((path, ident, false));
            }
        }
        UseTree::Rename(r) => {
            let ident = r.ident.to_string();
            let mut path = prefix;
            if ident != "self" {
                path.push(ident);
            }
            out.push((path, r.rename.to_string(), false));
        }
        UseTree::Glob(_) => {
            let alias = prefix.last().cloned().unwrap_or_default();
            out.push((prefix, alias, true));
        }
        UseTree::Group(g) => {
            for item in &g.items {
                expand_use(item, prefix.clone(), out);
            }
        }
    }
}

impl<'ast> Visit<'ast> for Collector {
    fn visit_item(&mut self, item: &'ast Item) {
        if is_cfg_test(item_attrs(item)) {
            return;
        }
        match item {
            Item::Struct(i) => self.define(&i.ident),
            Item::Enum(i) => self.define(&i.ident),
            Item::Union(i) => self.define(&i.ident),
            Item::Trait(i) => self.define(&i.ident),
            Item::TraitAlias(i) => self.define(&i.ident),
            Item::Type(i) => self.define(&i.ident),
            Item::Fn(i) => self.define(&i.sig.ident),
            Item::Const(i) => self.define(&i.ident),
            Item::Static(i) => self.define(&i.ident),
            Item::Macro(i) => {
                if let Some(ident) = &i.ident {
                    self.define(ident);
                }
            }
            _ => {}
        }
        visit::visit_item(self, item);
    }

    fn visit_item_mod(&mut self, m: &'ast ItemMod) {
        // `visit_item` already dropped `#[cfg(test)]` modules.
        let Some((_, items)) = &m.content else { return };
        self.module.push(m.ident.to_string());
        self.here();
        for item in items {
            self.visit_item(item);
        }
        self.module.pop();
    }

    fn visit_item_use(&mut self, u: &'ast syn::ItemUse) {
        let reexport = !matches!(u.vis, Visibility::Inherited);
        let mut entries = Vec::new();
        expand_use(&u.tree, Vec::new(), &mut entries);
        for (segments, alias, glob) in entries {
            if let Some(path) = RawPath::from_segments(u.leading_colon.is_some(), segments) {
                self.here().uses.push(UseEntry {
                    path,
                    alias,
                    glob,
                    reexport,
                });
            }
        }
    }

    // Each of the three below records its path once with the strong
    // kind and then walks only the rest of the node, so the same path
    // is not also recorded as a type mention by `visit_path`.

    /// `extern crate x as y;` brings `y` into scope as the crate `x`.
    fn visit_item_extern_crate(&mut self, e: &'ast syn::ItemExternCrate) {
        let alias = e
            .rename
            .as_ref()
            .map(|(_, ident)| ident.to_string())
            .unwrap_or_else(|| e.ident.to_string());
        self.here().uses.push(UseEntry {
            path: RawPath {
                anchor: Anchor::Bare,
                segments: vec![e.ident.to_string()],
            },
            alias,
            glob: false,
            reexport: !matches!(e.vis, Visibility::Inherited),
        });
    }

    fn visit_item_impl(&mut self, i: &'ast syn::ItemImpl) {
        if let Some((trait_path, _)) = &i.trait_ {
            self.record(trait_path, RelationKind::Implements);
            self.visit_generics(&i.generics);
            self.visit_type(&i.self_ty);
            for item in &i.items {
                self.visit_impl_item(item);
            }
            return;
        }
        visit::visit_item_impl(self, i);
    }

    fn visit_expr_struct(&mut self, e: &'ast syn::ExprStruct) {
        self.record(&e.path, RelationKind::Constructs);
        if let Some(qself) = &e.qself {
            self.visit_qself(qself);
        }
        for field in &e.fields {
            self.visit_field_value(field);
        }
        if let Some(rest) = &e.rest {
            self.visit_expr(rest);
        }
    }

    fn visit_expr_call(&mut self, e: &'ast syn::ExprCall) {
        if let Expr::Path(p) = &*e.func {
            self.record(&p.path, call_kind(&p.path));
            if let Some(qself) = &p.qself {
                self.visit_qself(qself);
            }
            for arg in &e.args {
                self.visit_expr(arg);
            }
            return;
        }
        visit::visit_expr_call(self, e);
    }

    fn visit_path(&mut self, p: &'ast syn::Path) {
        self.record(p, RelationKind::NamesType);
        visit::visit_path(self, p);
    }

    /// The macro's path is a reference; its body is walked when it
    /// parses as an expression, as comma-separated expressions, or as
    /// items, in that order. `Collector` stores nothing borrowed, so the
    /// parsed body can be walked with its own shorter lifetime.
    fn visit_macro(&mut self, m: &'ast syn::Macro) {
        self.visit_path(&m.path);
        if m.path.is_ident("macro_rules") {
            return;
        }
        if let Ok(expr) = m.parse_body::<Expr>() {
            Visit::visit_expr(self, &expr);
        } else if let Ok(exprs) = m.parse_body_with(Punctuated::<Expr, Token![,]>::parse_terminated)
        {
            for expr in &exprs {
                Visit::visit_expr(self, expr);
            }
        } else if let Ok(file) = m.parse_body::<syn::File>() {
            for item in &file.items {
                Visit::visit_item(self, item);
            }
        }
    }

    /// `#[derive(a::B)]` names each listed path; any other attribute
    /// names its own path (an attribute macro from another crate is a
    /// reference; `cfg`, `doc` and friends are single segments the
    /// aggregation drops). Attribute arguments are tokens and are not
    /// walked.
    fn visit_attribute(&mut self, a: &'ast syn::Attribute) {
        if a.path().is_ident("derive") {
            if let Ok(paths) =
                a.parse_args_with(Punctuated::<syn::Path, Token![,]>::parse_terminated)
            {
                for path in &paths {
                    self.record(path, RelationKind::NamesType);
                }
            }
            return;
        }
        self.record(a.path(), RelationKind::NamesType);
    }
}

/// The facts of every module whose items live in `file`, keyed by
/// module path; `module` is the path of the file's top-level items.
pub fn collect(file: &syn::File, module: &ModulePath) -> BTreeMap<ModulePath, ModuleFacts> {
    let mut c = Collector {
        module: module.clone(),
        facts: BTreeMap::new(),
    };
    c.here();
    for item in &file.items {
        c.visit_item(item);
    }
    c.facts
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts(src: &str) -> ModuleFacts {
        let file = syn::parse_file(src).unwrap();
        collect(&file, &vec![]).remove(&vec![]).unwrap()
    }

    fn refs(src: &str) -> Vec<(String, RelationKind)> {
        facts(src)
            .refs
            .into_iter()
            .map(|r| {
                (
                    format!("{:?}:{}", r.path.anchor, r.path.segments.join("::")),
                    r.kind,
                )
            })
            .collect()
    }

    fn uses(src: &str) -> Vec<(String, String, bool, bool)> {
        facts(src)
            .uses
            .into_iter()
            .map(|u| {
                (
                    format!("{:?}:{}", u.path.anchor, u.path.segments.join("::")),
                    u.alias,
                    u.glob,
                    u.reexport,
                )
            })
            .collect()
    }

    #[test]
    fn a_use_tree_expands_to_named_imports_renames_and_globs() {
        assert_eq!(
            uses("use a::{b::{C, D as E}, *};"),
            [
                ("Bare:a::b::C".to_string(), "C".to_string(), false, false),
                ("Bare:a::b::D".to_string(), "E".to_string(), false, false),
                ("Bare:a".to_string(), "a".to_string(), true, false),
            ]
        );
    }

    #[test]
    fn a_self_in_a_use_group_names_the_module_itself() {
        assert_eq!(
            uses("use crate::protocol::{self, Page};"),
            [
                (
                    "Crate:protocol".to_string(),
                    "protocol".to_string(),
                    false,
                    false
                ),
                (
                    "Crate:protocol::Page".to_string(),
                    "Page".to_string(),
                    false,
                    false
                ),
            ]
        );
    }

    #[test]
    fn a_pub_use_is_a_reexport_and_a_plain_use_is_not() {
        assert_eq!(
            uses("pub use a::B; pub(crate) use a::C; use a::D;")
                .iter()
                .map(|u| u.3)
                .collect::<Vec<_>>(),
            [true, true, false]
        );
    }

    #[test]
    fn a_use_inside_a_function_body_is_collected() {
        assert_eq!(
            uses("fn f() { use crate::server::object_factory::create_object; }").len(),
            1
        );
    }

    #[test]
    fn a_whole_crate_reexport_is_a_one_segment_bare_use() {
        assert_eq!(
            uses("pub use playwright_rs_trace as trace;"),
            [(
                "Bare:playwright_rs_trace".to_string(),
                "trace".to_string(),
                false,
                true
            )]
        );
    }

    #[test]
    fn an_impl_of_a_trait_records_implements_for_the_trait_path() {
        let r = refs("impl crate::util::Render for crate::a::Deep {}");
        assert!(
            r.contains(&("Crate:util::Render".to_string(), RelationKind::Implements)),
            "{r:?}"
        );
        assert!(
            r.contains(&("Crate:a::Deep".to_string(), RelationKind::NamesType)),
            "{r:?}"
        );
    }

    #[test]
    fn a_struct_literal_constructs() {
        let r = refs("fn f() { crate::a::Thing { x: 1 }; }");
        assert!(
            r.contains(&("Crate:a::Thing".to_string(), RelationKind::Constructs)),
            "{r:?}"
        );
    }

    #[test]
    fn a_constructor_call_on_a_type_constructs_and_another_associated_call_calls() {
        let r = refs(
            "fn f() { crate::a::Thing::new(); crate::a::Thing::parse(); Thing::with_capacity(1); }",
        );
        assert!(
            r.contains(&("Crate:a::Thing::new".to_string(), RelationKind::Constructs)),
            "{r:?}"
        );
        assert!(
            r.contains(&("Crate:a::Thing::parse".to_string(), RelationKind::Calls)),
            "{r:?}"
        );
        assert!(
            r.contains(&(
                "Bare:Thing::with_capacity".to_string(),
                RelationKind::Constructs
            )),
            "{r:?}"
        );
    }

    #[test]
    fn a_call_of_a_module_function_calls() {
        let r = refs("fn f() { crate::a::make(); }");
        assert!(
            r.contains(&("Crate:a::make".to_string(), RelationKind::Calls)),
            "{r:?}"
        );
    }

    #[test]
    fn a_method_call_on_a_value_records_nothing_for_the_method() {
        let r = refs("fn f(c: crate::a::Conn) { c.send(); }");
        assert!(
            r.contains(&("Crate:a::Conn".to_string(), RelationKind::NamesType)),
            "{r:?}"
        );
        assert!(!r.iter().any(|(p, _)| p.contains("send")), "{r:?}");
    }

    #[test]
    fn a_path_is_recorded_once_with_its_strongest_kind() {
        let r = refs("fn f() { crate::a::b(); crate::a::S { x: 1 }; }\nimpl crate::t::T for S {}");
        assert_eq!(r.iter().filter(|(p, _)| p == "Crate:a::b").count(), 1);
        assert_eq!(r.iter().filter(|(p, _)| p == "Crate:a::S").count(), 1);
        assert_eq!(r.iter().filter(|(p, _)| p == "Crate:t::T").count(), 1);
    }

    #[test]
    fn an_attribute_macro_path_is_a_reference_and_its_arguments_are_not_walked() {
        let r = refs("#[my_macros::locator(crate::a::B)] fn f() {}");
        assert!(
            r.contains(&(
                "Bare:my_macros::locator".to_string(),
                RelationKind::NamesType
            )),
            "{r:?}"
        );
        assert!(!r.iter().any(|(p, _)| p == "Crate:a::B"), "{r:?}");
    }

    #[test]
    fn paths_in_types_patterns_and_generic_arguments_name_types() {
        let r = refs("fn f(v: Vec<crate::a::B>) { match v { crate::a::C::D => {} } }");
        assert!(
            r.contains(&("Crate:a::B".to_string(), RelationKind::NamesType)),
            "{r:?}"
        );
        assert!(
            r.contains(&("Crate:a::C::D".to_string(), RelationKind::NamesType)),
            "{r:?}"
        );
        assert!(
            r.contains(&("Bare:Vec".to_string(), RelationKind::NamesType)),
            "{r:?}"
        );
    }

    #[test]
    fn a_cfg_test_module_and_a_cfg_test_item_are_skipped() {
        let r = refs(
            "#[cfg(test)] mod tests { use crate::a::B; }\n#[cfg(test)] fn t() { crate::a::b(); }\nfn kept() { crate::c::d(); }",
        );
        assert_eq!(r, [("Crate:c::d".to_string(), RelationKind::Calls)]);
        assert!(
            facts("#[cfg(test)] mod tests { use crate::a::B; }")
                .uses
                .is_empty()
        );
    }

    #[test]
    fn an_extern_crate_declaration_is_an_import_of_that_crate_under_its_alias() {
        assert_eq!(
            uses("extern crate other as o; pub extern crate third;"),
            [
                ("Bare:other".to_string(), "o".to_string(), false, false),
                ("Bare:third".to_string(), "third".to_string(), false, true),
            ]
        );
    }

    #[test]
    fn a_cfg_test_item_of_every_kind_leaves_no_trace() {
        let items = [
            "const C: u8 = 0;",
            "enum E {}",
            "extern crate foo;",
            "fn f() { crate::a::b(); }",
            "extern \"C\" { fn g(t: crate::a::T); }",
            "impl crate::t::T for S {}",
            "macro_rules! m { () => {} }",
            "mod tests { use crate::a::B; }",
            "static G: u8 = 0;",
            "struct S;",
            "trait T {}",
            "trait TA = T;",
            "type A = u8;",
            "union U { x: u8 }",
            "use crate::a::B;",
        ];
        for item in items {
            let file = syn::parse_file(&format!("#[cfg(test)] {item}")).unwrap();
            let all = collect(&file, &vec![]);
            assert_eq!(all.len(), 1, "{item}: {all:?}");
            assert_eq!(all[&vec![]], ModuleFacts::default(), "{item}");
        }
    }

    #[test]
    fn references_inside_an_inline_module_belong_to_it() {
        let file = syn::parse_file("mod inner { fn f() { crate::a::b(); } }").unwrap();
        let all = collect(&file, &vec!["outer".to_string()]);
        let inner = &all[&vec!["outer".to_string(), "inner".to_string()]];
        assert_eq!(inner.refs.len(), 1);
        assert!(all[&vec!["outer".to_string()]].refs.is_empty());
    }

    #[test]
    fn every_item_kind_with_a_name_is_a_definition() {
        let f = facts(
            "struct S; enum E {} union U { x: u8 } trait T {} trait TA = T; type A = u8; fn f() {} const C: u8 = 0; static G: u8 = 0; macro_rules! m { () => {} }",
        );
        let names: Vec<&str> = f.defines.iter().map(String::as_str).collect();
        assert_eq!(names, ["A", "C", "E", "G", "S", "T", "TA", "U", "f", "m"]);
    }

    #[test]
    fn anchors_are_recognized_and_self_type_paths_are_dropped() {
        let r = refs(
            "fn f() { super::super::x::Y; self::x::Y; ::std::x::Y; Self::new(); crate::a::B; }",
        );
        let anchors: Vec<&str> = r
            .iter()
            .map(|(p, _)| p.split(':').next().unwrap())
            .collect();
        assert_eq!(anchors, ["Super(2)", "SelfMod", "Extern", "Crate"]);
    }

    #[test]
    fn a_derive_path_is_a_reference() {
        let r = refs("#[derive(my_macros::Thing)] struct S;");
        assert!(
            r.contains(&("Bare:my_macros::Thing".to_string(), RelationKind::NamesType)),
            "{r:?}"
        );
    }

    #[test]
    fn a_macro_body_that_is_an_expression_yields_its_references_and_the_macro_path() {
        let r = refs("fn f() { fuzz_target!(|data: &[u8]| { crate::a::B::new(data); }); }");
        assert!(
            r.contains(&("Bare:fuzz_target".to_string(), RelationKind::NamesType)),
            "{r:?}"
        );
        assert!(
            r.contains(&("Crate:a::B::new".to_string(), RelationKind::Constructs)),
            "{r:?}"
        );
    }

    #[test]
    fn a_macro_body_that_is_a_comma_list_yields_each_argument_s_references() {
        let r = refs("fn f() { assert_eq!(crate::a::B::X, other::Y, \"{}\", z); }");
        assert!(r.iter().any(|(p, _)| p == "Crate:a::B::X"), "{r:?}");
        assert!(r.iter().any(|(p, _)| p == "Bare:other::Y"), "{r:?}");
    }

    #[test]
    fn a_macro_body_that_is_items_yields_their_references() {
        let r = refs("m! { impl crate::t::Trait for S {} }");
        assert!(
            r.contains(&("Crate:t::Trait".to_string(), RelationKind::Implements)),
            "{r:?}"
        );
    }

    #[test]
    fn a_macro_body_in_another_syntax_yields_only_the_macro_path() {
        let r = refs("fn f() { view! { <div class=\"x\">{crate::a::B}</div> }; }");
        assert_eq!(r, [("Bare:view".to_string(), RelationKind::NamesType)]);
        let r = refs("macro_rules! m { ($x:expr) => { crate::a::B::new($x) }; }");
        assert!(!r.iter().any(|(p, _)| p == "Crate:a::B::new"), "{r:?}");
    }

    #[test]
    fn raw_path_parse_matches_from_segments() {
        assert_eq!(
            RawPath::parse("super::super::x").unwrap().anchor,
            Anchor::Super(2)
        );
        assert_eq!(
            RawPath::parse("::pw::Page").unwrap(),
            RawPath {
                anchor: Anchor::Extern,
                segments: vec!["pw".into(), "Page".into()]
            }
        );
        assert_eq!(RawPath::parse("Self::x"), None);
        assert_eq!(RawPath::parse(""), None);
        assert_eq!(
            RawPath::parse("crate").unwrap(),
            RawPath {
                anchor: Anchor::Crate,
                segments: vec![]
            }
        );
    }

    #[test]
    fn constructor_names_are_the_listed_ones_and_the_listed_prefixes() {
        for name in [
            "new",
            "default",
            "builder",
            "from",
            "try_new",
            "new_pair",
            "from_str",
            "with_capacity",
        ] {
            assert!(is_constructor_name(name), "{name}");
        }
        for name in ["parse", "newest", "fromage", "build"] {
            assert!(!is_constructor_name(name), "{name}");
        }
    }
}
