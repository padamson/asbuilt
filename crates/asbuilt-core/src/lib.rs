//! The language-agnostic core of asbuilt.
//!
//! A front-end (one per language; `asbuilt-rust` is the first) reads a
//! code base and produces a [`Model`]: elements with ids, kinds, titles
//! and paths, and relations between them labeled with the item names
//! they reference. This crate owns that model, the `asbuilt.toml`
//! [`Config`] that adds the externals no code can state, and the
//! `.c4` emitter and drift check that follow in later commits. It never
//! spawns a process.

pub mod check;
pub mod config;
pub mod emit;
pub mod error;
pub mod externals;
pub mod frontend;
pub mod model;

pub use check::{Outcome, compare};
pub use config::{Config, External, ExternalRelation, OutputConfig};
pub use emit::{EmitOptions, emit};
pub use error::{Error, Result};
pub use frontend::{Frontend, survey};
pub use model::{Deployment, Element, ElementKind, Id, Model, Relation, RelationKind};
