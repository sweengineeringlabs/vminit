#!/usr/bin/env python3
"""Read the capability marker of a built vmfs and check it against a release tag.

    check_release_marker.py <vmfs> <tag> [<release-notes-file>]

Exit status 0 when the marker's version equals the tag without its leading "v";
1 when it differs, the marker is missing or malformed. With a third argument the
capability list is written there as Markdown, for the GitHub release body.

This is an independent implementation of the format in docs/xkvm_conf.md
("Capability marker"), standard library only, so CI does not rely on the Rust code
that wrote the note to also be the code that checks it.
"""
import struct
import sys

NOTE_NAME = b"vminit\0"
NOTE_TYPE = 1
FORMAT_VERSION = 1
PT_NOTE = 4


def fail(message):
    print(f"error: {message}", file=sys.stderr)
    sys.exit(1)


def read_marker(data):
    """Return (version, [capabilities]) from the vminit note of an ELF64 LE file."""
    if len(data) < 64 or data[:4] != b"\x7fELF" or data[4] != 2 or data[5] != 1:
        fail("not a little-endian ELF64 file")
    phoff, = struct.unpack_from("<Q", data, 0x20)
    phentsize, phnum = struct.unpack_from("<HH", data, 0x36)
    for index in range(phnum):
        header = phoff + index * phentsize
        p_type, = struct.unpack_from("<I", data, header)
        if p_type != PT_NOTE:
            continue
        offset, = struct.unpack_from("<Q", data, header + 8)
        size, = struct.unpack_from("<Q", data, header + 32)
        align, = struct.unpack_from("<Q", data, header + 48)
        pad = 8 if align == 8 else 4
        segment = data[offset:offset + size]
        at = 0
        while at < len(segment):
            namesz, descsz, note_type = struct.unpack_from("<III", segment, at)
            name_at = at + 12
            desc_at = name_at + -(-namesz // pad) * pad
            desc = segment[desc_at:desc_at + descsz]
            if len(desc) != descsz:
                fail("note runs past its segment")
            if segment[name_at:name_at + namesz] == NOTE_NAME and note_type == NOTE_TYPE:
                return decode_payload(desc)
            at = desc_at + -(-descsz // pad) * pad
    fail("no vminit note in the file")


def decode_payload(desc):
    (format_version,) = struct.unpack_from("<I", desc, 0)
    if format_version != FORMAT_VERSION:
        fail(f"marker format version {format_version} is not supported (expected {FORMAT_VERSION})")
    at = 4

    def text():
        nonlocal at
        (length,) = struct.unpack_from("<H", desc, at)
        value = desc[at + 2:at + 2 + length].decode("utf-8")
        at += 2 + length
        return value

    version = text()
    (count,) = struct.unpack_from("<H", desc, at)
    at += 2
    capabilities = [text() for _ in range(count)]
    if at != len(desc):
        fail("bytes after the last capability")
    return version, capabilities


def main(argv):
    if len(argv) not in (3, 4):
        fail("usage: check_release_marker.py <vmfs> <tag> [<release-notes-file>]")
    with open(argv[1], "rb") as handle:
        version, capabilities = read_marker(handle.read())
    tag = argv[2]
    if not tag.startswith("v") or tag[1:] != version:
        fail(f"the binary's marker says version {version!r} but the tag is {tag!r}; "
             f"set the version in main/features/bin/Cargo.toml to {tag[1:]!r} before tagging")
    print(f"vmfs {version} capabilities={','.join(capabilities)}")
    if len(argv) == 4:
        with open(argv[3], "w", encoding="utf-8") as notes:
            notes.write(f"## vmfs {version}\n\nCapabilities (config keys this vmfs accepts; "
                        "a host checks for the one it needs):\n\n")
            for name in capabilities:
                notes.write(f"- `{name}`\n")
            notes.write("\nThe list is read from the `.note.vminit` ELF note of the attached `vmfs`.\n\n")


main(sys.argv)
