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
- Unknown keys are silently ignored — forward-compatible with newer vminit builds

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

**Type:** boolean (`1` = true, any other value = false)  
**Default:** `0`

When `1`, vminit allocates a PTY and attaches stdin/stdout to the workload's
controlling terminal. Set this for shells and interactive CLI tools.

```
interactive=1
```

---

### `start_agent`

**Type:** boolean (`1` = true, any other value = false)  
**Default:** `0`

When `1`, vminit launches the `xkvm-agent` sidecar before starting the
workload. The agent exposes the in-guest gRPC surface used by the host daemon
(`xkvmd`) for exec, file I/O, and health probes.

```
start_agent=1
```

---

### `kali_mode`

**Type:** boolean (`1` = true, any other value = false)  
**Default:** `0`

When `1`, vminit applies Kali Linux-specific boot adjustments (service
ordering, default tool paths). Only meaningful when the guest rootfs is a Kali
image.

```
kali_mode=1
```

---

### `signal_mode`

**Type:** enum — `serial` | `shared_memory`  
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
