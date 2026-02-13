use ringest_io::Registry;

pub mod filter;
pub mod file;
pub mod dir;

lazy_static::lazy_static! {
    static ref IO_REGISTRY: Registry = Registry::new();
}