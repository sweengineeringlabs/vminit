//! Package installation — initrd tar.gz with network NAR fallback.

pub fn install_packages(
    packages: &[String],
    target_root: Option<&str>,
    manifest_text: Option<&str>,
) {
    let packages_dir = std::path::Path::new("/packages");
    let dest_dir = target_root.map(std::path::Path::new);

    if manifest_text.is_some() {
        let http = swe_justpkg_pkg::UreqClient;
        swe_vminit_pkg::run_install_step(packages, packages_dir, manifest_text, Some(&http), dest_dir);
    } else {
        swe_vminit_pkg::run_install_step(packages, packages_dir, None, None, dest_dir);
    }
}
