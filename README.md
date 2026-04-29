# vminit

PID 1 init for Linux x86_64 microVMs. Extracted from [vmisolate](https://github.com/sweengineeringlabs/vmisolate) (issue #79).

## Overview

`vminit` boots as PID 1 inside a microVM, reads `/etc/xkvm.conf`, signals readiness to the host over serial (`XIKA_READY`) or shared memory (`0x500`), and optionally drops into an interactive shell.

## Workspace layout

| Crate | Purpose |
|---|---|
| `main/features/init` (`swe_vminit_init`) | Config types, overlay manifest, signal protocol |
| `main/features/shell` (`swe_vminit_shell`) | Built-in utilities and interactive REPL |
| `main/features/bin` | PID 1 binary (WSL2 musl-only, excluded from workspace) |

## Build

```bash
# workspace crates (Windows or Linux, native target)
cargo build

# PID 1 binary (WSL2 only, musl target)
cargo build --target x86_64-unknown-linux-musl --release \
  --manifest-path main/features/bin/Cargo.toml
```

## Test

```bash
cargo test                          # all workspace tests
cargo test -p swe_vminit_init       # init crate only
cargo test -p swe_vminit_shell      # shell crate only
```

## Signal protocol

The guest signals the host via `/dev/ttyS0` (serial mode) or physical address `0x500` (shm mode):

| Event | Serial | SHM |
|---|---|---|
| Ready | `XIKA_READY\n` | `0xBEEF_CAFE` |
| Exit N | `XIKA_EXIT:N\n` | `0xDEAD_0000 \| (N & 0xFFFF)` |

## Architecture

Follows SEA (Stratified Encapsulation Architecture): `api/` → `core/` → `spi/` → `saf/`.
