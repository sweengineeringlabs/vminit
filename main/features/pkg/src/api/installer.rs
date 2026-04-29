use super::error::PackageError;

/// Installs a named package into a destination directory.
///
/// Implementors decide whether to source from initrd archives, a network NAR
/// cache, or a test stub. All implementations must be `Send` so that PID-1
/// can drive installation from any thread.
pub trait PackageInstaller: Send {
    fn install(&self, name: &str, dest_dir: &std::path::Path) -> Result<(), PackageError>;
}
