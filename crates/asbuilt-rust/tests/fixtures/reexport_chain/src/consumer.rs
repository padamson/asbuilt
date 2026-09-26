use crate::Page;
use crate::protocol::PageFrame;

pub fn f() -> Page {
    Page::new()
}

pub fn g(_: PageFrame) {}
