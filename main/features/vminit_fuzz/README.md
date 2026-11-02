# vminit_fuzz

`cargo-fuzz` (libFuzzer) targets for the parts of the init that read bytes it did not write. Development-time work: nothing here is built into `vmfs`. Tracked in [vminit#13](https://github.com/sweengineeringlabs/vminit/issues/13), part of [vmruntime#557](https://github.com/sweengineeringlabs/vmruntime/issues/557).

**Status (2026-11-02):** the four raw-bytes targets ran for one hour each on 2026-10-31 with no crash, hang or assertion failure, and found bugs planted by hand that panic or break a stated property; they **cannot see a logic bug** (with the volume mode inverted, the DNS value corrupted, the overlay uid and gid swapped, or the capabilities read in reverse, each ran a full 60 s without noticing). The `*_model` targets check the values returned and found each of those four logic bugs in 0 to 2 s. Two targets are of a different kind: `pkg_extract_model` (tar extraction, 4 of 4 planted bugs) and `dhcp_exchange_model` (the whole DHCP exchange against a scripted link: baseline clean after fixes of vminit#22 and vminit#24, which its baseline found; planted bugs J1 to J11, each found by the model and missed by the control). **Every target has had one hour (2026-11-02, no crash artifact):** `parse_dhcp_response` 146.7 M executions, `parse_dhcp_model` 56.1 M, `dhcp_exchange_model` 9.6 M, `parse_config_model` 8.8 M, `parse_config` 11.1 M, `parse_overlay_manifest` 58.8 M, `parse_overlay_model` 16.6 M, `read_marker` 102.4 M, `read_marker_model` 29.9 M, `pkg_extract_model` 1.4 M (vminit#13, vminit#19). Not covered: `NetworkManifest::parse` (not reachable through public API) and the NAR, narinfo and HTTP-body surfaces of the sibling `justpkg` repository.

## Targets

Raw bytes in, "does not panic" plus one stated property:

| Target | Function (package `swe_vminit_init`) | Input | Checked beyond "does not panic" |
|---|---|---|---|
| `read_marker` | `read_marker` (`spi/marker.rs`) | bytes of a `vmfs` file | a marker that was read has `MARKER_FORMAT_VERSION` |
| `parse_dhcp_response` | `parse_dhcp_response` (`spi/dhcp.rs`) | bytes of a server reply | a reply is `Some` only for at least 240 bytes with the magic cookie at 236 |
| `parse_config` | `parse_config` (`spi/config.rs`) | the text of `vminit.conf` (UTF-8 only) | at most `MAX_DNS_SERVERS` DNS servers |
| `parse_overlay_manifest` | `OverlayManifest::parse` (`api/overlay.rs`) | the text of `/overlay/.manifest` (UTF-8 only) | every entry of a manifest that parsed has an absolute destination |

Structure-aware, checking the values returned. The fuzzer's bytes choose the fields; the input is written from the documented format by code in the target, independent of the init's own encoder, and the result is compared with what those fields mean:

| Target | What it builds | What must hold |
|---|---|---|
| `read_marker_model` | an ELF64 file (non-note program headers, foreign notes before the marker, note alignment 4 or 8) holding a `vminit` note with a chosen version and capabilities | it reads back as exactly that version and capability list; the same file cut short never yields a marker |
| `parse_dhcp_model` | a DHCP reply (xid, yiaddr, options known and unknown, pad bytes, repeated and too-short options) | `transaction_id`, `ip`, `message_type`, `mask`, `gateway`, `dns`, `server_id` equal a model: a later option replaces an earlier one, an option shorter than it needs is ignored |
| `parse_overlay_model` | a manifest of `dest:mode:uid:gid` lines (octal mode with or without `0o`), comments, blank lines, indents | it parses to exactly those entries in order; with one malformed line added it is rejected |
| `parse_config_model` | a `vminit.conf` of valid lines for most keys, comments, blanks, and malformed lines | the `InitConfig` equals the one built line by line; each malformed line adds exactly one error naming its line number and changes nothing else |
| `dhcp_exchange_model` | `acquire_lease` over a scripted `DhcpLink` with a virtual clock | the whole exchange against RFC 2131: the first message is a DISCOVER, each REQUEST answers an OFFER of its transaction and carries its address and server identifier, the wait before each retransmission is within the randomized backoff, an `Ok` lease is the ACK's lease and a usable address, `NoOffer` and `NoAck` report the sends made and come only at the limit (or after more NAKs than the restart limit), and the sends stay bounded; the server's answers (an OFFER, ACK, NAK, a reply to another transaction, bytes that are no reply, nothing; each after 0 to 3 s) are chosen by the fuzzer |

These are checks against my reading of the documented formats. If one fails, the parser or the model may be wrong: read the reproducer before deciding.

## Run

Needs `cargo-fuzz`, the dated nightly `nightly-2026-10-02` (the one pinned for vmruntime's eBPF build; vminit has no toolchain pin of its own), and a C++ compiler: the build compiled libFuzzer with g++ 15 and needed no override. Linux and WSL only. These are the commands that were run on 2026-10-31 (WSL Ubuntu), with `cargo-fuzz` 0.13.2, `libfuzzer-sys` 0.4.13 and `arbitrary` 1.5.0 (`Cargo.lock` in this folder):

```bash
cargo install cargo-fuzz --locked              # once (19 min on a busy host)
rustup toolchain install nightly-2026-10-02    # once, if the nightly is not there

cd main/features/vminit_fuzz     # from the repository root; the targets are the package in bin/
# build into a directory on the WSL file system: a checkout under /mnt/c is slow to build in (first build 22 min on a busy host)
cargo +nightly-2026-10-02 fuzz build --fuzz-dir bin --target-dir "$HOME/vminit-fuzz-target"

# run against a COPY of the seeds: libFuzzer adds the inputs it finds to the corpus directory it is given
mkdir -p "$HOME/fuzz-work" && cp -r bin/corpus/read_marker "$HOME/fuzz-work/read_marker"
cargo +nightly-2026-10-02 fuzz run --fuzz-dir bin read_marker "$HOME/fuzz-work/read_marker" \
    --target-dir "$HOME/vminit-fuzz-target" -- -max_total_time=3600
```

Targets: `read_marker`, `parse_dhcp_response`, `parse_config`, `parse_overlay_manifest`. A crash is reported as `deadly signal` with the panic message above it, and the input is written to `bin/artifacts/<target>/crash-<hash>`.

To check that a target can fail, edit the parser to remove a check (for example, change `file.get(offset..end).ok_or(...)` in `read_marker` to `&file[offset..end]`), rebuild and run it: the fuzzer should find it within seconds; then restore the file with `git checkout` (or from a copy, if the checkout has CRLF line endings).

Crashes are written to `bin/artifacts/<target>/` (not committed). A crash is minimized with `cargo fuzz tmin`, fixed in `swe_vminit_init`, and kept as a regression test in `main/features/init/tests/` that fails without the fix.

## Seeds

`bin/corpus/<target>/` holds a few hundred bytes of seed per target, written from the documented formats (`docs/xkvm_conf.md`, the parser doc comments and the layout the marker tests build). **They are synthesized, not captured**: a real captured DHCP exchange and a real `vmfs` file are a later task. Small seed corpora are committed; larger ones are not.

## Check the targets before trusting them

A target that passes proves little until it has been shown to fail. `scripts/fuzz/planted_bugs.json` lists the bugs planted so far in the
extraction code (the entry name joined without `safe_path_join`, a symlink extracted, a file truncated, a directory not created); the tool
that plants them one at a time, rebuilds, and runs each model target and its control is in the vmruntime repository,
`scm/scripts/fuzz/plant_bug.py` (see its README):

```bash
python3 <vmruntime>/scm/scripts/fuzz/plant_bug.py --spec scripts/fuzz/planted_bugs.json --check-only   # the texts still match; builds nothing
python3 <vmruntime>/scm/scripts/fuzz/plant_bug.py --spec scripts/fuzz/planted_bugs.json --seconds 60    # plant, build, run
```

Result of 2026-10-31: 4 of 4 found by `pkg_extract_model`, none by `pkg_extract_survive` (vminit#18). The generator of that target carries a unit test
that counts how often a generated run reaches a case the model checks, because an earlier generator reached it in 0.8% of runs and missed a planted bug.
