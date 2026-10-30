# xkvm.conf key-value specification

`/etc/xkvm.conf` is the configuration file read by `vminit` (PID 1) at startup
inside every xkvm microVM guest. It is a plain-text key-value file with one
directive per line. The file is optional — a missing file or empty string yields
all defaults.

## Syntax rules

- One directive per line: `key=value`
- Blank lines and lines beginning with `#` are ignored
- Keys are case-sensitive, lower-snake-case
- `value` is everything after the first `=` on the line (may itself contain `=`)
- A line with a key this vminit does not list, a line without `=`, and a line of a known
  key whose value cannot be honoured are **errors** (see "Errors" below). Only blank lines
  and `#` comments are skipped.
- The keys a build lists are its *capabilities*. The host reads them from the file (see
  "Capability marker") before it boots a guest.

## Errors

vminit parses the whole file, collects one message per bad line (`line N: ...`, naming
the key but never the value), logs `FATAL: /etc/vminit.conf: line N: ...`, reports exit
code 1 and powers off, without running the workload. Rationale: a line the guest cannot
honour (an argument, a variable, a mount, a resolver) used to be dropped silently, so
the guest ran a different workload than the host asked for (vmruntime#437: an
`entrypoint_hex=` line sent to a vminit that predates it). Failing closed is the safe
direction; the host-side capability check refuses earlier, before the guest boots.

This replaces the earlier rule "unknown keys are silently ignored (forward
compatible)". A writer must therefore not emit a key the target `vmfs` does not list;
check the marker first. Value rules that are now errors rather than silent drops:
`env` without `NAME=` or with an empty name; `volume` / `block_mount` without three
non-empty fields; `volume` access other than `rw`/`ro`; `interactive`, `start_agent`,
`mount_rootfs` other than `0`/`1`; `signal_mode` other than `serial`/`shared_memory`;
`network` other than `dhcp`/`none`; `dns` that is not an IP address; `manifest_url` that
is not `https://`; an empty `install`, `manifest`, `manifest_hash` or `cache_base`.

## Keys

### `entrypoint`

**Type:** string (repeated)  
**Default:** none (empty argv — vminit falls back to an interactive shell when `interactive=1`, or exits immediately)

The process to launch as the workload. May appear multiple times; each occurrence
appends one element to `argv`. The first occurrence becomes `argv[0]` (the
executable path), subsequent occurrences are positional arguments.

```
entrypoint=/usr/bin/python3
entrypoint=-m
entrypoint=http.server
entrypoint=8080
```

A plain `entrypoint=` line cannot carry a newline or a carriage return, and the parser
trims trailing whitespace, so an argument with either is written with `entrypoint_hex`.

---

### `entrypoint_hex`

**Type:** hex string (repeated, shares the argument list with `entrypoint`)  
**Default:** none

One element of `argv`, written as the hex digits (either case) of its bytes. Lines of
`entrypoint` and `entrypoint_hex` append to the same list in file order. The bytes must
be UTF-8 and must not contain NUL (execve cannot carry one); an empty value is an empty
argument. A value that is not valid is a boot error, never a skipped argument: vminit
logs `FATAL: /etc/vminit.conf: line N: ...`, reports exit code 1 and powers off. The same
happens when `/etc/vminit.conf` fills the 262,144-byte (256 KiB) read buffer, because the file
may have been cut mid-line.

```
entrypoint=/bin/sh
entrypoint=-c
# the argument "echo A\necho B"
entrypoint_hex=6563686f20410a6563686f2042
```

A vminit older than v0.1.6 ignores this key (it also ignored every unknown key), so a
writer must check the capability marker for `entrypoint_hex` before emitting it. vminit
v0.1.5 has the key but no marker, so its capabilities read as unknown.

---

### `env`

**Type:** `KEY=VALUE` string (repeated)  
**Default:** none

Inject an environment variable into the workload process. May appear multiple
times to set multiple variables. The value is everything after the first `=`
following the key name, so values that contain `=` are handled correctly.

```
env=DATABASE_URL=postgres://localhost/app
env=LOG_LEVEL=debug
```

---

### `volume`

**Type:** `tag:mountpoint:rw|ro` string (repeated)  
**Default:** none

Mount a virtio-fs or plan 9 volume exported by the host.

| Field | Description |
|---|---|
| `tag` | Volume tag string that matches the host-side export label |
| `mountpoint` | Absolute path inside the guest where the volume is mounted |
| `rw` / `ro` | Mount mode: read-write or read-only |

```
volume=workspace:/mnt/workspace:rw
volume=certs:/etc/ssl/custom:ro
```

---

### `install`

**Type:** string (repeated)  
**Default:** none

Package name to install at boot from the embedded initrd store or the network
cache. May appear multiple times. Packages are resolved using the manifest at
`manifest=` (if set) and fetched from `cache_base=` when not present locally.

```
install=curl
install=jq
```

---

### `interactive`

**Type:** boolean (`1` = true, `0` = false; anything else is an error)  
**Default:** `0`

When `1`, vminit allocates a PTY and attaches stdin/stdout to the workload's
controlling terminal. Set this for shells and interactive CLI tools.

```
interactive=1
```

---

### `start_agent`

**Type:** boolean (`1` = true, `0` = false; anything else is an error)  
**Default:** `0`

When `1`, vminit launches the `xkvm-agent` sidecar before starting the
workload. The agent exposes the in-guest gRPC surface used by the host daemon
(`xkvmd`) for exec, file I/O, and health probes.

```
start_agent=1
```

---

### `mount_rootfs`

**Type:** boolean (`1` = true, `0` = false; anything else is an error)  
**Default:** `0`

When `1`, vminit mounts the root filesystem from the block device `/dev/vda`, applies
the overlay, and runs the workload chrooted into it.

```
mount_rootfs=1
```

---

### `block_mount`

**Type:** `device:mountpoint:fstype` string (repeated)  
**Default:** none

Mount a block device before the workload starts. All three fields must be non-empty.

```
block_mount=/dev/vdb:/data:ext4
```

---

### `network`

**Type:** enum — `dhcp` | `none`  
**Default:** `dhcp`

`dhcp` runs a DHCP exchange on `eth0` at boot; `none` skips network configuration
(the loopback interface is still configured).

```
network=none
```

#### DHCP timing

With `network=dhcp` the init waits for the link before it sends, and sends again when no reply comes (RFC 2131 section 4.1). The timing is fixed in the guest, no key sets it, and lives in `DhcpPolicy::default()` in `main/features/init/src/api/dhcp.rs`:

| Setting | Value | Reason |
|---|---|---|
| Wait for the link | up to 2 s, looking every 5 ms | The virtio-net driver turns the carrier on from a workqueue after the interface is up. A frame sent before that is dropped by the guest kernel and `sendto` still succeeds, so the first DISCOVER was lost with nothing logged (vmruntime#553). Measured with kernel 7.3: 0 ms in 439 of 440 guests and 1 ms in one, so 2 s is far above anything seen. A guest whose NIC never gets a link spends the 2 s once at boot. |
| First wait for a reply | 1 s, doubled after each send, at most 8 s | The earlier client waited 3 s once; a lost first message now costs about 1 s. RFC 2131 starts at 4 s, which suits a network and not the host-local NAT of a microVM that boots in 2 to 3 s. |
| Randomization | plus or minus 1/8 of each wait | RFC 2131 asks for it, so guests that start together do not send again together. |
| Overall limit | 15 s after the first DISCOVER | A guest with a NIC and no DHCP server gave up after 3 s and now gives up after 15 s; it then boots on without an address, as before. |

A NAK starts the exchange over with a new transaction id, at most three times. The console says when the link was waited for, when a message is sent again, and why no lease was obtained (`DHCP: no OFFER received after N DISCOVER(s) in M ms`, or `no ACK`). A normal boot writes nothing beyond `DHCP: configured`.

---

### `dns`

**Type:** IPv4 or IPv6 address (repeated, at most 3)  
**Default:** none (the DNS server of the DHCP lease, if any)

DNS servers for the guest. After network configuration vminit writes one
`nameserver <address>` line per `dns=` line, in order, to `/etc/resolv.conf`,
replacing the lease's server, and the file is copied into the rootfs when
`mount_rootfs=1`. A value that is not an IP address, or a fourth line, is an error.
vminit v0.1.6 is the first release that honours this key (earlier releases dropped it).

```
dns=10.0.2.3
dns=2001:4860:4860::8888
```

---

### `manifest_url`

**Type:** `https://` URL  
**Default:** none

URL of the package manifest, fetched at boot (builds with the `packages` feature).
Takes precedence over `manifest=`. A URL that does not start with `https://` is an error.

```
manifest_url=https://deploy.internal/manifest.json
```

---

### `manifest_hash`

**Type:** `sha256:<64 hex digits>`  
**Default:** none

Expected SHA-256 of the manifest; a mismatch aborts the boot. An empty value is an error.

```
manifest_hash=sha256:b94d27b9934d3e08a52e52d7da7dabfac484efe04294e576f4be72b4db32f73e
```

---

### `kali_mode` (not supported)

Earlier versions of this document listed `kali_mode`. vminit has never implemented it;
it is not in the capability list, so a `kali_mode=` line is an unknown-key error.

---

### `signal_mode`

**Type:** enum — `serial` | `shared_memory` (anything else is an error)  
**Default:** `serial`

Controls how vminit communicates lifecycle signals (`XIKA_READY`, `XIKA_EXIT:N`)
back to the host.

| Value | Mechanism |
|---|---|
| `serial` | Write signal strings to `/dev/ttyS0` (default; works on all hypervisors) |
| `shared_memory` | Write a magic `u32` to physical address `0x500` (lower latency; requires host support) |

```
signal_mode=shared_memory
```

---

### `manifest`

**Type:** absolute file path  
**Default:** none (network package fallback disabled)

Path to a JSON package manifest inside the guest (for example
`/etc/packages.json`). When set, vminit uses this manifest to resolve package
Nix store paths before attempting a network fetch. Required for offline-capable
`install=` directives.

```
manifest=/etc/packages.json
```

---

### `cache_base`

**Type:** URL string  
**Default:** `https://cache.nixos.org`

Base URL of the Nix binary cache used for network package installation. Packages
not found in the initrd are fetched from `<cache_base>/<store-path>.narinfo`.

```
cache_base=https://cache.example.internal
```

---

## Full example

```
# /etc/xkvm.conf — example workload configuration

entrypoint=/opt/app/server
entrypoint=--port=8080

env=APP_ENV=production
env=DATABASE_URL=postgres://db.internal/myapp

volume=data:/mnt/data:rw
volume=secrets:/run/secrets:ro

install=ca-certificates

start_agent=1
signal_mode=shared_memory
manifest=/etc/packages.json
cache_base=https://cache.nixos.org
```

## Parser source

`main/features/init/src/spi/config.rs` — pure function, no filesystem access.
The binary (`main/features/bin/`) reads the file and passes the text to
`parse_config`.

## Capability marker

Every `vmfs` from v0.1.6 on carries a machine-readable marker so a host can check what
it supports **without executing it**. The host checks for the capability it needs (a
config key name such as `entrypoint_hex`), not for a version number, and refuses before
booting a guest, naming the capability and the release that has it. A `vmfs` with no
marker (v0.1.5 and earlier) has *unknown* capabilities.

The capability list is `swe_vminit_init::CAPABILITIES`: the parser refuses any key not
in it, the marker embeds it, and the boot banner prints it. Current list (v0.1.6):
`entrypoint`, `entrypoint_hex`, `env`, `volume`, `block_mount`, `install`, `interactive`,
`start_agent`, `mount_rootfs`, `signal_mode`, `network`, `dns`, `manifest`,
`manifest_url`, `manifest_hash`, `cache_base`.

### Where it is

An ELF note in the allocated section `.note.vminit`, therefore in a `PT_NOTE` program
header segment. A host reads the `PT_NOTE` segments (section headers are not needed and
may be stripped), finds the note with owner name `vminit` and type `1`, and decodes the
payload. `strip` keeps it because the section is allocated.

### Note layout

All integers are little-endian (the file is x86_64). Standard ELF note framing:

| Offset | Size | Field |
|---|---|---|
| 0 | 4 | `namesz` = 7 (`"vminit"` and its NUL) |
| 4 | 4 | `descsz` = payload length, not counting padding |
| 8 | 4 | `type` = 1 |
| 12 | 8 | name `"vminit\0"` padded with zeros to a multiple of 4 |
| 20 | `descsz`, padded with zeros to a multiple of 4 | payload |

A segment can hold other notes (for example a GNU build id) before or after it: walk
the segment, step over each note by `12 + pad(namesz) + pad(descsz)`, and use the
segment's `p_align` (4, or 8 when it says 8) as the padding unit. A note with another
owner or type is skipped.

### Payload, format version 1

| Field | Size | Meaning |
|---|---|---|
| `format_version` | u32 | 1. **A reader that finds another value must report "unsupported format" and not guess.** |
| `version_len`, `version` | u16, bytes | UTF-8 package version, for example `0.1.6` |
| `capability_count` | u16 | number of capabilities |
| per capability: `len`, `name` | u16, bytes | UTF-8 config key |

The payload ends exactly after the last capability; extra bytes are malformed. A new
layout is a new `format_version`; a reader treats a note it does not understand as
"capabilities unknown", the same as no marker.

### Reading it

Rust, from this repository's `swe_vminit_init` crate (a `no_std` crate; the reader is
`alloc`-only and bounds-checked, so a truncated or hostile file gives an error):

```rust
use swe_vminit_init::{read_marker, MarkerError};

let file = std::fs::read("downloads/vmfs")?;
match read_marker(&file) {
    Ok(marker) if marker.has_capability("entrypoint_hex") => { /* supported */ }
    Ok(marker) => { /* refuse: names marker.version, lacks the capability */ }
    Err(MarkerError::NoMarker) => { /* v0.1.5 or earlier: capabilities unknown */ }
    Err(other) => { /* truncated, not ELF64, unsupported format, malformed */ }
}
```

Any language: `scripts/ci/check_release_marker.py` is a standard-library-only decoder
of the same format (CI uses it on the release binary). By eye:

```
readelf -n vmfs                      # lists the note: owner vminit, type 0x1
readelf -x .note.vminit vmfs         # the raw bytes
```

### Boot banner

At start, before `XIKA_READY`, vminit prints one line on the serial console, so every
boot log shows what ran: `vminit <version> capabilities=<comma separated list>`. The
`XIKA_READY` and `XIKA_EXIT:N` lines are unchanged.

### Release check

The release workflow decodes the marker of the binary it is about to publish, fails if
its version is not the tag without the leading `v`, and puts the capability list in the
release notes. The version comes from `main/features/bin/Cargo.toml`.
