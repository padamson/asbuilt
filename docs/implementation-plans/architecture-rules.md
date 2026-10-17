# Architecture rules - Implementation Plan

**Feature:** `asbuilt conform`: architecture patterns checked against the code

**User Story:** As a maintainer who has chosen an architecture (hexagonal,
layered, a modular monolith), I want to map my crates and modules to that
pattern's roles and have asbuilt fail when the code breaks the pattern's
rules, and point out likely problems I did not declare, so that the
architecture I describe is the one the code has and stays that way without
a hand-written fitness test.

**Related ADR (if applicable):** none yet. An ADR should come out of the
research below, before slice 1 starts.

**Approach:** Vertical Slicing with Outside-In TDD

**Status:** Proposed. The design and the slices are a first pass, to be
revised by the research in [Research before slice 1](#research-before-slice-1);
nothing here is built yet.

---

## Implementation Strategy

This implementation follows **vertical slicing** - each slice delivers end-to-end user value and can be tested/released independently.

The roadmap's 0.4.0 item "Architecture rules" (`roadmap-now-rules` in
`crates/site/public/roadmap.json`) is this feature; it is done when slices
1 to 4 have landed. The first consumer is rust-service-template, which
dropped its guppy test of the hexagonal rule when it adopted asbuilt
(`da5e043`, PR #25) and has had the rule unenforced since.

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

### Design (first pass)

These are starting positions from a short look at how ArchUnit (Java),
import-linter (Python), and dependency-cruiser (JavaScript) do the same job.
Each one is open to the research below; the slices assume them only so the
plan has a shape to revise.

**A separate command.** `asbuilt conform` surveys in memory and checks the
fresh model; `check` keeps meaning drift and nothing else. A consumer runs
both: one says the model matches the code, the other says the code matches
the architecture.

**Patterns asbuilt owns, roles the user assigns.** The user does not write
edge rules. They name a pattern and say which model ids (crates, or modules
as `crate.module`) play each of its roles; asbuilt knows the pattern's
rules. A member stands for itself and everything under it. Each pattern
compiles to a few primitives (may-use, independence, acyclic, an external
allowlist), which leaves room to expose the primitives later for an
architecture no pattern fits.

**One engine, two kinds of evidence.** The engine checks one edge list
built from:

- the model's relations, which work for any language and at module level,
  and carry the item names that make an edge (`svc_persistence.repo` names
  `svc_application.RankItems`); and
- each crate's declared Cargo dependencies, `[dependencies]` and
  `[build-dependencies]` on every platform, with `[dev-dependencies]`
  exempt. These come from the `cargo metadata --no-deps --offline` call
  discovery already makes (it lists each package's declared dependencies
  with no resolve and no lockfile), and they cover what the source walk
  cannot see: a crate reached only inside a macro body in another syntax, or
  through method calls on values, and the external crates themselves.

Each violation names which evidence produced it. Neither the resolved graph
(a full multi-platform resolve, which broke the guppy test this replaces)
nor compiler-level analysis is used.

**Violations and findings.** A violation breaks a rule the config declared,
and `conform` exits 1. A finding is a likely problem nobody declared (a
cycle, later a dependency on something less stable); it is reported and
exits 0 unless the config raises it to an error.

**The mapping is checked both ways.** A crate in no role and a role entry
that names nothing are both violations, so the table cannot drift from the
workspace in either direction. Crates outside the pattern (a landing site,
an `xtask`) are listed as `exempt`.

**Exceptions carry a reason and must match.** An allowed edge is written
with `from`, `to`, and `reason`; one that no longer matches an edge is a
violation, so fixing the code retires the exception instead of leaving a
stale pass behind.

**Exit codes and streams match `check`.** 0 conforms, 1 violation, 2 error.
Text goes to stdout with the verdict on stderr; `--format json` writes one
object to stdout, a public contract documented in `cli.md`.

---

## Research before slice 1

The goal is a feature engineers keep turned on, which means rules that
match how they already think about their architecture, violations they
agree with, and a way to adopt it on a code base that does not conform
yet. The first pass above answers none of that from evidence. In order:

1. **What existing users keep and what they abandon.** Read the issue
   trackers, discussions, and docs of the tools that do this, for the rule
   shapes people use, the complaints (false positives, config churn,
   slowness), and the features they asked for that never landed: ArchUnit
   and ArchUnitNET, NetArchTest, Konsist (Kotlin), import-linter and
   pytestarch (Python), dependency-cruiser and Nx's
   `enforce-module-boundaries` (JavaScript and TypeScript), deptrac (PHP),
   Bazel's `visibility`, and on the Rust side `cargo-deny`'s `wrappers`,
   `cargo-modules`, and any fitness-test crates.
2. **Patterns against tags.** Nx's model is generic: tag each project
   (`scope:billing`, `type:domain`) and declare which tags may depend on
   which. It covers hexagonal, layered, and modular monoliths with one
   mechanism and lets a project carry two orthogonal tags. Compare it with
   pattern presets for how easily a reader can see the intended
   architecture in the config, and whether presets should be shorthand
   that compiles to tags.
3. **Which edges count in Rust.** Run a throwaway script over the
   committed `model.c4` of real workspaces (this one, playwright-rust,
   panschema, cqa-grader, rust-service-template) with candidate rules, and
   read every edge it flags. The questions it answers: whether each
   relation kind (`implements`, `constructs`, `calls`, `names`, `uses`)
   should count, how a facade's `pub use` re-export should be treated, and
   how often the manifest catches an edge the relations miss.
4. **Adopting on a code base that does not conform.** Compare
   per-edge exceptions with reasons (import-linter's `ignore_imports`, which
   fail when stale) with a recorded baseline that fails only on new
   violations (ArchUnit's freeze, dependency-cruiser's `--ignore-known`).
   The second is easier to start with and easier to forget about.
5. **Where the result is read.** A pre-commit hook and a CI gate, a
   comment on the pull request (with `asbuilt diff`, the other 0.4.0 Now
   item), and the documentation tree, where a violating edge could be drawn
   in its own style on the views it appears in. Find out which of these
   people act on.
6. **Across languages.** How the same config reads when the Python
   front-end (0.5.0) supplies the model, and whether asbuilt should
   interoperate with import-linter's contracts for Python instead of
   replacing them.
7. **One real consumer end to end.** Write rust-service-template's rule in
   each candidate config shape and check that it restores everything its
   guppy test asserted (domain on nothing, external crates included;
   application and persistence on domain only; api on anything; every
   crate in the table), with nothing that test got wrong.

The output is an ADR recording the choice on each point and its reasons,
and a revision of the slices below to match.

---

## Vertical Slices

### Slice 1: hexagonal across crates

**Status:** Not Started

**User Value:** A workspace declares `[conform.hexagonal]`, and `asbuilt
conform` fails on any crate or module edge that breaks the pattern, naming
the edge, the rule, and the evidence.

The roles and their rules:

| Role | May use | Notes |
|---|---|---|
| `domain` | `domain` | external crates restricted in slice 2 |
| `application` | `application`, `domain` | |
| `driving` | `application`, `domain` | the adapters that call in (an HTTP API, a CLI) |
| `driven` | `domain` | the adapters called out to (persistence, a client), through ports the domain defines |
| `root` | anything | the composition root that wires the adapters together |

Within a role, a member may use itself; one adapter member using another
(two `driving`, two `driven`, or one of each) is a violation. Nothing may
use `root`. The split between `driving`, `driven`, and `root` is where this
differs from ArchUnit's onion preset, which has one adapter role. In
rust-service-template the `api` crate is both the HTTP adapter and the
composition root that constructs the persistence adapter, so with one
adapter role `api -> persistence` would fail as adapter to adapter, and
`persistence -> application`, which the template forbids, would pass.

```toml
[conform.hexagonal]
domain      = ["svc_domain"]
application = ["svc_application"]
driving     = []
driven      = ["svc_persistence"]
root        = ["svc_api"]
exempt      = ["svc_xtask"]

[[conform.allow]]
from   = "svc_persistence.legacy"
to     = "svc_application"
reason = "the v1 import path; removed with the v1 API"
```

**Acceptance Criteria:**
- [ ] `asbuilt conform` with a `[conform.hexagonal]` table exits 0 when every edge between mapped members is allowed
- [ ] A relation that breaks a rule exits 1 and reports the from and to members, both roles, the rule, and the relation (its source and target ids, kind, and items)
- [ ] A declared `[dependencies]` or `[build-dependencies]` entry on another workspace crate that breaks a rule is a violation even when no relation backs it, and is reported as manifest evidence (the table and the dependency name)
- [ ] A `[dev-dependencies]` entry, and a relation whose source is a `tests` or `examples` element, is never a violation
- [ ] A crate in no role and not in `exempt` is a violation; so is a role entry that names no element
- [ ] A member listed in two roles is a config error (exit 2) naming it and both roles
- [ ] A `[[conform.allow]]` entry suppresses the violations its `from` and `to` match; one that matches no violation is itself a violation
- [ ] `conform` with no `[conform]` table is an error that says so (slice 3 changes this to findings only)
- [ ] `--format json` writes one object: `conforms`, `pattern`, `violations` (each with `rule`, `from`, `to`, `from_role`, `to_role`, and `evidence`), `mapping` (unmapped crates and unknown entries), and `exceptions` (each with whether it matched)
- [ ] The emitted `.c4` is byte-identical before and after, for asbuilt itself and the consumer fixture: the new model field is never written
- [ ] asbuilt's own `asbuilt.toml` declares the pattern (`domain = ["asbuilt_core"]`, `driven = ["asbuilt_rust"]`, `root = ["asbuilt"]`, the two site crates `exempt`) and conforms; `asbuilt-rust` implements the `Frontend` port that `asbuilt-core` defines, which is the driven-adapter shape

**Core Library Implementation (`asbuilt-core`):**
- [ ] `model.rs`: `Model::dependencies: Vec<Dependency>`, where a `Dependency` has `from: Id`, `to: DependencyTarget` (`Element(Id)` or `External(String)`, the package name), `table: DependencyTable` (`Normal` or `Build`), and `platform: Option<String>`; `normalize` sorts and deduplicates them and `merge` appends them
- [ ] `emit.rs` untouched; a test holds that a model with dependencies emits the same text as one without
- [ ] `config.rs`: `Config::conform`, parsed with `deny_unknown_fields`; `[conform]` added to `OWN_TABLES`; a role or `exempt` entry is a model id as the file writes it
- [ ] New module `conform.rs`: `conform(&Model, &ConformConfig) -> Report`, pure, with `Report`, `Violation`, `Rule`, `Evidence`, and `MappingProblem` as values the CLI formats
- [ ] Errors for the config cases in `error.rs`, each carrying the offending entry

**Core Library Unit Tests:**
- [ ] One test per rule in the role table, each a model built by hand with exactly the edge under test, asserting the `Violation` (or its absence) with `assert_eq!`
- [ ] Membership by prefix: a module under a mapped crate takes the crate's role, and `a.b` is not under `a.bc`
- [ ] Manifest-only and relation-only evidence each produce the violation; both together produce one violation with two pieces of evidence
- [ ] Dev dependencies and `tests` or `examples` sources are exempt
- [ ] Both directions of mapping drift, the duplicate-role error, a matched and an unmatched exception
- [ ] A doctest on `conform` showing a three-crate model and its report

**Rust front-end (`asbuilt-rust`):**
- [ ] `discover.rs`: keep each package's declared dependencies from the metadata already fetched; drop `Development`
- [ ] Map a dependency whose package is a surveyed crate to `DependencyTarget::Element` with that crate's id, and any other to `External` with the package name (a `rename` changes the `use` path, not the package, so the package name is what is recorded)
- [ ] Fixture tests in `tests/fixtures/`: a workspace with a path dependency, a build dependency, a platform-specific dependency, a renamed dependency, and a dev dependency, asserting the dependency list

**CLI Implementation (`asbuilt`):**
- [ ] `Command::Conform { root, format }` in `main.rs`, and `commands::conform`, using `survey_model`
- [ ] Text: one block per violation on stdout (the edge, the rule, then each piece of evidence indented under it), and on stderr the verdict line (`asbuilt: 2 violations of [conform.hexagonal]`); `<root> conforms to hexagonal` on stdout when clean
- [ ] Exit codes `EXIT_OK`, `EXIT_DRIFT` (renamed or aliased so it reads right for a violation), and `EXIT_ERROR`
- [ ] Element kinds in messages through `Frontend::noun`, as `check` does (a crate, a module)

**CLI Integration Tests:**
- [ ] A fixture workspace `crates/asbuilt/tests/fixtures/hexagonal/` with four crates and a committed `asbuilt.toml`: conforming as committed, and each test copies it into a `TempDir` and adds one wrong-way edge
- [ ] Exit code and JSON for a relation violation, a manifest-only violation, an unmapped crate, and a stale exception
- [ ] `conform --help` names the command and its formats
- [ ] `conform` in this repo's own pre-commit config and in `test.yml`, so asbuilt's own pattern is held on every commit

**Documentation:**
- [ ] `config.md`: `[conform.hexagonal]`, the role table, `exempt`, and `[[conform.allow]]`
- [ ] `cli.md`: the command, its streams and exit codes, and the JSON shape
- [ ] README: a `conform` section, and the `asbuilt-conform` hook
- [ ] `.pre-commit-hooks.yaml`: the `asbuilt-conform` hook, with the same `files` pattern as `asbuilt-check`
- [ ] CHANGELOG, and the roadmap entry's blurb rewritten for `conform`

**Notes:**
- `conform` checks the code, not the committed model; a stale model is `check`'s job, so `conform` does not refuse one the way `docs` does.
- Edges to `[[externals]]` elements (a spawned process, a browser) are not code dependencies and are never checked.
- One pattern per config until slice 4; `[conform.layered]` and `[conform.hexagonal]` together are an error until then.
- For review before code: the role key names, `exempt` as the word for crates outside the pattern, and the text layout of a violation.

---

### Slice 2: the domain's external crates

**Status:** Not Started

**User Value:** The inner role can be held to a vetted list of external
crates, so `domain` stays free of I/O, async runtimes, and frameworks.

```toml
[conform.hexagonal]
domain          = ["svc_domain"]
domain_external = ["thiserror"]
```

**Acceptance Criteria:**
- [ ] With `domain_external` set, a `domain` member's declared `[dependencies]` or `[build-dependencies]` on any external crate not in the list is a violation naming the crate and the table
- [ ] Without `domain_external`, external crates are not checked (slice 1's behavior)
- [ ] `domain_external = []` forbids every external crate
- [ ] An entry that names no dependency of any `domain` member is a violation, as a stale exception is
- [ ] A dev dependency is never a violation
- [ ] asbuilt's own config lists `asbuilt-core`'s five (`pulldown-cmark`, `serde`, `similar`, `thiserror`, and `toml`) and conforms

**Core Library Implementation:**
- [ ] The external allowlist primitive in `conform.rs`, reading `DependencyTarget::External`
- [ ] `domain_external` in the hexagonal config

**Library Unit Tests:**
- [ ] An allowed, a forbidden, and a stale entry; the empty list; the key absent

**CLI Implementation:**
- [ ] The external violation's text and JSON form (`evidence` of kind `manifest` with the package name)

**CLI Integration Tests:**
- [ ] The hexagonal fixture with an external crate added to the domain crate

**Documentation:**
- [ ] `config.md`, `cli.md`, CHANGELOG

**Notes:**
- Only direct dependencies count. "Domain pulls in tokio through a crate it uses" needs the resolved graph, which this design rules out; the allowlist is where that judgment is made instead.
- This is the slice rust-service-template waits on: with slices 1 and 2 released it can restore the whole rule its guppy test carried.

---

### Slice 3: cycles, as a finding

**Status:** Not Started

**User Value:** Without declaring anything, `conform` reports each cycle
among sibling modules, with the relations that close it.

```toml
[conform]
cycles = "warn"   # "error" fails on them; "off" skips the check
```

**Acceptance Criteria:**
- [ ] `conform` with no pattern reports findings only, instead of slice 1's error
- [ ] Each strongly connected set of sibling modules (children of the same element) is one finding, listing its members and one relation per edge of a cycle through them
- [ ] `cycles = "warn"` is the default: findings are printed and the exit code is 0
- [ ] `cycles = "error"` makes a cycle exit 1; `"off"` skips the check
- [ ] Crates are not checked (cargo refuses a crate cycle outside dev dependencies)
- [ ] The count on asbuilt itself is recorded in this plan's notes, and either fixed or allowed with a reason

**Core Library Implementation:**
- [ ] The acyclic primitive: strongly connected components over sibling modules, lifted from the model's relations (a relation between descendants counts for their sibling ancestors)
- [ ] `Finding` beside `Violation` in `Report`, with a severity

**Library Unit Tests:**
- [ ] Two-module and three-module cycles, a cycle through nested modules lifted to siblings, an acyclic tree, each severity

**CLI Implementation:**
- [ ] Findings in text after violations, and as `findings` in JSON; the verdict line counts both

**CLI Integration Tests:**
- [ ] A fixture crate with a module cycle, under each severity

**Documentation:**
- [ ] `config.md`, `cli.md`, CHANGELOG

**Notes:**
- Mutual `use` between sibling modules is legal Rust and common, which is why the default is a warning and not an error.
- With this slice the roadmap item's "no cycles" half is in.

---

### Slice 4: layered and modular-monolith patterns

**Status:** Not Started

**User Value:** The two other common architectures are available, and a
pattern can apply to one crate's modules as well as across crates.

```toml
[conform.layered]
strict = false                      # true: a layer may use only the one below it
[[conform.layered.layers]]
name    = "api"
members = ["svc_api"]
[[conform.layered.layers]]
name        = "services"
members     = ["svc_billing", "svc_accounts"]
independent = true                  # the members may not use each other
[[conform.layered.layers]]
name    = "core"
members = ["svc_core"]

[conform.modular]
within   = "svc"                    # apply to this crate's modules
kernel   = ["svc.shared"]
features = ["svc.billing", "svc.accounts", "svc.reports"]
```

**Acceptance Criteria:**
- [ ] Layered: a layer may use the layers below it (only the next one when `strict`); an upward edge is a violation; `independent` forbids edges between the layer's own members
- [ ] Modular monolith: no feature may use another; every feature may use the kernel; the kernel may use no feature
- [ ] `within = "<id>"` scopes a pattern to that element's descendants, so mapping drift counts its children, not the workspace's crates
- [ ] Several patterns in one config are each checked, and each violation names its pattern
- [ ] Mapping drift, exceptions, both kinds of evidence, and the JSON shape work as in slice 1

**Core Library Implementation:**
- [ ] The two patterns as compositions of the existing primitives; `within` as a scope on any pattern

**Library Unit Tests:**
- [ ] Each rule of each pattern, `strict`, `independent`, `within`, two patterns at once

**CLI Implementation:**
- [ ] Pattern names in text and JSON

**CLI Integration Tests:**
- [ ] A fixture per pattern, conforming and with one violation

**Documentation:**
- [ ] `config.md`, `cli.md`, README, CHANGELOG; the roadmap item moves to done at release

**Notes:**
- `within` is import-linter's `containers` idea, and it is the first time a pattern sees modules without the user listing every crate.

---

### Later: dependencies against stability

Robert Martin's stable-dependencies principle as a finding: a crate or
module depending on one less stable than itself (instability as outgoing
over total coupling). It needs the coupling metrics planned for 0.5.0
(`roadmap-next-metrics`), so it waits for them.

---

## Slice Priority and Dependencies

| Slice | Priority | Depends On |  Status |
|-------|----------|------------|--------|
| Research and ADR | Must Have | None | Not Started |
| Slice 1: hexagonal across crates | Must Have | Research and ADR | Not Started |
| Slice 2: the domain's external crates | Must Have | Slice 1 | Not Started |
| Slice 3: cycles, as a finding | Must Have | Slice 1 | Not Started |
| Slice 4: layered and modular-monolith patterns | Should Have | Slice 1 | Not Started |
| Later: dependencies against stability | Nice to Have | 0.5.0 coupling metrics | Not Started |

Slices 2, 3, and 4 each need only slice 1's engine, so after it they can
land in any order; the order above is the waiting consumer's.

---

## Definition of Done

The feature is complete when ALL of the following are true:

- [ ] All acceptance criteria from user story are met
- [ ] Slices 1 to 4 marked as "Completed"
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

- To be filled in as slices land.

### Lessons for Future Features

- To be filled in as slices land.
