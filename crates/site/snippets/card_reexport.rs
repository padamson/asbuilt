// lib.rs            pub use protocol::Page;
// protocol/mod.rs   pub use page::{Page, Frame as PageFrame};
// consumer.rs
use crate::Page;
pub fn f() -> Page { Page::new() }

// app.consumer -[constructs]-> app.protocol.page 'Frame, Page'
