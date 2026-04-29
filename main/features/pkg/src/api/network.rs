/// Provides store path lookup for a named package from a manifest.
pub trait ManifestLookup {
    fn get_store_path(&self, name: &str) -> Option<&str>;
}
