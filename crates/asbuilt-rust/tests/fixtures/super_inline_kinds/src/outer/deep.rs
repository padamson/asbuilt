//! Deep.

use super::super::util::Helper;

pub mod nested {
    //! Nested inline.

    use super::super::Outer;

    pub struct Deep(pub crate::util::Helper);

    impl crate::util::Render for Deep {}

    pub fn build() -> Deep {
        use crate::util::Helper;
        let _ = Outer;
        Deep(Helper)
    }
}

pub fn h() -> Helper {
    Helper
}

#[cfg(test)]
mod tests {
    use crate::util::Helper;

    #[test]
    fn t() {
        let _ = Helper;
    }
}
