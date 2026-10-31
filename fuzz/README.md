# vminit_fuzz

`cargo-fuzz` (libFuzzer) targets for the parts of the init that read bytes it did not write. Development-time work: nothing here is built into `vmfs`. Tracked in [vminit#13](https://github.com/sweengineeringlabs/vminit/issues/13), part of [vmruntime#557](https://github.com/sweengineeringlabs/vmruntime/issues/557).

**Status (2026-10-31):** the four targets build and run. Each ran 30 s against its seeds with no crash or assertion failure, and each found a bug planted by hand in its parser within 1 to 3 s (the bug was restored afterwards). The first hour-long runs, and any real finding, are still to come (vminit#13 T3 and T4).

| Target | Function (package `swe_vminit_init`) | Input | Checked beyond "does not panic" |
|---|---|---|---|
| `read_marker` | `read_marker` (`spi/marker.rs`) | bytes of a `vmfs` file | a marker that was read has `MARKER_FORMAT_VERSION` |
| `parse_dhcp_response` | `parse_dhcp_response` (`spi/dhcp.rs`) | bytes of a server reply | a reply is `Some` only for at least 240 bytes with the magic cookie at 236 |
| `parse_config` | `parse_config` (`spi/config.rs`) | the text of `vminit.conf` (UTF-8 only) | at most `MAX_DNS_SERVERS` DNS servers |
| `parse_overlay_manifest` | `OverlayManifest::parse` (`api/overlay.rs`) | the text of `/overlay/.manifest` (UTF-8 only) | every entry of a manifest that parsed has an absolute destination |

## Run

Needs `cargo-fuzz`, the dated nightly `nightly-2026-10-02` (the one pinned for vmruntime's eBPF build; vminit has no toolchain pin of its own), and a C++ compiler: the build compiled libFuzzer with g++ 15 and needed no override. Linux and WSL only. These are the commands that were run on 2026-10-31 (WSL Ubuntu), with `cargo-fuzz` 0.13.2, `libfuzzer-sys` 0.4.13 and `arbitrary` 1.5.0 (`fuzz/Cargo.lock`):

```bash
cargo install cargo-fuzz --locked              # once (19 min on a busy host)
rustup toolchain install nightly-2026-10-02    # once, if the nightly is not there

cd fuzz
# build into a directory on the WSL file system: a checkout under /mnt/c is slow to build in (first build 22 min on a busy host)
cargo +nightly-2026-10-02 fuzz build --target-dir "$HOME/vminit-fuzz-target"

# run against a COPY of the seeds: libFuzzer adds the inputs it finds to the corpus directory it is given
mkdir -p "$HOME/fuzz-work" && cp -r corpus/read_marker "$HOME/fuzz-work/read_marker"
cargo +nightly-2026-10-02 fuzz run read_marker "$HOME/fuzz-work/read_marker" \
    --target-dir "$HOME/vminit-fuzz-target" -- -max_total_time=3600
```

Targets: `read_marker`, `parse_dhcp_response`, `parse_config`, `parse_overlay_manifest`. A crash is reported as `deadly signal` with the panic message above it, and the input is written to `fuzz/artifacts/<target>/crash-<hash>`.

To check that a target can fail, edit the parser to remove a check (for example, change `file.get(offset..end).ok_or(...)` in `read_marker` to `&file[offset..end]`), rebuild and run it: the fuzzer should find it within seconds; then restore the file with `git checkout` (or from a copy, if the checkout has CRLF line endings).

Crashes are written to `fuzz/artifacts/<target>/` (not committed). A crash is minimized with `cargo fuzz tmin`, fixed in `swe_vminit_init`, and kept as a regression test in `main/features/init/tests/` that fails without the fix.

## Seeds

`corpus/<target>/` holds a few hundred bytes of seed per target, written from the documented formats (`docs/xkvm_conf.md`, the parser doc comments and the layout the marker tests build). **They are synthesized, not captured**: a real captured DHCP exchange and a real `vmfs` file are a later task. Small seed corpora are committed; larger ones are not.
