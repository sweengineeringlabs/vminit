#![no_main]

use core::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use libfuzzer_sys::{arbitrary::Unstructured, fuzz_target};
use swe_vminit_init::{parse_config, BlockMount, GuestNetworkMode, InitConfig, SignalMode, VolumeSpec, MAX_DNS_SERVERS};

// Checks the VALUES `parse_config` returns. The fuzzer's bytes choose a list of lines: valid ones for most keys (written
// from the format in spi/config.rs), comment and blank lines, and malformed ones (no `=`, an unknown key, a bad value).
// The expected `InitConfig` is built here line by line from what each valid line means; the result must equal it, and
// every malformed line must add exactly one error that names its line number and must change nothing else.

const WORD: &[char] = &['a', 'b', 'z', '0', '9', '.', '_', '-', '/'];
const FREE: &[char] = &['a', 'z', '0', '=', ':', ' ', '"', '\'', 'é', '漢', '😀', '#', '$', '\\'];

fn word(u: &mut Unstructured) -> String {
    let len = u.int_in_range(1..=8usize).unwrap_or(1);
    (0..len).map(|_| *u.choose(WORD).unwrap_or(&'a')).collect()
}

fn free(u: &mut Unstructured) -> String {
    let len = u.int_in_range(0..=12usize).unwrap_or(0);
    (0..len).map(|_| *u.choose(FREE).unwrap_or(&'a')).collect()
}

fn name(u: &mut Unstructured) -> String {
    let len = u.int_in_range(1..=6usize).unwrap_or(1);
    (0..len).map(|_| *u.choose(&['A', 'B', 'x', '_', '7']).unwrap_or(&'A')).collect()
}

fuzz_target!(|bytes: &[u8]| {
    let mut u = Unstructured::new(bytes);
    let mut lines: Vec<String> = Vec::new();
    let mut expected = InitConfig::default();
    let mut error_lines: Vec<usize> = Vec::new();

    for _ in 0..u.int_in_range(0..=30).unwrap_or(0) {
        let indent = *u.choose(&["", "", " ", "\t", "  "]).unwrap_or(&"");
        let line;
        let mut bad = false;
        match u.int_in_range(0..=27).unwrap_or(0) {
            0 => line = String::new(),
            1 => line = format!("{indent}# a comment with = and key=value"),
            2 => {
                let arg = free(&mut u); // an entrypoint argument is kept exactly, spaces and all
                line = format!("{indent}entrypoint={arg}");
                expected.entrypoint.push(arg);
            }
            3 => {
                let arg = word(&mut u);
                let hex: String = arg.bytes().map(|b| if u.arbitrary::<bool>().unwrap_or(false) { format!("{b:02X}") } else { format!("{b:02x}") }).collect();
                line = format!("{indent}entrypoint_hex={hex}");
                expected.entrypoint.push(arg);
            }
            4 => {
                let (key, value) = (name(&mut u), free(&mut u));
                line = format!("{indent}env={key}={value}");
                expected.env.push((key, value));
            }
            5 => {
                let (tag, mount, read_only) = (word(&mut u), word(&mut u), u.arbitrary::<bool>().unwrap_or(false));
                line = format!("{indent}volume={tag}:{mount}:{}", if read_only { "ro" } else { "rw" });
                expected.volumes.push(VolumeSpec { tag, guest_mount: mount, read_only });
            }
            6 => {
                let (device, mount, fstype) = (word(&mut u), word(&mut u), word(&mut u));
                line = format!("{indent}block_mount={device}:{mount}:{fstype}");
                expected.block_mounts.push(BlockMount { device, guest_mount: mount, fstype });
            }
            7 => {
                let package = word(&mut u);
                line = format!("{indent}install={package}");
                expected.packages.push(package);
            }
            8 => {
                let on = u.arbitrary::<bool>().unwrap_or(false);
                line = format!("{indent}interactive={}", on as u8);
                expected.interactive = on;
            }
            9 => {
                let on = u.arbitrary::<bool>().unwrap_or(false);
                line = format!("{indent}start_agent={}", on as u8);
                expected.start_agent = on;
            }
            10 => {
                let on = u.arbitrary::<bool>().unwrap_or(false);
                line = format!("{indent}mount_rootfs={}", on as u8);
                expected.mount_rootfs = on;
            }
            11 => {
                let shared = u.arbitrary::<bool>().unwrap_or(false);
                line = format!("{indent}signal_mode={}", if shared { "shared_memory" } else { "serial" });
                expected.signal_mode = if shared { SignalMode::SharedMemory } else { SignalMode::Serial };
            }
            12 => {
                let dhcp = u.arbitrary::<bool>().unwrap_or(true);
                line = format!("{indent}network={}", if dhcp { "dhcp" } else { "none" });
                expected.network_mode = if dhcp { GuestNetworkMode::Dhcp } else { GuestNetworkMode::None };
            }
            13 => {
                let address: IpAddr = if u.arbitrary::<bool>().unwrap_or(true) {
                    IpAddr::V4(Ipv4Addr::from(u.arbitrary::<[u8; 4]>().unwrap_or([0; 4])))
                } else {
                    IpAddr::V6(Ipv6Addr::from(u.arbitrary::<[u8; 16]>().unwrap_or([0; 16])))
                };
                line = format!("{indent}dns={address}");
                if expected.dns_servers.len() < MAX_DNS_SERVERS {
                    expected.dns_servers.push(address);
                } else {
                    bad = true; // the fourth server is refused
                }
            }
            14 => {
                let path = word(&mut u);
                line = format!("{indent}manifest=/{path}");
                expected.manifest_path = Some(format!("/{path}"));
            }
            15 => {
                let host = word(&mut u);
                line = format!("{indent}manifest_url=https://{host}/m.json");
                expected.manifest_url = Some(format!("https://{host}/m.json"));
            }
            // malformed lines: one error each, nothing else changes
            16 => { line = format!("{indent}no-equals-sign-{}", word(&mut u)); bad = true; }
            17 => { line = format!("{indent}zz{}=1", word(&mut u)); bad = true; }
            18 => { line = format!("{indent}interactive=2"); bad = true; }
            19 => { line = format!("{indent}env=NOEQUALS"); bad = true; }
            20 => { line = format!("{indent}volume=a:b"); bad = true; }
            21 => { line = format!("{indent}network=bridge"); bad = true; }
            22 => { line = format!("{indent}dns=not-an-address"); bad = true; }
            23 => { line = format!("{indent}entrypoint_hex=abc"); bad = true; }
            24 => { line = format!("{indent}signal_mode=smoke"); bad = true; }
            25 => { line = format!("{indent}volume={}:{}:rwx", word(&mut u), word(&mut u)); bad = true; }
            26 => { line = format!("{indent}manifest_url=http://plain.example/m.json"); bad = true; }
            _ => { line = format!("{indent}install="); bad = true; }
        }
        lines.push(line);
        if bad {
            error_lines.push(lines.len());
        }
    }

    let separator = if u.arbitrary::<bool>().unwrap_or(false) { "\r\n" } else { "\n" };
    let text = lines.join(separator);
    let mut got = parse_config(&text);

    let errors = std::mem::take(&mut got.errors);
    assert_eq!(errors.len(), error_lines.len(), "errors {errors:?} for malformed lines {error_lines:?} in:\n{text}");
    for (error, line) in errors.iter().zip(&error_lines) {
        assert!(error.starts_with(&format!("line {line}:")), "error {error:?} should name line {line}");
    }
    assert_eq!(got, expected, "config differs for:\n{text}");
});
