#!/usr/bin/env python3
"""Boot test for the vmfs banner and config errors (vminit#7). Linux/KVM, needs a vmruntime daemon.

    boot_banner.py --daemon vmruntime --cli vmenginec --root <firmware root> --kernel bzImage --vmfs vmfs

Boots the given vmfs as /init of a small initramfs with three /etc/vminit.conf files and checks the serial output:
  valid      `vminit <version> capabilities=<list>` comes before XIKA_READY, XIKA_EXIT:0 follows, exit code 0
  unknown    a line `FATAL: /etc/vminit.conf: line 2: unknown key "bogus_key"`, XIKA_EXIT:1, no XIKA_READY, exit code 1
  malformed  a line `FATAL: /etc/vminit.conf: line 2: volume must be ...`, XIKA_EXIT:1, no XIKA_READY, exit code 1
The list in the banner must equal the capabilities in the vmfs file's ELF note (read with check_release_marker.py).
Exit status 0 when every check passed, 1 when one failed.
"""
import argparse
import os
import re
import shutil
import socket
import subprocess
import sys
import tempfile
import time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "..", "ci"))

BANNER = re.compile(r"^vminit (\S+) capabilities=(\S+)$", re.M)


def free_port():
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        return sock.getsockname()[1]


def wait_for_port(port, seconds=30):
    deadline = time.time() + seconds
    while time.time() < deadline:
        with socket.socket() as sock:
            if sock.connect_ex(("127.0.0.1", port)) == 0:
                return
        time.sleep(0.2)
    raise SystemExit("the daemon did not listen on port %d within %ds" % (port, seconds))


def make_initrd(work, name, vmfs, conf):
    tree = os.path.join(work, name)
    os.makedirs(tree + "/etc")
    shutil.copy(vmfs, tree + "/init")
    os.chmod(tree + "/init", 0o755)
    with open(tree + "/etc/vminit.conf", "w") as handle:
        handle.write(conf)
    out = os.path.join(work, name + ".cpio")
    subprocess.run("cd %s && find . | cpio -o -H newc --quiet > %s" % (tree, out), shell=True, check=True)
    return out


def boot(args, port, name, initrd):
    result = subprocess.run([args.cli, "boot", "--name", name, "--kernel", args.kernel, "--initrd", initrd,
                             "--memory", "192", "--timeout", "60"],
                            env=dict(os.environ, XKVM_DAEMON="http://127.0.0.1:%d" % port),
                            capture_output=True, text=True, timeout=120)
    return result.returncode, (result.stdout + result.stderr).replace("\r", "")


def marker_capabilities(vmfs):
    import check_release_marker
    with open(vmfs, "rb") as handle:
        return check_release_marker.read_marker(handle.read())


def check(problems, condition, message):
    if not condition:
        problems.append(message)


def main(argv):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    for option in ("--daemon", "--cli", "--root", "--kernel", "--vmfs"):
        parser.add_argument(option, required=True)
    args = parser.parse_args(argv)
    work = tempfile.mkdtemp(prefix="boot-banner-")
    port = free_port()
    daemon = subprocess.Popen([args.daemon, "--config", os.path.join(args.root, "main", "config", "application.toml"),
                               "--listen", "127.0.0.1:%d" % port],
                              stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, stdin=subprocess.DEVNULL)
    failures = 0
    try:
        wait_for_port(port)
        file_version, file_capabilities = marker_capabilities(args.vmfs)
        cases = [
            ("valid", "entrypoint=/bin/true\nnetwork=none\n", 0, None),
            ("unknown", "entrypoint=/bin/true\nbogus_key=secret-value\n", 1, 'line 2: unknown key "bogus_key"'),
            ("malformed", "entrypoint=/bin/true\nvolume=onlytag\n", 1, "line 2: volume must be"),
        ]
        for name, conf, want_code, want_fatal in cases:
            code, output = boot(args, port, "banner-" + name, make_initrd(work, name, args.vmfs, conf))
            problems = []
            banner = BANNER.search(output)
            check(problems, banner is not None, "no `vminit <version> capabilities=<list>` line")
            if banner:
                check(problems, banner.group(1) == file_version, "banner version %r, file says %r" % (banner.group(1), file_version))
                check(problems, banner.group(2).split(",") == file_capabilities,
                      "banner capabilities differ from the ELF note: %r vs %r" % (banner.group(2), file_capabilities))
            check(problems, code == want_code, "exit code %d, wanted %d" % (code, want_code))
            if want_fatal is None:
                check(problems, "XIKA_READY" in output, "no XIKA_READY")
                check(problems, "XIKA_EXIT:0" in output, "no XIKA_EXIT:0")
                if banner and "XIKA_READY" in output:
                    check(problems, banner.start() < output.index("XIKA_READY"), "banner is not before XIKA_READY")
            else:
                check(problems, "FATAL: /etc/vminit.conf: " + want_fatal in output, "no FATAL line naming %r" % want_fatal)
                check(problems, "XIKA_EXIT:1" in output, "no XIKA_EXIT:1")
                check(problems, "XIKA_READY" not in output, "XIKA_READY printed although the config was refused")
                check(problems, "secret-value" not in output, "the value of a refused line was printed")
            print("%-10s %s" % (name, "ok" if not problems else "FAILED"), flush=True)
            for problem in problems:
                print("    - " + problem)
            failures += 1 if problems else 0
    finally:
        daemon.terminate()
        try:
            daemon.wait(timeout=20)
        except subprocess.TimeoutExpired:
            daemon.kill()
        shutil.rmtree(work, ignore_errors=True)
    print("FAIL: %d case(s) failed" % failures if failures else "PASS: every case passed")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
