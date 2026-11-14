# ADR 0001: Architecture rules over groups, checked on direct edges

**Status:** Accepted

**Date:** 2026-11-14

**Related Documents:**
- [Implementation plan: architecture rules](../implementation-plans/architecture-rules.md)

---

## Context and Problem Statement

`asbuilt conform` (planned for 0.5.0 as the roadmap item `roadmap-now-rules`)
checks the references the code makes against an architecture declared in
`asbuilt.toml`: groups of crates or modules, and rules between the groups.
The [implementation plan](../implementation-plans/architecture-rules.md)
sets out the design and lists five research items to run before slice 1.
This ADR records what items 1 to 4 found and the choice made on each open
point. Item 5, the view spike, waits on "Rendering without a system
Graphviz" (`roadmap-now-wasm-graphviz`), because that changes which
Graphviz lays the views out.

The aim is a check engineers keep turned on: rules that match how they
already describe their architecture, violations they agree with when they
read them, and a way to adopt the check on a code base that does not conform
yet.

### What the research covered

1. **Prior art.** The docs and issue trackers of import-linter, tach,
   dependency-cruiser, and Nx's `enforce-module-boundaries`; on the Rust
   side, cargo-pup, rust_arkitect, cargo-modules, and archaven; and, for
   adoption, ArchUnit's `FreezingArchRule` and PHPStan's baseline.
2. **Real models.** Candidate rules for four workspaces (this one,
   playwright-rust, cqa-grader, and rust-service-template), written from
   each module's `//!` paragraph and each repo's ADRs rather than from its
   edges, run by a throwaway script over the committed `model.c4`, with
   every flagged edge read at its source. The same script found the
   strongly connected sets of sibling modules.
3. **Adoption.** Per-edge exceptions against recorded baselines, from the
   trackers above.
4. **rust-service-template end to end.** Its old hexagonal rule written in
   the plan's shape and run against its model and manifests.

### What the models showed

Between crates, every candidate rule passed in all four workspaces, which is
what Cargo already guarantees. Between modules, it did not:

| Architecture | Edges checked | Violations |
|---|---|---|
| asbuilt's crates (`asbuilt`, `asbuilt_rust`, `asbuilt_core`) | 78 | 0 |
| `asbuilt_core`, the plan's six groups | 20 | 0 |
| `asbuilt_rust`, in pipeline order | 10 | 1 |
| `playwright_rs`, in the layers of its ADR 0001 | 538 | 33 |
| `cqa_grader`, from its module docs | 11 | 1 |
| rust-service-template's four crates | 5 | 0 |

Read at the source, each violation is one an engineer would either fix or
record as an exception:

- A shared type living in one of its consumers: `asbuilt_rust.visit` uses
  `walk::ModulePath`, `cqa_grader.benchmark` constructs `trace::Graph`, and
  playwright-rust's `protocol` names or constructs four option types
  defined in `api`.
- A composition point in a low layer: playwright-rust's
  `server.object_factory` constructs every protocol object (28 of its 33
  violations come from it and `server.connection`).
- A test fake reached from production code: `protocol.page.lifecycle` calls
  `testing::FakeFileSystem`.

Some candidate groupings were wrong (playwright-rust's `server`
internals, its `tty_guard`), and the violation list is how that showed up.
A first declaration will be wrong in places, so the adoption path matters as
much as the rules.

Three facts about the model bear on the design:

- The model keeps one relation per pair of elements, with the strongest
  kind and the union of the item names (`model.rs`). The kind of each item
  is not kept.
- About half of all relations are `names` or `uses` only: 47% in asbuilt,
  53% in playwright-rust, 59% in cqa-grader.
- Every crate-internal glob import in the four workspaces is `use super::*`
  in a test module, an edge from child to parent that the model never
  records. Glob imports, which make no relation, cost nothing here.

Cycles among sibling modules: asbuilt has two (`check` with `read`, `docs`
with `svg`), cqa-grader and the template none, and playwright-rust four.
One of playwright-rust's is a set of 32 modules under `protocol` with 167
relations, most of them protocol objects naming each other's types. Counting
only `implements`, `constructs`, and `calls`, that set shrinks to eight
modules and 21 relations; asbuilt's two cycles are unchanged.

### What the prior art showed

- **Layers converge.** import-linter and tach both write one ordered list,
  top first, where a group may use what is below it and never what is above.
  Both make peers in one layer independent by default (import-linter's `|`,
  tach's undeclared same-layer use), and both later added a way for peers to
  use each other (import-linter's `:`, [PR
  #209](https://github.com/import-linter/import-linter/pull/209)).
- **Stale exceptions fail.** import-linter fails on an ignore that matches no
  import by default; tach ([#425](https://github.com/tach-org/tach/issues/425))
  and dependency-cruiser
  ([#1080](https://github.com/sverweij/dependency-cruiser/issues/1080)) added
  the same after users asked. None requires a reason: tach has an off-by-default
  rule for one, and dependency-cruiser added an optional `comment`
  ([#571](https://github.com/sverweij/dependency-cruiser/issues/571)).
- **Wildcard exceptions hide growth.** An import-linter ignore with a
  wildcard stays matched while the set it hides grows; the maintainer's
  answer was not to use wildcards
  ([#375](https://github.com/import-linter/import-linter/issues/375)).
- **Indirect chains are contested.** import-linter's layers contract follows
  chains through unlisted modules, so a rule cannot be bypassed through one.
  The cost is noise on large graphs
  ([#193](https://github.com/import-linter/import-linter/issues/193)), a
  request for direct-only checks that was granted for forbidden contracts and
  not for layers ([#91](https://github.com/import-linter/import-linter/issues/91)),
  and a stale-exception check that the maintainer called "thornier than it
  first appears" because each ignore would need the search rerun without it
  ([#351](https://github.com/import-linter/import-linter/issues/351)). tach
  checks direct imports only; Nx and dependency-cruiser follow chains only in
  their forbidden rules.
- **Baselines rot.** ArchUnit's frozen store loses matches after a refactor
  ([discussion #1068](https://github.com/TNG/ArchUnit/discussions/1068)),
  accumulates orphaned files
  ([#1264](https://github.com/TNG/ArchUnit/issues/1264)), and can grow on a
  green build ([#1700](https://github.com/TNG/ArchUnit/issues/1700));
  dependency-cruiser's baseline kept fixed violations until it added a stale
  check. PHPStan's docs say "the life goal of a baseline file is to not exist."
- **Adoption needs a ratchet anyway.** tach's `sync` writes the current
  dependencies into its config, dependency-cruiser's `--baseline` writes the
  known violations, and Nx's most-reacted complaint is a check that cannot be
  turned on a rule at a time
  ([#1018](https://github.com/nrwl/nx/issues/1018)). tach's `sync` also
  dropped comments from the config it rewrote
  ([#556](https://github.com/tach-org/tach/issues/556)).
- **No Rust tool combines groups, layers, and exceptions with reasons.**
  cargo-pup needs a pinned nightly and rustc internals, and its import rule
  reads only `use` items; rust_arkitect has per-module allow lists and a
  baseline that is a count, and has had no commit since January 2025
  ([#17](https://github.com/pfazzi/rust_arkitect/issues/17)); archaven
  (1.1.0) writes deny and allow pairs, not layers.
- **cargo-modules is not an oracle for missing edges.** Its `uses` edges
  come from signatures, fields, declared types, and module-scope imports
  (`src/graph/builder.rs`); it does not read function bodies. asbuilt's
  `syn` visitor does, which is where `calls` and `constructs` come from, so
  cargo-modules sees fewer module edges than asbuilt, not more.

### rust-service-template

`layers = ["api", ["application", "persistence"], "domain"]` over its four
crates, with no external crate allowed in `domain`, holds on its model and
its manifests with no violation, and covers each thing its old test
asserted: domain on nothing, external crates included; application and
persistence on domain only; api on anything; every crate placed. Only
`application -> domain` has a relation behind it today (`api` and
`persistence` declare crates they do not yet name), so slice 1 alone guards
one of its five declared crate edges, and slices 1 and 3 together restore
the whole rule. (The old test is not in the template's history, so this is
checked against the description of it in the request, not its code.)

## Decision Drivers

- A violation an engineer reads and agrees with, or records with a reason.
- An exception that cannot outlive the edge it excuses, or hide new ones.
- Adoption on a code base that does not conform, without a store that rots.
- No new dependency for `asbuilt-core`, and nothing that rewrites a user's
  `asbuilt.toml`.

## Decisions

### 1. Direct edges only

A rule is checked against each relation on its own. A chain through an
`exempt` member, or through any other group, is not an edge between the
groups at its ends. This keeps the stale-exception check cheap (an `allow`
is stale when it excuses no violation in one pass, the check import-linter
finds hard because it follows chains), and it avoids the noise of
import-linter #193. The loophole import-linter closes with chains is
narrower here, because every member of the scope must be in a group or
`exempt` (the plan's mapping check). The docs say that an `exempt` member
is not checked and a path through it is not followed.

*Alternative:* follow chains through `exempt` members only, which
import-linter #244 asks for and no tool has.

### 2. Every relation kind counts for the rules

A type named in a signature is as much a dependency as a call, and half of
all relations are `names` or `uses` only. Since the model keeps only the
strongest kind per pair, filtering by kind could only drop whole pairs.

### 3. Peers in one layer are independent

As the plan has it, and as import-linter's `|` and tach's default have it.
Two groups that need to use each other are one group. A way to mark peers
that may (import-linter's `:`) waits for someone to ask.

### 4. An exception names an edge, needs a reason, and fails when stale

As the plan has it: `allow` with `from`, `to`, and `reason`, where an empty
`reason` is a config error and an `allow` that excuses no violation is
itself a violation. This is stricter than any of the four tools (none
requires a reason, and import-linter's stale check asks only whether the
import exists).

An exception is a top-level `[[conform.allow]]` entry that names its
`architecture`. Nested under `[[conform.architecture]]`, a pasted entry
would belong to whichever architecture precedes it in the file, so where it
was pasted would decide what it excuses.

`from` and `to` match by prefix, as the plan has it, so one entry can excuse
`server.object_factory`'s 25 edges into `protocol`. Prefix matching is the
wildcard of import-linter #375: a new edge under the same prefixes passes
silently. Two things answer it. `conform` lists every edge each exception
excused, in its text and its JSON, and from slice 4 each excused edge is
drawn on the architecture view, so a new one changes `model.c4` and shows
in the diff `check` gates.

*Alternative:* exact ids only, one entry per relation, which is verbose on
a code base like playwright-rust's and puts a renamed module's exceptions
out of date.

### 5. Adoption by exceptions, with no baseline file

No frozen store and no count. When `conform` finds violations, its text
output ends with a `[[conform.allow]]` entry for each, ready to paste
anywhere in `asbuilt.toml`, with `reason = ""`. An entry pasted as it is
fails as a config error until someone writes the reason, so the ratchet is
a list of decisions rather than a snapshot. `conform` never writes
`asbuilt.toml` itself. A code base moving toward an architecture can also
run `conform` outside its hooks until the list is short.

*Alternative:* a recorded baseline that fails only on new violations
(ArchUnit's freeze, dependency-cruiser's `--ignore-known`). It is quicker to
start with, and the trackers above show how it is forgotten.

### 6. Cycles count strong relations only

The cycle finding (slice 2) looks at `implements`, `constructs`, and `calls`
relations. Two modules whose types name each other are common in a data
model and in a mirror of a remote protocol, and counting them made
playwright-rust's `protocol` one cycle of 32 modules, a finding nobody can
act on or write a `cycle_allow` for. A finding stays a warning by default,
as the plan has it.

*Alternative:* every kind, with `cycles = "warn"` absorbing the noise.

### 7. No rule through a facade

The model resolves a `pub use` to the module that defines the item, so a
rule such as "only through `lib.rs`" cannot be written against it. None of
the four workspaces has such a rule. It waits for a request.

### 8. No dependency on the other tools

As the plan has it. None of them has the combination asbuilt needs, and the
two that resolve semantically (cargo-modules and cargo-pup) carry
rust-analyzer's or rustc's crates. The edges the model misses (a value
reached through a method chain whose type is never named) are not counted:
cargo-modules cannot count them, and rust-analyzer's SCIP index, which
resolves method calls, is the way to count them later. Until then the docs
say `conform` catches named references.

### 9. The architecture stays in `asbuilt.toml`, not in the code

The rules could in principle be written in the code, as attributes on each
module or item, with a crate of macros (a `hexagonal` crate, say) that
fails the build when a module breaks its ring. Rust has no mechanism that
makes this work. Each probe below ran on rustc and clippy 1.100.

- **An attribute macro sees only the item it decorates.** On `mod
  file_module;` it compiles on stable and receives exactly `mod
  file_module;`, none of the file's contents. On an item it sees that
  item's tokens with no name resolution, so it cannot tell what `use
  super::x` refers to or which ring `x` is in. Nothing can require the
  attribute either, so "every module is decorated" needs an external check.
  `layered-crate` began as a proc-macro and moved, in 0.3.0, to a CLI that
  reads a `Layerfile.toml`.
- **Visibility runs along the module tree.** `pub(in path)` names only an
  ancestor (`E0742`, as RFC 1422 was implemented in 2016), so it can hide
  an item outside a subtree but cannot say that one sibling may use another
  and not the reverse. Internals threads asking for `pub(in a, b)` (2020,
  2024) went nowhere, and the `pub(api)` pre-RFC (April 2026) caps how far
  an item escapes rather than granting access sideways.
- **The language constraints that are coming govern implementers and
  writers, not dependencies.** RFC 3323's `impl(crate)` and `mut(crate)`
  are on nightly, in a call for testing since August 2026; RFC 3516's
  `exported_private_dependencies` lint (nightly) stops a private
  dependency's types leaking through a public API. No RFC, pre-RFC, or
  project goal proposes module or crate dependency rules.
- **Custom lints need nightly.** Dylint builds against `rustc_private`,
  `register_tool` is unstable, and the 2026 goal on Clippy lints chose to
  contribute to Clippy over a stable lint API.

What stable Rust does enforce, and how it sits beside `conform`:

- **A crate per ring** is the strongest check: a crate can name only what
  it declares, and Cargo refuses a cycle. It does not stop a sideways or
  layer-skipping edge between crates, or a new third-party dependency in
  the domain, which slice 3 checks.
- **`#![no_std]`** keeps `std::fs` and `std::net` out of a domain crate
  with a compile error. One `extern crate std;` line undoes it silently
  (no rustc or clippy lint flags it), so a hard version means checking the
  crate against a target with no `std`. That is untested here.
- **Clippy's `disallowed-methods` and `disallowed-types`**, from a
  `clippy.toml` in a crate's own directory, apply to that crate alone, and
  `disallowed-methods` resolves through type inference
  (`std::env::temp_dir().exists()` is caught as `Path::exists`). Every
  item is listed by path (`std::fs::*` and `std::fs` are both rejected), a
  type is caught only where it is named, and an `#[allow]` lifts either. A
  crate's own items can be banned this way and allowed per module, which
  gives direction inside a crate one item at a time.
- **`cargo::unused_dependencies`** (warn by default since Cargo 1.100)
  reports a declared crate the code does not use, which is the gap slice 3
  covers from the other side.

None of these can say "modules in this group may use that group and not
the reverse" inside a crate, which is what `conform` is for. They are
worth using beside it: a crate per ring and a `no_std` domain where the
code allows, and `conform` for the direction between modules and the
groups the compiler cannot see.

## Consequences

### Positive

- Every violation and exception is per edge and per id, with no line
  numbers and no message text, so refactors do not churn the config the
  way ArchUnit's store churns.
- An exception cannot outlive its edge, and from slice 4 cannot hide a new
  one from review.
- rust-service-template gets back the whole rule its test carried with
  slices 1 and 3 and no preset.

### Negative

- A layer can be bypassed through an `exempt` member.
- Adopting the check on a code base with many violations means pasting one
  `allow` per violation and writing a reason for each, which is slower than
  freezing a baseline.
- A cycle through types alone is not reported.

### Risks and mitigations

- *Prefix exceptions hide growth before slice 4.* The excused edges are
  listed in every run; slice 4 puts them in the model.
- *The model misses method-chain edges.* The docs say what is caught; the
  count waits on a SCIP-based check.

## Validation

- Slice 1's acceptance tests, and asbuilt's own two architectures declared
  and conforming.
- Slice 3 released, and rust-service-template's rule written as above,
  conforming in its CI.

## References

- import-linter layers contract: <https://import-linter.readthedocs.io/en/stable/contract_types/>
- tach: <https://github.com/tach-org/tach>
- dependency-cruiser rules and baseline: <https://github.com/sverweij/dependency-cruiser/blob/main/doc/cli.md>
- Nx `enforce-module-boundaries`: <https://github.com/nrwl/nx/issues/1018>
- ArchUnit `FreezingArchRule`: <https://www.archunit.org/userguide/html/000_Index.html>
- PHPStan baseline: <https://phpstan.org/user-guide/baseline>
- cargo-modules graph builder: <https://github.com/regexident/cargo-modules/blob/main/src/graph/builder.rs>
- cargo-pup: <https://github.com/DataDog/cargo-pup>
- rust_arkitect: <https://github.com/pfazzi/rust_arkitect>
- layered-crate: <https://github.com/Pistonite/layered-crate>
- Visibility and privacy (the ancestor rule): <https://doc.rust-lang.org/reference/visibility-and-privacy.html>
- RFC 3323, restrictions: <https://rust-lang.github.io/rfcs/3323-restrictions.html>
- RFC 3516, public and private dependencies: <https://rust-lang.github.io/rfcs/3516-public-private-dependencies.html>
- `pub(api)` pre-RFC: <https://internals.rust-lang.org/t/pre-rfc-pub-api-visibility-and-static-enforcement-of-crate-internal-api-boundaries/24184>
- Clippy lint configuration: <https://doc.rust-lang.org/clippy/lint_configuration.html>
- Cargo lints: <https://doc.rust-lang.org/cargo/reference/lints.html>
