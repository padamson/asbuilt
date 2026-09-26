//! Path resolution over an in-memory tree of targets and modules: which
//! module, and which item in it, a path written in some module refers
//! to. Pure functions; the tests build trees by hand. Re-exports are
//! followed by substitution to a fixed point, globs are consulted when
//! a direct lookup fails, and a path that stops at a segment that is
//! neither module nor re-export is attributed to the module reached
//! (the lenient stop), which is what keeps derive-generated names
//! resolvable.

use std::collections::{BTreeMap, BTreeSet};

use crate::visit::{Anchor, RawPath};
use crate::walk::ModulePath;

/// One compilation target: its own `crate::` namespace.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct TargetKey {
    pub crate_name: String,
    pub role: TargetRole,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum TargetRole {
    /// The lib (or proc-macro) target, or a bin-only package's `main.rs`.
    Main,
    /// A bin target beside a lib.
    Bin(String),
    /// A test or bench target.
    Tests(String),
    Examples(String),
}

impl TargetKey {
    pub fn main(crate_name: &str) -> Self {
        Self {
            crate_name: crate_name.to_string(),
            role: TargetRole::Main,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ResolveModule {
    pub children: BTreeSet<String>,
    pub defines: BTreeSet<String>,
    /// Every non-glob `use`, by local name.
    pub imports: BTreeMap<String, RawPath>,
    /// The subset of `imports` that is `pub` in some form.
    pub reexports: BTreeSet<String>,
    /// Glob imports in declaration order, with whether each is `pub`.
    pub globs: Vec<(RawPath, bool)>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ResolveTree {
    pub targets: BTreeMap<TargetKey, BTreeMap<ModulePath, ResolveModule>>,
    /// Crate names a path may start with, to the target they mean.
    pub by_crate_name: BTreeMap<String, TargetKey>,
}

/// A module within a target.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Location {
    pub target: TargetKey,
    pub module: ModulePath,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    pub module: Location,
    /// `None` when the path ends on the module itself.
    pub item: Option<String>,
}

/// Re-export hops before giving up. Real chains are a few hops; a cycle
/// does not compile; this only bounds a tree built from broken input.
pub const MAX_REEXPORT_HOPS: usize = 32;
const MAX_ALIAS_DEPTH: usize = 8;
const MAX_GLOB_DEPTH: usize = 8;

impl ResolveTree {
    /// Add a target with an empty root module. `nameable` registers the
    /// crate name for paths from other crates.
    pub fn add_target(&mut self, key: TargetKey, nameable: bool) {
        self.targets
            .entry(key.clone())
            .or_default()
            .entry(vec![])
            .or_default();
        if nameable {
            self.by_crate_name.insert(key.crate_name.clone(), key);
        }
    }

    /// The module at `path` in `target`, creating it and every missing
    /// ancestor (each registered as a child of its parent).
    pub fn module_mut(&mut self, target: &TargetKey, path: &[String]) -> &mut ResolveModule {
        let modules = self.targets.entry(target.clone()).or_default();
        for depth in 0..path.len() {
            modules
                .entry(path[..depth].to_vec())
                .or_default()
                .children
                .insert(path[depth].clone());
        }
        modules.entry(path.to_vec()).or_default()
    }

    pub fn module(&self, loc: &Location) -> Option<&ResolveModule> {
        self.targets.get(&loc.target)?.get(&loc.module)
    }
}

/// Where a path written in `from` starts, as an absolute
/// `(target, segments)`, or `None` for anything the model does not
/// cover (`std`, external crates, locals, generics).
///
/// `alias_depth` bounds import-alias chains within one module;
/// `glob_depth` is the glob recursion depth carried through any glob
/// lookup this anchor needs.
fn anchor_at(
    tree: &ResolveTree,
    from: &Location,
    path: &RawPath,
    alias_depth: usize,
    glob_depth: usize,
) -> Option<(TargetKey, Vec<String>)> {
    let segs = &path.segments;
    match &path.anchor {
        Anchor::Crate => Some((from.target.clone(), segs.clone())),
        Anchor::SelfMod => Some((from.target.clone(), [from.module.as_slice(), segs].concat())),
        Anchor::Super(n) => {
            if from.module.len() < *n {
                return None;
            }
            let base = &from.module[..from.module.len() - n];
            Some((from.target.clone(), [base, segs].concat()))
        }
        Anchor::Extern => {
            let key = tree.by_crate_name.get(segs.first()?)?;
            Some((key.clone(), segs[1..].to_vec()))
        }
        Anchor::Bare => {
            let first = segs.first()?;
            let module = tree.module(from)?;
            if let Some(target_path) = module.imports.get(first) {
                if alias_depth >= MAX_ALIAS_DEPTH {
                    return None;
                }
                let (target, mut resolved) =
                    anchor_at(tree, from, target_path, alias_depth + 1, glob_depth)?;
                resolved.extend(segs[1..].iter().cloned());
                return Some((target, resolved));
            }
            if module.children.contains(first) || module.defines.contains(first) {
                return Some((from.target.clone(), [from.module.as_slice(), segs].concat()));
            }
            // A name a glob import of this module brings in; globs
            // shadow the extern prelude, so this comes before crate names.
            if let Some(provider) = via_globs(tree, from, first, from, glob_depth) {
                return Some((provider.target, [provider.module.as_slice(), segs].concat()));
            }
            let key = tree.by_crate_name.get(first)?;
            Some((key.clone(), segs[1..].to_vec()))
        }
    }
}

/// Which module reachable through `at`'s globs provides `name`, if any.
fn via_globs(
    tree: &ResolveTree,
    at: &Location,
    name: &str,
    visible_from: &Location,
    depth: usize,
) -> Option<Location> {
    if depth >= MAX_GLOB_DEPTH {
        return None;
    }
    let module = tree.module(at)?;
    for (glob, is_pub) in &module.globs {
        let own = at == visible_from;
        if !(*is_pub || own) {
            continue;
        }
        let Some(target) = resolve_at(tree, at, glob, depth + 1) else {
            continue;
        };
        if target.item.is_some() {
            continue;
        }
        let provider = target.module;
        let Some(pm) = tree.module(&provider) else {
            continue;
        };
        if pm.defines.contains(name) || pm.reexports.contains(name) || pm.children.contains(name) {
            return Some(provider);
        }
        if let Some(deeper) = via_globs(tree, &provider, name, visible_from, depth + 1) {
            return Some(deeper);
        }
    }
    None
}

/// The module and item a path written in `from` names.
pub fn resolve(tree: &ResolveTree, from: &Location, path: &RawPath) -> Option<Resolved> {
    resolve_at(tree, from, path, 0)
}

fn resolve_at(
    tree: &ResolveTree,
    from: &Location,
    path: &RawPath,
    glob_depth: usize,
) -> Option<Resolved> {
    let (mut target, mut segs) = anchor_at(tree, from, path, 0, glob_depth)?;
    // One pass to walk the path, plus one per re-export or glob hop.
    for _ in 0..=MAX_REEXPORT_HOPS {
        let modules = tree.targets.get(&target)?;
        let mut here: ModulePath = Vec::new();
        let mut k = 0;
        while k < segs.len() && modules.get(&here)?.children.contains(&segs[k]) {
            here.push(segs[k].clone());
            k += 1;
        }
        let location = Location {
            target: target.clone(),
            module: here.clone(),
        };
        if k == segs.len() {
            return Some(Resolved {
                module: location,
                item: None,
            });
        }
        let node = modules.get(&here)?;
        let name = segs[k].clone();
        let rest = segs[k + 1..].to_vec();

        // A re-export, or a private import seen from inside the module
        // that made it (`super::Y` from a child).
        let visible_privately = target == from.target && from.module.starts_with(&here);
        if let Some(import) = node.imports.get(&name)
            && (node.reexports.contains(&name) || visible_privately)
        {
            let (t, mut s) = anchor_at(tree, &location, import, 0, glob_depth)?;
            s.extend(rest);
            target = t;
            segs = s;
            continue;
        }
        if node.defines.contains(&name) {
            return Some(Resolved {
                module: location,
                item: Some(name),
            });
        }
        if let Some(provider) = via_globs(tree, &location, &name, from, glob_depth) {
            let mut s = provider.module.clone();
            s.push(name);
            s.extend(rest);
            target = provider.target;
            segs = s;
            continue;
        }
        // The lenient stop: attributed to the module reached.
        return Some(Resolved {
            module: location,
            item: Some(name),
        });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> RawPath {
        RawPath::parse(s).unwrap()
    }

    fn m(s: &str) -> ModulePath {
        if s.is_empty() {
            vec![]
        } else {
            s.split('.').map(str::to_string).collect()
        }
    }

    fn at(krate: &str, module: &str) -> Location {
        Location {
            target: TargetKey::main(krate),
            module: m(module),
        }
    }

    /// `app` with `protocol` (re-exporting `page::Page` and renaming
    /// `page::Frame` to `PageFrame`), `protocol.page` (defining `Page`
    /// and `Frame`), `consumer` (importing `crate::Page`), root
    /// re-exporting `protocol::Page`; and `other` importing `app`.
    fn playwright_like() -> ResolveTree {
        let mut t = ResolveTree::default();
        let app = TargetKey::main("app");
        t.add_target(app.clone(), true);
        t.add_target(TargetKey::main("other"), true);
        let root = t.module_mut(&app, &[]);
        root.imports.insert("Page".into(), p("protocol::Page"));
        root.reexports.insert("Page".into());
        let protocol = t.module_mut(&app, &m("protocol"));
        protocol.imports.insert("Page".into(), p("page::Page"));
        protocol
            .imports
            .insert("PageFrame".into(), p("page::Frame"));
        protocol
            .reexports
            .extend(["Page".to_string(), "PageFrame".to_string()]);
        let page = t.module_mut(&app, &m("protocol.page"));
        page.defines
            .extend(["Page".to_string(), "Frame".to_string()]);
        let consumer = t.module_mut(&app, &m("consumer"));
        consumer.imports.insert("Page".into(), p("crate::Page"));
        consumer
            .imports
            .insert("protocol".into(), p("crate::protocol"));
        t
    }

    fn resolved(
        tree: &ResolveTree,
        from: &Location,
        path: &str,
    ) -> Option<(String, String, Option<String>)> {
        resolve(tree, from, &p(path)).map(|r| {
            (
                r.module.target.crate_name,
                r.module.module.join("."),
                r.item,
            )
        })
    }

    #[test]
    fn a_crate_path_is_absolute_in_the_writing_crate() {
        let t = playwright_like();
        assert_eq!(
            resolved(&t, &at("app", "consumer"), "crate::protocol::page::Frame"),
            Some(("app".into(), "protocol.page".into(), Some("Frame".into())))
        );
    }

    #[test]
    fn a_self_path_is_relative_to_the_writing_module() {
        let t = playwright_like();
        assert_eq!(
            resolved(&t, &at("app", "protocol"), "self::page::Frame"),
            Some(("app".into(), "protocol.page".into(), Some("Frame".into())))
        );
    }

    #[test]
    fn super_climbs_one_module_per_repetition() {
        let t = playwright_like();
        assert_eq!(
            resolved(&t, &at("app", "protocol.page"), "super::super::consumer"),
            Some(("app".into(), "consumer".into(), None))
        );
        assert_eq!(
            resolved(&t, &at("app", "protocol.page"), "super::PageFrame"),
            Some(("app".into(), "protocol.page".into(), Some("Frame".into())))
        );
    }

    #[test]
    fn more_supers_than_depth_is_nothing() {
        let t = playwright_like();
        assert_eq!(
            resolved(&t, &at("app", "protocol"), "super::super::x"),
            None
        );
    }

    #[test]
    fn a_crate_name_first_segment_enters_that_crate() {
        let t = playwright_like();
        assert_eq!(
            resolved(&t, &at("other", ""), "app::protocol::page::Page"),
            Some(("app".into(), "protocol.page".into(), Some("Page".into())))
        );
        assert_eq!(
            resolved(&t, &at("other", ""), "::app::consumer"),
            Some(("app".into(), "consumer".into(), None))
        );
    }

    #[test]
    fn a_bare_child_module_is_self_relative() {
        let t = playwright_like();
        assert_eq!(
            resolved(&t, &at("app", "protocol"), "page::Frame"),
            Some(("app".into(), "protocol.page".into(), Some("Frame".into())))
        );
    }

    #[test]
    fn a_bare_import_alias_is_substituted() {
        let t = playwright_like();
        assert_eq!(
            resolved(&t, &at("app", "consumer"), "protocol::page::Frame"),
            Some(("app".into(), "protocol.page".into(), Some("Frame".into())))
        );
        assert_eq!(
            resolved(&t, &at("app", "consumer"), "Page::new"),
            Some(("app".into(), "protocol.page".into(), Some("Page".into())))
        );
    }

    #[test]
    fn std_unknown_crates_and_locals_are_nothing() {
        let t = playwright_like();
        assert_eq!(resolved(&t, &at("app", "consumer"), "std::fs::read"), None);
        assert_eq!(
            resolved(&t, &at("app", "consumer"), "serde::Serialize"),
            None
        );
        assert_eq!(resolved(&t, &at("app", "consumer"), "Vec"), None);
    }

    #[test]
    fn a_reexport_chain_lands_on_the_defining_module() {
        let t = playwright_like();
        // other -> app root (pub use protocol::Page) -> protocol (pub use page::Page) -> page
        assert_eq!(
            resolved(&t, &at("other", ""), "app::Page"),
            Some(("app".into(), "protocol.page".into(), Some("Page".into())))
        );
    }

    #[test]
    fn a_rename_resolves_to_the_original_item_name() {
        let t = playwright_like();
        assert_eq!(
            resolved(&t, &at("other", ""), "app::protocol::PageFrame"),
            Some(("app".into(), "protocol.page".into(), Some("Frame".into())))
        );
    }

    #[test]
    fn trailing_segments_after_the_item_are_not_consumed() {
        let t = playwright_like();
        assert_eq!(
            resolved(&t, &at("other", ""), "app::Page::new"),
            Some(("app".into(), "protocol.page".into(), Some("Page".into())))
        );
    }

    #[test]
    fn a_path_ending_on_a_module_has_no_item() {
        let t = playwright_like();
        assert_eq!(
            resolved(&t, &at("app", "consumer"), "crate::protocol"),
            Some(("app".into(), "protocol".into(), None))
        );
        assert_eq!(
            resolved(&t, &at("other", ""), "app"),
            Some(("app".into(), "".into(), None))
        );
    }

    #[test]
    fn a_glob_reexport_is_followed_when_the_direct_lookup_fails() {
        let mut t = ResolveTree::default();
        let app = TargetKey::main("app");
        t.add_target(app.clone(), true);
        t.module_mut(&app, &m("a")).globs.push((p("inner"), true));
        t.module_mut(&app, &m("a.inner"))
            .defines
            .insert("Thing".into());
        t.module_mut(&app, &m("b"));
        assert_eq!(
            resolved(&t, &at("app", "b"), "crate::a::Thing"),
            Some(("app".into(), "a.inner".into(), Some("Thing".into())))
        );
    }

    #[test]
    fn a_glob_is_followed_to_a_name_the_provider_reexports() {
        let mut t = ResolveTree::default();
        let app = TargetKey::main("app");
        t.add_target(app.clone(), true);
        t.module_mut(&app, &m("a")).globs.push((p("inner"), true));
        let inner = t.module_mut(&app, &m("a.inner"));
        inner
            .imports
            .insert("Thing".into(), p("crate::deep::Thing"));
        inner.reexports.insert("Thing".into());
        t.module_mut(&app, &m("deep"))
            .defines
            .insert("Thing".into());
        assert_eq!(
            resolved(&t, &at("app", ""), "crate::a::Thing"),
            Some(("app".into(), "deep".into(), Some("Thing".into())))
        );
    }

    #[test]
    fn a_glob_is_followed_to_a_child_module_of_the_provider() {
        let mut t = ResolveTree::default();
        let app = TargetKey::main("app");
        t.add_target(app.clone(), true);
        t.module_mut(&app, &m("a")).globs.push((p("inner"), true));
        t.module_mut(&app, &m("a.inner.sub"))
            .defines
            .insert("X".into());
        assert_eq!(
            resolved(&t, &at("app", ""), "crate::a::sub::X"),
            Some(("app".into(), "a.inner.sub".into(), Some("X".into())))
        );
    }

    #[test]
    fn a_glob_is_followed_through_a_second_glob() {
        let mut t = ResolveTree::default();
        let app = TargetKey::main("app");
        t.add_target(app.clone(), true);
        t.module_mut(&app, &m("a")).globs.push((p("inner"), true));
        t.module_mut(&app, &m("a.inner"))
            .globs
            .push((p("crate::deep"), true));
        t.module_mut(&app, &m("deep"))
            .defines
            .insert("Thing".into());
        assert_eq!(
            resolved(&t, &at("app", ""), "crate::a::Thing"),
            Some(("app".into(), "deep".into(), Some("Thing".into())))
        );
    }

    #[test]
    fn a_glob_that_provides_nothing_leaves_the_lenient_stop() {
        let mut t = ResolveTree::default();
        let app = TargetKey::main("app");
        t.add_target(app.clone(), true);
        t.module_mut(&app, &m("a")).globs.push((p("inner"), true));
        t.module_mut(&app, &m("a.inner"));
        assert_eq!(
            resolved(&t, &at("app", ""), "crate::a::Generated"),
            Some(("app".into(), "a".into(), Some("Generated".into())))
        );
    }

    #[test]
    fn a_private_glob_serves_only_its_own_module() {
        let mut t = ResolveTree::default();
        let app = TargetKey::main("app");
        t.add_target(app.clone(), true);
        t.module_mut(&app, &m("a"))
            .globs
            .push((p("crate::x"), false));
        t.module_mut(&app, &m("x")).defines.insert("Thing".into());
        // Through the module that wrote the glob: found.
        assert_eq!(
            resolved(&t, &at("app", "a"), "self::Thing"),
            Some(("app".into(), "x".into(), Some("Thing".into())))
        );
        // From elsewhere: the lenient stop at `a`.
        assert_eq!(
            resolved(&t, &at("app", ""), "crate::a::Thing"),
            Some(("app".into(), "a".into(), Some("Thing".into())))
        );
    }

    /// `m0` re-exports `X` from `m1`, which re-exports it from `m2`, ...
    /// up to `m<hops>`, which defines it.
    fn reexport_chain(hops: usize) -> ResolveTree {
        let mut t = ResolveTree::default();
        let app = TargetKey::main("app");
        t.add_target(app.clone(), true);
        for i in 0..hops {
            let node = t.module_mut(&app, &m(&format!("m{i}")));
            node.imports
                .insert("X".into(), p(&format!("crate::m{}::X", i + 1)));
            node.reexports.insert("X".into());
        }
        t.module_mut(&app, &m(&format!("m{hops}")))
            .defines
            .insert("X".into());
        t
    }

    #[test]
    fn a_chain_of_exactly_the_maximum_hops_resolves() {
        let t = reexport_chain(MAX_REEXPORT_HOPS);
        assert_eq!(
            resolved(&t, &at("app", ""), "crate::m0::X"),
            Some((
                "app".into(),
                format!("m{MAX_REEXPORT_HOPS}"),
                Some("X".into())
            ))
        );
    }

    #[test]
    fn a_chain_one_hop_past_the_maximum_is_nothing() {
        let t = reexport_chain(MAX_REEXPORT_HOPS + 1);
        assert_eq!(resolved(&t, &at("app", ""), "crate::m0::X"), None);
    }

    /// `a` globs `g1`, `g1` globs `g2`, ... `g<depth>` defines `Thing`.
    fn glob_chain(depth: usize) -> ResolveTree {
        let mut t = ResolveTree::default();
        let app = TargetKey::main("app");
        t.add_target(app.clone(), true);
        t.module_mut(&app, &m("a"))
            .globs
            .push((p("crate::g1"), true));
        for i in 1..depth {
            t.module_mut(&app, &m(&format!("g{i}")))
                .globs
                .push((p(&format!("crate::g{}", i + 1)), true));
        }
        t.module_mut(&app, &m(&format!("g{depth}")))
            .defines
            .insert("Thing".into());
        t
    }

    #[test]
    fn a_glob_chain_of_exactly_the_maximum_depth_resolves() {
        let t = glob_chain(MAX_GLOB_DEPTH);
        assert_eq!(
            resolved(&t, &at("app", ""), "crate::a::Thing"),
            Some((
                "app".into(),
                format!("g{MAX_GLOB_DEPTH}"),
                Some("Thing".into())
            ))
        );
    }

    #[test]
    fn a_glob_chain_one_past_the_maximum_depth_stops_leniently() {
        let t = glob_chain(MAX_GLOB_DEPTH + 1);
        assert_eq!(
            resolved(&t, &at("app", ""), "crate::a::Thing"),
            Some(("app".into(), "a".into(), Some("Thing".into())))
        );
    }

    #[test]
    fn a_glob_cycle_stops_leniently() {
        let mut t = ResolveTree::default();
        let app = TargetKey::main("app");
        t.add_target(app.clone(), true);
        t.module_mut(&app, &m("a"))
            .globs
            .push((p("crate::b"), true));
        t.module_mut(&app, &m("b"))
            .globs
            .push((p("crate::a"), true));
        assert_eq!(
            resolved(&t, &at("app", ""), "crate::a::Thing"),
            Some(("app".into(), "a".into(), Some("Thing".into())))
        );
    }

    #[test]
    fn a_glob_whose_own_path_needs_a_glob_in_a_cycle_stops_leniently() {
        // Neither glob path resolves directly: `a`'s needs `b`'s globs
        // and `b`'s needs `a`'s, so only the depth carried into each
        // path's resolution ends the recursion.
        let mut t = ResolveTree::default();
        let app = TargetKey::main("app");
        t.add_target(app.clone(), true);
        t.module_mut(&app, &m("a"))
            .globs
            .push((p("crate::b::y"), true));
        t.module_mut(&app, &m("b"))
            .globs
            .push((p("crate::a::x"), true));
        assert_eq!(
            resolved(&t, &at("app", ""), "crate::a::Thing"),
            Some(("app".into(), "a".into(), Some("Thing".into())))
        );
    }

    #[test]
    fn a_bare_name_from_a_glob_import_resolves_in_the_writing_module() {
        let mut t = ResolveTree::default();
        let app = TargetKey::main("app");
        t.add_target(app.clone(), true);
        t.module_mut(&app, &m("a")).defines.insert("Thing".into());
        t.module_mut(&app, &m("b"))
            .globs
            .push((p("crate::a"), false));
        assert_eq!(
            resolved(&t, &at("app", "b"), "Thing::new"),
            Some(("app".into(), "a".into(), Some("Thing".into())))
        );
        assert_eq!(
            resolved(&t, &at("app", "b"), "Thing"),
            Some(("app".into(), "a".into(), Some("Thing".into())))
        );
    }

    #[test]
    fn a_glob_import_shadows_a_crate_name() {
        let mut t = ResolveTree::default();
        let app = TargetKey::main("app");
        t.add_target(app.clone(), true);
        t.add_target(TargetKey::main("other"), true);
        t.module_mut(&app, &m("a.other"));
        t.module_mut(&app, &m("b"))
            .globs
            .push((p("crate::a"), false));
        assert_eq!(
            resolved(&t, &at("app", "b"), "other::X"),
            Some(("app".into(), "a.other".into(), Some("X".into())))
        );
    }

    #[test]
    fn a_glob_whose_path_is_only_reachable_through_a_glob_in_a_cycle_terminates() {
        let mut t = ResolveTree::default();
        let app = TargetKey::main("app");
        t.add_target(app.clone(), true);
        t.module_mut(&app, &m("a"))
            .globs
            .push((p("b_alias"), false));
        t.module_mut(&app, &m("b"))
            .globs
            .push((p("a_alias"), false));
        assert_eq!(resolved(&t, &at("app", "a"), "Thing"), None);
    }

    #[test]
    fn a_reexport_cycle_stops_at_the_bound() {
        let mut t = ResolveTree::default();
        let app = TargetKey::main("app");
        t.add_target(app.clone(), true);
        let a = t.module_mut(&app, &m("a"));
        a.imports.insert("X".into(), p("crate::b::X"));
        a.reexports.insert("X".into());
        let b = t.module_mut(&app, &m("b"));
        b.imports.insert("X".into(), p("crate::a::X"));
        b.reexports.insert("X".into());
        assert_eq!(resolved(&t, &at("app", ""), "crate::a::X"), None);
    }

    #[test]
    fn a_private_import_is_followed_from_inside_its_module_and_stops_from_outside() {
        let mut t = ResolveTree::default();
        let app = TargetKey::main("app");
        t.add_target(app.clone(), true);
        t.module_mut(&app, &m("a"))
            .imports
            .insert("Y".into(), p("crate::x::Y"));
        t.module_mut(&app, &m("a.b"));
        t.module_mut(&app, &m("x")).defines.insert("Y".into());
        assert_eq!(
            resolved(&t, &at("app", "a.b"), "super::Y"),
            Some(("app".into(), "x".into(), Some("Y".into())))
        );
        assert_eq!(
            resolved(&t, &at("app", ""), "crate::a::Y"),
            Some(("app".into(), "a".into(), Some("Y".into())))
        );
    }

    #[test]
    fn an_alias_that_names_another_alias_in_the_same_module_is_followed() {
        let mut t = ResolveTree::default();
        let app = TargetKey::main("app");
        t.add_target(app.clone(), true);
        let root = t.module_mut(&app, &[]);
        root.imports.insert("p".into(), p("crate::protocol"));
        root.imports.insert("q".into(), p("p::page"));
        t.module_mut(&app, &m("protocol.page"))
            .defines
            .insert("Page".into());
        assert_eq!(
            resolved(&t, &at("app", ""), "q::Page"),
            Some(("app".into(), "protocol.page".into(), Some("Page".into())))
        );
    }

    #[test]
    fn a_self_referential_alias_is_nothing() {
        let mut t = ResolveTree::default();
        let app = TargetKey::main("app");
        t.add_target(app.clone(), true);
        t.module_mut(&app, &[])
            .imports
            .insert("p".into(), p("p::x"));
        assert_eq!(resolved(&t, &at("app", ""), "p::Y"), None);
    }

    #[test]
    fn a_test_target_has_its_own_crate_namespace_but_no_name() {
        let mut t = ResolveTree::default();
        let lib = TargetKey::main("app");
        let tests = TargetKey {
            crate_name: "app".into(),
            role: TargetRole::Tests("smoke".into()),
        };
        t.add_target(lib.clone(), true);
        t.add_target(tests.clone(), false);
        t.module_mut(&tests, &m("common"))
            .defines
            .insert("Server".into());
        t.module_mut(&lib, &m("api")).defines.insert("Api".into());
        let from = Location {
            target: tests.clone(),
            module: vec![],
        };
        let r = resolve(&t, &from, &p("crate::common::Server")).unwrap();
        assert_eq!((r.module.target, r.item), (tests, Some("Server".into())));
        let r = resolve(&t, &from, &p("app::api::Api")).unwrap();
        assert_eq!((r.module.target, r.item), (lib, Some("Api".into())));
    }

    #[test]
    fn module_mut_registers_every_ancestor_as_a_child_of_its_parent() {
        let mut t = ResolveTree::default();
        let app = TargetKey::main("app");
        t.add_target(app.clone(), true);
        t.module_mut(&app, &m("a.b.c"));
        assert_eq!(
            t.module(&at("app", "")).unwrap().children,
            BTreeSet::from(["a".to_string()])
        );
        assert_eq!(
            t.module(&at("app", "a")).unwrap().children,
            BTreeSet::from(["b".to_string()])
        );
        assert!(t.module(&at("app", "a.b.c")).is_some());
    }
}
