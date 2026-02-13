use dashmap::DashMap;
use ringest_io::Registry;

pub mod filter;
pub mod file;
pub mod dir;

lazy_static::lazy_static! {
    static ref IO_REGISTRY: Registry = Registry::new();
    /// File ID - (name, path)
    static ref REGISTERED_FILES: DashMap<u64, (String, String)> = DashMap::new();
}