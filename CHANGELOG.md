# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased]

### Fixed
- **A guest could start with no address because its one DHCP DISCOVER left before the link was up** (sweengineeringlabs/vminit#11, sweengineeringlabs/vmruntime#553). The client sent one DISCOVER at once, ignored `sendto`'s result and never sent again. While the virtio-net interface had no carrier the guest kernel dropped the packet and `sendto` still succeeded, so about 1 in 100 net guests logged `DHCP: no OFFER received` and went on without an address. The client now waits for the carrier (at most 2 s), sends DISCOVER and REQUEST again with a doubling, randomized wait (1 s up to 8 s, 15 s overall), takes only a reply to its own transaction, uses a fresh transaction id per exchange, and logs a failed send. The exchange moved into `swe_vminit_init` (`acquire_lease`) and is tested there against a scripted link; `bin` supplies the socket and sysfs. The values and their reasons are in `docs/xkvm_conf.md` ("DHCP timing"). `DHCP: configured <ip> on <if>` is unchanged.

### Changed (compatibility rule D15)
- **Config lines vminit cannot honour are now boot errors, not silent drops.** The earlier rule was "unknown keys are silently ignored (forward compatible)". Now an unknown key, a line without `=`, and a known key with a malformed value (`env` without `NAME=`, `volume`/`block_mount` without three non-empty fields, `volume` access other than `rw`/`ro`, a flag other than `0`/`1`, `signal_mode`/`network` outside their values, `dns` that is not an IP address, `manifest_url` that is not `https://`, an empty `install`/`manifest`/`manifest_hash`/`cache_base`) are recorded in `InitConfig::errors`; vminit logs `FATAL`, exits with code 1 and does not run the workload. Reason: a dropped line runs a different workload than the host asked for (vmruntime#437). Comments and blank lines are still skipped. A host must not write a key the target `vmfs` does not list; it checks the marker first. `kali_mode`, which the docs listed but vminit never implemented, is now an unknown key

### Added
- Capability marker (sweengineeringlabs/vminit#7): `vmfs` carries an ELF note `.note.vminit` (owner `vminit`, type 1, payload format 1) holding its package version and the list of config keys it understands, readable without executing the file (`swe_vminit_init::read_marker`, `scripts/ci/check_release_marker.py`, format in `docs/xkvm_conf.md`). At start it prints `vminit <version> capabilities=<list>` on the serial console before `XIKA_READY`. The release workflow fails when the marker's version is not the tag and lists the capabilities in the release notes. The package version of `swe_vminit_bin` is now 0.1.6. Host side: sweengineeringlabs/vmruntime#468
- `dns=` config key (IPv4/IPv6, up to 3): written to `/etc/resolv.conf` after DHCP, replacing the lease's server. The host already wrote this key; earlier vminit releases dropped it silently
- Documented `block_mount`, `mount_rootfs`, `network`, `manifest_url`, `manifest_hash` in `docs/xkvm_conf.md`
- `entrypoint_hex=` config key: an entrypoint argument written as hex of its bytes, for arguments a line cannot carry (newline, carriage return, trailing whitespace); a malformed value, or a `vminit.conf` that fills the 64 KiB read buffer, is a boot error (exit code 1) instead of a silently different command line (sweengineeringlabs/vmruntime#437)
- `swe_vminit_init`: config parsing (`/etc/xkvm.conf`), overlay manifest, and signal protocol constants
- `swe_vminit_shell`: built-in utilities (`echo`, `cat`, `ls`, `grep`, `wc`, and 20+ others) and interactive REPL with history, tab completion, and line editing
- Signal protocol: `XIKA_READY` / `XIKA_EXIT:N` over serial; `SHM_MAGIC_READY` / `SHM_MAGIC_EXIT_MASK` over shared memory at `0x500`
- `bin` crate: 14-step PID 1 boot sequence (WSL2/musl-only)
- Full SEA compliance: `api/`, `core/`, `spi/`, `saf/` layers across both library crates
