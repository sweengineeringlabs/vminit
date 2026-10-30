# vminit_fuzz

`cargo-fuzz` (libFuzzer) targets for the parts of the init that read bytes it did not write. Development-time work: nothing here is built into `vmfs`. Tracked in [vminit#13](https://github.com/sweengineeringlabs/vminit/issues/13), part of [vmruntime#557](https://github.com/sweengineeringlabs/vmruntime/issues/557).

**Status: written, not yet built or run.** The targets have not been compiled, the seeds have not been run through the parsers, and the commands below are unverified until the first build (task T2 of vminit#13).

| Target | Function (package `swe_vminit_init`) | Input | Checked beyond "does not panic" |
|---|---|---|---|
| `read_marker` | `read_marker` (`spi/marker.rs`) | bytes of a `vmfs` file | a marker that was read has `MARKER_FORMAT_VERSION` |
| `parse_dhcp_response` | `parse_dhcp_response` (`spi/dhcp.rs`) | bytes of a server reply | a reply is `Some` only for at least 240 bytes with the magic cookie at 236 |
| `parse_config` | `parse_config` (`spi/config.rs`) | the text of `vminit.conf` (UTF-8 only) | at most `MAX_DNS_SERVERS` DNS servers |
| `parse_overlay_manifest` | `OverlayManifest::parse` (`api/overlay.rs`) | the text of `/overlay/.manifest` (UTF-8 only) | every entry of a manifest that parsed has an absolute destination |

## Run (to be confirmed by the first build)

Needs the dated nightly of this project's toolchain pin, `cargo-fuzz`, and a C++ compiler (`clang` or `g++`). Linux and WSL only.

```bash
cargo install cargo-fuzz --locked          # once; the version is recorded in vminit#13 when chosen
cd fuzz
cargo +<dated nightly> fuzz run read_marker -- -max_total_time=3600
```

Crashes are written to `fuzz/artifacts/<target>/` (not committed). A crash is minimized with `cargo fuzz tmin`, fixed in `swe_vminit_init`, and kept as a regression test in `main/features/init/tests/` that fails without the fix.

## Seeds

`corpus/<target>/` holds a few hundred bytes of seed per target, written from the documented formats (`docs/xkvm_conf.md`, the parser doc comments and the layout the marker tests build). **They are synthesized, not captured**: a real captured DHCP exchange and a real `vmfs` file are a later task. Small seed corpora are committed; larger ones are not.
