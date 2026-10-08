//! The language-agnostic core of asbuilt.
//!
//! A front-end (one per language; `asbuilt-rust` is the first) reads a
//! code base and produces a [`Model`]: elements with ids, kinds, titles
//! and paths, and relations between them labeled with the item names
//! they reference. This crate owns that model, the `asbuilt.toml`
//! [`Config`] that adds what no code can state (externals, the theme,
//! the documentation settings), the `.c4` emitter and its reader, the
//! drift check, and the generator of the documentation tree. It never
//! spawns a process.

#![doc(
    html_logo_url = "https://padamson.github.io/asbuilt/mark.svg",
    html_favicon_url = "https://padamson.github.io/asbuilt/mark.svg"
)]

pub mod check;
pub mod config;
pub mod docs;
pub mod emit;
pub mod error;
pub mod externals;
pub mod frontend;
pub mod model;
pub mod read;
pub mod svg;
pub mod theme;

pub use check::{Change, ElementField, Outcome, compare, summarize};
pub use config::{
    ColorScheme, Config, DocsConfig, External, ExternalRelation, OutputConfig, ThemeColor,
};
pub use docs::{DocsOptions, Site};
pub use emit::{EmitOptions, emit, view_ids};
pub use error::{Error, Result};
pub use frontend::{Frontend, survey};
pub use model::{Deployment, Element, ElementKind, Id, Model, Relation, RelationKind};
pub use read::{Written, WrittenElement, WrittenRelation, read};
