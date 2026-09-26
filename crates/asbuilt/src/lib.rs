//! The library half of the `asbuilt` binary.
//!
//! Everything downstream of the model (validation, export, rendering)
//! is LikeC4's, reached by shelling out to `npx likec4` at one pinned
//! version. That pin lives here so the binary and the tests that need
//! LikeC4 read the same constant.

/// The LikeC4 release every `npx likec4` call is pinned to. The emitter
/// facts in `asbuilt-core` were verified against this version.
pub const LIKEC4_VERSION: &str = "1.59.3";

/// The package spec handed to `npx --yes`, so every call resolves the
/// same release.
///
/// ```
/// assert_eq!(asbuilt::likec4_package(), "likec4@1.59.3");
/// ```
pub fn likec4_package() -> String {
    format!("likec4@{LIKEC4_VERSION}")
}

#[cfg(test)]
mod tests {
    // The doctest above makes the same claim, but cargo-mutants runs
    // nextest, which does not run doctests; this is the test that kills
    // a mutated package spec.
    #[test]
    fn the_package_spec_pins_the_verified_release() {
        assert_eq!(super::likec4_package(), "likec4@1.59.3");
    }
}
