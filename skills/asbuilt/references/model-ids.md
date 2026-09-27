# Model ids, for curated views

A curated view names generated elements by id, so it helps to know how
the survey spells them.

- **Crate:** the crate name, which is the package name with `-` as `_`
  (`playwright-rs` is `playwright_rs`; a tests-only package
  `playwright-rs-site-e2e` is `playwright_rs_site_e2e`).
- **Module:** `crate.module.submodule`, following `mod` declarations,
  inline modules included. A bin beside a lib is `crate.<bin name>`
  with its modules under it; a bin-only package's `main.rs` modules sit
  directly under the crate.
- **Tests and examples:** `crate.tests` and `crate.examples`, one each
  per crate, whatever the number of targets.
- **Reserved words:** likec4 1.59.3 refuses about fifty words as ids
  (`view`, `views`, `link`, `title`, `kind`, `from`, `import`, `style`,
  `icon`, `size` and more), so a module with one of those names gets a
  trailing `_`: a module `view` is `crate.view_`. `model` and `element`
  are accepted as they are.
- **Externals:** the `id` from `asbuilt.toml`, top level.
- **Generated views:** `index`, and `view_<id with _ for .>` for every
  element with children (`view_playwright_rs_server`). Two ids that
  collapse to one view name get a numeric suffix. Name curated views
  something else; a duplicate view name fails `asbuilt validate`.

Relation kinds available to view predicates: `implements`, `constructs`,
`calls`, `names`, `uses`.

```likec4
// docs/architecture/views.c4
views {
  view context {
    title 'playwright-rust: system context'
    include playwright_rs_site_e2e, playwright_rs_macros, playwright_rs, node_driver, browsers
    autoLayout LeftRight
  }
  view server of playwright_rs.server {
    title 'server: from Playwright::launch to a browser'
    include *, playwright_rs.protocol.playwright, node_driver, browsers
    autoLayout LeftRight
  }
}
```
