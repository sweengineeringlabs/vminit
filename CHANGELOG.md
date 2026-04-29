# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased]

### Added
- `swe_vminit_init`: config parsing (`/etc/xkvm.conf`), overlay manifest, and signal protocol constants
- `swe_vminit_shell`: built-in utilities (`echo`, `cat`, `ls`, `grep`, `wc`, and 20+ others) and interactive REPL with history, tab completion, and line editing
- Signal protocol: `XIKA_READY` / `XIKA_EXIT:N` over serial; `SHM_MAGIC_READY` / `SHM_MAGIC_EXIT_MASK` over shared memory at `0x500`
- `bin` crate: 14-step PID 1 boot sequence (WSL2/musl-only)
- Full SEA compliance: `api/`, `core/`, `spi/`, `saf/` layers across both library crates
