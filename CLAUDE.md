# asbuilt

`asbuilt` keeps a LikeC4 architecture model that describes the code as
it is: `asbuilt survey` generates the `.c4` model from a code base, and
`asbuilt check` fails when the committed model no longer matches. The
model is generated, never edited by hand, and the code carries no
annotations for it. `README.md` has the longer description.

## Crates

- `crates/asbuilt-core`: the language-agnostic model, the LikeC4 emitter,
  `asbuilt.toml` parsing, externals, and the drift check. Never spawns a
  process.
- `crates/asbuilt-rust`: the Rust front-end. `cargo metadata` for crates,
  `syn` for references, pure resolution over an in-memory module tree.
- `crates/asbuilt`: the CLI. The only crate that shells out to `npx likec4`
  (validate, export, render), at the version pinned in its `lib.rs`.

One version for the workspace, set in the root `Cargo.toml`.

## Development

```bash
cargo build --workspace
cargo nextest run --workspace
cargo test --doc --workspace
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --all
cargo deny check         # advisories, licenses, bans, sources (one ignore list: deny.toml)
cargo vet                # supply chain review
```

## Claude Code sandbox

The sandbox is on in user settings for every repo, and a session never
writes outside its own working tree. There is no per-repo sandbox file
to look for.

## Pre-commit hooks

```bash
cargo install prek
prek install --overwrite   # --overwrite replaces any legacy pre-commit hook
```

Hooks mirror CI checks: fmt, clippy, check, nextest, doctest, deny, vet,
and the skill version guard.

## Watching CI

```bash
./scripts/ci-watch.sh            # stream job results for HEAD until they finish
./scripts/ci-watch.sh <sha>      # for a specific commit
```

One line per job as it reaches a terminal state, then an exit code:
0 all green, 1 something failed, 2 timed out. A `gh` call inside a
shell loop is not matched by the sandbox's command exclusion, so this
script is excluded as a whole in `.claude/settings.json`.

## Agent skills

`skills/asbuilt/` is the skill this repo ships to consumers (`npx skills
add padamson/asbuilt`). Edits to it must bump `metadata.version` in its
`SKILL.md`; the pre-commit hook and the `Skill version guard` CI job both
enforce that.

Skills of tools this crate depends on are managed installs, not vendored:
`skills-lock.json` (tracked) records each source and a content hash, and
`npx skills add <owner>/<repo>` fetches the content into `.agents/` and
links it from `.claude/skills/`. Both paths are gitignored, so run the
installs once after cloning, from a plain terminal (the sandbox denies
writes under `.claude/skills/`). Verify with `ls -l .claude/skills/`: one
entry per installed skill. `npx skills update`
defaults to Global scope at its prompt; choose Project, and confirm by
reading `metadata.version` out of the installed `SKILL.md`.

## Mutation testing

```bash
./scripts/mutants.sh                 # diff HEAD~1..HEAD (default)
./scripts/mutants.sh main            # diff main..HEAD
./scripts/mutants.sh --working       # diff uncommitted edits against HEAD
./scripts/mutants.sh -- --jobs 4     # pass extra cargo-mutants args
```

`scripts/mutants.sh` wraps `cargo mutants --in-diff`, scoping mutation
testing to just the lines a commit touched. A full-codebase run grows
linearly with codebase size and routinely takes hours; `--in-diff`
keeps the loop fast enough to use while the test is still warm. Use
`--working` to gate edits before committing them (new files need
`git add -N <file>` first to show up in the diff). Don't reach for
`cargo mutants -f <file>` as the fast path: with `examine_globs` set it
ignores the filter and sweeps everything in scope.

Run one mutation job at a time; every run writes `mutants.out/`. A
stray run is stopped with `pkill -f cargo-mutants` (hyphen; the binary
is `cargo-mutants`).

CI runs the per-diff variant on every push and PR (`mutation-testing-diff`
in `security.yml`). The full-codebase job (`mutation-testing`) is
manual-only via `workflow_dispatch` — use it for occasional audits or
big refactors, never on a schedule.

Scope the baseline with `.cargo/mutants.toml`, which ships live (a
`.mutants.toml` at the repo root is ignored silently); `--in-diff`
narrows from there.

## Tests

A test is a claim about behavior, named as the claim, one claim per
test, asserting the value (`assert_eq!` on an enum, `matches!` on the
error variant and what it names) rather than prose. The seed crate shows
the shapes: a decision returned as a value, an error that carries the
offending input, an environment read through an injected lookup, a
`TempDir`-owning fixture and a bounded poll in `tests/common/mod.rs`,
and one `#[ignore = "reason; run with ..."]` test as the gate for
anything a fresh clone may lack. Never `return` from a test because a
precondition is missing.

`.config/nextest.toml` bounds every test with a slow-timeout; the hook
runs the default profile (no retries) and CI runs `--profile ci` (one
retry, reported FLAKY). nextest does not run doctests, so `cargo test
--doc` is its own step, and the crate keeps at least one doctest so that
step is not a no-op.

## Release process

1. Update `workspace.package.version` in the root `Cargo.toml`, and the
   `version` on the `asbuilt-core` and `asbuilt-rust` entries under
   `[workspace.dependencies]` in the same file (crates.io needs a version
   on a path dep)
2. Update `CHANGELOG.md`
3. Commit: `git commit -m "Release vX.Y.Z"`
4. Tag: `git tag vX.Y.Z`
5. Push: `git push origin main --tags`

The tag triggers CI which builds, tests, creates a GitHub Release, and
publishes all three crates to crates.io with `cargo publish --workspace`.

<!-- Add custom skills under .claude/skills/ as needed -->
