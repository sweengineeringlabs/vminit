//! Package installation — extracts tar.gz archives from the initrd into the rootfs.

use crate::serial;

const PACKAGES_DIR: &str = "/packages";

pub fn install_packages(packages: &[String], target_root: Option<&str>) {
    if packages.is_empty() { return; }
    let root = target_root.unwrap_or("");
    for pkg in packages {
        let archive_path = format!("{}/{}.tar.gz", PACKAGES_DIR, pkg);
        if !std::path::Path::new(&archive_path).exists() {
            serial::log(&format!("package not found: {}", archive_path));
            continue;
        }
        serial::log(&format!("installing package: {}", pkg));
        let extract_dir = if root.is_empty() { "/".to_string() } else { root.to_string() };
        let tar_path = format!("{}/bin/tar", root);
        let busybox_path = format!("{}/bin/busybox", root);
        let result = if std::path::Path::new(&tar_path).exists() {
            extract_with_tar(&tar_path, &archive_path, &extract_dir)
        } else if std::path::Path::new(&busybox_path).exists() {
            extract_with_busybox(&busybox_path, &archive_path, &extract_dir)
        } else {
            Err("no tar binary available".into())
        };
        match result {
            Ok(()) => serial::log(&format!("installed: {}", pkg)),
            Err(e) => serial::log(&format!("failed to install {}: {}", pkg, e)),
        }
    }
}

fn extract_with_tar(tar_bin: &str, archive: &str, dest: &str) -> Result<(), String> {
    let status = std::process::Command::new(tar_bin)
        .args(["-xzf", archive, "-C", dest])
        .status()
        .map_err(|e| format!("tar exec failed: {e}"))?;
    if status.success() { Ok(()) } else { Err(format!("tar exited {}", status.code().unwrap_or(-1))) }
}

fn extract_with_busybox(busybox: &str, archive: &str, dest: &str) -> Result<(), String> {
    let status = std::process::Command::new(busybox)
        .args(["tar", "-xzf", archive, "-C", dest])
        .status()
        .map_err(|e| format!("busybox tar exec failed: {e}"))?;
    if status.success() { Ok(()) } else { Err(format!("busybox tar exited {}", status.code().unwrap_or(-1))) }
}
