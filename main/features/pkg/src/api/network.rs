/// Provides SRI hash lookup for a named package from a manifest.
pub trait ManifestLookup {
    fn get_sri(&self, name: &str) -> Option<&str>;
}
