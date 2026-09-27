# Setup Checklist

After creating a repo from this template, complete these steps.

## Automated (already in the template)

- [x] GitHub Actions CI: test, security, release workflows, a daily advisory monitor
- [x] Dependabot: weekly cargo and GitHub Actions updates, auto-merged behind the required checks
- [x] Pre-commit hooks via prek
- [x] cargo-deny configuration (the one advisory ignore list)
- [x] Tracked Claude Code sandbox settings (`.claude/settings.json`)
- [x] An agent skill scaffold under `skills/` (hidden from installers until you ship it), with its version guard
- [x] CHANGELOG scaffold
- [x] CLAUDE.md with development commands

## Search and replace

- [x] Replace `my-project` with your project name in `Cargo.toml`, `README.md`, and `CLAUDE.md`
- [x] Replace `OWNER/REPO` with your GitHub path in `Cargo.toml` and `CHANGELOG.md`
- [x] Replace `Your Name` with your name in `Cargo.toml`, and `Paul Adamson` in `LICENSE-MIT`
- [x] Decide about `skills/my-project`: ship it under your crate's name, or delete it (see "Agent skill" below). Shipped as `skills/asbuilt`
- [x] Update `description`, `categories`, and `keywords` in `Cargo.toml`
- [x] Update `README.md` with your project description and usage
- [x] Update `CLAUDE.md` with project-specific context
- [x] Strip `<!-- Template users: ... -->` and similar meta-comments from `README.md` and `CLAUDE.md` after the placeholder substitutions

## GitHub Settings (manual)

These settings cannot be configured via code and must be set in the GitHub UI.

### Repository settings

- [ ] **Settings > General > Features:** Enable "Issues" and "Projects" if not already
- [ ] **Settings > Branches > Branch protection:** Add rule for `main`.

  **Apply protection only after the first PR has merged** — otherwise the rules block the very PR that first makes the required checks real.

  **Require every PR-triggered job.** The required checks are the entire
  merge gate for Dependabot: `dependabot-auto-merge.yml` enables auto-merge
  with `GITHUB_TOKEN`, and merges made with that token trigger no push run
  on `main`, so nothing tests a bump after it lands. A job left off this
  list (Supply Chain Review is the classic omission) means a bump merges
  with that job red.

  - `MSRV Check` — cheap; catches accidental use of post-MSRV features
  - `Lint` — fmt, clippy, doctest, cargo doc (ubuntu-only)
  - `Test on ubuntu-latest`, `Test on macos-latest`, `Test on windows-latest`
  - `License & Dependency Check`, `Supply Chain Review`
  - `Mutation Testing (diff)`, `Skill version guard`
  - `LikeC4 validate` — the `#[ignore]` tests that need `npx likec4`, ubuntu-only
  - `Excluded crates compile` — the site crates, which no `--workspace` command sees
  - `Build, dogfood-gate, and deploy` — the Pages job, which gates on the playwright-rs test

  **Leave "require branches to be up to date" off** (`strict: false`).
  With it on, the second green Dependabot PR of the week is stale the moment
  the first merges, auto-merge never updates branches, and Dependabot only
  rebases on a conflict, so PRs sit blocked until someone clicks "update
  branch" and every rebase re-runs CI. Off, each green PR merges once. The
  residual risk (two green PRs that conflict semantically without
  conflicting in git) is negligible for Dependabot: cargo PRs always collide
  on `Cargo.lock` and get rebased and re-tested, and an actions bump beside
  a cargo bump touches disjoint files.

  Either click through the UI, or apply via `gh api` (requires `admin:repo` scope — run `gh auth refresh -s admin:repo` first if needed):

  ```bash
  gh api -X PUT repos/padamson/asbuilt/branches/main/protection \
    --input - <<'JSON'
  {
    "required_status_checks": {
      "strict": false,
      "checks": [
        {"context": "MSRV Check"},
        {"context": "Lint"},
        {"context": "Test on ubuntu-latest"},
        {"context": "Test on macos-latest"},
        {"context": "Test on windows-latest"},
        {"context": "License & Dependency Check"},
        {"context": "Supply Chain Review"},
        {"context": "Mutation Testing (diff)"},
        {"context": "Skill version guard"},
        {"context": "LikeC4 validate"},
        {"context": "Excluded crates compile"},
        {"context": "Build, dogfood-gate, and deploy"}
      ]
    },
    "enforce_admins": false,
    "required_pull_request_reviews": null,
    "restrictions": null,
    "required_linear_history": false,
    "allow_force_pushes": false,
    "allow_deletions": false,
    "required_conversation_resolution": true
  }
  JSON
  ```

### GitHub Pages

- [ ] After the first push to `main` with `pages.yml` has created the
  `gh-pages` branch: **Settings > Pages > Build and deployment > Source:
  Deploy from a branch > Branch: `gh-pages` / (root)**, or
  `gh api -X POST repos/padamson/asbuilt/pages -f build_type=legacy -f source[branch]=gh-pages -f source[path]=/`.
  The site is served at https://padamson.github.io/asbuilt/; the workflow
  writes `/asbuilt/dev/` from main and `/asbuilt/vX.Y.Z/` per release
  (`docs/versioned-site.md`).

### Code security

- [ ] **Settings > Code security and analysis** (`/settings/security_analysis`): Enable:
  - Private vulnerability reporting
  - Dependabot alerts
  - Dependabot security updates
  - Dependabot malware alerts
  - Code scanning (CodeQL) via default setup
  - Secret scanning
  - Push protection

### crates.io Trusted Publishing

The release workflow publishes with a short-lived token minted from
GitHub's OIDC identity for this repo and this workflow. There is no
`CARGO_REGISTRY_TOKEN` secret to create, leak, or rotate.

- [ ] On crates.io, for each of `asbuilt-core`, `asbuilt-rust` and `asbuilt`: **Settings > Trusted Publishing > Add a GitHub publisher** with owner `padamson`, repository `asbuilt`, workflow `release.yml`, and no environment (unless you add one below). The workflow runs `cargo publish --workspace`, so all three need the entry.

  A crate that has never been published has no settings page yet. Publish
  the first version from your machine with a token scoped to
  `publish-new`, then add the trusted publisher and revoke the token; every
  later release goes through the workflow.

  If the publisher is missing or misnamed, the `Authenticate to crates.io`
  step fails and the run is red. That is the intended signal. The workflow
  has no "skip if not configured" path and no `continue-on-error` on the
  publish, because both turn a broken release into a green run with no
  crate.

### Environments (optional)

- [ ] Create `release` environment with required reviewers if you want manual approval before publishing

## Local setup

### Install tools

```bash
cargo install cargo-nextest cargo-deny cargo-vet cargo-mutants cargo-semver-checks prek
```

If any `cargo install` fails with a rustc version mismatch (`requires rustc X.Y.Z or newer, while the currently active rustc version is ...`), run `rustup update stable` — or run `cargo install` from outside any directory containing a `rust-toolchain.toml` override. `cargo install` uses the *active* toolchain, which is directory-scoped when a toolchain file is present in the working tree.

### Enable pre-commit hooks

```bash
prek install --overwrite
```

`--overwrite` replaces any pre-existing `.git/hooks/pre-commit` from the
legacy Python `pre-commit`. Without it, prek keeps the old hook at
`.git/hooks/pre-commit.legacy` and runs both — every check fires twice.

Run this by hand once per clone. It writes into `.git/hooks`, which the
Claude Code sandbox protects, so a session can't do it for you.

### Verify everything works

```bash
cargo build
cargo nextest run
cargo deny check
```

### Initialize cargo-vet

See the cargo-vet section below.

## cargo-vet initialization

Until `cargo vet init` has run, both the pre-commit `cargo vet` hook and
the CI `Supply Chain Review` job print a skip message and pass, so the
first commit and push of a new repo are not blocked on this section.

After your first `cargo build`, initialize supply chain auditing:

```bash
cargo vet init
cargo vet import bytecode-alliance
cargo vet import embark-studios
cargo vet import fermyon
cargo vet import google
cargo vet import isrg
cargo vet import mozilla
cargo vet import zcash
```

`cargo vet import <name>` resolves the canonical URL for each known
trusted org — no need to pin a URL. Together, these seven imports
absorb the bulk of common transitive dependencies (in one dogfooding
run on a real project they converted 142 exemptions into real audits
without any maintainer work).

If your crate has no third-party dependencies yet, `cargo vet` passes immediately with nothing to audit. The imports above pre-seed trusted audit sets so that when you add your first dependency, the audits are already in place — you can safely defer the `cargo vet import ...` commands until then.

### Transitively trust publishers your imported orgs trust

Bigger leverage on top of imports. The orgs you've imported have
already certified specific *individual* publishers. `cargo vet trust`
extends those certifications to your tree — no first-hand audit work.

Run `cargo vet suggest` to see candidates — it surfaces publishers
already trusted by 2+ of your imported orgs. High-leverage names to
apply by default (broadly endorsed, ship widely-used crates):

```bash
cargo vet trust --all seanmonstar    # hyper, reqwest, mime_guess, ...
cargo vet trust --all BurntSushi     # regex, regex-syntax, csv, ...
cargo vet trust --all epage          # clap, toml, anstream, ...
cargo vet trust --all kennykerr      # windows-rs, windows-targets, ...
cargo vet trust --all Lokathor       # bytemuck, ...
```

For multi-maintainer flagship packages (clap, tower, windows-*), add
`--allow-multiple-publishers` so a release by any of the maintainers
counts (not just the trusted one):

```bash
cargo vet trust --all epage --allow-multiple-publishers
cargo vet trust --all seanmonstar --allow-multiple-publishers
cargo vet trust --all kennykerr --allow-multiple-publishers
```

In dogfooding on a real project, this took **304 → 237 exemptions**
(a further 22% reduction on top of imports) with zero audit work.

Then exempt any remaining unaudited dependencies:

```bash
cargo vet
# Follow the prompts to exempt crates
```

### Per-PR workflow for unvetted entries

**Never auto-regenerate exemptions in CI.** A Supply Chain Review
failure on a Dependabot PR is the *signal* that new unvetted code is
arriving — pause and decide before merging. Auto-regenerating
rubber-stamps every upstream change and defeats the point of
`cargo vet`.

For each unvetted entry in a failing PR:

- **`cargo vet certify <crate> <version>`** — gold standard. Read the
  diff, attest, commit the new audit onto the PR branch.
- **`cargo vet regenerate exemptions`** — bronze standard. Acceptable
  for patch bumps from known publishers; not for new transitives or
  majors. Commit the config change onto the PR branch.
- **Reject** if the diff looks sketchy.

## Agent skill

`skills/asbuilt/` is the shape of the skill this crate would ship:
the directory `npx skills add padamson/asbuilt` installs into a consumer's
repo, so an agent there knows how to use the crate. As scaffolded it is
hidden from installers (`metadata.internal: true`) and nothing loads it.
Decide now:

- [x] **Ship it.** Rename the directory to your crate name, remove the
  `internal` line from `SKILL.md`, write it for an agent that has never
  seen the crate, and fill `references/`. Every later content edit bumps
  `metadata.version`; the pre-commit hook and the `Skill version guard`
  CI job refuse one that doesn't. Keep the "Agent skill" block in
  `README.md`.

  Only if this repo uses its own crate (examples, an e2e suite, docs that
  exercise it) is it worth auto-loading the skill in-repo too. Claude
  Code discovers project skills at `.claude/skills/` alone, so add one
  tracked symlink and un-ignore it:

  ```bash
  ln -s ../../skills/<name> .claude/skills/<name>
  echo '!/.claude/skills/<name>' >> .gitignore
  ```

- [ ] **Don't ship one.** Delete `skills/`,
  `scripts/check-skill-version-bumped.sh`, the `skill-version-bumped`
  hook in `.pre-commit-config.yaml`, the `skill-version` job in
  `test.yml`, and the "Agent skill" block in `README.md`, and drop
  `Skill version guard` from the required checks.

Skills of the tools this crate depends on are the other direction, and
CLAUDE.md covers them: `npx skills add <owner>/<repo>` once per clone,
content gitignored, pin tracked in `skills-lock.json`.

## Keeping `target/` under control

A project that builds many permutations (default, `--all-features`,
per-target, mutation testing, fuzzing) accumulates tens of gigabytes
under `target/`, most of it incremental-compilation cache with no
payoff outside a single edit-rebuild loop.

- **Sweep periodically.** `cargo install cargo-sweep`, then
  `cargo sweep --time 15` drops artifacts unused for 15 days, and
  `cargo sweep --installed` keeps only the current toolchain's.
- **Consider turning incremental off** once the permutation count grows:
  `[build] incremental = false` in `.cargo/config.toml`. CI already has
  it off (`Swatinem/rust-cache` sets `CARGO_INCREMENTAL=0`).
- **Build scripts that download large assets** (drivers, models,
  fixtures) must not write them into `OUT_DIR`: its path carries a
  per-toolchain, per-feature hash, so every permutation re-downloads and
  keeps its own copy. Write to a stable, version-keyed path outside
  `target/` (env-selectable, defaulting to `OUT_DIR` for zero-config
  local use), give the script a skip-download env var for compile-only
  jobs like MSRV, and cache that path in CI with its own `actions/cache`
  keyed on the asset version. `Swatinem/rust-cache` deliberately does not
  cache workspace crates' build-script output.

## Converting to a workspace

The template ships a single crate on purpose: one `Cargo.toml`, one
`src/lib.rs`, every CI invocation already carrying `--workspace` so
nothing needs to change when a second crate appears. Convert when a
second crate has a reason to exist (a `-core` library behind a CLI,
a proc-macro crate, a separate binary), not before.

1. `mkdir -p crates/<name>` and `git mv src tests crates/<name>/`.
2. Turn the root `Cargo.toml` into a virtual manifest:
   `[workspace] members = ["crates/*"]`, `resolver = "3"`, a
   `[workspace.package]` block holding `edition`, `rust-version`,
   `license`, `repository`, `authors`, and a `[workspace.dependencies]`
   table. Move the crate's own `[package]` to `crates/<name>/Cargo.toml`
   with `edition.workspace = true` and friends.
3. `.cargo/mutants.toml`: `examine_globs = ["crates/*/src/**/*.rs"]`.
   `deny.toml` already allows versionless path deps between private
   workspace members.
4. `release.yml` publishes one crate per tag with `cargo publish`; a
   workspace that publishes several needs `-p <crate>` per step, a
   trusted-publisher entry on crates.io for each, and a tag scheme that
   says which crate a tag releases.
5. Run `cargo nextest run --workspace`, `cargo clippy --all-targets
   --all-features -- -D warnings`, `cargo deny check`, and
   `./scripts/mutants.sh --working --list` before committing; the last
   one confirms the mutants config still resolves files.

## Reporting the build commit from `--version`

For a binary people install from source (`cargo install --git`, or
`--path`), make `--version` say which commit it was built from when it
is not a tagged release. Three cases: no git available (a crates.io
install) prints the bare crate version; HEAD exactly at the tag
`v<version>` prints the bare version, since that is the release; anything
else prints `<version> (<short sha>)`. No dirty-tree marker: nothing
re-runs the build script on an uncommitted edit, so it would be wrong as
often as right.

Add a `build.rs`:

```rust
fn main() {
    let version = std::env::var("CARGO_PKG_VERSION").unwrap_or_default();
    let build_id = match git_short_sha() {
        Some(sha) if !at_release_tag(&version) => format!("{version} ({sha})"),
        _ => version,
    };
    println!("cargo:rustc-env=CRATE_VERSION_WITH_BUILD={build_id}");

    // Freshness: `.git/HEAD` covers checkouts and branch switches; the
    // branch's ref file covers commits. Use `../../.git` from a workspace
    // member.
    let git_dir = std::path::Path::new(".git");
    if !git_dir.exists() {
        return;
    }
    println!("cargo:rerun-if-changed=.git/HEAD");
    if let Ok(head) = std::fs::read_to_string(git_dir.join("HEAD"))
        && let Some(reference) = head.strip_prefix("ref: ")
        && git_dir.join(reference.trim()).exists()
    {
        println!("cargo:rerun-if-changed=.git/{}", reference.trim());
    }
}

fn git_short_sha() -> Option<String> {
    let out = std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let sha = String::from_utf8(out.stdout).ok()?.trim().to_string();
    (!sha.is_empty()).then_some(sha)
}

fn at_release_tag(version: &str) -> bool {
    std::process::Command::new("git")
        .args(["describe", "--exact-match", "--tags", "HEAD"])
        .stderr(std::process::Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .is_some_and(|tag| tag.trim() == format!("v{version}"))
}
```

With clap derive: `#[command(version = env!("CRATE_VERSION_WITH_BUILD"))]`.

Test the shape, not the environment, so the same test passes in a dev
checkout and in a tagged release build: the output starts with
`CARGO_PKG_VERSION`, and any suffix matches exactly ` (<7+ hex digits>)`.

## Release ceremony (final step — cut vX.Y.Z)

Only run this after every box above is checked and your first feature PR has merged.

1. Promote `## [Unreleased]` in `CHANGELOG.md` to `## [X.Y.Z] - YYYY-MM-DD` (use today's date); leave a fresh `## [Unreleased]` above it for future work.
2. Update the compare links at the bottom of `CHANGELOG.md`:
   - Change `[Unreleased]` to `https://github.com/padamson/asbuilt/compare/vX.Y.Z...HEAD`.
   - Add `[X.Y.Z]: https://github.com/padamson/asbuilt/releases/tag/vX.Y.Z`.
3. Update the version in the root `Cargo.toml` to `X.Y.Z` (`workspace.package.version` and the two path-dep entries).
4. Remove this file — you won't need it again: `git rm SETUP.md`.
5. Commit, tag, push:
   ```bash
   git add Cargo.toml CHANGELOG.md
   git commit -m "Release vX.Y.Z"
   git tag vX.Y.Z
   git push origin main --tags
   ```
6. Watch the Release workflow. On success, verify end-to-end:
   ```bash
   cargo install <crate> && <crate> --version
   ```
