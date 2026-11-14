# Architecture rules - Implementation Plan

**Feature:** `asbuilt conform`: an architecture declared in `asbuilt.toml`,
checked against the references the code makes, and drawn on its own view

**User Story:** As a maintainer who has chosen an architecture (layers, a
hexagon, a modular monolith), I want to place my crates and modules in
groups, say which groups may use which, and have asbuilt fail when the code
breaks those rules, warn me about cycles nobody declared, and draw the
architecture so I can see where the code departs from it and why, so that
the architecture I describe is the one the code has and stays that way
without a hand-written fitness test.

**Related ADR (if applicable):** [ADR 0001: Architecture rules over
groups, checked on direct edges](../adr/0001-architecture-rules.md),
accepted.

**Approach:** Vertical Slicing with Outside-In TDD

**Status:** Accepted with ADR 0001: rules over groups are the config
surface, with patterns as presets that expand to them, and the model's
relations are the evidence, with the manifest as a second source where the
groups hold crates. Nothing here is built yet.

---

## Implementation Strategy

This implementation follows **vertical slicing** - each slice delivers end-to-end user value and can be tested/released independently.

The roadmap's 0.5.0 item "Architecture rules" (`roadmap-now-rules` in
`crates/site/public/roadmap.json`) is done when slices 1 to 3 have landed:
declared layers, cycles, and what `Cargo.toml` declares. Slice 4, the
view, is the 0.6.0 item `roadmap-next-architecture-view`, and slice 5, the
remaining rules with the presets, is `roadmap-later-patterns`, which waits
for someone to ask.

The first consumer is rust-service-template, which dropped its guppy test
of the hexagonal rule when it adopted asbuilt (`da5e043`, PR #25) and has
had the rule unenforced since. That rule is between four crates, which
Cargo already makes visible: a crate cannot name another it does not
declare. A `cargo metadata --no-deps` test in the template would hold it in
the meantime, and nothing in this plan waits on that.

Documentation to update as the slices land:
- [README](../../README.md): a `conform` section beside `check`.
- [CLAUDE](../../CLAUDE.md): the `asbuilt-core` and CLI crate lines.
- [The skill's config reference](../../skills/asbuilt/references/config.md)
  and [CLI reference](../../skills/asbuilt/references/cli.md), with a
  `metadata.version` bump in `skills/asbuilt/SKILL.md` each time.
- [CHANGELOG](../../CHANGELOG.md), and the roadmap entry's blurb, which
  says "checked like drift and reported the same way" and has to say
  `conform` instead (slice 1).
- [`.pre-commit-hooks.yaml`](../../.pre-commit-hooks.yaml): an
  `asbuilt-conform` hook beside `asbuilt-check`. The action needs no new
  input, since `command: conform` already works.

### What it is for

Keeping a code base on an architecture someone has chosen, and moving one
toward it: declare the target, and the violations are the list of edges to
fix, each exception a recorded decision with its reason. It is not a tool
for designing an architecture before the code exists, because every edge it
checks or draws is one the code already makes.

Rust gives crates hard boundaries already, so the rules earn the most
inside a crate, between modules, where nothing else checks direction
(`pub(crate)` hides an item but cannot say which module may use which). The
rules are written against model ids, so a group can hold crates
(`asbuilt_core`) or modules (`asbuilt_core.emit`) with the same syntax.

### What comes from the code, and what from the config

From the code, through `survey`: every element (crates and modules), every
relation between them, its kind, and the item names that make it
(`svc_persistence.repo` names `svc_application.RankItems`). A new module
joins its parent's group by prefix with no config change.

From `asbuilt.toml`: which ids are in which group, the rules between
groups (or a preset that supplies them), what is exempt, and each exception
with its reason. This is the declared intent, and nothing in the code can
supply it.

The config is checked against the code in three ways, so it cannot drift
silently:

- every member of an architecture's scope is in a group or `exempt`, so a
  new crate (or a new module, under `within`) fails until it is placed;
- a group entry that names no element fails, so a rename or a deletion is
  caught;
- an exception that matches no edge fails, so fixing the code retires the
  exception instead of leaving a stale pass behind.

What it cannot check is whether a module is in the *right* group, and
references the model does not record: method calls on values, a macro body
in another syntax (a Leptos `view!`), and anything in `std`, so `std::fs`
in a domain module is invisible to it. One named reference between two
modules is enough to make their edge, so the edge that goes missing is one
where a value is reached through a method chain and its type never named
(`ctx.repo().save()`). Research item 2 counts how often that happens; until
then the documentation says "catches named references" and no more.

### Design (second pass)

**A separate command.** `asbuilt conform` surveys in memory and checks the
fresh model; `check` keeps meaning drift and nothing else. A consumer runs
both: one says the model matches the code, the other says the code matches
the architecture.

**Groups and three rules.** An architecture names its groups and the rules
between them. The rules are the ones import-linter and dependency-cruiser
use:

| Rule | Says | Slice |
|---|---|---|
| `layers` | an ordered list, top first; a group may use only groups in lower layers; groups listed together in one layer may not use each other | 1 |
| `independent` | the listed groups may not use one another; `independent_members` says the same of one group's own members | 5 |
| `forbidden` | `from` may not use `to` | 5 |

A member stands for itself and everything under it, and members of one
group may use each other unless `independent_members` names the group. A
`tests` or `examples` element is in no scope: it is never a member, and a
relation from one is never a violation.

**Patterns are presets over the rules (slice 5).** A preset fixes the
group names and supplies the rules, so the config says which code plays
each part and asbuilt knows the pattern. Hexagonal expands to:

```toml
layers = ["root", "driving", ["application", "driven"], "domain"]
independent_members = ["driving", "driven"]
[[conform.architecture.forbidden]]
from = "driving"
to   = "driven"
```

That gives each rule of the hexagon: the domain uses nothing above it;
application and driven adapters use the domain only, and not each other
(the application reaches a driven adapter through a port the domain
defines); driving adapters use application and domain but not driven
adapters; adapters do not use each other; and nothing uses the composition
root. The split between `driving`, `driven`, and `root` is where this
differs from ArchUnit's onion preset, which has one adapter role. In
rust-service-template the `api` crate is both the HTTP adapter and the
composition root that constructs the persistence adapter, so with one
adapter role `api -> persistence` would fail as adapter to adapter, and
`persistence -> application`, which the template forbids, would pass.

This is one dialect of the pattern: the ports are the domain's. A code base
that defines its ports in the application layer has driven adapters that
use `application`, and this preset fails it. The documentation says which
dialect the preset is, and the ADR decides whether the other is a second
preset or left to the rules.

`asbuilt conform --explain` prints a preset's expansion, and each violation
names the rule it broke as well as the preset, so a config that outgrows a
preset can be replaced by its expansion and edited.

**Evidence is the model's relations, and the manifest where the groups
hold crates.** Every rule is checked against the relations `survey`
records, which work for any front-end and at module level. Where a group's
members are crates, a `[dependencies]` or `[build-dependencies]` entry on
another surveyed crate also counts as an edge (slice 3), for two reasons.
Cargo lets code use a declared crate through references the model does not
record, and a scaffold declares a dependency before it references it: on
2026-11-10 rust-service-template's model backs one of its five declared
crate dependencies with a relation, because `api` and `persistence` do not
yet name the crates they declare (asbuilt and playwright-rust each back
three of three). `Cargo.toml` is also the only evidence for the external
crates a group may use, since the model has no element for one.
`[dev-dependencies]` are exempt, and each violation names the evidence
behind it. Neither the resolved graph (a full multi-platform resolve, which
broke the guppy test this replaces) nor compiler-level analysis is used.

**Violations and findings.** A violation breaks a rule the config declared,
and `conform` exits 1. A finding is a likely problem nobody declared (a
cycle, later a dependency on something less stable); it is reported and
exits 0 unless the config raises it to an error.

**Exit codes and streams match `check`.** 0 conforms, 1 violation, 2 error.
Text goes to stdout with the verdict on stderr; `--format json` writes one
object to stdout, a public contract documented in `cli.md`.

**The architecture is drawn.** Each declared architecture gets a generated
view (slice 4): its groups as group boxes in layer order, and each excepted
edge, cycle, and violation in its own style, with the relation's items as
the label. Where `conform` gates every commit, a violation never reaches
`main`, so the published view shows the exceptions and the cycles (the
debt on record); the violations show in a working tree, and on a code base
still being moved toward the architecture. LikeC4 1.60.0 has no hexagon
shape and lays out through Graphviz `dot`, which places nodes in ranks and
draws a group box only as a rectangle, so no view is Cockburn's hexagon.
See [Later: a drawn hexagon](#later-a-drawn-hexagon) for the alternative.

**Nothing is taken as a dependency from the tools that do this elsewhere.**
`cargo-modules` (rust-analyzer's crates, pinned to exact versions) and
`cargo-pup` (a pinned nightly and compiler internals) are each a larger
supply chain than asbuilt's own, and `rust_arkitect` parses with `syn` as
asbuilt does and has not been pushed to since January 2025. They are
references. None of them can count the edges the model misses:
`cargo-modules` does not read function bodies, so it sees fewer module
edges than the model, not more. The cycle search in slice 2 is written by
hand (Tarjan's algorithm on a few dozen nodes) to keep `asbuilt-core` at
its five dependencies.

---

## Research before slice 1

The goal is a feature engineers keep turned on: rules that match how they
already think about their architecture, violations they agree with, and a
way to adopt it on a code base that does not conform yet. In order:

1. **Config shapes and complaints, from the tools in use.** import-linter
   (`layers` with independent siblings, `containers`, `ignore_imports`),
   tach (modules with `depends_on` and layers in `tach.toml`, a dependency
   that can be marked deprecated instead of removed, unchecked modules for
   adopting it a part at a time, and `tach show` for a graph; the closest
   in shape), dependency-cruiser (`forbidden` rules, `--ignore-known`), and
   Nx's `enforce-module-boundaries` (tags, which groups resemble): how each
   writes the rules above, and what their most-voted issues say about false
   positives and config churn. On the Rust side, the rules `cargo-pup` and
   `rust_arkitect` offer, and whether their few users say why. One question
   to settle from import-linter, whose layers contract follows indirect
   imports: whether a chain through an `exempt` member should count as an
   edge between the groups at its ends. This plan checks direct edges only.
2. **Which edges count in Rust, and which are missing.** Run a throwaway
   script with candidate rules over the committed `model.c4` of real
   workspaces (this one, playwright-rust, cqa-grader, rust-service-template),
   and read every edge it flags. The questions it answers: whether each
   relation kind (`implements`, `constructs`, `calls`, `names`, `uses`)
   should count, and whether resolving a `pub use` through to the defining
   module (which the model does) loses a rule anyone wants, such as "only
   through the facade". Then, for the edges the model does not have, take
   rust-analyzer's SCIP index of the same workspaces (it resolves method
   calls through type inference) and diff its module edges against the
   model's. The count decides whether "catches named references" is good
   enough for a gate, and whether the diff is worth keeping as an
   `#[ignore]`d test beside the `likec4_*` ones.
3. **Adopting on a code base that does not conform.** Compare per-edge
   exceptions with reasons (import-linter's `ignore_imports`, which fail
   when stale) with a recorded baseline that fails only on new violations
   (ArchUnit's freeze, dependency-cruiser's `--ignore-known`). The second
   is easier to start with and easier to forget about.
4. **One real consumer end to end.** Write rust-service-template's rule in
   the chosen shape and check that it restores everything its guppy test
   asserted (domain on nothing, external crates included; application and
   persistence on domain only; api on anything; every crate in the table),
   with nothing that test got wrong.
5. **The view, against the real renderer.** In LikeC4 1.60.0, write a
   hand-made view of a layered architecture and of a hexagon, with a
   `group` per group, and render both through `asbuilt render`. Every
   member sits inside a group box, and upstream issue
   [#2465](https://github.com/likec4/likec4/issues/2465) reports a ranked
   element drawn outside its parent's box, so find out first whether a
   `rank` rule works on a node inside a view `group`. If it does not, the
   layer order has to come from `dot`'s own ranking (which follows edge
   direction, so a conforming layered architecture is already drawn in
   order), and the hexagon reads top to bottom as layers, not left to
   right. Report what the spike finds on #2465, with the sample. Run this
   item after "Rendering without a system Graphviz"
   (`roadmap-now-wasm-graphviz`) lands, since that changes which Graphviz
   lays the views out; the other items do not wait on it.

Items 1 to 4 are done, and ADR 0001 records what they found. The count of
missing edges in item 2 has not run.

The output is an ADR recording the choice on each point and its reasons,
and a revision of the slices below to match. Where the result is read
beyond a hook, CI, and the docs tree (a pull-request comment with `asbuilt
diff`), and whether the Python front-end reads a project's existing tach or
import-linter config instead of asking for the rules again, are left until
there is usage to learn from.

---

## Vertical Slices

### Slice 1: groups in layers

**Status:** Not Started

**User Value:** A workspace declares its groups and their layers, and
`asbuilt conform` fails on any relation that points up, naming the edge,
the rule, and the items that make the edge.

```toml
[[conform.architecture]]
name   = "core"
within = "asbuilt_core"           # its modules; omit for the workspace's crates
layers = [["front", "drift", "docs"], "derive", "data", "base"]

[conform.architecture.groups]
front  = ["asbuilt_core.frontend"]
drift  = ["asbuilt_core.check", "asbuilt_core.read"]
docs   = ["asbuilt_core.docs", "asbuilt_core.svg"]
derive = ["asbuilt_core.emit", "asbuilt_core.externals", "asbuilt_core.theme"]
data   = ["asbuilt_core.config", "asbuilt_core.model"]
base   = ["asbuilt_core.error"]
```

The groups above are a candidate for asbuilt's own `asbuilt-core`. Its
eleven modules conform to them today (checked by hand against the
2026-11-10 model); the research may change them. An exception names
the architecture whose rule it excuses, as in this one from a service's
architecture, and must match an edge that rule would otherwise fail. It is
a top-level table, so an entry pasted anywhere in the file belongs to the
architecture it names:

```toml
[[conform.allow]]
architecture = "service"
from   = "svc_persistence.legacy"
to     = "svc_application"
reason = "the v1 import path; removed with the v1 API"
```

**Acceptance Criteria:**
- [ ] A relation from a group to one in a higher layer exits 1 and names the source and target ids, both groups, the architecture, the rule, and the relation's kind and items (the roadmap's `conform_names_each_relation_that_breaks_a_rule`)
- [ ] A model that keeps every declared rule exits 0 (`conform_passes_a_model_that_keeps_its_rules`)
- [ ] `asbuilt check` still reports drift alone: a broken rule does not fail it (`check_does_not_fail_on_a_broken_rule`)
- [ ] Two groups listed together in one layer may not use each other
- [ ] Membership is by prefix: a module under a member is in its group, and `a.b` is not under `a.bc`
- [ ] Without `within`, the scope is every surveyed crate (`extra_manifests` included); with `within = "<id>"`, it is that element's children; a member of the scope in no group and not in `exempt` is a violation, and so is a group entry that names no element
- [ ] A `tests` or `examples` element is not part of any scope (the `tests` child of `asbuilt_core` needs no group), and a relation whose source is one is never a violation; edges to `[[externals]]` elements (a spawned process, a browser) are never checked
- [ ] An id in two groups, or under another group's member, is a config error (exit 2) naming both; so is a group entry outside the scope
- [ ] An `allow` entry suppresses the violations of its `architecture` that its `from` and `to` match, by prefix; one that matches no violation is itself a violation
- [ ] An `allow` with an empty `reason`, or naming an architecture the config does not declare, is a config error (exit 2) naming the entry
- [ ] Every run lists each edge an exception excused, in text and JSON, so an excused edge that is new shows in the output
- [ ] With violations, the text output ends with a `[[conform.allow]]` entry for each, with `reason = ""`, ready to paste
- [ ] Each architecture in the config is checked, and each violation names its architecture
- [ ] `conform` with no `[conform]` table is an error that says so (slice 2 changes this to findings only)
- [ ] `--format json` writes one object: `conforms`, and per architecture its `name`, `violations` (each with `rule`, `from`, `to`, `from_group`, `to_group`, and `evidence`, a list whose entries here have `source` of `relation` with the `kind` and `items`), `mapping` (unplaced members and entries that name nothing), and `exceptions` (each with the edges it excused, empty when it is stale)
- [ ] The emitted `.c4` is byte-identical before and after, for asbuilt itself and the consumer fixture
- [ ] asbuilt's own `asbuilt.toml` declares two architectures and conforms to both: the crates (`asbuilt` above `asbuilt_rust` above `asbuilt_core`, the two site crates `exempt`) and `asbuilt-core`'s modules (above)

**Core Library Implementation (`asbuilt-core`):**
- [ ] `config.rs`: `Config::conform`, parsed with `deny_unknown_fields`; `[conform]` added to `OWN_TABLES`; ids as the file writes them; `layers` entries are a group name or an array of them (a shared layer)
- [ ] New module `conform.rs`: `conform(&Model, &ConformConfig) -> Report`, pure, with `Report`, `Violation`, `Rule`, `Evidence`, and `MappingProblem` as values the CLI formats
- [ ] Errors for the config cases in `error.rs`, each carrying the offending entry
- [ ] No change to `model.rs` or `emit.rs`: the slice reads only what the model already holds

**Core Library Unit Tests:**
- [ ] One test per layer case (down, up, across a shared layer, within a group), each a model built by hand with exactly the edge under test, asserting the `Violation` (or its absence) with `assert_eq!`
- [ ] Membership by prefix, including `a.b` against `a.bc`
- [ ] Scope with and without `within`; a `tests` child left out of it; both directions of mapping drift; the overlap and out-of-scope config errors
- [ ] `tests` and `examples` sources exempt; edges to externals unchecked
- [ ] A matched and an unmatched exception; an exception's excused edges; an empty `reason` and an unknown `architecture`
- [ ] A doctest on `conform` showing a three-group model and its report

**CLI Implementation (`asbuilt`):**
- [ ] `Command::Conform { root, format }` in `main.rs`, and `commands::conform`, using `survey_model`
- [ ] Text: one block per violation on stdout (the edge, the rule, then the relation's kind and items indented under it), then the excused edges, then a paste-ready `[[conform.allow]]` entry per violation, and on stderr the verdict line (`asbuilt: 2 violations of core`); `<root> conforms` on stdout when clean
- [ ] Exit codes `EXIT_OK`, `EXIT_DRIFT` (renamed or aliased so it reads right for a violation), and `EXIT_ERROR`
- [ ] Element kinds in messages through `Frontend::noun`, as `check` does (a crate, a module)

**CLI Integration Tests:**
- [ ] A fixture workspace `crates/asbuilt/tests/fixtures/conform/` with three crates, one of them with modules and a `tests` directory, and a committed `asbuilt.toml`: conforming as committed, and each test copies it into a `TempDir` and adds one wrong-way reference
- [ ] Exit code and JSON for a layer violation, a module-level violation under `within`, an unplaced crate, and a stale exception
- [ ] `conform --help` names the command and its formats
- [ ] `conform` in this repo's own pre-commit config and in `test.yml`, so asbuilt's own architecture is held on every commit

**Documentation:**
- [ ] `config.md`: `[[conform.architecture]]`, `layers`, `within`, `exempt`, and `[[conform.allow]]`
- [ ] `cli.md`: the command, its streams and exit codes, and the JSON shape
- [ ] README: a `conform` section, what it catches and what it does not, and the `asbuilt-conform` hook
- [ ] `.pre-commit-hooks.yaml`: the `asbuilt-conform` hook, with the same `files` pattern as `asbuilt-check`
- [ ] CHANGELOG, and the roadmap entry's blurb rewritten for `conform`

**Notes:**
- `conform` checks the code, not the committed model; a stale model is `check`'s job, so `conform` does not refuse one the way `docs` does.
- `evidence` is a list from the start so that slice 3 adds a second source without changing the JSON shape.
- A `strict` option (a layer may use only the next one down) is left out until someone asks for it.
- A violation's text block reads:
  ```
  asbuilt_rust.visit (visit) -> asbuilt_rust.walk (walk)
    breaks layers in rust: visit is below walk
    calls ModulePath, is_cfg_test
  ```
  and the paste-ready entries follow every violation under `# To accept, paste and give each a reason:`.
- Rules are checked on direct edges only: a path through an `exempt` member is not followed, and the docs say so.

---

### Slice 2: cycles, as a finding

**Status:** Not Started

**User Value:** Without declaring anything, `conform` reports each cycle
among sibling modules, with the relations that close it.

```toml
[conform]
cycles = "warn"   # "error" fails on them; "off" skips the check

[[conform.cycle_allow]]
members = ["asbuilt_core.check", "asbuilt_core.read"]
reason  = "..."
```

**Acceptance Criteria:**
- [ ] `conform` with no architecture reports findings only, instead of slice 1's error
- [ ] Each strongly connected set of sibling modules (children of the same element), over `implements`, `constructs`, and `calls` relations, is one finding, listing its members and one relation per edge of a cycle through them; a cycle of `names` or `uses` relations alone is not reported
- [ ] `cycles = "warn"` is the default: findings are printed and the exit code is 0 (the roadmap's `conform_reports_a_module_cycle_without_failing`)
- [ ] `cycles = "error"` makes a cycle exit 1; `"off"` skips the check
- [ ] A `cycle_allow` entry suppresses the finding whose members it lists; one that matches no finding is a violation
- [ ] Crates are not checked (cargo refuses a crate cycle outside dev dependencies)
- [ ] asbuilt's own two cycles in the 2026-11-10 model, `check` with `read` and `docs` with `svg`, are each fixed or allowed with a reason

**Core Library Implementation:**
- [ ] Strongly connected components over sibling modules (Tarjan's algorithm, written here; no new dependency), lifted from the model's `implements`, `constructs`, and `calls` relations (a relation between descendants counts for their sibling ancestors)
- [ ] `Finding` beside `Violation` in `Report`, with a severity

**Library Unit Tests:**
- [ ] Two-module and three-module cycles, a cycle through nested modules lifted to siblings, a cycle of `names` relations not reported, an acyclic tree, each severity, a matched and a stale `cycle_allow`

**CLI Implementation:**
- [ ] Findings in text after violations, and as `findings` in JSON; the verdict line counts both

**CLI Integration Tests:**
- [ ] A fixture crate with a module cycle, under each severity

**Documentation:**
- [ ] `config.md`, `cli.md`, CHANGELOG
- [ ] The roadmap's cycle criterion in `roadmap.json` says the cycle is one of calls, constructions, or implementations

**Notes:**
- Mutual `use` between sibling modules is legal Rust and common, which is why the default is a warning and not an error.
- `cargo modules dependencies --acyclic` already finds cycles in one crate. What this adds is the allowed cycle with its reason, the same report and JSON as the rules, and no rust-analyzer.
- With this slice the roadmap item's "no cycles" half is in.

---

### Slice 3: what `Cargo.toml` declares

**Status:** Not Started

**User Value:** Between crates, a declared dependency that breaks a rule
fails before any code uses it, and a group of crates can be held to a
vetted list of external crates, so a domain stays free of I/O, async
runtimes, and frameworks.

```toml
[conform.architecture.externals]
domain = ["thiserror"]
```

**Acceptance Criteria:**
- [ ] Where a group's members are crates, a `[dependencies]` or `[build-dependencies]` entry (on any platform) on another surveyed crate is an edge: one that breaks a rule is a violation even when no relation backs it, with `evidence` of `source` `manifest` naming the table and the dependency (the roadmap's `conform_fails_on_a_declared_dependency_no_code_uses`)
- [ ] A relation and a manifest entry for the same pair make one violation with two pieces of evidence
- [ ] A crate in a group with an `externals` entry that declares an external crate not in the list is a violation naming the crate, the package, and the table
- [ ] A group with no `externals` entry is not checked for external crates; an empty list forbids every one
- [ ] A list entry that names no dependency of any member is a violation, as a stale exception is
- [ ] A `[dev-dependencies]` entry is never an edge and never a violation
- [ ] An `externals` entry for a group whose members are modules is a config error (a manifest belongs to a crate), and manifest evidence is never attributed to a module
- [ ] asbuilt's own crate architecture lists `asbuilt-core`'s five (`pulldown-cmark`, `serde`, `similar`, `thiserror`, and `toml`) and conforms

**Core Library Implementation:**
- [ ] `model.rs`: `Model::dependencies: Vec<Dependency>`, each with `from: Id`, `to: DependencyTarget` (`Element(Id)` for a surveyed crate, `External(String)` for any other package, by name), `table` (`Normal` or `Build`), and `platform: Option<String>`; `normalize` sorts and deduplicates them and `merge` appends them
- [ ] `emit.rs` untouched; a test holds that a model with dependencies emits the same text as one without
- [ ] `conform.rs`: manifest edges beside relation edges for crate members, and the allowlist check

**Rust front-end (`asbuilt-rust`):**
- [ ] `discover.rs`: keep each package's declared dependencies from the `cargo metadata --no-deps --offline` call it already makes; drop `Development`
- [ ] Record the package name, not a `rename` (which changes the `use` path, not the package)
- [ ] Fixture tests in `tests/fixtures/`: a path dependency, a build dependency, a platform-specific dependency, a renamed dependency, and a dev dependency, asserting the dependency list

**Library Unit Tests:**
- [ ] Manifest-only and relation-only evidence each produce the violation; both together produce one violation with two pieces of evidence
- [ ] An allowed, a forbidden, and a stale `externals` entry; the empty list; the key absent; the module-group config error

**CLI Implementation:**
- [ ] Manifest evidence in a violation's text, and the external violation's text and JSON form (`rule` of `externals` with the package and table)

**CLI Integration Tests:**
- [ ] The conform fixture with a wrong-way dependency declared and not used, and with an external crate added to its innermost crate

**Documentation:**
- [ ] `config.md`, `cli.md`, CHANGELOG

**Notes:**
- Only direct dependencies count. "The domain pulls in tokio through a crate it uses" needs the resolved graph, which this design rules out; the allowlist is where that judgment is made instead.
- With slices 1 and 3 released, rust-service-template can restore the whole rule its guppy test carried, with no preset: `layers = ["api", ["application", "persistence"], "domain"]` over its four crates, and `domain = []` under `externals`. Its `api` and `persistence` crates declare dependencies they do not yet reference, which is why the manifest has to count there.
- Undecided: whether the external-crate allowlist gets an acceptance criterion of its own on the roadmap item `roadmap-now-rules`. Its two new criteria cover cycles and a declared dependency between crates, not the allowlist, so as written the item can show done with the allowlist unbuilt.

---

### Slice 4: a view per architecture

**Status:** Not Started

**User Value:** Each declared architecture is drawn: its groups as boxes in
layer order, with each excepted edge, cycle, and violation in its own
style, and a docs page that says which rule each one concerns and why.

**Acceptance Criteria:**
- [ ] For each architecture, `survey` emits a view `view_conform_<name>` with one LikeC4 `group` per group holding its members, in the order of its `layers`, laid out as research item 5 settles (the roadmap's `a_declared_architecture_gets_a_view_of_its_groups`)
- [ ] An excepted edge is drawn in an exception style and a violating relation in a violation style, both named in `[theme]`'s defaults, each labelled with the relation's items (the roadmap's `a_violating_relation_is_drawn_in_the_violation_style`)
- [ ] Cycles from slice 2 are marked on the view the same way, as findings
- [ ] `asbuilt docs` writes a page per architecture: the view, then each violation, finding, and exception, with its rule, its evidence, and the exception's reason, each linked to the pages of its two ends (the roadmap's `an_architecture_page_lists_what_departs_from_it`)
- [ ] A model with no `[conform]` table emits exactly what slice 3 emitted (the roadmap's `a_model_without_an_architecture_emits_no_view_for_one`)
- [ ] `check` reports a stale architecture view as drift, as for any other view
- [ ] The `likec4_*` tests validate and render a conforming and a violating view

**Core Library Implementation:**
- [ ] `emit.rs`: the architecture views, from the model and the conform report
- [ ] `docs.rs`: the architecture page

**CLI Implementation:**
- [ ] `survey` runs the conform check to style the view; a violation does not fail `survey`

**Documentation:**
- [ ] `config.md` for the view and its theme keys; the docs tree's description in the README; CHANGELOG; `roadmap-next-architecture-view` in `roadmap.json`

**Notes:**
- In a repo where `conform` gates every commit, the committed view carries exceptions and cycles and no violations. The violation style is seen in a working tree (`asbuilt survey`, then `render`) and in a repo that runs `conform` without failing on it while it adopts the architecture.
- A manifest-only violation has no relation to style; it is listed on the docs page and not drawn.
- The emitted model now depends on the rules as well as the code; the docs say so, since a change to `[conform]` changes `model.c4`.
- Undecided: whether the Later item "Relation styles in [theme]" (`roadmap-later-relation-styles`) moves up beside this one or is folded into it. This slice adds exception and violation edge styles to `[theme]`, which is the mechanism that item asks for.

---

### Slice 5: the other rules, and presets

**Status:** Not Started

**User Value:** Rules that layers cannot say are available, and the common
architectures are available by name, with `--explain` showing the rules
each one stands for.

```toml
[[conform.architecture]]
name   = "service"
preset = "hexagonal"
exempt = ["svc_xtask"]

[conform.architecture.groups]
domain      = ["svc_domain"]
application = ["svc_application"]
driven      = ["svc_persistence"]
root        = ["svc_api"]
```

**Acceptance Criteria:**
- [ ] `independent` forbids edges among the listed groups in both directions; `independent_members` forbids edges among one group's members
- [ ] A `forbidden` rule fails on any edge from a member of `from` to a member of `to`
- [ ] `preset = "hexagonal"` requires its groups by name (`domain`, `application`, `driving`, `driven`, `root`; an absent group is empty) and expands to the rules in [Design](#design-second-pass)
- [ ] `preset = "layered"` takes `layers` from the config and adds nothing; `preset = "modular"` requires `kernel` and `features`, forbids edges between features (`independent_members`), and puts features above the kernel
- [ ] Rules written beside a preset are added to its expansion, not substituted for it
- [ ] A group name the preset does not know is a config error naming it and the preset's groups
- [ ] `asbuilt conform --explain` prints each architecture's rules after expansion, in config syntax that parses back to the same rules
- [ ] A violation names the rule and the preset; JSON adds `preset`
- [ ] A preset architecture's view (slice 4) is drawn from its expansion, and the hexagon's as research item 5 settles

**Core Library Implementation:**
- [ ] The two rules in `conform.rs`, over the same edges as `layers`
- [ ] Each preset as a function from groups to rules; no rule of its own

**Library Unit Tests:**
- [ ] `independent`, `independent_members`, and `forbidden`, one test each way
- [ ] Each rule of each preset, through its expansion; a preset with an added rule; the round trip of `--explain`

**CLI Implementation:**
- [ ] `--explain` on `Command::Conform`

**CLI Integration Tests:**
- [ ] The conform fixture as a hexagon, conforming and with one violation of each kind

**Documentation:**
- [ ] `config.md` (the two rules, each preset with its expansion, and which dialect of the hexagon it is), `cli.md`, README, CHANGELOG; `roadmap-later-patterns` in `roadmap.json`

---

### Later: a drawn hexagon

If slice 4's view is not enough to read a hexagon once slice 5 has the
preset, two routes:

- **Upstream:** LikeC4 has had a hexagon-shape request open since 2023
  ([#86](https://github.com/likec4/likec4/issues/86)), and its maintainer
  has said the work can be sponsored. A hexagon *node* could stand for the
  whole core in a context view, but Graphviz draws a group box only as a
  rectangle, so no shape gives a hexagon that contains the application and
  domain. LikeC4's custom node renderers draw only in its React viewer,
  which `asbuilt docs` does not use (it runs `likec4 gen dot` and then
  `dot` itself).
- **In asbuilt:** a pure SVG generator in `asbuilt-core` for the hexagonal
  preset alone, with Cockburn's fixed geometry (the domain inside the
  application, driving adapters on the left face, driven on the right,
  ports where their edges cross the boundary). asbuilt already owns the step
  from `.dot` to SVG, so this needs no upstream change; it is a second
  renderer to maintain, and its layout has to hold up past a handful of
  adapters.

### Later: dependencies against stability

Robert Martin's stable-dependencies principle as a finding: a crate or
module depending on one less stable than itself (instability as outgoing
over total coupling). It needs the coupling metrics planned for 0.6.0
(`roadmap-next-metrics`), so it waits for them.

### Later: `std` paths

The domain's I/O through `std::fs`, `std::net`, or `std::process` makes no
relation and no manifest entry. Recording references to chosen `std`
modules as externals of the model would let a group's allowlist cover
them.

---

## Slice Priority and Dependencies

| Slice | Priority | Depends On |  Status |
|-------|----------|------------|--------|
| Research and ADR | Must Have | None | Items 1 to 4 done and ADR 0001 accepted; item 5 after `roadmap-now-wasm-graphviz` |
| Slice 1: groups in layers | Must Have | Research and ADR | Not Started |
| Slice 2: cycles, as a finding | Must Have | Slice 1 | Not Started |
| Slice 3: what `Cargo.toml` declares | Must Have | Slice 1 | Not Started |
| Slice 4: a view per architecture | Should Have | Slice 1 (slice 2 for marked cycles) | Not Started |
| Slice 5: the other rules, and presets | Nice to Have | Slice 1 | Not Started |
| Later: a drawn hexagon | Nice to Have | Slices 4 and 5 | Not Started |
| Later: dependencies against stability | Nice to Have | 0.6.0 coupling metrics | Not Started |
| Later: `std` paths | Nice to Have | Slice 3 | Not Started |

Slices 2 to 5 each need only slice 1's engine, so after it they can land in
any order. Slices 1 to 3 are 0.5.0's roadmap item, and slice 3 is the one
rust-service-template waits on; slice 4 is planned for 0.6.0, and slice 5
has no release.

---

## Definition of Done

The roadmap's 0.5.0 item is done when slices 1 to 3 are. The feature is
complete when ALL of the following are true:

- [ ] All acceptance criteria from user story are met
- [ ] Slices 1 to 5 marked as "Completed"
- [ ] All tests passing: `cargo nextest run --workspace` and `cargo test --doc --workspace`
- [ ] The `likec4_*` tests pass, so the emitter still meets the real parser: `cargo nextest run --workspace --run-ignored only -E 'test(/^likec4_/)'`
- [ ] Fixture snapshots current, each refresh read before it was committed
- [ ] `./scripts/mutants.sh` clean on each slice's diff, with any survivor killed or explained
- [ ] Library documentation complete with examples: `cargo doc --open`
- [ ] CLI help text accurate: `asbuilt conform --help`
- [ ] Code formatted: `cargo fmt --check`
- [ ] No clippy warnings: `cargo clippy --all-targets --all-features -- -D warnings`
- [ ] `asbuilt check` and `asbuilt conform` both pass on this repo
- [ ] README, skill references (with `metadata.version` bumped), CHANGELOG, and `roadmap.json` updated
- [ ] `cargo-semver-checks` passes for the release that carries it (new public types in `asbuilt-core` are additions; check that none of them changes an existing one)
- [ ] Code review completed for each slice
- [ ] No unsafe code
- [ ] Implementation plan marked as completed

---

## Learnings & Adjustments

### What's Working Well

- To be filled in as slices land.

### Challenges Encountered

- To be filled in as slices land.

### Adjustments Made to Plan

- 2026-11-10, second pass: rules over groups replace patterns as the
  config surface, and patterns become presets that expand to them;
  module-level rules move from the last slice to the first; a generated
  view per architecture is added; the research is cut from seven items to
  five.
- 2026-11-10, after a review of the second pass:
  - The second pass dropped the manifest as evidence between workspace
    crates. It is back for groups that hold crates (slice 3), because
    rust-service-template's model backs one of its five declared crate
    dependencies with a relation.
  - `tests` and `examples` elements are outside every scope; without that,
    slice 1's own example failed on `asbuilt_core`'s `tests` child.
  - Slice 1 is cut to `layers`; `independent` and `forbidden` move to
    the last slice with the presets that need them, and `strict` is
    dropped.
  - The roadmap's 0.4.0 item is slices 1 to 3. The view becomes slice 4
    and a 0.5.0 item; the other rules and the presets become slice 5 and a
    Later item, since no consumer waits on them and the view needs only
    slice 1.
  - The research names tach and the Rust tools, and uses `cargo-modules`
    to count the edges the model misses.
  - The view no longer assumes a `rank` rule works inside a group, and says
    where a violation is and is not seen.
  - The hexagonal preset is documented as the ports-in-the-domain dialect.
- 2026-11-13: 0.4.0 shipped the LikeC4 1.60.0 pin and the action's
  Graphviz install on Ubuntu 26.04, so the roadmap moved on by one
  release: slices 1 to 3 are 0.5.0's item, and the view is 0.6.0's.

### Lessons for Future Features

- To be filled in as slices land.
