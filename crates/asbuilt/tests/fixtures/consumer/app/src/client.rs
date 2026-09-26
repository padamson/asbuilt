use crate::server::Driver;

pub fn connect() -> Driver {
    crate::server::spawn()
}
