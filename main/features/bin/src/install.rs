//! Package installation — extracts tar.gz archives from the initrd into the rootfs.

pub fn install_packages(packages: &[String], target_root: Option<&str>) {
    let packages_dir = std::path::Path::new("/packages");
    let dest_dir = target_root.map(std::path::Path::new);
    swe_vminit_pkg::run_install_step(packages, packages_dir, None, None, dest_dir);
}
